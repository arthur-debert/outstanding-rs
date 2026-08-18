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
pub mod cases;
pub mod exec;
pub mod questionnaire;
pub mod report;
pub mod sandbox;
pub mod session;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::archetype::Archetype;
use crate::report::{
    AcceptanceReport, ArchetypeStamp, Blindness, EvaluationStamp, Pins, RunReport, SCHEMA_VERSION,
};

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

/// Inputs for objective re-evaluation of one preserved historical run.
pub struct ReevaluationConfig {
    pub archetype: String,
    pub archetypes_dir: PathBuf,
    pub docs_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub source_report: PathBuf,
    pub output_report: PathBuf,
    pub produced_binary: Option<PathBuf>,
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
    /// Each invariant-cell invocation (acceptance cases carry their own
    /// per-case `timeout_seconds`).
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
     agent, build, and produced-binary descendants use disposable homes and a \
     kernel filesystem sandbox that excludes the source checkout and host user-data roots; \
     consulted sources are self-reported in the exit questionnaire";

/// Runs the full loop and returns the written report plus the run directory.
///
/// A run that completes the loop always writes `report.json`, even when the
/// app never built or every case failed — those are findings. Errors are
/// reserved for the runner's own failures (unloadable archetype, unwritable
/// run directory, unspawnable agent).
pub fn run(config: &RunConfig) -> anyhow::Result<(RunReport, PathBuf)> {
    let archetype = Archetype::load(&config.archetypes_dir, &config.archetype)?;

    std::fs::create_dir_all(&config.runs_dir)
        .with_context(|| format!("creating runs directory {}", config.runs_dir.display()))?;
    let source_root = config
        .docs_dir
        .canonicalize()
        .with_context(|| format!("resolving docs directory {}", config.docs_dir.display()))?
        .parent()
        .map(Path::to_path_buf)
        .context("docs directory has no repository parent")?;
    let canonical_runs = config
        .runs_dir
        .canonicalize()
        .with_context(|| format!("resolving runs directory {}", config.runs_dir.display()))?;
    if canonical_runs.starts_with(&source_root) {
        anyhow::bail!(
            "runs directory {} is inside source checkout {}; choose an external --runs-dir",
            canonical_runs.display(),
            source_root.display()
        );
    }
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
    workspace
        .isolation
        .verify_boundary(&source_root)
        .map_err(anyhow::Error::msg)
        .context("verifying blind-workspace isolation")?;

    // Phase 2: the instrumented implementation session.
    eprintln!(
        "[corpus] running implementation session: {}",
        config.agent_cmd
    );
    let transcript_path = run_dir.join(session::TRANSCRIPT_FILENAME);
    let session_report = session::run_agent(
        &workspace.root,
        &workspace.isolation,
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
    let (acceptance_report, invariant_cells, binary_sha256) = match acceptance::build_app(
        &workspace.app_dir,
        archetype.binary(),
        config.timeouts.build,
        &workspace.isolation,
    ) {
        Ok(binary) => {
            let cases_dir = run_dir.join("cases");
            let suite_report = cases::run_cases(
                &binary,
                &archetype.suite.cases,
                &cases_dir,
                &workspace.isolation,
            );
            let cells = acceptance::run_invariants(
                &binary,
                archetype.invariants(),
                config.timeouts.check,
                &workspace.isolation,
                &cases_dir.join("_invariants"),
            );
            let binary_sha = digest_file(&binary).ok();
            (suite_report, cells, binary_sha)
        }
        Err(detail) => (
            AcceptanceReport::build_failed(detail.clone()),
            acceptance::not_run_invariants(archetype.invariants(), &detail),
            None,
        ),
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
            acceptance_sha256: archetype.acceptance_sha256().to_string(),
            questionnaire_fingerprint: questionnaire::definition().fingerprint().to_string(),
        },
        evaluation: EvaluationStamp {
            origin: "full-run".to_string(),
            isolation_backend: sandbox::backend_name().to_string(),
            binary_sha256,
        },
        blindness: Blindness {
            policy: BLINDNESS_POLICY.to_string(),
            env_allowlist: workspace::ENV_ALLOWLIST
                .iter()
                .map(ToString::to_string)
                .collect(),
            framework_source_excluded: true,
            isolation_backend: sandbox::backend_name().to_string(),
            credential_exceptions: Vec::new(),
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

/// Re-evaluates a preserved produced workspace against the current suite and
/// matrix without rerunning or rewriting the historical agent session.
pub fn reevaluate(config: &ReevaluationConfig) -> anyhow::Result<RunReport> {
    let archetype = Archetype::load(&config.archetypes_dir, &config.archetype)?;
    let source_root = config
        .docs_dir
        .canonicalize()?
        .parent()
        .map(Path::to_path_buf)
        .context("docs directory has no repository parent")?;
    let isolation = workspace::Isolation::new(&config.workspace_root, &source_root)?;
    isolation
        .verify_boundary(&source_root)
        .map_err(anyhow::Error::msg)?;

    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config.source_report)?)?;
    value["schema_version"] = serde_json::json!(SCHEMA_VERSION);
    value["pins"]["acceptance_sha256"] = serde_json::json!(archetype.acceptance_sha256());
    value["evaluation"] = serde_json::json!({
        "origin": "isolated-re-evaluation",
        "isolation_backend": sandbox::backend_name(),
        "binary_sha256": null,
    });
    value["blindness"]["framework_source_excluded"] = serde_json::json!(false);
    value["blindness"]["isolation_backend"] = serde_json::json!("historical-partial");
    value["blindness"]["credential_exceptions"] = serde_json::json!([
        "historical agent session inherited host HOME, CARGO_HOME, and RUSTUP_HOME"
    ]);
    value["blindness"]["policy"] = serde_json::json!(
        "historical session was partially blind: its workspace was nested beneath a source checkout and inherited host homes; acceptance and ROB01 matrix results were later regenerated from an external workspace under the recorded evaluation sandbox"
    );
    value["invariants"] = serde_json::json!([]);
    let mut report: RunReport = serde_json::from_value(value)?;
    report.archetype.spec_sha256 = archetype.spec_sha256();
    report.pins.acceptance_sha256 = archetype.acceptance_sha256().to_string();

    let app_dir = config.workspace_root.join("app");
    let cases_dir = config.workspace_root.join(".reevaluation-cases");
    let binary_result = match config.produced_binary.as_deref() {
        Some(path) if path.is_file() => Ok(path.to_path_buf()),
        Some(path) => Err(format!(
            "provided produced binary {} does not exist",
            path.display()
        )),
        None => acceptance::build_app(
            &app_dir,
            archetype.binary(),
            config.timeouts.build,
            &isolation,
        ),
    };
    match binary_result {
        Ok(binary) => {
            report.evaluation.binary_sha256 = digest_file(&binary).ok();
            report.acceptance =
                cases::run_cases(&binary, &archetype.suite.cases, &cases_dir, &isolation);
            report.invariants = acceptance::run_invariants(
                &binary,
                archetype.invariants(),
                config.timeouts.check,
                &isolation,
                &cases_dir.join("_invariants"),
            );
        }
        Err(detail) => {
            report.acceptance = AcceptanceReport::build_failed(detail.clone());
            report.invariants = acceptance::not_run_invariants(archetype.invariants(), &detail);
        }
    }
    report.write(&config.output_report)?;
    Ok(report)
}

fn digest_file(path: &Path) -> anyhow::Result<String> {
    let digest = Sha256::digest(std::fs::read(path)?);
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// Prints a human summary of the report to stderr.
pub fn print_summary(report: &RunReport) {
    use crate::report::CaseOutcome;
    let passed = report
        .acceptance
        .cases
        .iter()
        .filter(|c| c.outcome.is_expected())
        .count();
    let total = report.acceptance.cases.len();
    let inv_passed = report
        .invariants
        .iter()
        .filter(|c| c.status.passed())
        .count();
    eprintln!(
        "[corpus] {}: built={} acceptance={}/{} invariants={}/{} questionnaire={}",
        report.run_id,
        report.acceptance.built,
        passed,
        total,
        inv_passed,
        report.invariants.len(),
        if report.questionnaire.collected {
            "collected"
        } else {
            "NOT collected"
        },
    );
    for case in &report.acceptance.cases {
        match case.outcome {
            CaseOutcome::Fail => eprintln!("[corpus]   FAIL case: {}", case.name),
            CaseOutcome::ExpectedFail => {
                eprintln!(
                    "[corpus]   expected-fail case: {} (gap {})",
                    case.name,
                    case.gap.as_deref().unwrap_or("?")
                );
            }
            CaseOutcome::UnexpectedPass => {
                eprintln!(
                    "[corpus]   UNEXPECTED PASS case: {} (gap {} may be closed)",
                    case.name,
                    case.gap.as_deref().unwrap_or("?")
                );
            }
            CaseOutcome::Pass => {}
        }
    }
    for cell in report
        .invariants
        .iter()
        .filter(|c| c.status == crate::report::InvariantStatus::Fail)
    {
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
