//! The implementation session: run the agent command in the blind workspace,
//! scrubbed, instrumented, and deadlined.
//!
//! The agent is a seam, not a hard dependency: any shell command works (the
//! walking-skeleton test uses a scripted agent), and the default is a
//! non-interactive Claude Code session whose stream-json transcript yields
//! turn and token counts. The default session is hardened: no user/project
//! settings or plugins load (`--setting-sources ''`) and no MCP servers or
//! connectors attach (`--strict-mcp-config`). The child runs with `env_clear()`
//! plus the recorded allowlist and a disposable home, inside the kernel
//! isolation boundary. No host credentials or repo secrets reach the
//! produced code, which is treated as untrusted. The process group is killed
//! when the configured deadline expires; that is a reported outcome, not a
//! runner error.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;

use crate::exec;
use crate::report::SessionReport;
use crate::workspace;

/// The transcript filename inside the run directory.
pub const TRANSCRIPT_FILENAME: &str = "transcript.jsonl";

/// Most transcript bytes read back for instrumentation. The stats live in
/// the trailing `result` event, so only this much tail is ever ingested — a
/// runaway agent can write a transcript far larger than runner memory.
const TRANSCRIPT_TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// The default agent: a non-interactive Claude Code session over the
/// workspace's instructions file, with host settings, plugins, and MCP
/// servers/connectors disabled.
pub fn default_agent_cmd() -> String {
    "claude --dangerously-skip-permissions \
     --setting-sources '' --strict-mcp-config \
     -p \"Read INSTRUCTIONS.md in the current directory and carry it out completely.\" \
     --output-format stream-json --verbose"
        .to_string()
}

/// Runs `agent_cmd` (via `sh -c`) with the workspace as its working
/// directory, capturing stdout+stderr to `transcript_path`. The session is
/// killed when `timeout` expires; the kill is recorded in the report.
///
/// Fails only when the agent process cannot be spawned at all; a nonzero
/// agent exit or a timeout is recorded in the report, because a failed
/// session is still a reportable run.
pub fn run_agent(
    workspace: &Path,
    isolation: &workspace::Isolation,
    agent_cmd: &str,
    transcript_path: &Path,
    timeout: Duration,
) -> anyhow::Result<SessionReport> {
    let transcript_file = std::fs::File::create(transcript_path)
        .with_context(|| format!("creating transcript {}", transcript_path.display()))?;
    let stderr_file = transcript_file.try_clone()?;

    let mut command = Command::new("sh");
    command.arg("-c").arg(agent_cmd).current_dir(workspace);
    isolation
        .apply_agent(&mut command)
        .map_err(anyhow::Error::msg)?;
    command
        .stdin(Stdio::null())
        .stdout(transcript_file)
        .stderr(stderr_file);

    let started = Instant::now();
    let outcome = exec::run(&mut command, timeout, false)
        .map_err(|err| anyhow::anyhow!("agent command {agent_cmd}: {err}"))?;
    let wall_seconds = started.elapsed().as_secs_f64();

    let stats = stream_json_stats(&read_tail(transcript_path, TRANSCRIPT_TAIL_BYTES));

    Ok(SessionReport {
        agent_cmd: agent_cmd.to_string(),
        wall_seconds,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
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

/// Reads at most the last `limit` bytes of `transcript_path` (lossy UTF-8;
/// a line truncated at the window's start simply fails to parse and is
/// skipped). An unreadable transcript yields an empty string — the same
/// best-effort contract as the stats themselves.
fn read_tail(path: &Path, limit: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = match file.metadata() {
        Ok(meta) => meta.len(),
        Err(_) => return String::new(),
    };
    if len > limit && file.seek(SeekFrom::End(-(limit as i64))).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file.take(limit).read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).into_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    const RESULT_LINE: &str =
        r#"{"type":"result","num_turns":7,"usage":{"input_tokens":123,"output_tokens":45}}"#;

    #[test]
    fn transcript_ingestion_is_bounded_to_the_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        // A transcript far larger than the read window, stats event last —
        // where a stream-json result always lives.
        let mut oversized =
            format!("{{\"type\":\"noise\",\"fill\":\"{}\"}}\n", "x".repeat(200)).repeat(200);
        oversized.push_str(RESULT_LINE);
        oversized.push('\n');
        let limit = 4096;
        assert!(oversized.len() as u64 > limit);
        std::fs::write(&path, &oversized).unwrap();

        let tail = read_tail(&path, limit);
        assert!(tail.len() as u64 <= limit, "read {} bytes", tail.len());
        let stats = stream_json_stats(&tail);
        assert_eq!(stats.turns, Some(7));
        assert_eq!(stats.input_tokens, Some(123));
        assert_eq!(stats.output_tokens, Some(45));
    }

    #[test]
    fn short_and_missing_transcripts_read_whole_and_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, format!("{RESULT_LINE}\n")).unwrap();
        let stats = stream_json_stats(&read_tail(&path, TRANSCRIPT_TAIL_BYTES));
        assert_eq!(stats.turns, Some(7));

        assert_eq!(read_tail(&dir.path().join("absent.jsonl"), 4096), "");
    }
}
