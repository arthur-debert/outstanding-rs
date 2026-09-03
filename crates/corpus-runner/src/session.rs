//! The implementation session: runs the agent command in the blind workspace,
//! scrubbed, instrumented, and deadlined. The agent is a seam — any shell
//! command works — with a hardened non-interactive Claude Code session as
//! the default. A session brokering a credential gives that seam up: the
//! broker answers one pid, so the runner spawns the agent itself rather than
//! a shell that spawns it.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};

use crate::broker::Broker;
use crate::digest;
use crate::exec;
use crate::report::SessionReport;
use crate::workspace;

pub const TRANSCRIPT_FILENAME: &str = "transcript.jsonl";

const TRANSCRIPT_TAIL_BYTES: u64 = 4 * 1024 * 1024;

pub fn default_agent_cmd() -> String {
    "claude --dangerously-skip-permissions \
     --setting-sources '' --strict-mcp-config \
     -p \"Read INSTRUCTIONS.md in the current directory and carry it out completely.\" \
     --output-format stream-json --verbose"
        .to_string()
}

pub fn run_agent(
    workspace: &Path,
    isolation: &workspace::Isolation,
    agent_cmd: &str,
    broker: Option<&Broker>,
    transcript_path: &Path,
    timeout: Duration,
) -> anyhow::Result<SessionReport> {
    let transcript_file = std::fs::File::create(transcript_path)
        .with_context(|| format!("creating transcript {}", transcript_path.display()))?;
    let stderr_file = transcript_file.try_clone()?;

    let mut command = match broker {
        Some(_) => {
            let argv = direct_argv(agent_cmd)?;
            let mut command = Command::new(resolve_program(&argv[0])?);
            command.args(&argv[1..]);
            command
        }
        None => {
            let mut command = Command::new("sh");
            command.arg("-c").arg(agent_cmd);
            command
        }
    };
    command.current_dir(workspace);
    isolation
        .apply_agent(&mut command)
        .map_err(anyhow::Error::msg)?;
    if let Some(broker) = broker {
        broker.apply_agent_env(&mut command);
    }
    command
        .stdin(Stdio::null())
        .stdout(transcript_file)
        .stderr(stderr_file);

    let started = Instant::now();
    let outcome = exec::run_watched(&mut command, timeout, false, |pid| {
        if let Some(broker) = broker {
            broker.authorize(pid);
        }
    })
    .map_err(|err| anyhow::anyhow!("agent command {agent_cmd}: {err}"))?;
    if let Some(broker) = broker {
        broker.revoke();
    }
    let wall_seconds = started.elapsed().as_secs_f64();

    let stats = stream_json_stats(&read_tail(transcript_path, TRANSCRIPT_TAIL_BYTES));
    let transcript_sha256 = hash_transcript(transcript_path)
        .with_context(|| format!("hashing transcript {}", transcript_path.display()))?;

    Ok(SessionReport {
        agent_cmd: agent_cmd.to_string(),
        wall_seconds,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        turns: stats.turns,
        input_tokens: stats.input_tokens,
        output_tokens: stats.output_tokens,
        transcript: TRANSCRIPT_FILENAME.to_string(),
        transcript_sha256: Some(transcript_sha256),
    })
}

// Streamed rather than read whole: a transcript can run to tens of megabytes.
fn hash_transcript(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    std::io::copy(&mut reader, &mut hasher)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(digest::hex(hasher.finalize()))
}

// A bare name is searched on the runner's own PATH, as the shell it replaces would.
fn resolve_program(program: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = std::path::Path::new(program);
    if path.components().count() > 1 {
        return Ok(path.to_path_buf());
    }
    let search = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&search)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
        .with_context(|| format!("no executable {program:?} on PATH"))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

// The runner spawns one program and hands that pid to the broker; it cannot do what a shell would.
const SHELL_STRUCTURE: &[char] = &['|', '&', ';', '<', '>', '(', ')', '\n'];

const SHELL_GLOB: &[char] = &['*', '?', '[', ']'];

/// Honors quotes and backslash escapes, expands nothing, and refuses a command that needs a shell.
pub fn direct_argv(agent_cmd: &str) -> anyhow::Result<Vec<String>> {
    let mut argv: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = agent_cmd.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            _ if SHELL_STRUCTURE.contains(&ch) => bail!(
                "agent command {agent_cmd:?} needs a shell to interpret {ch:?}, but a \
                 brokered session must be spawned directly so the broker can attribute \
                 its connections"
            ),
            '$' | '`' => bail!(
                "agent command {agent_cmd:?} asks for shell expansion ({ch:?}), which a \
                 brokered session does not perform"
            ),
            _ if SHELL_GLOB.contains(&ch) => bail!(
                "agent command {agent_cmd:?} has an unquoted {ch:?}, which a shell would \
                 expand against the filesystem and a brokered session would pass through \
                 literally; quote it to mean it literally"
            ),
            '~' if !started => bail!(
                "agent command {agent_cmd:?} starts a word with an unquoted '~', which a \
                 shell would expand to a home directory and a brokered session would not; \
                 write the path out"
            ),
            _ if ch.is_whitespace() => {
                if started {
                    argv.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(quoted) => word.push(quoted),
                        None => bail!("agent command {agent_cmd:?} has an unclosed quote"),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        // The set a shell unescapes inside double quotes.
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => word.push(escaped),
                            Some('\n') => {}
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => bail!("agent command {agent_cmd:?} has an unclosed quote"),
                        },
                        Some(expansion @ ('$' | '`')) => bail!(
                            "agent command {agent_cmd:?} asks for shell expansion \
                             ({expansion:?}), which a brokered session does not perform"
                        ),
                        Some(quoted) => word.push(quoted),
                        None => bail!("agent command {agent_cmd:?} has an unclosed quote"),
                    }
                }
            }
            '\\' => {
                started = true;
                match chars.next() {
                    Some(escaped) => word.push(escaped),
                    None => bail!("agent command {agent_cmd:?} ends in a dangling escape"),
                }
            }
            _ => {
                started = true;
                word.push(ch);
            }
        }
    }
    if started {
        argv.push(word);
    }
    if argv.is_empty() {
        bail!("agent command is empty");
    }
    Ok(argv)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    pub turns: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

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
    fn the_default_session_splits_into_one_program_and_its_arguments() {
        let argv = direct_argv(&default_agent_cmd()).unwrap();
        assert_eq!(argv[0], "claude");
        assert!(
            argv.contains(&"--strict-mcp-config".to_string()),
            "{argv:?}"
        );
        let sources = argv
            .iter()
            .position(|arg| arg == "--setting-sources")
            .unwrap();
        assert_eq!(argv[sources + 1], "");
        let prompt = argv.iter().position(|arg| arg == "-p").unwrap();
        assert_eq!(
            argv[prompt + 1],
            "Read INSTRUCTIONS.md in the current directory and carry it out completely."
        );
    }

    #[test]
    fn a_command_that_needs_a_shell_is_refused_rather_than_run_unattributed() {
        for unbrokerable in [
            "claude -p prompt > transcript",
            "printf x | claude",
            "claude; rm -rf .",
            "claude -p \"$PROMPT\"",
            "claude -p `cat prompt`",
            "claude -p 'unclosed",
            "claude --prompt-file prompts/*.md",
            "claude --prompt-file prompt?.md",
            "claude --prompt-file prompt[12].md",
            "claude --prompt-file ~/prompts/one.md",
            "",
        ] {
            assert!(
                direct_argv(unbrokerable).is_err(),
                "{unbrokerable:?} should not be spawnable without a shell"
            );
        }
    }

    #[test]
    fn quoted_expansion_characters_are_ordinary_text() {
        assert_eq!(
            direct_argv(r#"claude -p "does it? *everything*" '~/literal' a\*b"#).unwrap(),
            vec!["claude", "-p", "does it? *everything*", "~/literal", "a*b",]
        );
    }

    #[test]
    fn escapes_inside_double_quotes_unescape_the_way_a_shell_does() {
        assert_eq!(
            direct_argv(r#"claude -p "cost is \$5 or \`half\`""#).unwrap(),
            vec!["claude", "-p", "cost is $5 or `half`"]
        );
        assert_eq!(
            direct_argv(r#"claude -p "say \"hi\" \\ then \n""#).unwrap(),
            vec!["claude", "-p", r#"say "hi" \ then \n"#]
        );
        assert_eq!(
            direct_argv("claude -p \"one\\\ntwo\"").unwrap(),
            vec!["claude", "-p", "onetwo"]
        );
        assert!(direct_argv(r#"claude -p "cost is $5""#).is_err());
    }

    #[test]
    fn quoting_and_escapes_survive_the_split() {
        assert_eq!(
            direct_argv(r#"/bin/agent --say "hello  world" --path a\ b '{"k": "v"}'"#).unwrap(),
            vec![
                "/bin/agent",
                "--say",
                "hello  world",
                "--path",
                "a b",
                r#"{"k": "v"}"#,
            ]
        );
    }

    #[test]
    fn transcript_ingestion_is_bounded_to_the_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
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
