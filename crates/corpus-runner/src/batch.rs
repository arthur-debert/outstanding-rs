//! Runs a set of archetypes through [`crate::run`] in order, sanitizes each
//! run's evidence outside the checkout with `sanitize-run.py`, and writes
//! both scorecards from it with `scorecard.py` (see `corpus/README.md`,
//! Decision D29). Two host-broker credentials cannot share a session, so
//! archetypes run serially rather than in parallel.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

use crate::broker::BrokerConfig;
use crate::{run, RunConfig, Timeouts};

pub struct BatchConfig {
    pub archetypes: Vec<String>,
    pub archetypes_dir: PathBuf,
    pub docs_dir: PathBuf,
    pub runs_dir: PathBuf,
    pub out_dir: PathBuf,
    pub agent_cmd: String,
    pub broker: Option<BrokerConfig>,
    pub framework_version: String,
    pub timeouts: Timeouts,
    pub sanitize_script: PathBuf,
    pub scorecard_script: PathBuf,
    pub account: Option<String>,
}

/// One archetype's result within the batch: the run id it was sanitized
/// under, or a detail naming what could not complete — provisioning,
/// the agent session, the build/check loop, or sanitizing the evidence.
/// The batch records this and moves on to the next archetype rather than
/// stopping the set.
pub type ArchetypeOutcome = Result<String, String>;

/// Runs every archetype in order, then always writes both scorecards from
/// whatever evidence `--out` holds — including a partial set, when an
/// earlier archetype failed. The caller decides the process exit status
/// from the returned outcomes (non-zero when any is `Err`, per D29).
pub fn batch(config: &BatchConfig) -> anyhow::Result<Vec<(String, ArchetypeOutcome)>> {
    std::fs::create_dir_all(&config.out_dir).with_context(|| {
        format!(
            "creating batch output directory {}",
            config.out_dir.display()
        )
    })?;
    crate::require_outside_checkout(
        &config.out_dir,
        &config.docs_dir,
        "batch output directory",
        "--out",
    )?;
    std::fs::create_dir_all(&config.runs_dir)
        .with_context(|| format!("creating runs directory {}", config.runs_dir.display()))?;
    require_distinct_directories(&config.out_dir, &config.runs_dir)?;

    let outcomes: Vec<(String, ArchetypeOutcome)> = config
        .archetypes
        .iter()
        .map(|archetype| (archetype.clone(), run_one(config, archetype)))
        .collect();

    write_scorecards(&config.scorecard_script, &config.out_dir)
        .context("writing batch scorecards")?;

    Ok(outcomes)
}

/// Rejects `--out` and `--runs-dir` resolving to the same directory, or one
/// nesting inside the other. `run_one` sanitizes an archetype's evidence
/// into `--out` and then removes its scratch subdirectory of `--runs-dir`;
/// were the two configured to overlap, that removal would delete the
/// sanitized copy it just wrote instead of only the scratch original.
fn require_distinct_directories(out_dir: &Path, runs_dir: &Path) -> anyhow::Result<()> {
    let canonical_out = out_dir
        .canonicalize()
        .with_context(|| format!("resolving batch output directory {}", out_dir.display()))?;
    let canonical_runs = runs_dir
        .canonicalize()
        .with_context(|| format!("resolving runs directory {}", runs_dir.display()))?;
    if canonical_out == canonical_runs
        || canonical_out.starts_with(&canonical_runs)
        || canonical_runs.starts_with(&canonical_out)
    {
        bail!(
            "--out {} and --runs-dir {} must not be the same directory or nested in each other",
            canonical_out.display(),
            canonical_runs.display()
        );
    }
    Ok(())
}

fn run_one(config: &BatchConfig, archetype: &str) -> ArchetypeOutcome {
    let run_config = RunConfig {
        archetype: archetype.to_string(),
        archetypes_dir: config.archetypes_dir.clone(),
        runs_dir: config.runs_dir.clone(),
        docs_dir: config.docs_dir.clone(),
        agent_cmd: config.agent_cmd.clone(),
        broker: config.broker.clone(),
        framework_version: config.framework_version.clone(),
        timeouts: config.timeouts,
    };
    let (report, run_dir) = run(&run_config).map_err(|err| format!("{err:#}"))?;
    crate::print_summary(&report);

    let dest = config.out_dir.join(&report.run_id);
    sanitize_and_cleanup(
        &config.sanitize_script,
        &run_dir,
        &dest,
        config.account.as_deref(),
    )?;
    Ok(report.run_id)
}

/// Sanitizes `run_dir`'s evidence into `dest`, then removes `run_dir` — the
/// scratch copy under `--runs-dir` — since it now only duplicates what
/// sanitizing wrote. A sanitize failure returns before the removal and
/// leaves `run_dir` in place for inspection.
fn sanitize_and_cleanup(
    script: &Path,
    run_dir: &Path,
    dest: &Path,
    account: Option<&str>,
) -> Result<(), String> {
    sanitize(script, run_dir, dest, account)
        .map_err(|err| format!("sanitizing {}: {err:#}", run_dir.display()))?;
    std::fs::remove_dir_all(run_dir).map_err(|err| {
        format!(
            "removing scratch run directory {}: {err}",
            run_dir.display()
        )
    })
}

fn sanitize(
    script: &Path,
    run_dir: &Path,
    dest: &Path,
    account: Option<&str>,
) -> anyhow::Result<()> {
    let mut args = vec![run_dir.as_os_str().to_owned(), dest.as_os_str().to_owned()];
    if let Some(account) = account {
        args.push("--account".into());
        args.push(account.into());
    }
    python3(script, &args).map(drop)
}

// The objective table (`scorecard.py`'s default markdown output) and the
// comparability/fingerprint form (`--json`, what a later re-run compares
// against) are both written under `--out`, from the same sanitized runs.
fn write_scorecards(script: &Path, out_dir: &Path) -> anyhow::Result<()> {
    let label = format!("batch={}", out_dir.display());
    let markdown = python3(script, &[label.clone().into()])?;
    let json = python3(script, &[label.into(), "--json".into()])?;
    std::fs::write(out_dir.join("scorecard.md"), markdown)
        .with_context(|| format!("writing {}", out_dir.join("scorecard.md").display()))?;
    std::fs::write(out_dir.join("scorecard.json"), json)
        .with_context(|| format!("writing {}", out_dir.join("scorecard.json").display()))?;
    Ok(())
}

/// Runs `python3 <script> <args>...` to completion and returns its stdout as
/// text; a non-zero exit carries stderr in the error.
fn python3(script: &Path, args: &[std::ffi::OsString]) -> anyhow::Result<String> {
    let output = Command::new("python3")
        .arg(script)
        .args(args)
        .output()
        .with_context(|| format!("running {}", script.display()))?;
    if !output.status.success() {
        bail!(
            "{} exited with {}: {}",
            script.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout)
        .with_context(|| format!("{} wrote non-UTF-8 output", script.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn require_distinct_directories_rejects_the_same_directory() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join("shared");
        std::fs::create_dir_all(&dir).unwrap();

        let err = require_distinct_directories(&dir, &dir).unwrap_err();

        assert!(
            format!("{err:#}").contains("must not be the same directory"),
            "{err:#}"
        );
    }

    #[test]
    fn require_distinct_directories_rejects_runs_dir_nested_under_out() {
        let scratch = tempfile::tempdir().unwrap();
        let out_dir = scratch.path().join("out");
        let runs_dir = out_dir.join("runs");
        std::fs::create_dir_all(&runs_dir).unwrap();

        let err = require_distinct_directories(&out_dir, &runs_dir).unwrap_err();

        assert!(
            format!("{err:#}").contains("must not be the same directory"),
            "{err:#}"
        );
    }

    #[test]
    fn require_distinct_directories_rejects_out_nested_under_runs_dir() {
        let scratch = tempfile::tempdir().unwrap();
        let runs_dir = scratch.path().join("runs");
        let out_dir = runs_dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        let err = require_distinct_directories(&out_dir, &runs_dir).unwrap_err();

        assert!(
            format!("{err:#}").contains("must not be the same directory"),
            "{err:#}"
        );
    }

    #[test]
    fn require_distinct_directories_accepts_sibling_directories() {
        let scratch = tempfile::tempdir().unwrap();
        let out_dir = scratch.path().join("out");
        let runs_dir = scratch.path().join("runs");
        std::fs::create_dir_all(&out_dir).unwrap();
        std::fs::create_dir_all(&runs_dir).unwrap();

        require_distinct_directories(&out_dir, &runs_dir).unwrap();
    }

    #[test]
    fn sanitize_and_cleanup_removes_scratch_dir_on_success() {
        let scratch = tempfile::tempdir().unwrap();
        let run_dir = scratch.path().join("run");
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::write(run_dir.join("report.json"), "{}").unwrap();
        let dest = scratch.path().join("out").join("run-id");
        let script = python_script(
            scratch.path(),
            "sanitize-ok.py",
            "import os, shutil, sys\n\
             src, dst = sys.argv[1], sys.argv[2]\n\
             os.makedirs(dst, exist_ok=True)\n\
             shutil.copy(os.path.join(src, 'report.json'), os.path.join(dst, 'report.json'))\n",
        );

        sanitize_and_cleanup(&script, &run_dir, &dest, None).unwrap();

        assert!(
            !run_dir.exists(),
            "scratch run directory should be removed once sanitized"
        );
        assert!(dest.join("report.json").is_file());
    }

    #[test]
    fn sanitize_and_cleanup_keeps_scratch_dir_on_sanitize_failure() {
        let scratch = tempfile::tempdir().unwrap();
        let run_dir = scratch.path().join("run");
        std::fs::create_dir_all(&run_dir).unwrap();
        let dest = scratch.path().join("out").join("run-id");
        let script = python_script(
            scratch.path(),
            "sanitize-fail.py",
            "import sys\nsys.exit(1)\n",
        );

        let err = sanitize_and_cleanup(&script, &run_dir, &dest, None).unwrap_err();

        assert!(
            run_dir.exists(),
            "scratch run directory should survive a sanitize failure"
        );
        assert!(err.contains(&run_dir.display().to_string()), "{err}");
    }
}
