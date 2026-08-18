//! Blind-workspace provisioning: everything the agent sees, and nothing more.
//!
//! The workspace materializes exactly the archetype spec, an instructions
//! file, the rendered exit questionnaire, a snapshot of the *published*
//! documentation set, and a cargo scaffold whose standout dependencies are
//! exact-version crates.io pins — no path or git dependencies, so cargo
//! cannot resolve into a local checkout. This is the enforcement half of the
//! blindness protocol recorded in `corpus/README.md`; the recording half is
//! the questionnaire's sources questions.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};
use sha2::{Digest, Sha256};

use crate::archetype::Archetype;
use crate::digest;
use crate::questionnaire;
use crate::sandbox::{self, Policy};

/// The published documentation set: what the mdbook ships, relative to the
/// repo's `docs/` directory. ADRs, internal specs, proposals, and dev notes
/// are deliberately absent.
const PUBLISHED_DOCS: &[&str] = &["index.md", "intro.md", "guides", "topics", "crates"];

/// Names present in every phase after scrubbing. Values are phase-local:
/// HOME, CARGO_HOME, and TMPDIR never point at their host counterparts.
pub const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "CARGO_HOME",
    "RUSTUP_HOME",
];

fn apply_phase_env(command: &mut Command, home: &Path) {
    command.env_clear();
    for key in ["PATH", "SHELL", "TERM", "LANG", "LC_ALL", "RUSTUP_HOME"] {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    command.env("HOME", home);
    command.env("CARGO_HOME", home.join("cargo"));
    command.env("TMPDIR", home.join("tmp"));
}

/// Applies the roster cases' scrubbed baseline (`corpus/README.md`, Run
/// semantics): `env_clear()` plus `PATH` from the runner, `HOME` pointing
/// into the sandbox, and `LANG`/`LC_ALL` = `C.UTF-8`. Everything that
/// steers output — `TERM`, `NO_COLOR`, pager variables, tool variables —
/// is unset unless the case sets it, so a case's env is complete rather
/// than a delta against whatever the host exports.
pub fn apply_case_baseline_env(command: &mut Command, home: &Path) {
    command.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        command.env("PATH", path);
    }
    command.env("HOME", home);
    command.env("LANG", "C.UTF-8");
    command.env("LC_ALL", "C.UTF-8");
    command.env("TMPDIR", home.join("tmp"));
}

/// Enforced isolation roots for a run. Each phase has a disposable home,
/// while the source checkout is absent from every child policy.
#[derive(Debug, Clone)]
pub struct Isolation {
    pub workspace_root: PathBuf,
    pub agent_home: PathBuf,
    pub build_home: PathBuf,
    pub check_home: PathBuf,
    system_read: Vec<PathBuf>,
    denied_read: Vec<PathBuf>,
}

impl Isolation {
    pub fn new(workspace_root: &Path, source_root: &Path) -> anyhow::Result<Self> {
        let homes = workspace_root.join(".isolated-homes");
        let agent_home = homes.join("agent");
        let build_home = homes.join("build");
        let check_home = homes.join("check");
        for home in [&agent_home, &build_home, &check_home] {
            std::fs::create_dir_all(home.join("cargo"))?;
            std::fs::create_dir_all(home.join("tmp"))?;
        }
        Ok(Self {
            workspace_root: workspace_root.to_path_buf(),
            agent_home,
            build_home,
            check_home,
            system_read: sandbox::system_read_roots(),
            denied_read: [
                Some(source_root.to_path_buf()),
                std::env::var_os("HOME").map(PathBuf::from),
                Some(PathBuf::from("/Users")),
                Some(PathBuf::from("/Volumes")),
                Some(PathBuf::from("/home")),
                Some(PathBuf::from("/Network")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        })
    }

    fn policy(&self, writable: &Path, home: &Path, network: bool) -> Policy {
        let mut read = self.system_read.clone();
        read.push(self.workspace_root.clone());
        read.push(writable.to_path_buf());
        Policy::new(
            read,
            vec![writable.to_path_buf(), home.to_path_buf()],
            self.denied_read.clone(),
            network,
        )
    }

    pub fn apply_agent(&self, command: &mut Command) -> Result<(), String> {
        apply_phase_env(command, &self.agent_home);
        sandbox::apply(
            command,
            &self.policy(&self.workspace_root, &self.agent_home, true),
        )
    }

    pub fn apply_build(&self, command: &mut Command) -> Result<(), String> {
        apply_phase_env(command, &self.build_home);
        sandbox::apply(
            command,
            &self.policy(&self.workspace_root, &self.build_home, true),
        )
    }

    pub fn apply_check(&self, command: &mut Command, sandbox_root: &Path) -> Result<(), String> {
        apply_case_baseline_env(command, sandbox_root);
        std::fs::create_dir_all(sandbox_root.join("tmp"))
            .map_err(|e| format!("creating check tmp: {e}"))?;
        sandbox::apply(command, &self.policy(sandbox_root, &self.check_home, false))
    }

    /// Proves the source checkout and host home are unreadable from the same
    /// enforced boundary the agent receives. A run refuses to start if the
    /// kernel policy is missing or porous.
    pub fn verify_boundary(&self, source_root: &Path) -> Result<(), String> {
        let host_probe = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| source_root.to_path_buf())
            .join(".gitconfig");
        let mut command = Command::new("sh");
        // Perform real opens through shell redirections. A permission query
        // such as `test -r` may use access(2), which Landlock deliberately
        // leaves unrestricted and therefore cannot prove file readability.
        command
            .args([
                "-c",
                "if { : < \"$1\"; } 2>/dev/null; then \
                     echo source-readable >&2; exit 1; \
                 fi; \
                 if { : < \"$2\"; } 2>/dev/null; then \
                     echo host-home-readable >&2; exit 1; \
                 fi",
                "corpus-boundary",
            ])
            .arg(source_root.join("Cargo.toml"))
            .arg(host_probe)
            .current_dir(&self.workspace_root);
        self.apply_agent(&mut command)?;
        let output = command
            .output()
            .map_err(|e| format!("probing isolation boundary: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "OS sandbox allowed source-checkout/host-home access or failed to enforce \
                 (status {:?}, stdout {:?}, stderr {:?})",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
}

/// A provisioned blind workspace.
#[derive(Debug)]
pub struct Workspace {
    /// The directory the agent works in.
    pub root: PathBuf,
    /// The cargo project directory inside it.
    pub app_dir: PathBuf,
    /// sha256 (hex) over the provisioned docs snapshot's actual bytes.
    pub docs_sha256: String,
    pub isolation: Isolation,
}

/// Provisions the blind workspace under `run_dir/workspace`.
///
/// `framework_version` is the exact crates.io version the scaffold pins
/// (`=x.y.z`); `docs_dir` is the checkout's `docs/` directory the published
/// set is snapshotted from.
pub fn provision(
    run_dir: &Path,
    archetype: &Archetype,
    docs_dir: &Path,
    framework_version: &str,
) -> anyhow::Result<Workspace> {
    let root = run_dir.join("workspace");
    std::fs::create_dir_all(&root)?;

    std::fs::write(root.join("SPEC.md"), &archetype.spec)?;
    std::fs::write(root.join("INSTRUCTIONS.md"), instructions())?;
    std::fs::write(
        root.join(questionnaire::SHEET_FILENAME),
        questionnaire::definition().render_answer_sheet(),
    )?;

    let docs_dest = root.join("docs");
    std::fs::create_dir_all(&docs_dest)?;
    let repo_root = docs_dir
        .canonicalize()
        .with_context(|| format!("resolving docs directory {}", docs_dir.display()))?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for entry in PUBLISHED_DOCS {
        let src = docs_dir.join(entry);
        if !src.exists() {
            continue;
        }
        copy_recursive(&src, &docs_dest.join(entry), &repo_root)
            .with_context(|| format!("snapshotting docs entry {}", src.display()))?;
    }
    let docs_sha256 = docs_digest(&docs_dest)?;

    let app_dir = root.join("app");
    std::fs::create_dir_all(app_dir.join("src"))?;
    std::fs::write(
        app_dir.join("Cargo.toml"),
        scaffold_manifest(archetype.binary(), framework_version),
    )?;
    std::fs::write(
        app_dir.join("src").join("main.rs"),
        "fn main() {\n    // Replace this stub with the implementation of SPEC.md.\n}\n",
    )?;

    let isolation = Isolation::new(&root, &repo_root)?;
    Ok(Workspace {
        root,
        app_dir,
        docs_sha256,
        isolation,
    })
}

/// sha256 (hex) over the provisioned docs snapshot: every file in sorted
/// relative-path order, each hashed as `<relpath>\0<bytes>`. This pins the
/// bytes the agent actually saw — `docs_commit` alone says nothing when the
/// source working tree is dirty or drifts after provisioning.
pub fn docs_digest(docs_root: &Path) -> anyhow::Result<String> {
    let mut files = Vec::new();
    collect_relative_files(docs_root, docs_root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in &files {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(docs_root.join(rel))?);
    }
    Ok(digest::hex(hasher.finalize()))
}

/// Collects every regular file under `dir` as a `/`-separated path relative
/// to `root`.
fn collect_relative_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_relative_files(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("walk stays under root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push(rel);
        }
    }
    Ok(())
}

/// The git commit of the checkout `docs_dir` lives in, or `"unknown"`.
pub fn docs_commit(docs_dir: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(docs_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// The agent-facing brief provisioned as `INSTRUCTIONS.md`.
fn instructions() -> String {
    "# Instructions

You are implementing a small CLI application with the `standout` framework.

Workspace layout:

- `SPEC.md` — what to build. The spec is the contract; build exactly it.
- `docs/` — the published standout documentation. This is your standout
  reference.
- `app/` — the cargo project to build in. Its dependencies are already
  pinned; keep the package name and the standout version pins exactly as
  they are. You may add other crates.io dependencies. Never add a `path` or
  `git` dependency.
- `QUESTIONNAIRE.md` — the exit questionnaire.

Rules:

1. Work only inside this workspace.
2. Do not consult the standout framework source code or repository. Use
   `docs/` (and general Rust knowledge) instead. If you nevertheless rely on
   anything beyond `docs/` — web search, prior knowledge of standout
   internals, other source code — you must say exactly what in the
   questionnaire's sources questions. Honest answers are worth more than
   blind ones.
3. Implement `SPEC.md` in `app/` until `cargo build` succeeds and the binary
   behaves as specified. Verify your work by running the binary.
4. Before finishing, answer `QUESTIONNAIRE.md` in place: write each answer on
   the line(s) below its question, and leave the `#!` header lines and the
   `<id:...>` tags exactly as they are.
"
    .to_string()
}

/// The blind scaffold's `Cargo.toml`: crates.io exact pins, no path deps.
///
/// The empty `[workspace]` table is load-bearing isolation: without it,
/// cargo walks up from the run directory and adopts whatever workspace the
/// runs live under (when runs sit inside the framework checkout, that is
/// the framework's own workspace — a build failure and a blindness leak in
/// one; found by the first live smoke run).
fn scaffold_manifest(binary: &str, framework_version: &str) -> String {
    format!(
        r#"[workspace]

[package]
name = "{binary}"
version = "0.1.0"
edition = "2021"

[dependencies]
standout = "={framework_version}"
standout-dispatch = "={framework_version}"
clap = {{ version = "4", features = ["derive"] }}
serde = {{ version = "1", features = ["derive"] }}
anyhow = "1"
"#
    )
}

/// Copies a file or directory tree from `src` to `dest` with an explicit
/// symlink policy: a link is dereferenced (copied as regular content, never
/// as a link back into the checkout) only when its canonical target stays
/// inside one exact published root: the five mdbook source entries or a
/// crate's `docs/` tree (mounted under `docs/crates/<name>`). Any other link
/// is a provisioning error, not a silent follow.
fn copy_recursive(src: &Path, dest: &Path, repo_root: &Path) -> anyhow::Result<()> {
    let meta =
        std::fs::symlink_metadata(src).with_context(|| format!("inspecting {}", src.display()))?;
    if meta.file_type().is_symlink() {
        let target = src
            .canonicalize()
            .with_context(|| format!("resolving symlink {}", src.display()))?;
        if !is_published_docs_target(&target, repo_root) {
            bail!(
                "refusing to snapshot symlink {} -> {}: target is outside the \
                 published docs surface (blindness boundary)",
                src.display(),
                target.display()
            );
        }
        return copy_recursive(&target, dest, repo_root);
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()), repo_root)?;
        }
    } else {
        std::fs::copy(src, dest)
            .with_context(|| format!("copying {} to {}", src.display(), dest.display()))?;
    }
    Ok(())
}

/// True when a canonical symlink target lies inside one exact published root.
fn is_published_docs_target(target: &Path, repo_root: &Path) -> bool {
    let Ok(rel) = target.strip_prefix(repo_root) else {
        return false;
    };
    let mut components = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    match components.next().as_deref() {
        Some("docs") => matches!(
            components.next().as_deref(),
            Some("index.md" | "intro.md" | "guides" | "topics" | "crates")
        ),
        Some("crates") => {
            components.next(); // the crate directory name
            components.next().as_deref() == Some("docs")
        }
        _ => false,
    }
}
