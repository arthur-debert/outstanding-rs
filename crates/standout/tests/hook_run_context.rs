use std::cell::RefCell;
use std::rc::Rc;

use clap::{ArgMatches, Command};
use clapfig::{Clapfig, SearchPath};
use serde::{Deserialize, Serialize};
use serde_json::json;
use standout::cli::{
    App, CommandContext, CommandContextInput, FnHandler, HelpResult, Output, StreamSink,
    TermSettings,
};
use standout::{ColorPolicy, EmbeddedTemplates, InputSources, Representation, TemplateRef};
use standout_test::{serial, TestHarness, TestResult};

const STOPPED: &str = "apply stopped early";
const TEMPLATES: &[(&str, &str)] = &[("apply", "{{ note }}")];

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
struct FixtureConfig {
    term: TermSettings,
}

type Seen = Rc<RefCell<Vec<(Representation, ColorPolicy)>>>;

fn apply_app(seen: Seen) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .config(
            Clapfig::typed::<FixtureConfig>()
                .app_name("hookapp")
                .search_paths(vec![SearchPath::Cwd]),
        )
        .term_settings(|config: &FixtureConfig| &config.term)
        .command_with(
            "apply",
            FnHandler::new(|_matches: &ArgMatches, _ctx: &CommandContext| {
                Ok(Output::Render(json!({ "note": STOPPED })))
            }),
            move |cfg| {
                cfg.template_name("apply").post_dispatch(
                    move |_matches: &ArgMatches, ctx: &CommandContext, data: serde_json::Value| {
                        seen.borrow_mut()
                            .push((ctx.representation(), ctx.color_policy()));
                        if ctx.representation().is_structured() {
                            ctx.warn(STOPPED);
                        }
                        Ok(data)
                    },
                )
            },
        )
        .unwrap()
        .build()
        .unwrap()
}

fn hookapp() -> Command {
    Command::new("hookapp").subcommand(Command::new("apply"))
}

fn run(
    harness: TestHarness,
    file: &str,
    args: &[&str],
) -> (Vec<(Representation, ColorPolicy)>, TestResult) {
    let seen = Seen::default();
    let app = apply_app(seen.clone());
    let result = harness
        .fixture("hookapp.toml", file.to_string())
        .run(&app, hookapp(), args);
    let observed = seen.borrow().clone();
    (observed, result)
}

#[test]
#[serial]
fn the_human_run_prints_the_sentence_once_and_pushes_no_warning() {
    let (seen, result) = run(TestHarness::new(), "", &["hookapp", "apply"]);

    result.assert_success();
    assert_eq!(seen, vec![(Representation::Human, ColorPolicy::Auto)]);
    assert_eq!(result.stdout_plain().matches(STOPPED).count(), 1);
    assert!(result.warnings().is_empty(), "{:?}", result.warnings());
}

#[test]
#[serial]
fn the_structured_run_pushes_the_sentence_as_a_warning() {
    let (seen, result) = run(
        TestHarness::new(),
        "",
        &["hookapp", "apply", "--output", "json"],
    );

    result.assert_success();
    assert_eq!(seen, vec![(Representation::Json, ColorPolicy::Auto)]);
    assert_eq!(result.warnings(), [STOPPED.to_string()]);
}

#[test]
#[serial]
fn every_representation_reaches_the_hook_as_the_run_resolved_it() {
    for (flag, expected) in [
        (None, Representation::Human),
        (Some("term-debug"), Representation::TermDebug),
        (Some("json"), Representation::Json),
        (Some("yaml"), Representation::Yaml),
        (Some("csv"), Representation::Csv),
        (Some("ndjson"), Representation::Ndjson),
    ] {
        let mut args = vec!["hookapp", "apply"];
        if let Some(flag) = flag {
            args.extend(["--output", flag]);
        }
        let (seen, result) = run(TestHarness::new(), "", &args);
        result.assert_success();
        assert_eq!(seen.first().map(|pair| pair.0), Some(expected), "{args:?}");
        assert_eq!(
            result.warnings().is_empty(),
            !expected.is_structured(),
            "{args:?}"
        );
    }
}

#[test]
#[serial]
fn the_configured_term_output_reaches_the_hook_when_no_flag_overrides_it() {
    let (configured, result) = run(
        TestHarness::new(),
        "[term]\noutput = \"json\"\n",
        &["hookapp", "apply"],
    );
    result.assert_success();
    assert_eq!(
        configured.first().map(|pair| pair.0),
        Some(Representation::Json)
    );

    let (flagged, result) = run(
        TestHarness::new(),
        "[term]\noutput = \"json\"\n",
        &["hookapp", "apply", "--output", "yaml"],
    );
    result.assert_success();
    assert_eq!(
        flagged.first().map(|pair| pair.0),
        Some(Representation::Yaml)
    );
}

#[test]
#[serial]
fn the_color_policy_the_hook_reads_is_the_one_the_run_resolved() {
    for (file, args, programmatic, expected) in [
        (
            "",
            vec!["hookapp", "apply"],
            ColorPolicy::Auto,
            ColorPolicy::Auto,
        ),
        (
            "",
            vec!["hookapp", "apply"],
            ColorPolicy::Never,
            ColorPolicy::Never,
        ),
        (
            "[term]\ncolor = \"never\"\n",
            vec!["hookapp", "apply"],
            ColorPolicy::Auto,
            ColorPolicy::Never,
        ),
        (
            "[term]\ncolor = \"never\"\n",
            vec!["hookapp", "apply", "--color", "always"],
            ColorPolicy::Auto,
            ColorPolicy::Always,
        ),
    ] {
        let (seen, result) = run(TestHarness::new().color(programmatic), file, &args);
        result.assert_success();
        assert_eq!(
            seen.first().map(|pair| pair.1),
            Some(expected),
            "{args:?} with {programmatic:?} and {file:?}"
        );
    }
}

struct Cwd(std::path::PathBuf);

impl Cwd {
    fn enter(dir: &std::path::Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(previous)
    }
}

impl Drop for Cwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
#[serial]
fn run_command_gives_the_hook_the_same_resolved_pair() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("hookapp.toml"),
        "[term]\noutput = \"json\"\n",
    )
    .unwrap();
    let _cwd = Cwd::enter(dir.path());

    let seen = Seen::default();
    let app = apply_app(seen.clone());
    let matches = match app.get_matches_from(
        hookapp(),
        ["hookapp", "apply"],
        &InputSources::from_process(),
    ) {
        HelpResult::Matches(matches) => matches,
        other => panic!("{other:?}"),
    };
    let sub = matches.subcommand_matches("apply").unwrap();

    app.run_command(
        "apply",
        sub,
        FnHandler::new(|_matches: &ArgMatches, _ctx: &CommandContext| {
            Ok(Output::Render(json!({ "note": STOPPED })))
        }),
        TemplateRef::Inline("{{ note }}".to_string()),
        ColorPolicy::Never,
        StreamSink::new(Vec::new()),
    )
    .unwrap();

    assert_eq!(
        seen.borrow().clone(),
        vec![(Representation::Json, ColorPolicy::Never)]
    );
}

#[test]
fn a_context_built_without_a_run_reports_the_defaults() {
    let ctx = CommandContext::default();
    assert_eq!(ctx.representation(), Representation::Human);
    assert_eq!(ctx.color_policy(), ColorPolicy::Auto);
}
