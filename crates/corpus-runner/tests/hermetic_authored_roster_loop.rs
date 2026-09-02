// The six completion archetypes, driven through the full `run()` orchestration
// against a produced binary that fails every invocation. Runs in its own test
// binary because it prepends to the process-wide PATH.

#![cfg(unix)]

mod common;

use std::path::Path;

use corpus_runner::archetype::Archetype;
use corpus_runner::report::{CaseOutcome, InvariantStatus};
use corpus_runner::{run, RunConfig, Timeouts};

const AUTHORED: &[&str] = &[
    "brewlike",
    "cargolike",
    "dockerlike",
    "gcloudlike",
    "kubelike",
    "pnpmlike",
];

#[test]
fn authored_archetypes_complete_the_loop_against_a_failing_binary() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let archetypes_dir = repo.join("corpus/archetypes");
    let scratch = tempfile::tempdir().unwrap();

    for name in AUTHORED {
        let bin_dir = scratch.path().join(format!("bin-{name}"));
        let trivially_failing = format!("echo \"{name}: not implemented\" >&2\nexit 1");
        common::install_fake_cargo(&bin_dir, name, &trivially_failing);
        common::questionnaire_agent(
            &bin_dir,
            "agent.sh",
            "",
            &[
                ("summary", "Left the scaffold as generated."),
                ("sources.docs", "docs/index.md"),
                ("sources.external", "none"),
                ("confidence", "low"),
                ("confidence_reason", "The scaffold does not build."),
            ],
            true,
        );

        let config = RunConfig {
            archetype: (*name).to_string(),
            archetypes_dir: archetypes_dir.clone(),
            runs_dir: scratch.path().join(format!("runs-{name}")),
            docs_dir: repo.join("docs"),
            agent_cmd: "agent.sh".to_string(),
            broker: None,
            framework_version: env!("CARGO_PKG_VERSION").to_string(),
            timeouts: Timeouts::default(),
        };

        let (report, run_dir) = run(&config).unwrap();

        assert!(run_dir.join("report.json").is_file(), "{name}");
        assert!(
            report.acceptance.built,
            "{name}: {:?}",
            report.acceptance.build_detail
        );

        let archetype = Archetype::load(&archetypes_dir, name).unwrap();
        assert_eq!(
            report.acceptance.cases.len(),
            archetype.suite.cases.len(),
            "{name}: every authored case is reported"
        );
        for case in &report.acceptance.cases {
            let detail = case.detail.as_deref().unwrap_or_default();
            assert!(
                !detail.contains("case execution error"),
                "{name}/{}: {detail}",
                case.name
            );
            let expected_outcome = match case.expected.as_str() {
                "fail" => CaseOutcome::ExpectedFail,
                _ => CaseOutcome::Fail,
            };
            assert_eq!(
                case.outcome, expected_outcome,
                "{name}/{}: {detail}",
                case.name
            );
        }
        assert!(!report.invariants.is_empty(), "{name}");
        for cell in &report.invariants {
            assert_ne!(
                cell.status,
                InvariantStatus::NotRun,
                "{name}/{} {}/{}: {}",
                cell.command,
                cell.mode,
                cell.check,
                cell.detail.as_deref().unwrap_or_default()
            );
        }
    }
}
