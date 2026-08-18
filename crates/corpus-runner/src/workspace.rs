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

use anyhow::Context;

use crate::archetype::Archetype;
use crate::questionnaire;

/// The published documentation set: what the mdbook ships, relative to the
/// repo's `docs/` directory. ADRs, internal specs, proposals, and dev notes
/// are deliberately absent.
const PUBLISHED_DOCS: &[&str] = &["index.md", "intro.md", "guides", "topics", "crates"];

/// The only environment variables the agent process inherits (recorded in
/// the report). HOME is the known blindness residue: it grants the agent its
/// own credentials and caches, which is what makes the session runnable.
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

/// A provisioned blind workspace.
pub struct Workspace {
    /// The directory the agent works in.
    pub root: PathBuf,
    /// The cargo project directory inside it.
    pub app_dir: PathBuf,
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
    for entry in PUBLISHED_DOCS {
        let src = docs_dir.join(entry);
        if !src.exists() {
            continue;
        }
        copy_recursive(&src, &docs_dest.join(entry))
            .with_context(|| format!("snapshotting docs entry {}", src.display()))?;
    }

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

    Ok(Workspace { root, app_dir })
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

/// Copies a file or directory tree from `src` to `dest`.
fn copy_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(src, dest)
            .with_context(|| format!("copying {} to {}", src.display(), dest.display()))?;
    }
    Ok(())
}
