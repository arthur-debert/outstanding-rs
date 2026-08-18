//! OS-enforced filesystem isolation for every untrusted process.
//!
//! Environment scrubbing is not a filesystem boundary: a build script can
//! still open `~/.cargo/credentials.toml`, and an agent can walk to a nearby
//! checkout by absolute path. The runner therefore installs a kernel
//! filesystem policy in the child before it executes. macOS Seatbelt denies
//! host user-data/source reads and writes outside the phase workspace; Linux
//! Landlock admits only the phase workspace and selected system/toolchain
//! roots. If the host cannot enforce either policy, the run refuses to start.
//!
//! The two backends are not equivalent, and reports must not pretend they
//! are: Seatbelt is allow-default with denied roots and enforces network
//! denial; Landlock (ABI v1) is default-deny over admitted roots and is
//! filesystem-only — a `network = false` policy is NOT enforced there. The
//! gap is recorded per phase policy by [`capability`] and warned about at
//! apply time, so a report never reads stronger than what was enforced.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::report::{IsolationRecord, NetworkEnforcement};

/// One process's filesystem policy.
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

/// What this host's backend actually enforces for one phase policy,
/// recorded verbatim in reports (see [`IsolationRecord`]).
///
/// `network_allowed` is the phase policy's network flag (see
/// [`Policy::new`]): a policy that allows network yields
/// [`NetworkEnforcement::AllowedByPolicy`] on every backend, while a
/// requested denial is enforced by Seatbelt and loudly unenforced on
/// Landlock (ABI v1, filesystem-only).
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

/// Installs `policy` around `command` without changing its argv, cwd, env, or
/// stdio. Call this after configuring cwd/env and before configuring stdio.
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

/// Warns (once per process) when a policy asks for a network denial the
/// active backend cannot enforce. The report records the same gap via
/// [`capability`]; this makes it loud at run time too.
fn warn_unenforced_network(policy: &Policy) {
    #[cfg(target_os = "linux")]
    {
        static WARNED: std::sync::Once = std::sync::Once::new();
        if !policy.network {
            WARNED.call_once(|| {
                eprintln!(
                    "[corpus] warning: this phase's policy disables network, but \
                     linux-landlock (ABI v1) is filesystem-only and cannot enforce it; \
                     the report records network isolation as NOT ENFORCED"
                );
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = policy;
}

/// Refuses a policy whose allowlist contains a denied root or one of its
/// ancestors. Landlock cannot subtract a denied subtree from a broader
/// admitted hierarchy, and a broad Seatbelt exception would reopen the same
/// subtree. Narrow tool paths beneath a denied user/source root remain valid:
/// admitting that exact subtree does not grant access to its parent or siblings.
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

/// System/toolchain paths that are safe to expose read-only. User data and
/// the repository are deliberately absent. The Rust toolchain is added
/// explicitly because rustup normally installs it outside these roots.
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
    // Permit PATH directories read-only. If one lives beneath an explicitly
    // denied source/home root, the macOS profile admits only this narrower
    // subtree; Landlock's default-deny model likewise adds only this path.
    // A PATH entry named `bin`/`sbin` also admits its sibling `lib`:
    // relocatable toolchains (rustup-less pixi/conda envs) resolve their
    // dylibs and sysroot through `bin/../lib`, so a `bin` admitted without
    // its `lib` yields an rustc that launches and then aborts loading
    // librustc_driver. The name check keeps this narrow: an arbitrary PATH
    // entry (e.g. `$HOME/bin` where `bin` is just a personal scripts dir)
    // still only admits a sibling `lib` when the directory is actually named
    // `bin`/`sbin`, not any directory that happens to have a `lib` sibling.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if !dir.is_dir() {
                continue;
            }
            if let Some(lib) = toolchain_sibling_lib(&dir) {
                roots.push(lib);
            }
            roots.push(dir);
        }
    }
    existing(roots)
}

/// The sibling `lib` directory for a PATH entry, admitted only when the
/// entry is itself named `bin`/`sbin` (the toolchain layout this targets)
/// and that sibling actually exists. Kept as a pure, name-gated helper so an
/// unrelated PATH entry (any directory not named `bin`/`sbin`) never widens
/// the sandbox's read surface to its sibling directories.
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
    // A disposable HOME is insufficient on macOS: CLI credentials may live
    // in Keychain. Deny the broker to the agent and every generated child.
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
    // Every caller establishes a complete phase environment with
    // `env_clear()`. Reconstructing the command must preserve that negative
    // state; `Command::get_envs()` exposes additions/removals, not the clear
    // marker itself.
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
    // `/dev/null` is the ONLY non-directory ever admitted for write (see
    // `Isolation::policy`): a `PathBeneath` rule's parent must either be a
    // directory, or request only the access rights legitimate for a file
    // (`AccessFs::from_file`) — directory-only rights (MakeChar, RemoveFile,
    // ReadDir, ...) on a non-directory target make the kernel reject the
    // whole rule with EINVAL (see landlock_add_rule(2)). Two earlier
    // attempts at inferring "is this a directory" from filesystem state at
    // rule-construction time (`path.is_dir()`, then
    // `path.exists() && !path.is_dir()`) proved unnecessary risk for no
    // observed benefit — every other write root here is unconditionally a
    // real, already-provisioned directory (`workspace_root`, a phase's
    // isolated home), so there is nothing to infer: key on the literal
    // `/dev/null` path instead of a runtime type check.
    //
    // `AccessFs::from_file` intersects with the ABI-V1-only `all` above, so
    // it never includes `Truncate` (added at ABI V3) regardless of what the
    // running kernel additionally supports: this ruleset only ever
    // `handle_access`es the V1 set, and Landlock leaves any access-right
    // category outside a process's handled set ungoverned (not restricted),
    // so a `>`-style truncating redirect onto `/dev/null` is unaffected
    // either way.
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

        // `envs/default/bin` with a `lib` sibling: the relocatable-toolchain
        // shape this is meant to admit.
        let toolchain = root.path().join("envs/default");
        std::fs::create_dir_all(toolchain.join("bin")).unwrap();
        std::fs::create_dir_all(toolchain.join("lib")).unwrap();
        assert_eq!(
            toolchain_sibling_lib(&toolchain.join("bin")),
            Some(toolchain.join("lib"))
        );

        // An arbitrary PATH entry that is not named `bin`/`sbin` (e.g. a
        // personal scripts directory) must not leak its sibling `lib`, even
        // when one exists.
        let scripts = root.path().join("home").join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::create_dir_all(root.path().join("home").join("lib")).unwrap();
        assert_eq!(toolchain_sibling_lib(&scripts), None);

        // A `bin` entry with no sibling `lib` admits nothing extra.
        let bare = root.path().join("nolib");
        std::fs::create_dir_all(bare.join("bin")).unwrap();
        assert_eq!(toolchain_sibling_lib(&bare.join("bin")), None);
    }
}
