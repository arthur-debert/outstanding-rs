//! The downstream-corpus runner: runs the blind-agent loop for one archetype
//! and files a structured run report. See `corpus/README.md`.

pub mod acceptance;
pub mod archetype;
pub mod batch;
pub mod broker;
pub mod cases;
mod digest;
pub mod exec;
pub mod manifest;
pub mod peer;
pub mod provenance;
pub mod questionnaire;
pub mod report;
pub mod sandbox;
pub mod session;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;

use crate::archetype::Archetype;
use crate::report::{
    AcceptanceReport, ArchetypeStamp, Blindness, EvaluationStamp, HistoricalRun, InvariantCell,
    IsolationRecord, NetworkEnforcement, Pins, RunReport, HISTORICAL_SCHEMA_MIN, SCHEMA_VERSION,
};

pub struct RunConfig {
    pub archetype: String,
    pub archetypes_dir: PathBuf,
    pub runs_dir: PathBuf,
    pub docs_dir: PathBuf,
    pub agent_cmd: String,
    /// `None` grants no credential exception: an agent backend that needs one fails closed.
    pub broker: Option<broker::BrokerConfig>,
    pub framework_version: String,
    pub timeouts: Timeouts,
}

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

#[derive(Clone, Copy)]
pub struct Timeouts {
    pub agent: Duration,
    pub build: Duration,
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

const BLINDNESS_POLICY: &str = "workspace contains spec + published docs + crates.io pins only; \
     agent, build, and produced-binary descendants use disposable homes and a \
     kernel filesystem sandbox that excludes the source checkout and host user-data roots; \
     consulted sources are self-reported in the exit questionnaire";

pub fn run(config: &RunConfig) -> anyhow::Result<(RunReport, PathBuf)> {
    let archetype = Archetype::load(&config.archetypes_dir, &config.archetype)?;

    std::fs::create_dir_all(&config.runs_dir)
        .with_context(|| format!("creating runs directory {}", config.runs_dir.display()))?;
    require_outside_checkout(
        &config.runs_dir,
        &config.docs_dir,
        "runs directory",
        "--runs-dir",
    )?;
    let source_root = checkout_root(&config.docs_dir)?;
    let base = format!("{}-{}", archetype.name, unix_timestamp());
    let (run_id, run_dir) = claim_run_dir(&config.runs_dir, &base)?;

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

    let mut broker = config
        .broker
        .as_ref()
        .map(|broker| broker::Broker::start(broker.clone()))
        .transpose()?;
    if let Some(broker) = &broker {
        eprintln!(
            "[corpus] run-credential broker listening on {} for the agent phase",
            broker.base_url()
        );
    }

    eprintln!(
        "[corpus] running implementation session: {}",
        config.agent_cmd
    );
    let transcript_path = run_dir.join(session::TRANSCRIPT_FILENAME);
    let session_report = session::run_agent(
        &workspace.root,
        &workspace.isolation,
        &config.agent_cmd,
        broker.as_ref(),
        &transcript_path,
        config.timeouts.agent,
    )?;
    // The build and check phases that follow never reach a live broker.
    let credential_exceptions = match &mut broker {
        Some(broker) => {
            broker.shutdown();
            eprintln!(
                "[corpus] broker admitted {} connection(s), denied {}",
                broker.admitted(),
                broker.denied()
            );
            broker.credential_exceptions()
        }
        None => Vec::new(),
    };
    eprintln!(
        "[corpus] session finished in {:.0}s (exit {:?})",
        session_report.wall_seconds, session_report.exit_code
    );

    let questionnaire_report = questionnaire::collect(&workspace.root);
    eprintln!(
        "[corpus] questionnaire collected: {}",
        questionnaire_report.collected
    );

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
        &workspace.app_dir,
    );

    let report = RunReport {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        archetype: ArchetypeStamp {
            name: archetype.name.clone(),
            spec_sha256: archetype.spec_sha256(),
        },
        pins: Pins {
            framework_version: config.framework_version.clone(),
            docs_commit: workspace.docs_commit.clone(),
            docs_sha256: workspace.docs_sha256.clone(),
            docs_source: workspace.docs_source,
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
            env_allowlist: agent_env_allowlist(config.broker.is_some()),
            framework_source_excluded: true,
            isolation: workspace.isolation.agent_capability(),
            credential_exceptions,
            agent_reported_docs: questionnaire_report.answers.get("sources.docs").cloned(),
            agent_reported_external_sources: questionnaire_report
                .answers
                .get("sources.external")
                .cloned(),
        },
        session: session_report,
        provenance: provenance::describe(&config.agent_cmd, &transcript_path),
        acceptance: evaluation.acceptance,
        invariants: evaluation.invariants,
        questionnaire: questionnaire_report,
    };
    let report_path = run_dir.join("report.json");
    report.write(&report_path)?;
    eprintln!("[corpus] report written to {}", report_path.display());

    Ok((report, run_dir))
}

fn agent_env_allowlist(brokered: bool) -> Vec<String> {
    let mut keys: Vec<String> = workspace::ENV_ALLOWLIST
        .iter()
        .map(ToString::to_string)
        .collect();
    if brokered {
        keys.extend(broker::AGENT_ENV_KEYS.iter().map(ToString::to_string));
    }
    keys
}

const HISTORICAL_BLINDNESS_POLICY: &str =
    "historical session was partially blind: its workspace was nested beneath a source \
     checkout and inherited host homes; acceptance and ROB01 matrix results were later \
     regenerated from an external workspace under the recorded evaluation sandbox";

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
        &config.workspace_root.join("app"),
    );

    // Before schema 4 the recorded command is all that can be said about the agent.
    let provenance = match source.provenance {
        Some(stated) => stated,
        None => provenance::recorded(&source.session.agent_cmd),
    };

    let blindness = match (
        source.blindness.policy,
        source.blindness.framework_source_excluded,
        source.blindness.isolation,
        source.blindness.credential_exceptions,
    ) {
        (
            Some(policy),
            Some(framework_source_excluded),
            Some(agent_isolation),
            Some(credential_exceptions),
        ) => Blindness {
            policy,
            env_allowlist: source.blindness.env_allowlist,
            framework_source_excluded,
            isolation: agent_isolation,
            credential_exceptions,
            agent_reported_docs: source.blindness.agent_reported_docs,
            agent_reported_external_sources: source.blindness.agent_reported_external_sources,
        },
        _ => Blindness {
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
    };

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
        blindness,
        session: source.session,
        provenance,
        acceptance: evaluation.acceptance,
        invariants: evaluation.invariants,
        questionnaire: source.questionnaire,
    };
    report.write(&config.output_report)?;
    Ok(report)
}

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

struct Evaluation {
    acceptance: AcceptanceReport,
    invariants: Vec<InvariantCell>,
    binary_sha256: Option<String>,
}

fn evaluate_binary(
    archetype: &Archetype,
    binary: Result<PathBuf, String>,
    cases_dir: &Path,
    check_timeout: Duration,
    isolation: &workspace::Isolation,
    app_dir: &Path,
) -> Evaluation {
    match binary {
        Ok(binary) => {
            // Read through the same no-follow, regular-file-only primitive
            // the sandbox inventory reads case files with: a symlink,
            // a non-regular file, or a read error is "evidence unknown",
            // not "evidence absent".
            let app_cargo_toml = cases::read_regular_file_no_follow(
                &app_dir.join("Cargo.toml"),
                cases::MAX_INVENTORIED_BYTES,
            )
            .and_then(|bytes| String::from_utf8(bytes).map_err(|err| err.to_string()));
            Evaluation {
                acceptance: cases::run_cases(
                    &binary,
                    &archetype.suite.cases,
                    cases_dir,
                    isolation,
                    &archetype.gaps,
                    app_cargo_toml.as_deref().map_err(String::as_str),
                ),
                invariants: acceptance::run_invariants(
                    &binary,
                    archetype.invariants(),
                    check_timeout,
                    isolation,
                    &cases_dir.join("_invariants"),
                ),
                binary_sha256: std::fs::read(&binary).map(digest::sha256_hex).ok(),
            }
        }
        Err(detail) => Evaluation {
            acceptance: AcceptanceReport::build_failed(detail.clone()),
            invariants: acceptance::not_run_invariants(archetype.invariants(), &detail),
            binary_sha256: None,
        },
    }
}

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
            CaseOutcome::HandRolledPass => {
                eprintln!(
                    "[corpus]   hand-rolled pass case: {} (gap {} — evidence crate absent)",
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

/// The repository root that owns `docs_dir` (its canonical parent): the
/// boundary a blind workspace's isolation is verified against, and the
/// boundary [`require_outside_checkout`] rejects external paths for
/// resolving inside.
fn checkout_root(docs_dir: &Path) -> anyhow::Result<PathBuf> {
    docs_dir
        .canonicalize()
        .with_context(|| format!("resolving docs directory {}", docs_dir.display()))?
        .parent()
        .map(Path::to_path_buf)
        .context("docs directory has no repository parent")
}

/// Rejects `path` when it resolves inside the source checkout that owns
/// `docs_dir`: a workspace or scratch output living there would leak into
/// the blind sandbox's exclusion boundary. `what` and `flag` name the
/// rejected path and the CLI flag that set it, for the error.
pub(crate) fn require_outside_checkout(
    path: &Path,
    docs_dir: &Path,
    what: &str,
    flag: &str,
) -> anyhow::Result<PathBuf> {
    let source_root = checkout_root(docs_dir)?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolving {what} {}", path.display()))?;
    if canonical.starts_with(&source_root) {
        anyhow::bail!(
            "{what} {} is inside source checkout {}; choose an external {flag}",
            canonical.display(),
            source_root.display()
        );
    }
    Ok(canonical)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// `create_dir`, never `create_dir_all`: adopting an existing directory would share a run.
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
