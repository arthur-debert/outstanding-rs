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
pub mod questionnaire;
pub mod report;
pub mod session;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
}

/// The one-line blindness policy statement recorded in every report.
const BLINDNESS_POLICY: &str = "workspace contains spec + published docs + crates.io pins only; \
     agent env is cleared to a recorded allowlist; consulted sources are \
     self-reported in the exit questionnaire";

/// Runs the full loop and returns the written report plus the run directory.
///
/// A run that completes the loop always writes `report.json`, even when the
/// app never built or every check failed — those are findings. Errors are
/// reserved for the runner's own failures (unloadable archetype, unwritable
/// run directory, unspawnable agent).
pub fn run(config: &RunConfig) -> anyhow::Result<(RunReport, PathBuf)> {
    let archetype = Archetype::load(&config.archetypes_dir, &config.archetype)?;

    let run_id = format!("{}-{}", archetype.name, unix_timestamp());
    let run_dir = config.runs_dir.join(&run_id);
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("creating run directory {}", run_dir.display()))?;

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
    let session_report = session::run_agent(&workspace.root, &config.agent_cmd, &transcript_path)?;
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
    let (acceptance_report, invariant_cells) =
        match acceptance::build_app(&workspace.app_dir, &archetype.acceptance.binary) {
            Ok(binary) => (
                acceptance::run_checks(&binary, &archetype.acceptance.checks),
                acceptance::run_invariants(&binary, &archetype.acceptance.invariants),
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
