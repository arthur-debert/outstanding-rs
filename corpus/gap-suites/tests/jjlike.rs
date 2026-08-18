//! `jjlike` acceptance suite — runtime-template hardening, one milestone group.
//!
//! **Owning epic: not yet minted.** No parity Spec covers user-supplied runtime
//! templates today (the existing three are PAR01 config layering, PAR02 machine
//! contract, PAR03 terminal citizenship); epic codes are human-assigned, so this suite
//! names its owner in prose — the future runtime-templates parity epic — instead of
//! inventing a code. See `corpus/archetypes/jjlike/spec.md` and
//! `corpus/gap-suites/README.md`.
//!
//! Behavior under test: the typed-function surface (`upper`, `lower`) rendering per
//! record, and templates as *untrusted input* — an unknown filter or tag must produce a
//! diagnostic naming the function/tag and byte offset rather than a panic, and a
//! template exceeding the render budget must terminate promptly rather than hang.
//! Every assertion is black-box against the binary named by `CORPUS_JJLIKE_BIN` and
//! runs with expected-fail semantics (`corpus_gap_suites::expect_gap`).

use std::path::Path;
use std::time::Duration;

use corpus_gap_suites::{expect_gap, reject_panic, run, Output};

/// Milestone group and owning epic, printed with every outcome.
const GATE: &str = "jjlike/runtime-templates -> owning parity epic not yet minted";
/// Env var locating the produced archetype binary.
const BIN: &str = "CORPUS_JJLIKE_BIN";

/// Two-record NDJSON data fixture the templates render over.
const DATA: &str = concat!(
    "{\"id\":\"a1\",\"author\":\"amy\",\"message\":\"first\"}\n",
    "{\"id\":\"b2\",\"author\":\"bob\",\"message\":\"second\"}\n",
);

/// Runs `jjlike log` over `data` with `template` and `extra_args`.
fn render_over(
    binary: &Path,
    data: &str,
    template: &str,
    extra_args: &[&str],
) -> Result<Output, String> {
    let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
    std::fs::write(dir.path().join("entries.ndjson"), data)
        .unwrap_or_else(|err| panic!("suite broken: writing fixture: {err}"));
    let mut args = vec!["log", "--data", "entries.ndjson", "-T", template];
    args.extend_from_slice(extra_args);
    run(binary, &args, dir.path())
}

/// Runs `jjlike log` over the standard data fixture with `template` and `extra_args`.
fn render(binary: &Path, template: &str, extra_args: &[&str]) -> Result<Output, String> {
    render_over(binary, DATA, template, extra_args)
}

/// Reports a mismatch when a success case leaks anything onto stderr — the spec's only
/// stderr traffic is diagnostics, and success cases have none.
fn require_clean_stderr(out: &Output) -> Result<(), String> {
    reject_panic(&out.stderr)?;
    if !out.stderr.is_empty() {
        return Err(format!(
            "a successful render must leave stderr empty, got {:?}",
            out.stderr
        ));
    }
    Ok(())
}

/// Extracts the single JSON diagnostic the spec requires on stderr.
///
/// Strict per the spec's stderr contract ("diagnostics are single-line JSON objects on
/// stderr"): *every* stderr line must parse as a JSON object — prose wrapped around a
/// valid diagnostic is a mismatch, not noise to skip — there must be exactly one line,
/// and it must carry `"severity":"error"`.
fn single_diagnostic(out: &Output) -> Result<serde_json::Value, String> {
    reject_panic(&out.stderr)?;
    let mut diagnostics = Vec::new();
    for (index, line) in out.stderr.lines().enumerate() {
        let value: serde_json::Value = serde_json::from_str(line).map_err(|_| {
            format!(
                "stderr line {} is not a JSON object (only single-line JSON diagnostics \
                 may print there): {line:?}",
                index + 1
            )
        })?;
        if !value.is_object() {
            return Err(format!(
                "stderr line {} parses as JSON but is not an object: {line:?}",
                index + 1
            ));
        }
        diagnostics.push(value);
    }
    if diagnostics.len() != 1 {
        return Err(format!(
            "expected exactly one JSON diagnostic line on stderr, found {} (stderr: {:?})",
            diagnostics.len(),
            out.stderr
        ));
    }
    let diagnostic = diagnostics.pop().expect("length checked");
    if diagnostic["severity"] != "error" {
        return Err(format!(
            "the diagnostic's severity should be \"error\", was {}",
            diagnostic["severity"]
        ));
    }
    Ok(diagnostic)
}

#[test]
fn expected_fail_well_formed_template_renders_per_record() {
    expect_gap(GATE, BIN, "no runtime-template surface exists", |binary| {
        let out = render(binary, "{{ id }}: {{ message }}", &[])?;
        require_clean_stderr(&out)?;
        if out.code != Some(0) {
            return Err(format!(
                "baseline render should exit 0, exited {:?}",
                out.code
            ));
        }
        if out.stdout != "a1: first\nb2: second\n" {
            return Err(format!("unexpected rendering: {:?}", out.stdout));
        }
        Ok(())
    });
}

#[test]
fn expected_fail_upper_filter_renders_per_record() {
    expect_gap(GATE, BIN, "no typed template functions exist", |binary| {
        let out = render(binary, "{{ message | upper }}", &[])?;
        require_clean_stderr(&out)?;
        if out.code != Some(0) {
            return Err(format!("upper render should exit 0, exited {:?}", out.code));
        }
        if out.stdout != "FIRST\nSECOND\n" {
            return Err(format!(
                "upper should uppercase each record's message, got {:?}",
                out.stdout
            ));
        }
        Ok(())
    });
}

#[test]
fn expected_fail_lower_filter_renders_per_record() {
    expect_gap(GATE, BIN, "no typed template functions exist", |binary| {
        // Uppercase source text, so an identity "filter" cannot pass by accident.
        let data = concat!(
            "{\"id\":\"c3\",\"author\":\"cal\",\"message\":\"THIRD\"}\n",
            "{\"id\":\"d4\",\"author\":\"dot\",\"message\":\"FOURTH\"}\n",
        );
        let out = render_over(binary, data, "{{ message | lower }}", &[])?;
        require_clean_stderr(&out)?;
        if out.code != Some(0) {
            return Err(format!("lower render should exit 0, exited {:?}", out.code));
        }
        if out.stdout != "third\nfourth\n" {
            return Err(format!(
                "lower should lowercase each record's message, got {:?}",
                out.stdout
            ));
        }
        Ok(())
    });
}

#[test]
fn expected_fail_unknown_filter_diagnoses_function_and_offset() {
    expect_gap(
        GATE,
        BIN,
        "unknown template functions panic or pass silently",
        |binary| {
            let out = render(binary, "{{ message | frobnicate }}", &[])?;
            if out.code != Some(1) {
                return Err(format!(
                    "unknown filter should exit 1, exited {:?}",
                    out.code
                ));
            }
            let diagnostic = single_diagnostic(&out)?;
            if diagnostic["function"] != "frobnicate" {
                return Err(format!(
                    "diagnostic should name the function, carried {}",
                    diagnostic["function"]
                ));
            }
            // Byte offset of `frobnicate` within `{{ message | frobnicate }}`.
            if diagnostic["offset"] != 13 {
                return Err(format!(
                    "diagnostic offset should be 13, was {}",
                    diagnostic["offset"]
                ));
            }
            if !out.stdout.is_empty() {
                return Err(format!(
                    "nothing may render on a bad template, got {:?}",
                    out.stdout
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_unknown_filter_offset_counts_bytes_not_chars() {
    expect_gap(
        GATE,
        BIN,
        "unknown template functions panic or pass silently",
        |binary| {
            // `→` is one char but three UTF-8 bytes, so byte and char indexing become
            // observably different: `frobnicate` sits at char 14 but byte 16, and the
            // contract fixes `offset` as a byte offset.
            let out = render(binary, "→{{ message | frobnicate }}", &[])?;
            if out.code != Some(1) {
                return Err(format!(
                    "unknown filter should exit 1, exited {:?}",
                    out.code
                ));
            }
            let diagnostic = single_diagnostic(&out)?;
            if diagnostic["function"] != "frobnicate" {
                return Err(format!(
                    "diagnostic should name the function, carried {}",
                    diagnostic["function"]
                ));
            }
            if diagnostic["offset"] != 16 {
                return Err(format!(
                    "offset must be the byte offset 16 (a char-index implementation \
                     would say 14), was {}",
                    diagnostic["offset"]
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_unknown_tag_errors_by_default_with_tag_and_offset() {
    expect_gap(
        GATE,
        BIN,
        "unknown tags have no configured behavior",
        |binary| {
            let out = render(binary, "{% frob %}X{% endfrob %}", &[])?;
            if out.code != Some(1) {
                return Err(format!("unknown tag should exit 1, exited {:?}", out.code));
            }
            let diagnostic = single_diagnostic(&out)?;
            if diagnostic["tag"] != "frob" {
                return Err(format!(
                    "diagnostic should name the tag, carried {}",
                    diagnostic["tag"]
                ));
            }
            // Byte offset of `frob` within `{% frob %}X{% endfrob %}`.
            if diagnostic["offset"] != 3 {
                return Err(format!(
                    "diagnostic offset should be 3, was {}",
                    diagnostic["offset"]
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_unknown_tag_degrades_to_inner_text_when_configured() {
    expect_gap(
        GATE,
        BIN,
        "unknown tags have no configured behavior",
        |binary| {
            let out = render(
                binary,
                "{% frob %}X{% endfrob %}",
                &["--unknown-tags", "inner"],
            )?;
            // "exit 0, no diagnostic": the configured degrade is a success case, so
            // stderr carries nothing at all.
            require_clean_stderr(&out)?;
            if out.code != Some(0) {
                return Err(format!(
                    "configured degrade should exit 0, exited {:?}",
                    out.code
                ));
            }
            if out.stdout != "X\nX\n" {
                return Err(format!(
                    "inner text should survive per record, got {:?}",
                    out.stdout
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_render_budget_exceeded_fails_promptly_instead_of_hanging() {
    expect_gap(
        GATE,
        BIN,
        "rendering has no budget and can hang",
        |binary| {
            // A hang is caught by the harness spawn timeout, and a binary that ignores
            // the budget while spraying output is caught by the harness output cap —
            // both reported as mismatches.
            let out = render(
                binary,
                "{% for i in range(1000000000) %}{{ i }}{% endfor %}",
                &["--render-budget-ms", "500"],
            )?;
            // "Promptly after the budget elapses": 5s gives the 500ms budget generous,
            // non-flaky slack for process startup and CI jitter while staying far below
            // the 30s harness timeout — a renderer that only dies near the timeout
            // (or ignores the budget entirely) cannot pass.
            if out.duration > Duration::from_secs(5) {
                return Err(format!(
                    "a 500ms budget must terminate rendering promptly, took {:?}",
                    out.duration
                ));
            }
            if out.code != Some(1) {
                return Err(format!(
                    "an exceeded budget should exit 1, exited {:?}",
                    out.code
                ));
            }
            let diagnostic = single_diagnostic(&out)?;
            let summary = diagnostic["summary"].as_str().unwrap_or_default();
            if !summary.contains("render budget exceeded") {
                return Err(format!(
                    "diagnostic summary should say the budget was exceeded, was {summary:?}"
                ));
            }
            Ok(())
        },
    );
}
