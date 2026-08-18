//! `jjlike` acceptance suite — runtime-template hardening, one milestone group.
//!
//! **Owning epic: not yet minted.** No parity Spec covers user-supplied runtime
//! templates today (the existing three are PAR01 config layering, PAR02 machine
//! contract, PAR03 terminal citizenship); epic codes are human-assigned, so this suite
//! names its owner in prose — the future runtime-templates parity epic — instead of
//! inventing a code. See `corpus/archetypes/jjlike/SPEC.md` and
//! `corpus/gap-suites/README.md`.
//!
//! Behavior under test: templates as *untrusted input* — an unknown filter or tag must
//! produce a diagnostic naming the function/tag and byte offset rather than a panic,
//! and a template exceeding the render budget must fail rather than hang. Every
//! assertion is black-box against the binary named by `CORPUS_JJLIKE_BIN` and runs
//! with expected-fail semantics (`corpus_gap_suites::expect_gap`).

use std::path::Path;

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

/// Runs `jjlike log` over the standard data fixture with `template` and `extra_args`.
fn render(binary: &Path, template: &str, extra_args: &[&str]) -> Result<Output, String> {
    let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
    std::fs::write(dir.path().join("entries.ndjson"), DATA)
        .unwrap_or_else(|err| panic!("suite broken: writing fixture: {err}"));
    let mut args = vec!["log", "--data", "entries.ndjson", "-T", template];
    args.extend_from_slice(extra_args);
    run(binary, &args, dir.path())
}

/// Extracts the single JSON diagnostic the spec requires on stderr, reporting a
/// mismatch when there is none, more than one, or panic output instead.
fn single_diagnostic(out: &Output) -> Result<serde_json::Value, String> {
    reject_panic(&out.stderr)?;
    let diagnostics: Vec<serde_json::Value> = out
        .stderr
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|value: &serde_json::Value| value["severity"] == "error")
        .collect();
    match diagnostics.len() {
        1 => Ok(diagnostics.into_iter().next().expect("length checked")),
        n => Err(format!(
            "expected exactly one JSON error diagnostic on stderr, found {n} (stderr: {:?})",
            out.stderr
        )),
    }
}

#[test]
fn expected_fail_well_formed_template_renders_per_record() {
    expect_gap(GATE, BIN, "no runtime-template surface exists", |binary| {
        let out = render(binary, "{{ id }}: {{ message }}", &[])?;
        reject_panic(&out.stderr)?;
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
            reject_panic(&out.stderr)?;
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
            // A hang is caught by the harness spawn timeout and reported as a mismatch.
            let out = render(
                binary,
                "{% for i in range(1000000000) %}{{ i }}{% endfor %}",
                &["--render-budget-ms", "500"],
            )?;
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
