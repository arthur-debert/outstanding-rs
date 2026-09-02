// `corpus-runner batch`, end to end, against a fake `cargo` and a scripted
// agent (no network, no credential): the walking skeleton's smoke archetype
// needs neither, so it is the batch command's integration test — done when
// `corpus-runner batch smoke --out <tmp>` produces the two scorecard files.
// Runs in its own test binary because it prepends to the process-wide PATH.

#![cfg(unix)]

mod common;

use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

const SMOKE: &str = r#"cmd="$1"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
if [ "$cmd" = "about" ]; then
  case "$mode" in
    json) echo '{"name":"smoke","purpose":"a tiny fixed star catalog"}' ;;
    *) echo 'smoke — a tiny fixed star catalog' ;;
  esac
else
  case "$mode" in
    json) echo '{"stars":[{"name":"Aldebaran","constellation":"Taurus","magnitude":0.86},{"name":"Rigel","constellation":"Orion","magnitude":0.13},{"name":"Vega","constellation":"Lyra","magnitude":0.03}]}' ;;
    *) printf 'Star Catalog\nAldebaran  Taurus  0.86\nRigel      Orion   0.13\nVega       Lyra    0.03\n' ;;
  esac
fi
"#;

fn repo() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn batch_smoke_writes_both_scorecards_and_sanitized_evidence() {
    let scratch = tempfile::tempdir().unwrap();

    let bin_dir = scratch.path().join("bin");
    common::install_fake_cargo(&bin_dir, "smoke", SMOKE);
    common::questionnaire_agent(
        &bin_dir,
        "agent.sh",
        "",
        &[
            ("summary", "Implemented smoke from SPEC.md."),
            ("sources.docs", "docs/guides/minimal-single-crate.md"),
            ("sources.external", "none"),
            ("confidence", "high"),
            ("confidence_reason", "Every case passes."),
        ],
        true,
    );

    let out_dir = scratch.path().join("out");

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-runner"))
        .args([
            "batch",
            "smoke",
            "--framework-version",
            env!("CARGO_PKG_VERSION"),
            "--agent-cmd",
            "agent.sh",
            "--out",
        ])
        .arg(&out_dir)
        .current_dir(repo())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "batch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let scorecard_md = out_dir.join("scorecard.md");
    let scorecard_json = out_dir.join("scorecard.json");
    assert!(scorecard_md.is_file(), "{}", scorecard_md.display());
    assert!(scorecard_json.is_file(), "{}", scorecard_json.display());

    let markdown = std::fs::read_to_string(&scorecard_md).unwrap();
    assert!(markdown.contains("smoke"), "{markdown}");

    let rows: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&scorecard_json).unwrap()).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["archetype"], "smoke");
    assert_eq!(rows[0]["comparable"], "single run");

    // One sanitized run directory landed directly under --out, beside the
    // two scorecards and the batch's hidden scratch directory; its
    // transcript never enters the checkout.
    let run_dirs: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.is_dir() && !path.file_name().unwrap().to_string_lossy().starts_with('.')
        })
        .collect();
    assert_eq!(run_dirs.len(), 1, "{run_dirs:?}");
    let run_dir = &run_dirs[0];
    assert!(run_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("smoke-"));

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join("report.json")).unwrap())
            .unwrap();
    assert_eq!(report["archetype"]["name"], "smoke");
    assert_eq!(report["schema_version"], 5);
    assert!(report["acceptance"]["built"].as_bool().unwrap());

    let transcript_path = run_dir.join("transcript.jsonl");
    assert!(transcript_path.is_file());
    let digest = sha256_hex(&std::fs::read(&transcript_path).unwrap());
    assert_eq!(report["session"]["transcript_sha256"], digest);

    // Sanitizing scrubbed the scratch paths out of both artifacts.
    let transcript = std::fs::read_to_string(&transcript_path).unwrap();
    assert!(!transcript.contains(scratch.path().to_str().unwrap()));
    let report_text = std::fs::read_to_string(run_dir.join("report.json")).unwrap();
    assert!(!report_text.contains(scratch.path().to_str().unwrap()));

    // The scratch run directory under --out/.scratch held only what
    // sanitizing duplicated into --out; nothing survives there once the
    // run completes.
    let scratch_runs: Vec<_> = std::fs::read_dir(out_dir.join(".scratch"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(scratch_runs.is_empty(), "{scratch_runs:?}");
}

#[test]
fn batch_rejects_an_out_dir_inside_the_source_checkout() {
    let out_dir = repo()
        .join("target")
        .join("corpus-batch-out-inside-checkout-test");
    let _cleanup = RemoveOnDrop(out_dir.clone());

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-runner"))
        .args([
            "batch",
            "smoke",
            "--framework-version",
            env!("CARGO_PKG_VERSION"),
            "--agent-cmd",
            "agent.sh",
            "--out",
        ])
        .arg(&out_dir)
        .current_dir(repo())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "batch should refuse an --out inside the source checkout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("inside source checkout"), "{stderr}");
}

/// Deletes `out_dir` on drop: `batch` creates it (via `create_dir_all`)
/// before rejecting it as being inside the checkout.
struct RemoveOnDrop(std::path::PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
