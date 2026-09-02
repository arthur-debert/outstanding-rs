//! `jjlike` acceptance suite: runtime-template hardening, black-box against the
//! binary named by `CORPUS_JJLIKE_BIN`. Behavior under test is
//! `corpus/archetypes/jjlike/spec.md`; the gate and its owner are recorded in
//! `gaps.toml`.

use std::path::Path;
use std::time::Duration;

use corpus_gap_suites::{expect_gap, reject_panic, run, Output};

const GATE: &str = "jjlike/runtime-templates -> owning parity epic not yet minted";
const BIN: &str = "CORPUS_JJLIKE_BIN";

const DATA: &str = concat!(
    "{\"id\":\"a1\",\"author\":\"amy\",\"message\":\"first\"}\n",
    "{\"id\":\"b2\",\"author\":\"bob\",\"message\":\"second\"}\n",
);

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

fn render(binary: &Path, template: &str, extra_args: &[&str]) -> Result<Output, String> {
    render_over(binary, DATA, template, extra_args)
}

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

/// Every stderr line must be a JSON object, there must be exactly one, and it must be an error.
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
            // `→` is one char but three bytes: `frobnicate` sits at char 14 but byte 16.
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
            let out = render(
                binary,
                "{% for i in range(1000000000) %}{{ i }}{% endfor %}",
                &["--render-budget-ms", "500"],
            )?;
            // 5s: slack over the 500ms budget for startup and CI jitter, far below the
            // harness timeout.
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
