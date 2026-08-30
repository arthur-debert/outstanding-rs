//! OS-enforced filesystem isolation for every untrusted process (env
//! scrubbing alone is not a boundary): macOS Seatbelt or Linux Landlock,
//! whichever the host can enforce. The backends are not equivalent —
//! Landlock (ABI v1) cannot enforce a network denial — so [`capability`]
//! records what was actually enforced.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::report::{IsolationRecord, NetworkEnforcement};

#[derive(Debug, Clone)]
pub struct Policy {
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub deny_read: Vec<PathBuf>,
    pub network: bool,
}

impl Policy {
    pub fn new(
        read: Vec<PathBuf>,
        write: Vec<PathBuf>,
        deny_read: Vec<PathBuf>,
        network: bool,
    ) -> Self {
        Self {
            read: existing(read),
            write: existing(write),
            deny_read: existing(deny_read),
            network,
        }
    }
}

pub fn capability(network_allowed: bool) -> IsolationRecord {
    #[cfg(target_os = "macos")]
    {
        IsolationRecord {
            backend: "macos-seatbelt".to_string(),
            filesystem: "allow-default; reads denied under listed user/source roots, \
                         writes denied outside the phase workspace"
                .to_string(),
            network: if network_allowed {
                NetworkEnforcement::AllowedByPolicy
            } else {
                NetworkEnforcement::Denied
            },
        }
    }
    #[cfg(target_os = "linux")]
    {
        IsolationRecord {
            backend: "linux-landlock".to_string(),
            filesystem: "default-deny; only admitted workspace/system/toolchain roots \
                         are reachable"
                .to_string(),
            network: if network_allowed {
                NetworkEnforcement::AllowedByPolicy
            } else {
                NetworkEnforcement::DenialRequestedButUnsupported
            },
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = network_allowed;
        IsolationRecord {
            backend: "unavailable".to_string(),
            filesystem: "not enforced".to_string(),
            network: NetworkEnforcement::NotEnforced,
        }
    }
}

// Call after configuring cwd/env and before configuring stdio.
pub fn apply(command: &mut Command, policy: &Policy) -> Result<(), String> {
    validate_denied_boundaries(policy)?;
    warn_unenforced_network(policy);

    #[cfg(target_os = "macos")]
    return apply_macos(command, policy);

    #[cfg(target_os = "linux")]
    return apply_linux(command, policy);

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (command, policy);
        Err("corpus isolation requires macOS Seatbelt or Linux Landlock".to_string())
    }
}

fn warn_unenforced_network(policy: &Policy) {
    #[cfg(target_os = "linux")]
    {
        static WARNED: std::sync::Once = std::sync::Once::new();
        if !policy.network {
            WARNED.call_once(|| {
                eprintln!(
                    "[corpus] warning: this phase's policy disables network, but \
                     linux-landlock (ABI v1) is filesystem-only and cannot enforce it; \
                     the report records network isolation as \
                     denial-requested-but-unsupported"
                );
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = policy;
}

// A policy admitting a denied root (or an ancestor of one) is invalid: a
// broad exception there would reopen the denied subtree.
fn validate_denied_boundaries(policy: &Policy) -> Result<(), String> {
    for denied in &policy.deny_read {
        for admitted in policy.read.iter().chain(&policy.write) {
            if denied.starts_with(admitted) {
                return Err(format!(
                    "sandbox policy admits {} which covers denied root {}",
                    admitted.display(),
                    denied.display()
                ));
            }
        }
    }
    Ok(())
}

fn existing(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = paths
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

pub fn system_read_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = [
        "/bin",
        "/sbin",
        "/usr",
        "/lib",
        "/lib64",
        "/etc",
        "/dev",
        "/proc",
        "/System/Library",
        "/System/Cryptexes",
        "/Library",
        "/opt/homebrew",
        "/usr/local",
        "/private/etc",
        "/private/var",
        "/AppleInternal",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .collect();
    if let Ok(rustup) = std::env::var("RUSTUP_HOME") {
        roots.push(PathBuf::from(rustup));
    } else if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join(".rustup"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if !dir.is_dir() {
                continue;
            }
            // Relocatable toolchains need their sibling `lib` admitted too.
            if let Some(lib) = toolchain_sibling_lib(&dir) {
                roots.push(lib);
            }
            roots.push(dir);
        }
    }
    existing(roots)
}

fn toolchain_sibling_lib(dir: &Path) -> Option<PathBuf> {
    match dir.file_name().and_then(|name| name.to_str()) {
        Some("bin") | Some("sbin") => {}
        _ => return None,
    }
    let lib = dir.parent()?.join("lib");
    lib.is_dir().then_some(lib)
}

#[cfg(target_os = "macos")]
fn apply_macos(command: &mut Command, policy: &Policy) -> Result<(), String> {
    use std::ffi::OsString;

    fn quote(path: &Path) -> String {
        format!(
            "\"{}\"",
            path.display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        )
    }

    let mut profile = String::from("(version 1)\n(allow default)\n");
    for denied in &policy.deny_read {
        profile.push_str(&format!(
            "(deny file-read* (require-all (subpath {})",
            quote(denied)
        ));
        for allowed in &policy.read {
            profile.push_str(&format!(" (require-not (subpath {}))", quote(allowed)));
        }
        profile.push_str("))\n");
    }
    profile.push_str("(deny file-write* (require-all");
    for path in &policy.write {
        profile.push_str(&format!(" (require-not (subpath {}))", quote(path)));
    }
    profile.push_str("))\n");
    if !policy.network {
        profile.push_str("(deny network*)\n");
    }
    // Keychain access is not gated by HOME, so deny the security broker too.
    profile.push_str("(deny mach-lookup (global-name-prefix \"com.apple.securityd\"))\n");
    profile.push_str("(deny mach-lookup (global-name-prefix \"com.apple.security.agent\"))\n");

    let program = command.get_program().to_os_string();
    let args: Vec<OsString> = command.get_args().map(ToOwned::to_owned).collect();
    let cwd = command.get_current_dir().map(Path::to_path_buf);
    let env: Vec<(OsString, Option<OsString>)> = command
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(ToOwned::to_owned)))
        .collect();

    let mut wrapped = Command::new("/usr/bin/sandbox-exec");
    // `get_envs()` exposes only additions/removals, not env_clear itself.
    wrapped.env_clear();
    wrapped
        .args(["-p", &profile])
        .arg("--")
        .arg(program)
        .args(args);
    if let Some(cwd) = cwd {
        wrapped.current_dir(cwd);
    }
    for (key, value) in env {
        match value {
            Some(value) => {
                wrapped.env(key, value);
            }
            None => {
                wrapped.env_remove(key);
            }
        }
    }
    *command = wrapped;
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_linux(command: &mut Command, policy: &Policy) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let policy = policy.clone();
    // SAFETY: the closure only performs the async-signal-safe Landlock
    // syscalls and opens preselected paths before exec. It does not touch
    // shared application state.
    unsafe {
        command.pre_exec(move || {
            enforce_landlock(&policy)
                .map_err(|detail| std::io::Error::new(std::io::ErrorKind::PermissionDenied, detail))
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn enforce_landlock(policy: &Policy) -> Result<(), String> {
    use landlock::{
        Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus, ABI,
    };

    let abi = ABI::V1;
    let all = AccessFs::from_all(abi);
    let read = AccessFs::from_read(abi);
    // `/dev/null` needs file-legal rights, not directory rights, or the
    // kernel rejects the rule with EINVAL.
    let file_only = AccessFs::from_file(abi);
    let dev_null = Path::new("/dev/null");
    let mut ruleset = Ruleset::default()
        .handle_access(all)
        .map_err(|e| format!("creating Landlock access set: {e}"))?
        .create()
        .map_err(|e| format!("creating Landlock ruleset: {e}"))?;
    for path in &policy.read {
        let fd = PathFd::new(path)
            .map_err(|e| format!("opening {} for Landlock: {e}", path.display()))?;
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, read))
            .map_err(|e| format!("allowing Landlock read {}: {e}", path.display()))?;
    }
    for path in &policy.write {
        let fd = PathFd::new(path)
            .map_err(|e| format!("opening {} for Landlock: {e}", path.display()))?;
        let access = if path.as_path() == dev_null {
            file_only
        } else {
            all
        };
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, access))
            .map_err(|e| format!("allowing Landlock write {}: {e}", path.display()))?;
    }
    let status = ruleset
        .restrict_self()
        .map_err(|e| format!("enforcing Landlock: {e}"))?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        return Err(format!(
            "Landlock is not fully enforced: {:?}",
            status.ruleset
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(read: &[&str], write: &[&str], denied: &[&str]) -> Policy {
        Policy {
            read: read.iter().map(PathBuf::from).collect(),
            write: write.iter().map(PathBuf::from).collect(),
            deny_read: denied.iter().map(PathBuf::from).collect(),
            network: false,
        }
    }

    #[test]
    fn broad_allow_cannot_cover_a_denied_root() {
        let policy = policy(
            &["/system", "/home/runner/work"],
            &["/tmp/workspace"],
            &["/home/runner/work/project"],
        );
        let err = validate_denied_boundaries(&policy).unwrap_err();
        assert!(err.contains("/home/runner/work"), "{err}");
        assert!(err.contains("/home/runner/work/project"), "{err}");
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn capability_reflects_the_phase_policy_network_request() {
        assert_eq!(
            capability(true).network,
            NetworkEnforcement::AllowedByPolicy
        );
        #[cfg(target_os = "macos")]
        assert_eq!(capability(false).network, NetworkEnforcement::Denied);
        #[cfg(target_os = "linux")]
        assert_eq!(
            capability(false).network,
            NetworkEnforcement::DenialRequestedButUnsupported
        );
    }

    #[test]
    fn narrow_tool_path_beneath_a_denied_root_does_not_cover_siblings() {
        let policy = policy(
            &[
                "/system",
                "/home/runner/work/project/.pixi/envs/default/bin",
            ],
            &["/tmp/workspace"],
            &["/home/runner/work/project"],
        );
        validate_denied_boundaries(&policy).unwrap();
    }

    #[test]
    fn toolchain_sibling_lib_admits_only_a_bin_or_sbin_entrys_lib() {
        let root = tempfile::tempdir().unwrap();

        let toolchain = root.path().join("envs/default");
        std::fs::create_dir_all(toolchain.join("bin")).unwrap();
        std::fs::create_dir_all(toolchain.join("lib")).unwrap();
        assert_eq!(
            toolchain_sibling_lib(&toolchain.join("bin")),
            Some(toolchain.join("lib"))
        );

        let scripts = root.path().join("home").join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::create_dir_all(root.path().join("home").join("lib")).unwrap();
        assert_eq!(toolchain_sibling_lib(&scripts), None);

        let bare = root.path().join("nolib");
        std::fs::create_dir_all(bare.join("bin")).unwrap();
        assert_eq!(toolchain_sibling_lib(&bare.join("bin")), None);
    }
}
