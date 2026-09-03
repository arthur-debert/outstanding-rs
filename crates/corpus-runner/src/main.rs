//! `corpus-runner` — the single command that runs one corpus archetype's
//! full blind-agent loop and files a run report (see `corpus/README.md`).

use std::path::PathBuf;
use std::process::ExitCode;

use std::time::Duration;

use clap::{Parser, Subcommand};

use corpus_runner::batch::{batch, BatchConfig};
use corpus_runner::broker::{BrokerConfig, Credential};
use corpus_runner::{
    absolute, print_summary, reevaluate, run, session, ReevaluationConfig, RunConfig, Timeouts,
};

#[derive(Parser)]
#[command(name = "corpus-runner", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the full loop for one archetype: provision a blind workspace,
    /// execute the agent session, collect the questionnaire, run the
    /// acceptance suite + invariant matrix, and write report.json.
    Run {
        /// Archetype name under the corpus archetypes directory.
        archetype: String,
        /// The corpus directory (holding archetypes/ and runs/).
        #[arg(long, default_value = "corpus")]
        corpus_dir: PathBuf,
        /// External directory for untrusted run workspaces. It must not live
        /// beneath the framework checkout.
        #[arg(long)]
        runs_dir: Option<PathBuf>,
        /// The docs directory the published snapshot is copied from.
        #[arg(long, default_value = "docs")]
        docs_dir: PathBuf,
        /// Shell command implementing the session (default: a
        /// non-interactive Claude Code session over INSTRUCTIONS.md).
        #[arg(long)]
        agent_cmd: Option<String>,
        /// Authenticate the agent session through the run-credential
        /// broker: the host Claude subscription credential stays on the
        /// host, and the session reaches the API only through a proxy that
        /// answers the agent process alone. The agent command must then be
        /// spawnable without a shell. The destination is not configurable:
        /// a host credential forwards to the Anthropic API and nowhere else.
        #[arg(long)]
        broker: bool,
        /// Exact crates.io framework version the blind scaffold pins.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        framework_version: String,
        /// Seconds before the agent session is killed (recorded in the
        /// report as timed_out).
        #[arg(long)]
        agent_timeout: Option<u64>,
        /// Seconds before the cargo build of the produced app is killed.
        #[arg(long)]
        build_timeout: Option<u64>,
        /// Seconds before each invariant invocation is killed. Acceptance
        /// cases are not affected: each carries its own authored
        /// `timeout_seconds`.
        #[arg(long)]
        check_timeout: Option<u64>,
    },
    /// Re-evaluate a preserved produced workspace without rerunning its
    /// historical agent session.
    Reevaluate {
        archetype: String,
        #[arg(long, default_value = "corpus")]
        corpus_dir: PathBuf,
        #[arg(long, default_value = "docs")]
        docs_dir: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        source_report: PathBuf,
        #[arg(long)]
        output_report: PathBuf,
        /// Exact already-built executable to evaluate; it must live beneath
        /// the preserved workspace, since checks run inside a sandbox that
        /// admits only the workspace and system roots. When omitted, the
        /// preserved app is rebuilt inside the isolated build phase.
        #[arg(long)]
        binary: Option<PathBuf>,
        #[arg(long)]
        build_timeout: Option<u64>,
        /// Seconds before each invariant invocation is killed. Acceptance
        /// cases are not affected: each carries its own authored
        /// `timeout_seconds`.
        #[arg(long)]
        check_timeout: Option<u64>,
    },
    /// Run a set of archetypes through the full loop in order, sanitize
    /// each run's evidence outside the checkout, and write both scorecards
    /// under `--out`. Exits non-zero if any archetype's run failed to
    /// complete.
    Batch {
        /// Archetype names to run, in order.
        #[arg(required = true)]
        archetypes: Vec<String>,
        /// The corpus directory (holding archetypes/, sanitize-run.py and
        /// scorecard.py).
        #[arg(long, default_value = "corpus")]
        corpus_dir: PathBuf,
        /// The docs directory the published snapshot is copied from.
        #[arg(long, default_value = "docs")]
        docs_dir: PathBuf,
        /// Shell command implementing the session (default: a
        /// non-interactive Claude Code session over INSTRUCTIONS.md).
        #[arg(long)]
        agent_cmd: Option<String>,
        /// Authenticate every agent session through the run-credential
        /// broker; see `run --help`.
        #[arg(long)]
        broker: bool,
        /// Exact crates.io framework version every blind scaffold pins.
        #[arg(long, default_value = env!("CARGO_PKG_VERSION"))]
        framework_version: String,
        /// Seconds before an agent session is killed.
        #[arg(long)]
        agent_timeout: Option<u64>,
        /// Seconds before a produced app's build is killed.
        #[arg(long)]
        build_timeout: Option<u64>,
        /// Seconds before each invariant invocation is killed.
        #[arg(long)]
        check_timeout: Option<u64>,
        /// The one directory the batch owns: sanitized run evidence and
        /// both scorecards land under it (one `<run-id>/` per archetype,
        /// plus `scorecard.json` and `scorecard.md`), and it holds each
        /// archetype's untrusted run workspace until sanitizing removes it.
        /// It must not live beneath the framework checkout.
        #[arg(long)]
        out: PathBuf,
        /// Host account name to scrub from sanitized transcripts (forwarded
        /// to `sanitize-run.py --account`).
        #[arg(long)]
        account: Option<String>,
    },
}

fn resolve_broker(broker: bool) -> anyhow::Result<Option<BrokerConfig>> {
    if !broker {
        return Ok(None);
    }
    let credential = Credential::from_host_store()?;
    eprintln!(
        "[corpus] brokering the credential from {}",
        credential.source()
    );
    Ok(Some(BrokerConfig::for_host(credential)))
}

fn resolve_timeouts(agent: Option<u64>, build: Option<u64>, check: Option<u64>) -> Timeouts {
    let mut timeouts = Timeouts::default();
    if let Some(secs) = agent {
        timeouts.agent = Duration::from_secs(secs);
    }
    if let Some(secs) = build {
        timeouts.build = Duration::from_secs(secs);
    }
    if let Some(secs) = check {
        timeouts.check = Duration::from_secs(secs);
    }
    timeouts
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            archetype,
            corpus_dir,
            runs_dir,
            docs_dir,
            agent_cmd,
            broker,
            framework_version,
            agent_timeout,
            build_timeout,
            check_timeout,
        } => {
            let corpus_dir = absolute(&corpus_dir);
            let broker = match resolve_broker(broker) {
                Ok(broker) => broker,
                Err(err) => {
                    eprintln!("[corpus] runner error: {err:#}");
                    return ExitCode::FAILURE;
                }
            };
            let timeouts = resolve_timeouts(agent_timeout, build_timeout, check_timeout);
            let config = RunConfig {
                archetype,
                archetypes_dir: corpus_dir.join("archetypes"),
                runs_dir: runs_dir
                    .map(|path| absolute(&path))
                    .unwrap_or_else(|| std::env::temp_dir().join("standout-corpus-runs")),
                docs_dir: absolute(&docs_dir),
                agent_cmd: agent_cmd.unwrap_or_else(session::default_agent_cmd),
                broker,
                framework_version,
                timeouts,
                run_id: None,
            };
            match run(&config) {
                Ok((report, _run_dir)) => {
                    print_summary(&report);
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("[corpus] runner error: {err:#}");
                    ExitCode::FAILURE
                }
            }
        }
        Commands::Reevaluate {
            archetype,
            corpus_dir,
            docs_dir,
            workspace,
            source_report,
            output_report,
            binary,
            build_timeout,
            check_timeout,
        } => {
            let timeouts = resolve_timeouts(None, build_timeout, check_timeout);
            let config = ReevaluationConfig {
                archetype,
                archetypes_dir: absolute(&corpus_dir).join("archetypes"),
                docs_dir: absolute(&docs_dir),
                workspace_root: absolute(&workspace),
                source_report: absolute(&source_report),
                output_report: absolute(&output_report),
                produced_binary: binary.as_deref().map(absolute),
                timeouts,
            };
            match reevaluate(&config) {
                Ok(report) => {
                    print_summary(&report);
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("[corpus] re-evaluation error: {err:#}");
                    ExitCode::FAILURE
                }
            }
        }
        Commands::Batch {
            archetypes,
            corpus_dir,
            docs_dir,
            agent_cmd,
            broker,
            framework_version,
            agent_timeout,
            build_timeout,
            check_timeout,
            out,
            account,
        } => {
            let corpus_dir = absolute(&corpus_dir);
            let broker = match resolve_broker(broker) {
                Ok(broker) => broker,
                Err(err) => {
                    eprintln!("[corpus] batch error: {err:#}");
                    return ExitCode::FAILURE;
                }
            };
            let timeouts = resolve_timeouts(agent_timeout, build_timeout, check_timeout);
            let config = BatchConfig {
                archetypes,
                archetypes_dir: corpus_dir.join("archetypes"),
                docs_dir: absolute(&docs_dir),
                out_dir: absolute(&out),
                agent_cmd: agent_cmd.unwrap_or_else(session::default_agent_cmd),
                broker,
                framework_version,
                timeouts,
                sanitize_script: corpus_dir.join("sanitize-run.py"),
                scorecard_script: corpus_dir.join("scorecard.py"),
                account,
            };
            match batch(&config) {
                Ok(outcomes) => {
                    let mut failed = 0;
                    for (archetype, outcome) in &outcomes {
                        match outcome {
                            Ok(run_id) => eprintln!("[corpus] batch: {archetype} -> {run_id}"),
                            Err(detail) => {
                                failed += 1;
                                eprintln!("[corpus] batch: {archetype} FAILED: {detail}");
                            }
                        }
                    }
                    if failed > 0 {
                        eprintln!(
                            "[corpus] batch: {failed} of {} archetype run(s) failed to complete",
                            outcomes.len()
                        );
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(err) => {
                    eprintln!("[corpus] batch error: {err:#}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
