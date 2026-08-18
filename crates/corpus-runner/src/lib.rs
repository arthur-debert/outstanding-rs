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
mod digest;
pub mod exec;
pub mod questionnaire;
pub mod report;
pub mod sandbox;
pub mod session;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;

use crate::archetype::{Archetype, Suite};
use crate::report::{
    AcceptanceReport, ArchetypeStamp, Blindness, EvaluationStamp, HistoricalRun, InvariantCell,
    IsolationRecord, NetworkEnforcement, Pins, RunReport, HISTORICAL_SCHEMA_MIN, SCHEMA_VERSION,
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
    /// Explicit already-built executable to evaluate instead of rebuilding
    /// the preserved app. It must be a regular file beneath
    /// `workspace_root`: the check sandbox admits only the workspace and
    /// system roots, so an outside path would be unexecutable under
    /// Landlock's default-deny model — the contract is validated up front
    /// and refused identically on every backend.
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
     agent, build, and produced-binary descendants use disposable homes and a \
     kernel filesystem sandbox that excludes the source checkout and host user-data roots; \
     consulted sources are self-reported in the exit questionnaire";

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
    let evaluation = evaluate_binary(
        &archetype,
        acceptance::build_app(
            &workspace.app_dir,
            archetype.binary(),
            config.timeouts.build,
            &workspace.isolation,
        ),
        &run_dir.join("cases"),
        config.timeouts.check,
        &workspace.isolation,
    );

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
            isolation: workspace.isolation.evaluation_capability(),
            binary_sha256: evaluation.binary_sha256,
        },
        blindness: Blindness {
            policy: BLINDNESS_POLICY.to_string(),
            env_allowlist: workspace::ENV_ALLOWLIST
                .iter()
                .map(ToString::to_string)
                .collect(),
            framework_source_excluded: true,
            isolation: workspace.isolation.agent_capability(),
            credential_exceptions: Vec::new(),
            agent_reported_docs: questionnaire_report.answers.get("sources.docs").cloned(),
            agent_reported_external_sources: questionnaire_report
                .answers
                .get("sources.external")
                .cloned(),
        },
        session: session_report,
        acceptance: evaluation.acceptance,
        invariants: evaluation.invariants,
        questionnaire: questionnaire_report,
    };
    let report_path = run_dir.join("report.json");
    report.write(&report_path)?;
    eprintln!("[corpus] report written to {}", report_path.display());

    Ok((report, run_dir))
}

/// The blindness statement re-evaluation records: the historical session's
/// partial blindness plus the regenerated objective results' provenance.
const HISTORICAL_BLINDNESS_POLICY: &str =
    "historical session was partially blind: its workspace was nested beneath a source \
     checkout and inherited host homes; acceptance and ROB01 matrix results were later \
     regenerated from an external workspace under the recorded evaluation sandbox";

/// Re-evaluates a preserved produced workspace against the current suite and
/// matrix without rerunning or rewriting the historical agent session.
///
/// The historical report is read typed ([`HistoricalRun`]): its identity,
/// session instrumentation, questionnaire, and agent-reported sources are
/// preserved verbatim, and its pins are preserved except
/// `acceptance_sha256`, which is recomputed from the loaded suite so the
/// pin identifies the suite behind the regenerated objective results. The
/// source's `schema_version` must fall in the supported historical range;
/// an off-shape or out-of-range source report is a diagnostic error naming
/// the file, never a panic.
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

    let source_text = std::fs::read_to_string(&config.source_report)
        .with_context(|| format!("reading source report {}", config.source_report.display()))?;
    let source: HistoricalRun = serde_json::from_str(&source_text).with_context(|| {
        format!(
            "source report {} does not deserialize as a run report",
            config.source_report.display()
        )
    })?;
    if source.schema_version < HISTORICAL_SCHEMA_MIN || source.schema_version > SCHEMA_VERSION {
        anyhow::bail!(
            "source report {} records schema_version {}, outside the supported historical \
             range {}..={}",
            config.source_report.display(),
            source.schema_version,
            HISTORICAL_SCHEMA_MIN,
            SCHEMA_VERSION
        );
    }
    if source.archetype.name != archetype.name {
        anyhow::bail!(
            "source report {} records archetype {:?}, but re-evaluation was asked for {:?}",
            config.source_report.display(),
            source.archetype.name,
            archetype.name
        );
    }

    let binary_result = match config.produced_binary.as_deref() {
        Some(path) => provided_binary(path, &config.workspace_root),
        None => acceptance::build_app(
            &config.workspace_root.join("app"),
            archetype.binary(),
            config.timeouts.build,
            &isolation,
        ),
    };
    let evaluation = evaluate_binary(
        &archetype,
        binary_result,
        &config.workspace_root.join(".reevaluation-cases"),
        config.timeouts.check,
        &isolation,
    );

    let report = RunReport {
        schema_version: SCHEMA_VERSION,
        run_id: source.run_id,
        archetype: ArchetypeStamp {
            name: archetype.name.clone(),
            spec_sha256: archetype.spec_sha256(),
        },
        pins: Pins {
            acceptance_sha256: archetype.acceptance_sha256().to_string(),
            ..source.pins
        },
        evaluation: EvaluationStamp {
            origin: "isolated-re-evaluation".to_string(),
            isolation: isolation.evaluation_capability(),
            binary_sha256: evaluation.binary_sha256,
        },
        blindness: Blindness {
            policy: HISTORICAL_BLINDNESS_POLICY.to_string(),
            env_allowlist: source.blindness.env_allowlist,
            framework_source_excluded: false,
            isolation: IsolationRecord {
                backend: "historical-partial".to_string(),
                filesystem: "historical agent session ran without the source-exclusion \
                             boundary (workspace nested beneath a source checkout, host \
                             homes inherited)"
                    .to_string(),
                network: NetworkEnforcement::NotEnforced,
            },
            credential_exceptions: vec![
                "historical agent session inherited host HOME, CARGO_HOME, and RUSTUP_HOME"
                    .to_string(),
            ],
            agent_reported_docs: source.blindness.agent_reported_docs,
            agent_reported_external_sources: source.blindness.agent_reported_external_sources,
        },
        session: source.session,
        acceptance: evaluation.acceptance,
        invariants: evaluation.invariants,
        questionnaire: source.questionnaire,
    };
    report.write(&config.output_report)?;
    Ok(report)
}

/// Validates an explicit produced-binary override: it must be a regular
/// file beneath the preserved workspace, because the check sandbox admits
/// only the workspace and system roots — under Landlock's default-deny
/// model an outside path would be unexecutable, so the contract is refused
/// up front, loudly and identically on every backend. A refusal is a
/// finding (a build-failed report), consistent with the missing-path case.
fn provided_binary(path: &Path, workspace_root: &Path) -> Result<PathBuf, String> {
    if !path.is_file() {
        return Err(format!(
            "provided produced binary {} {}",
            path.display(),
            if path.exists() {
                "is not a regular file"
            } else {
                "does not exist"
            }
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("resolving provided produced binary {}: {e}", path.display()))?;
    let root = workspace_root
        .canonicalize()
        .map_err(|e| format!("resolving workspace root {}: {e}", workspace_root.display()))?;
    if !canonical.starts_with(&root) {
        return Err(format!(
            "provided produced binary {} lies outside the preserved workspace {}; the \
             evaluation sandbox admits only the workspace and system roots",
            canonical.display(),
            root.display()
        ));
    }
    Ok(canonical)
}

/// The objective evaluation both entry points share: acceptance suite plus
/// invariant matrix against one produced binary, or their build-failed /
/// not-run shapes when no binary exists.
struct Evaluation {
    acceptance: AcceptanceReport,
    invariants: Vec<InvariantCell>,
    binary_sha256: Option<String>,
}

/// Runs the archetype's suite and the ROB01 matrix against the outcome of a
/// build. A build failure is a finding — it becomes a build-failed
/// acceptance report with the full not-run matrix — never a runner error.
fn evaluate_binary(
    archetype: &Archetype,
    binary: Result<PathBuf, String>,
    cases_dir: &Path,
    check_timeout: Duration,
    isolation: &workspace::Isolation,
) -> Evaluation {
    match binary {
        Ok(binary) => Evaluation {
            acceptance: match &archetype.suite {
                Suite::Checks(suite) => {
                    acceptance::run_checks(&binary, &suite.checks, check_timeout, isolation)
                }
                Suite::Cases(suite) => {
                    cases::run_cases(&binary, &suite.cases, cases_dir, isolation)
                }
            },
            invariants: acceptance::run_invariants(
                &binary,
                archetype.invariants(),
                check_timeout,
                isolation,
                &cases_dir.join("_invariants"),
            ),
            binary_sha256: std::fs::read(&binary).map(digest::sha256_hex).ok(),
        },
        Err(detail) => Evaluation {
            acceptance: AcceptanceReport::build_failed(detail.clone()),
            invariants: acceptance::not_run_invariants(archetype.invariants(), &detail),
            binary_sha256: None,
        },
    }
}

/// Prints a human summary of the report to stderr.
pub fn print_summary(report: &RunReport) {
    use crate::report::CaseOutcome;
    let passed = report.acceptance.checks.iter().filter(|c| c.passed).count()
        + report
            .acceptance
            .cases
            .iter()
            .filter(|c| c.outcome.is_expected())
            .count();
    let total = report.acceptance.checks.len() + report.acceptance.cases.len();
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
    for check in report.acceptance.checks.iter().filter(|c| !c.passed) {
        eprintln!("[corpus]   FAIL acceptance: {}", check.name);
    }
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
