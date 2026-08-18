//! The downstream-corpus runner: one command runs the full blind-agent loop
//! for one archetype and files a structured run report.
//!
//! The five phases (spec `docs/spec/robustness-corpus.md`, Proposed Shape
//! piece 2): provision a blind workspace, execute the instrumented
//! implementation session, collect the exit questionnaire, run the
//! acceptance suite plus the ROB01 invariant matrix against the produced
//! binary, and write `report.json`. The runner is a means, not a product:
//! it is the minimum that makes runs reproducible and comparable. Decisions
//! (blindness protocol, report schema) are recorded in `corpus/README.md`.

pub mod acceptance;
pub mod archetype;
pub mod exec;
pub mod questionnaire;
pub mod report;
pub mod session;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;

use crate::archetype::Archetype;
use crate::report::{AcceptanceReport, ArchetypeStamp, Blindness, Pins, RunReport, SCHEMA_VERSION};

/// Everything one run needs to start. The agent command is a seam: any
/// shell command that implements the workspace's `INSTRUCTIONS.md`.
pub struct RunConfig {
    pub archetype: String,
    /// `corpus/archetypes/` — where archetypes live.
    pub archetypes_dir: PathBuf,
    /// Where run directories are created.
    pub runs_dir: PathBuf,
    /// The checkout's `docs/` directory (published-set snapshot source).
    pub docs_dir: PathBuf,
    pub agent_cmd: String,
    /// Exact crates.io version the blind scaffold pins.
    pub framework_version: String,
    /// Per-phase deadlines; an expired phase is a reported finding.
    pub timeouts: Timeouts,
}

/// Per-phase deadlines. A phase that overruns is killed (whole process
/// group) and recorded in the report — the durable-report contract must
/// survive a prompting, deadlocked, or looping produced CLI.
pub struct Timeouts {
    /// The whole agent session.
    pub agent: Duration,
    /// The cargo build of the produced app.
    pub build: Duration,
    /// Each acceptance check / invariant-cell invocation.
    pub check: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            agent: Duration::from_secs(3600),
            build: Duration::from_secs(1800),
            check: Duration::from_secs(120),
        }
    }
}

/// The one-line blindness policy statement recorded in every report.
const BLINDNESS_POLICY: &str = "workspace contains spec + published docs + crates.io pins only; \
     agent, build, and produced-binary processes run env-cleared to a recorded \
     allowlist; consulted sources are self-reported in the exit questionnaire";

/// Runs the full loop and returns the written report plus the run directory.
///
/// A run that completes the loop always writes `report.json`, even when the
/// app never built or every check failed — those are findings. Errors are
/// reserved for the runner's own failures (unloadable archetype, unwritable
/// run directory, unspawnable agent).
pub fn run(config: &RunConfig) -> anyhow::Result<(RunReport, PathBuf)> {
    let archetype = Archetype::load(&config.archetypes_dir, &config.archetype)?;

    std::fs::create_dir_all(&config.runs_dir)
        .with_context(|| format!("creating runs directory {}", config.runs_dir.display()))?;
    let base = format!("{}-{}", archetype.name, unix_timestamp());
    let (run_id, run_dir) = claim_run_dir(&config.runs_dir, &base)?;

    // Phase 1: provision the blind workspace.
    eprintln!(
        "[corpus] provisioning blind workspace in {}",
        run_dir.display()
    );
    let workspace = workspace::provision(
        &run_dir,
        &archetype,
        &config.docs_dir,
        &config.framework_version,
    )?;

    // Phase 2: the instrumented implementation session.
    eprintln!(
        "[corpus] running implementation session: {}",
        config.agent_cmd
    );
    let transcript_path = run_dir.join(session::TRANSCRIPT_FILENAME);
    let session_report = session::run_agent(
        &workspace.root,
        &config.agent_cmd,
        &transcript_path,
        config.timeouts.agent,
    )?;
    eprintln!(
        "[corpus] session finished in {:.0}s (exit {:?})",
        session_report.wall_seconds, session_report.exit_code
    );

    // Phase 3: collect the exit questionnaire.
    let questionnaire_report = questionnaire::collect(&workspace.root);
    eprintln!(
        "[corpus] questionnaire collected: {}",
        questionnaire_report.collected
    );

    // Phase 4: acceptance suite + invariant matrix against the produced binary.
    eprintln!("[corpus] building produced app and running acceptance suite");
    let (acceptance_report, invariant_cells) = match acceptance::build_app(
        &workspace.app_dir,
        &archetype.acceptance.binary,
        config.timeouts.build,
    ) {
        Ok(binary) => (
            acceptance::run_checks(&binary, &archetype.acceptance.checks, config.timeouts.check),
            acceptance::run_invariants(
                &binary,
                &archetype.acceptance.invariants,
                config.timeouts.check,
            ),
        ),
        Err(detail) => (AcceptanceReport::build_failed(detail), Vec::new()),
    };

    // Phase 5: file the report.
    let report = RunReport {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        archetype: ArchetypeStamp {
            name: archetype.name.clone(),
            spec_sha256: archetype.spec_sha256(),
        },
        pins: Pins {
            framework_version: config.framework_version.clone(),
            docs_commit: workspace::docs_commit(&config.docs_dir),
            docs_sha256: workspace.docs_sha256.clone(),
            questionnaire_fingerprint: questionnaire::definition().fingerprint().to_string(),
        },
        blindness: Blindness {
            policy: BLINDNESS_POLICY.to_string(),
            env_allowlist: workspace::ENV_ALLOWLIST
                .iter()
                .map(ToString::to_string)
                .collect(),
            framework_source_excluded: true,
            agent_reported_docs: questionnaire_report.answers.get("sources.docs").cloned(),
            agent_reported_external_sources: questionnaire_report
                .answers
                .get("sources.external")
                .cloned(),
        },
        session: session_report,
        acceptance: acceptance_report,
        invariants: invariant_cells,
        questionnaire: questionnaire_report,
    };
    let report_path = run_dir.join("report.json");
    report.write(&report_path)?;
    eprintln!("[corpus] report written to {}", report_path.display());

    Ok((report, run_dir))
}

/// Prints a human summary of the report to stderr.
pub fn print_summary(report: &RunReport) {
    let passed = report.acceptance.checks.iter().filter(|c| c.passed).count();
    let inv_passed = report.invariants.iter().filter(|c| c.passed).count();
    eprintln!(
        "[corpus] {}: built={} acceptance={}/{} invariants={}/{} questionnaire={}",
        report.run_id,
        report.acceptance.built,
        passed,
        report.acceptance.checks.len(),
        inv_passed,
        report.invariants.len(),
        if report.questionnaire.collected {
            "collected"
        } else {
            "NOT collected"
        },
    );
    for check in report.acceptance.checks.iter().filter(|c| !c.passed) {
        eprintln!("[corpus]   FAIL acceptance: {}", check.name);
    }
    for cell in report.invariants.iter().filter(|c| !c.passed) {
        eprintln!(
            "[corpus]   FAIL invariant: {} — {}",
            cell.command, cell.check
        );
    }
}

/// Seconds since the epoch, as the run-id suffix.
fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Claims a fresh run directory atomically: `create_dir` (never
/// `create_dir_all`, which would silently adopt an existing directory) with
/// a numeric suffix retry, so two runs claiming the same base id — parallel
/// runs of one archetype, or two scripted runs inside one second — can
/// never share a workspace, transcript, or report.
fn claim_run_dir(runs_dir: &Path, base: &str) -> anyhow::Result<(String, PathBuf)> {
    for attempt in 0..1000u32 {
        let run_id = if attempt == 0 {
            base.to_string()
        } else {
            format!("{base}-{attempt}")
        };
        let run_dir = runs_dir.join(&run_id);
        match std::fs::create_dir(&run_dir) {
            Ok(()) => return Ok((run_id, run_dir)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("creating run directory {}", run_dir.display()))
            }
        }
    }
    anyhow::bail!("could not claim a run directory for {base} after 1000 attempts");
}

/// Resolves a possibly-relative CLI path against the current directory.
pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claimed_run_dirs_never_collide() {
        let runs = tempfile::tempdir().unwrap();
        let (first, first_dir) = claim_run_dir(runs.path(), "smoke-42").unwrap();
        let (second, second_dir) = claim_run_dir(runs.path(), "smoke-42").unwrap();
        assert_eq!(first, "smoke-42");
        assert_eq!(second, "smoke-42-1");
        assert_ne!(first_dir, second_dir);
        assert!(first_dir.is_dir() && second_dir.is_dir());
    }
}
