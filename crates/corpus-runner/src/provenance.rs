//! The agent side of a run, recorded rather than assumed: which backend the
//! runner spawned, which version of it announced itself, the model asked for
//! and the model that answered, the session prompt, and the remaining
//! settings the runner passed.
//!
//! Two runs are comparable evidence only when this block matches. Where a
//! later run cannot reproduce an earlier one — a retired model, a newer
//! backend — the block is what lets the comparison state the delta instead of
//! reading as if there were none.
//!
//! Two sources, and only two: the command the runner spawned, split the way
//! `session` splits it, and the session's own transcript. The runner never
//! asks the agent executable about itself — running an unknown program to
//! read its `--version` executes it on the host, outside every boundary the
//! run is built on, and an agent binary is under no obligation to be inert
//! about it. A field neither source states stays `None`; it is never filled
//! in from a plausible default.

use std::path::Path;

use crate::report::AgentProvenance;
use crate::session::direct_argv;

// The backend announces itself in the first event; a whole transcript is megabytes.
const TRANSCRIPT_HEAD_BYTES: usize = 256 * 1024;

// Records are read whole (the init event alone can exceed the head budget, and half
// a record is not JSON); this caps the one record the head budget cannot.
const TRANSCRIPT_RECORD_BYTES: usize = 8 * 1024 * 1024;

pub fn describe(agent_cmd: &str, transcript: &Path) -> AgentProvenance {
    let mut provenance = recorded(agent_cmd);
    let announced = announced_in_transcript(&head(transcript));
    provenance.executable_version = announced.version;
    provenance.model_observed = announced.model;
    provenance
}

/// From the recorded command alone: a re-evaluation has no transcript of its own.
pub fn recorded(agent_cmd: &str) -> AgentProvenance {
    from_argv(direct_argv(agent_cmd).ok().as_deref())
}

fn from_argv(argv: Option<&[String]>) -> AgentProvenance {
    let Some(argv) = argv else {
        return AgentProvenance::default();
    };
    let backend = Path::new(&argv[0])
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());

    let mut model_requested = None;
    let mut prompt = None;
    let mut settings = Vec::new();
    let mut args = argv[1..].iter().peekable();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with('-') => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        match flag {
            "-p" | "--print" => prompt = inline.or_else(|| take_value(&mut args)),
            "--model" => model_requested = inline.or_else(|| take_value(&mut args)),
            _ => settings.push(arg.clone()),
        }
    }

    AgentProvenance {
        backend,
        executable_version: None,
        model_requested,
        model_observed: None,
        prompt,
        settings,
    }
}

// `-p` and `--model` take an optional value; the next flag is not it.
fn take_value<'a>(
    args: &mut std::iter::Peekable<impl Iterator<Item = &'a String>>,
) -> Option<String> {
    match args.peek() {
        Some(next) if !next.starts_with('-') => args.next().cloned(),
        _ => None,
    }
}

// The record that spends the head budget is still read to its end.
fn head(path: &Path) -> String {
    use std::io::{BufRead, BufReader, Read};
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut reader = BufReader::new(file);
    let mut head = String::new();
    while head.len() < TRANSCRIPT_HEAD_BYTES {
        let mut record = Vec::new();
        match reader
            .by_ref()
            .take(TRANSCRIPT_RECORD_BYTES as u64)
            .read_until(b'\n', &mut record)
        {
            Ok(0) | Err(_) => break,
            Ok(_) => head.push_str(&String::from_utf8_lossy(&record)),
        }
    }
    head
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Announced {
    pub version: Option<String>,
    pub model: Option<String>,
}

/// Reads a Claude Code stream-json transcript; any other shape announces nothing.
pub fn announced_in_transcript(transcript: &str) -> Announced {
    let mut announced = Announced::default();
    for line in transcript.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let text = |field: &str| {
            value
                .get(field)
                .and_then(|found| found.as_str())
                .map(ToString::to_string)
        };
        match value.get("type").and_then(|t| t.as_str()) {
            Some("system") if value.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
                announced.version = text("claude_code_version").or_else(|| text("version"));
                if let Some(model) = text("model") {
                    announced.model = Some(model);
                    return announced;
                }
            }
            Some("assistant") if announced.model.is_none() => {
                announced.model = value
                    .get("message")
                    .and_then(|message| message.get("model"))
                    .and_then(|model| model.as_str())
                    .map(ToString::to_string);
            }
            _ => {}
        }
    }
    announced
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::default_agent_cmd;

    #[test]
    fn the_default_session_records_its_backend_prompt_and_settings() {
        let provenance = recorded(&default_agent_cmd());
        assert_eq!(provenance.backend.as_deref(), Some("claude"));
        assert_eq!(
            provenance.prompt.as_deref(),
            Some("Read INSTRUCTIONS.md in the current directory and carry it out completely.")
        );
        // No `--model`: only the transcript can say what the default resolved to.
        assert_eq!(provenance.model_requested, None);
        assert!(
            provenance
                .settings
                .contains(&"--dangerously-skip-permissions".to_string()),
            "{:?}",
            provenance.settings
        );
        // The prompt is its own field, not a setting.
        assert!(
            !provenance.settings.iter().any(|s| s == "-p"),
            "{provenance:?}"
        );
        assert!(
            !provenance
                .settings
                .iter()
                .any(|s| s.contains("INSTRUCTIONS")),
            "{provenance:?}"
        );
    }

    #[test]
    fn a_requested_model_is_recorded_in_either_spelling() {
        for command in [
            "/usr/local/bin/claude --model claude-opus-5 -p do-it",
            "/usr/local/bin/claude --model=claude-opus-5 -p=do-it",
        ] {
            let provenance = recorded(command);
            assert_eq!(provenance.backend.as_deref(), Some("claude"));
            assert_eq!(provenance.model_requested.as_deref(), Some("claude-opus-5"));
            assert_eq!(provenance.prompt.as_deref(), Some("do-it"));
            assert!(provenance.settings.is_empty(), "{provenance:?}");
        }
    }

    #[test]
    fn a_valueless_flag_does_not_swallow_the_next_flag() {
        let provenance = recorded("claude -p --verbose");
        assert_eq!(provenance.prompt, None);
        assert_eq!(provenance.settings, vec!["--verbose".to_string()]);
    }

    #[test]
    fn a_shell_command_records_nothing_it_cannot_parse() {
        assert_eq!(
            recorded("printf x | claude -p go"),
            AgentProvenance::default()
        );
    }

    #[test]
    fn the_init_event_announces_the_backend_version_and_session_model() {
        let transcript = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-opus-5[1m]","#,
            r#""claude_code_version":"2.1.252"}"#,
            "\n",
            r#"{"type":"assistant","message":{"model":"claude-opus-5"}}"#,
            "\n",
        );
        assert_eq!(
            announced_in_transcript(transcript),
            Announced {
                version: Some("2.1.252".to_string()),
                model: Some("claude-opus-5[1m]".to_string()),
            }
        );
    }

    #[test]
    fn an_assistant_message_answers_when_no_init_event_does() {
        let transcript = concat!(
            "not json at all\n",
            r#"{"type":"system","subtype":"other"}"#,
            "\n",
            r#"{"type":"assistant","message":{"model":"claude-opus-5"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"model":"claude-haiku-4"}}"#,
            "\n",
        );
        assert_eq!(
            announced_in_transcript(transcript).model.as_deref(),
            Some("claude-opus-5")
        );
        assert_eq!(
            announced_in_transcript("scripted agent output\n"),
            Announced::default()
        );
    }

    #[test]
    fn describe_takes_the_version_and_model_from_the_session_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("transcript.jsonl");
        // The init event leads a transcript far longer than the scanned head.
        std::fs::write(
            &transcript,
            format!(
                "{}\n{}\n",
                r#"{"type":"system","subtype":"init","model":"claude-opus-5[1m]","claude_code_version":"2.1.252"}"#,
                "x".repeat(TRANSCRIPT_HEAD_BYTES),
            ),
        )
        .unwrap();

        let provenance = describe("/opt/agents/claude -p hello", &transcript);
        assert_eq!(provenance.backend.as_deref(), Some("claude"));
        assert_eq!(provenance.prompt.as_deref(), Some("hello"));
        assert_eq!(provenance.executable_version.as_deref(), Some("2.1.252"));
        assert_eq!(
            provenance.model_observed.as_deref(),
            Some("claude-opus-5[1m]")
        );

        let absent = describe(
            "/opt/agents/claude -p hello",
            &dir.path().join("gone.jsonl"),
        );
        assert_eq!(absent.model_observed, None);
        assert_eq!(absent.executable_version, None);
        assert_eq!(absent.backend.as_deref(), Some("claude"));
    }

    #[test]
    fn an_init_record_larger_than_the_head_budget_still_announces() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("transcript.jsonl");
        let inventory = "x".repeat(TRANSCRIPT_HEAD_BYTES + 1);
        std::fs::write(
            &transcript,
            format!(
                concat!(
                    r#"{{"type":"system","subtype":"init","model":"claude-opus-5[1m]","#,
                    r#""claude_code_version":"2.1.252","tools":"{}"}}"#,
                    "\n",
                ),
                inventory
            ),
        )
        .unwrap();

        let provenance = describe("/opt/agents/claude -p hello", &transcript);
        assert_eq!(provenance.executable_version.as_deref(), Some("2.1.252"));
        assert_eq!(
            provenance.model_observed.as_deref(),
            Some("claude-opus-5[1m]")
        );
    }

    #[test]
    fn describing_a_session_never_executes_the_agent() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("it-ran");
        let agent = dir.path().join("agent");
        std::fs::write(
            &agent,
            format!("#!/bin/sh\ntouch {}\n", marker.to_string_lossy()),
        )
        .unwrap();
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();

        let provenance = describe(
            &format!("{} -p hello", agent.to_string_lossy()),
            &dir.path().join("no-transcript.jsonl"),
        );
        assert_eq!(provenance.backend.as_deref(), Some("agent"));
        assert!(!marker.exists(), "the agent executable was run");
    }
}
