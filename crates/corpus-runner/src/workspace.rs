// Blind-workspace provisioning: everything the agent sees, and nothing
// more — spec, instructions, questionnaire, a published-docs snapshot, and
// a cargo scaffold pinned to exact-version crates.io dependencies.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use sha2::{Digest, Sha256};

use crate::archetype::Archetype;
use crate::digest;
use crate::questionnaire;
use crate::report::{DocsSource, IsolationRecord};
use crate::sandbox::{self, Policy};

const PUBLISHED_DOCS: &[&str] = &["index.md", "intro.md", "guides", "topics", "crates"];

const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");

// CLAUDE_CODE_TMPDIR is added for the agent phase alone, by `Isolation::apply_agent`.
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
    "CLAUDE_CODE_TMPDIR",
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
            // `Stdio::null()` opens `/dev/null` for writing; denied, it fails with EPERM.
            vec![
                writable.to_path_buf(),
                home.to_path_buf(),
                PathBuf::from("/dev/null"),
            ],
            self.denied_read.clone(),
            network,
        )
    }

    pub fn apply_agent(&self, command: &mut Command) -> Result<(), String> {
        apply_phase_env(command, &self.agent_home);
        // Claude Code keeps its shell scratch under /tmp, not TMPDIR; denied it, the
        // session loses its shell. Redirected into the disposable home, not admitted.
        command.env("CLAUDE_CODE_TMPDIR", self.agent_home.join("tmp"));
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

    pub fn agent_capability(&self) -> IsolationRecord {
        sandbox::capability(true)
    }

    pub fn evaluation_capability(&self) -> IsolationRecord {
        sandbox::capability(false)
    }

    pub fn verify_boundary(&self, source_root: &Path) -> Result<(), String> {
        let host_probe = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| source_root.to_path_buf())
            .join(".gitconfig");
        let probe_log = self.workspace_root.join(".boundary-probe.log");
        let _ = std::fs::remove_file(&probe_log);
        let mut command = Command::new("sh");
        // Real opens, not `test -r`: Landlock leaves access(2) unrestricted. `true`,
        // not `:`, so dash does not abort the script on the redirection error.
        command
            .args([
                "-c",
                "log=\"$3\"; \
                 echo checkpoint:start >> \"$log\" 2>&1; \
                 if { true < \"$1\"; } >> \"$log\" 2>&1; then \
                     echo source-readable >&2; \
                     echo checkpoint:source-readable >> \"$log\" 2>&1; exit 1; \
                 fi; \
                 echo checkpoint:source-denied >> \"$log\" 2>&1; \
                 if { true < \"$2\"; } >> \"$log\" 2>&1; then \
                     echo host-home-readable >&2; \
                     echo checkpoint:host-home-readable >> \"$log\" 2>&1; exit 1; \
                 fi; \
                 echo checkpoint:host-home-denied >> \"$log\" 2>&1",
                "corpus-boundary",
            ])
            .arg(source_root.join("Cargo.toml"))
            .arg(host_probe)
            .arg(&probe_log)
            .current_dir(&self.workspace_root);
        self.apply_agent(&mut command)?;
        let output = command
            .output()
            .map_err(|e| format!("probing isolation boundary: {e}"))?;
        if !output.status.success() {
            let log = std::fs::read_to_string(&probe_log)
                .unwrap_or_else(|e| format!("<no probe log: {e}>"));
            return Err(format!(
                "OS sandbox allowed source-checkout/host-home access or failed to enforce \
                 (status {:?}, stdout {:?}, stderr {:?}, probe log {:?})",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim(),
                log.trim()
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub app_dir: PathBuf,
    pub docs_sha256: String,
    pub docs_commit: String,
    pub docs_source: DocsSource,
    pub isolation: Isolation,
}

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

    let mut _tag_archive_guard = None;
    let (docs_source_dir, docs_filter_root, docs_source, resolved_docs_commit) =
        if framework_version == RUNNER_VERSION {
            (
                docs_dir.to_path_buf(),
                repo_root.clone(),
                DocsSource::Checkout,
                docs_commit(docs_dir),
            )
        } else {
            let tag = format!("v{framework_version}");
            let commit = resolve_tag_commit(&repo_root, &tag)?;
            let archive_dir = create_docs_archive_scratch_dir(&tag)?;
            extract_tag_docs(&repo_root, &tag, archive_dir.path())?;
            // Canonicalized so `is_published_docs_target`'s `strip_prefix`
            // agrees with the canonicalized symlink targets `copy_recursive`
            // resolves below (macOS's `/var` -> `/private/var`, otherwise).
            let archive_root = archive_dir.path().canonicalize().with_context(|| {
                format!(
                    "resolving docs archive scratch directory {}",
                    archive_dir.path().display()
                )
            })?;
            _tag_archive_guard = Some(archive_dir);
            (
                archive_root.join("docs"),
                archive_root,
                DocsSource::Tag,
                commit,
            )
        };
    for entry in PUBLISHED_DOCS {
        let src = docs_source_dir.join(entry);
        if !src.exists() {
            continue;
        }
        copy_recursive(&src, &docs_dest.join(entry), &docs_filter_root)
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
        docs_commit: resolved_docs_commit,
        docs_source,
        isolation,
    })
}

fn resolve_tag_commit(repo_root: &Path, tag: &str) -> anyhow::Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--verify", &format!("{tag}^{{commit}}")])
        .output()
        .with_context(|| format!("resolving tag {tag} in {}", repo_root.display()))?;
    if !output.status.success() {
        bail!(
            "framework version pin {tag} has no matching tag in the checkout's repository \
             {} (docs cannot be sourced for it)",
            repo_root.display()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A scratch directory for the tag's docs archive, in the system temp
/// directory. There is no isolation concern with that placement here:
/// `extract_tag_docs` archives only `docs/` and the specific
/// `crates/<name>/docs` directories its symlinks point to, never the tag's
/// whole tree, so nothing sensitive ever lands in it; the directory is
/// created, extracted into, read from, and dropped by the RAII guard,
/// entirely before the agent process starts.
fn create_docs_archive_scratch_dir(tag: &str) -> anyhow::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix(".corpus-docs-tag-archive-")
        .tempdir()
        .with_context(|| format!("creating docs archive scratch directory for tag {tag}"))
}

/// Extracts `docs/` and the specific `crates/<name>/docs` directories its
/// `docs/crates/*` symlinks point to at this tag — nothing else — into
/// `extract_root`, so those symlinks resolve exactly as they do in the live
/// checkout without archiving the tag's whole tree.
fn extract_tag_docs(repo_root: &Path, tag: &str, extract_root: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(extract_root).with_context(|| {
        format!(
            "creating docs archive scratch directory {}",
            extract_root.display()
        )
    })?;
    let crate_doc_paths = resolve_crate_doc_symlink_targets(repo_root, tag)?;
    let mut archive = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["archive", "--format=tar", tag, "docs"])
        .args(&crate_doc_paths)
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning git archive for tag {tag}"))?;
    let stdout = archive.stdout.take().expect("piped stdout");
    let tar_status = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(extract_root)
        .stdin(stdout)
        .status()
        .with_context(|| format!("extracting archive for tag {tag}"))?;
    let archive_status = archive
        .wait()
        .with_context(|| format!("waiting for git archive of tag {tag}"))?;
    if !archive_status.success() {
        bail!("git archive {tag} exited with {archive_status}");
    }
    if !tar_status.success() {
        bail!("extracting tag {tag} archive exited with {tar_status}");
    }
    Ok(())
}

/// Reads `docs/crates/*`'s symlink entries at `tag` (without extracting
/// anything — a `git ls-tree`/`git cat-file` listing) and resolves each
/// target to the `crates/<name>/docs` path it points at, so
/// `extract_tag_docs` can archive exactly those directories alongside
/// `docs/` instead of the tag's whole tree. `git archive`'s own pathspec
/// matching does not glob (`crates/*/docs` matches nothing), so each path
/// is resolved and passed explicitly. Empty if `docs/crates/` does not
/// exist at this tag.
fn resolve_crate_doc_symlink_targets(repo_root: &Path, tag: &str) -> anyhow::Result<Vec<String>> {
    let listing = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["ls-tree", tag, "docs/crates/"])
        .output()
        .with_context(|| format!("listing docs/crates/ at tag {tag}"))?;
    if !listing.status.success() {
        return Ok(Vec::new());
    }
    let mut targets = Vec::new();
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        // `<mode> <type> <sha>\t<path>`
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        if !meta.starts_with("120000") {
            continue; // not a symlink entry
        }
        let Some(sha) = meta.split_whitespace().nth(2) else {
            continue;
        };
        let target = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["cat-file", "-p", sha])
            .output()
            .with_context(|| format!("reading symlink target for {path} at tag {tag}"))?;
        if !target.status.success() {
            bail!("reading symlink target for {path} at tag {tag} failed");
        }
        let target_text = String::from_utf8_lossy(&target.stdout).trim().to_string();
        let parent = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
        let resolved = normalize_relative_path(&parent.join(&target_text));
        targets.push(resolved.to_string_lossy().into_owned());
    }
    Ok(targets)
}

/// Resolves `..`/`.` components lexically (no filesystem access), so a
/// relative symlink target like `../../crates/widget/docs` resolved
/// against its own directory yields a clean `crates/widget/docs`.
fn normalize_relative_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

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

// The empty `[workspace]` table stops cargo adopting the framework checkout's workspace.
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
            components.next();
            components.next().as_deref() == Some("docs")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_archive_scratch_dir_is_created() {
        let scratch = create_docs_archive_scratch_dir("v1.2.3").unwrap();
        assert!(scratch.path().is_dir());
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap_or_else(|e| panic!("running git {args:?}: {e}"));
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn crate_doc_symlink_targets_are_resolved_from_the_tag() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        std::fs::create_dir_all(root.join("docs/crates")).unwrap();
        std::fs::create_dir_all(root.join("crates/widget/docs")).unwrap();
        std::fs::write(root.join("crates/widget/docs/index.md"), "widget docs\n").unwrap();
        std::os::unix::fs::symlink("../../crates/widget/docs", root.join("docs/crates/widget"))
            .unwrap();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "corpus@example.test"]);
        git(root, &["config", "user.name", "corpus"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "tagged docs"]);
        git(root, &["tag", "v1.2.3"]);

        let targets = resolve_crate_doc_symlink_targets(root, "v1.2.3").unwrap();
        assert_eq!(targets, vec!["crates/widget/docs".to_string()]);
    }
}
