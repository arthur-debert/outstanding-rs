//! The implementation session: run the agent command in the blind workspace,
//! scrubbed and instrumented.
//!
//! The agent is a seam, not a hard dependency: any shell command works (the
//! walking-skeleton test uses a scripted agent), and the default is a
//! non-interactive Claude Code session whose stream-json transcript yields
//! turn and token counts. The child runs with `env_clear()` plus the
//! recorded allowlist — no repo secrets reach the produced code, which is
//! treated as untrusted.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::Context;

use crate::report::SessionReport;
use crate::workspace::ENV_ALLOWLIST;

/// The transcript filename inside the run directory.
pub const TRANSCRIPT_FILENAME: &str = "transcript.jsonl";

/// The default agent: a non-interactive Claude Code session over the
/// workspace's instructions file.
pub fn default_agent_cmd() -> String {
    "claude --dangerously-skip-permissions \
     -p \"Read INSTRUCTIONS.md in the current directory and carry it out completely.\" \
     --output-format stream-json --verbose"
        .to_string()
}

/// Runs `agent_cmd` (via `sh -c`) with the workspace as its working
/// directory, capturing stdout+stderr to `transcript_path`.
///
/// Fails only when the agent process cannot be spawned at all; a nonzero
/// agent exit is recorded in the report, because a failed session is still
/// a reportable run.
pub fn run_agent(
    workspace: &Path,
    agent_cmd: &str,
    transcript_path: &Path,
) -> anyhow::Result<SessionReport> {
    let transcript_file = std::fs::File::create(transcript_path)
        .with_context(|| format!("creating transcript {}", transcript_path.display()))?;
    let stderr_file = transcript_file.try_clone()?;

    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(agent_cmd)
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(transcript_file)
        .stderr(stderr_file)
        .env_clear();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }

    let started = Instant::now();
    let status = command
        .status()
        .with_context(|| format!("spawning agent command: {agent_cmd}"))?;
    let wall_seconds = started.elapsed().as_secs_f64();

    let transcript_text = std::fs::read_to_string(transcript_path).unwrap_or_default();
    let stats = stream_json_stats(&transcript_text);

    Ok(SessionReport {
        agent_cmd: agent_cmd.to_string(),
        wall_seconds,
        exit_code: status.code(),
        attempts: 1,
        turns: stats.turns,
        input_tokens: stats.input_tokens,
        output_tokens: stats.output_tokens,
        transcript: TRANSCRIPT_FILENAME.to_string(),
    })
}

/// Turn/token counts parsed from a transcript, when it carries them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    pub turns: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Extracts instrumentation from a Claude Code stream-json transcript: the
/// final `"type": "result"` event carries `num_turns` and `usage`. Any other
/// transcript format yields empty stats — instrumentation is best-effort by
/// design ("tokens where available").
pub fn stream_json_stats(transcript: &str) -> StreamStats {
    let mut stats = StreamStats::default();
    for line in transcript.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("result") {
            continue;
        }
        stats.turns = value.get("num_turns").and_then(|n| n.as_u64());
        let usage = value.get("usage");
        stats.input_tokens = usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(|n| n.as_u64());
        stats.output_tokens = usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(|n| n.as_u64());
    }
    stats
}
