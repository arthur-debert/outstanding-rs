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
use crate::questionnaire;

/// The published documentation set: what the mdbook ships, relative to the
/// repo's `docs/` directory. ADRs, internal specs, proposals, and dev notes
/// are deliberately absent.
const PUBLISHED_DOCS: &[&str] = &["index.md", "intro.md", "guides", "topics", "crates"];

/// The only environment variables any untrusted-side process inherits — the
/// agent session, the cargo build of the produced app, and every produced-
/// binary invocation (recorded in the report). HOME is the known blindness
/// residue: it grants the agent its own credentials and caches, which is
/// what makes the session runnable; it also keeps cargo caches shared.
pub const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "CARGO_HOME",
    "RUSTUP_HOME",
];

/// Applies the blindness environment policy to a command: `env_clear()`
/// plus exactly [`ENV_ALLOWLIST`], inherited from the runner's own
/// environment. Every process on the untrusted side of the fence (agent,
/// build, produced binary) must pass through this.
pub fn apply_env_policy(command: &mut Command) {
    command.env_clear();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
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
        scaffold_manifest(&archetype.acceptance.binary, framework_version),
    )?;
    std::fs::write(
        app_dir.join("src").join("main.rs"),
        "fn main() {\n    // Replace this stub with the implementation of SPEC.md.\n}\n",
    )?;

    Ok(Workspace {
        root,
        app_dir,
        docs_sha256,
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
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
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
/// inside the repo's published docs surface — `docs/` or a crate's `docs/`
/// (the mdbook mounts `docs/crates/<name>` as a symlink to
/// `crates/standout-<name>/docs`). Any other link is a provisioning error,
/// not a silent follow: it would pull content from outside the published
/// set into the blind workspace.
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

/// True when a canonical symlink target lies inside the published docs
/// surface of the checkout rooted at `repo_root`: under `docs/`, or under a
/// crate's own `docs/` directory (`crates/<name>/docs`).
fn is_published_docs_target(target: &Path, repo_root: &Path) -> bool {
    let Ok(rel) = target.strip_prefix(repo_root) else {
        return false;
    };
    let mut components = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    match components.next().as_deref() {
        Some("docs") => true,
        Some("crates") => {
            components.next(); // the crate directory name
            components.next().as_deref() == Some("docs")
        }
        _ => false,
    }
}
