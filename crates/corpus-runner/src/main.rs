//! `corpus-runner` — the single command that runs one corpus archetype's
//! full blind-agent loop and files a run report (see `corpus/README.md`).

use std::path::PathBuf;
use std::process::ExitCode;

use std::time::Duration;

use clap::{Parser, Subcommand};

use corpus_runner::{absolute, print_summary, run, session, RunConfig, Timeouts};

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
        /// The docs directory the published snapshot is copied from.
        #[arg(long, default_value = "docs")]
        docs_dir: PathBuf,
        /// Shell command implementing the session (default: a
        /// non-interactive Claude Code session over INSTRUCTIONS.md).
        #[arg(long)]
        agent_cmd: Option<String>,
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
        /// Seconds before each acceptance/invariant invocation is killed.
        #[arg(long)]
        check_timeout: Option<u64>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            archetype,
            corpus_dir,
            docs_dir,
            agent_cmd,
            framework_version,
            agent_timeout,
            build_timeout,
            check_timeout,
        } => {
            let corpus_dir = absolute(&corpus_dir);
            let mut timeouts = Timeouts::default();
            if let Some(secs) = agent_timeout {
                timeouts.agent = Duration::from_secs(secs);
            }
            if let Some(secs) = build_timeout {
                timeouts.build = Duration::from_secs(secs);
            }
            if let Some(secs) = check_timeout {
                timeouts.check = Duration::from_secs(secs);
            }
            let config = RunConfig {
                archetype,
                archetypes_dir: corpus_dir.join("archetypes"),
                runs_dir: corpus_dir.join("runs"),
                docs_dir: absolute(&docs_dir),
                agent_cmd: agent_cmd.unwrap_or_else(session::default_agent_cmd),
                framework_version,
                timeouts,
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
    }
}
