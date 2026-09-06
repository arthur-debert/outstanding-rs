use clap::Subcommand;
use serde_json::json;
use standout::cli::{App, CommandContext, Dispatch, GroupBuilder, HandlerResult, Output};
use standout::ColorPolicy;
use standout::{EmbeddedSource, Representation, TemplateResource};
use standout_test::TestHarness;

mod handlers {
    use super::*;
    use clap::ArgMatches;

    pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn add(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn show_all(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn export(
        _matches: &ArgMatches,
        _ctx: &CommandContext,
    ) -> HandlerResult<serde_json::Value> {
        Ok(Output::Render(json!({"name": "Ada"})))
    }

    pub fn download(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Binary {
            data: vec![1, 2, 3],
            filename: "data.bin".to_string(),
        })
    }

    macro_rules! silent_handlers {
        ($($name:ident),* $(,)?) => {
            $(
                pub fn $name(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
                    Ok(Output::Silent)
                }
            )*
        };
    }

    silent_handlers!(
        x2fa,
        a1b2,
        sha256_sum,
        utf8_check,
        http_server,
        list_units,
        r#move
    );
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum BasicCommands {
    List,
    Add,
}

#[test]
fn test_basic_dispatch_compiles() {
    let config: fn(GroupBuilder) -> GroupBuilder =
        |builder| BasicCommands::dispatch_config()(builder);
    let _ = config;
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum MultiWordCommands {
    ShowAll,
}

#[test]
fn multi_word_variant_registers_the_kebab_case_name() {
    let builder = MultiWordCommands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("show-all"));
    assert!(!builder.contains("show_all"));
}

/// Digit/acronym runs and raw identifiers are where a hand-rolled splitter and `heck` part ways.
#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum ParityCommands {
    X2FA,
    A1B2,
    Sha256Sum,
    Utf8Check,
    HTTPServer,
    ListUnits,
    r#Move,
}

#[test]
fn derived_names_match_the_ones_clap_registers() {
    let clap_names: Vec<String> = ParityCommands::augment_subcommands(clap::Command::new("app"))
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();
    assert_eq!(
        clap_names,
        [
            "x2fa",
            "a1b2",
            "sha256-sum",
            "utf8-check",
            "http-server",
            "list-units",
            "move"
        ]
    );

    let builder = ParityCommands::dispatch_config()(GroupBuilder::new());
    for name in &clap_names {
        assert!(builder.contains(name), "dispatch did not register `{name}`");
    }
}

/// Every `#[dispatch(...)]` on a variant applies, not only the first.
#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SplitAttrCommands {
    #[dispatch(name = "listing")]
    #[dispatch(default)]
    ListUnits,
}

#[test]
fn attributes_spread_over_several_dispatch_attrs_all_apply() {
    let builder = SplitAttrCommands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("listing"));
    assert!(!builder.contains("list-units"));
    assert_eq!(builder.get_default_command(), Some("listing"));
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum RenamedCommands {
    #[dispatch(name = "ls", handler = handlers::list)]
    List,
    #[dispatch(name = "show", default)]
    ShowAll,
}

#[test]
fn variant_rename_replaces_the_derived_name() {
    let builder = RenamedCommands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("ls"));
    assert!(builder.contains("show"));
    assert!(!builder.contains("list"));
    assert!(!builder.contains("show-all"));
    assert_eq!(builder.get_default_command(), Some("show"));
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum OverrideCommands {
    #[dispatch(handler = handlers::list)]
    Custom,
}

#[test]
fn test_handler_override_compiles() {
    let _ = OverrideCommands::dispatch_config();
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TemplateCommands {
    #[dispatch(template_name = "custom.j2")]
    List,
}

#[test]
fn test_template_override_compiles() {
    let _ = TemplateCommands::dispatch_config();
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TemplateAbsenceCommands {
    #[dispatch(template_name = "list")]
    Export,
    #[dispatch(silent)]
    Add,
    #[dispatch(binary, handler = handlers::download)]
    Download,
    #[dispatch(structured_only, handler = handlers::export)]
    ShowAll,
}

#[test]
fn test_template_absence_attributes_build_mixed_apps() {
    let app = App::builder()
        .templates(EmbeddedSource::<TemplateResource>::new(
            &[("list.jinja", "Hello {{ name }}")],
            "/path/that/does/not/exist",
        ))
        .commands(TemplateAbsenceCommands::dispatch_config())
        .unwrap()
        .build()
        .unwrap();
    let command = || {
        clap::Command::new("app")
            .subcommand(clap::Command::new("export"))
            .subcommand(clap::Command::new("add"))
            .subcommand(clap::Command::new("download"))
            .subcommand(clap::Command::new("show-all"))
    };

    let rendered =
        TestHarness::new()
            .color(ColorPolicy::Never)
            .run(&app, command(), ["app", "export"]);
    rendered.assert_success();
    assert_eq!(rendered.stdout(), "Hello Ada");

    let silent = TestHarness::new().run(&app, command(), ["app", "add"]);
    silent.assert_success();
    assert_eq!(silent.stdout(), "");

    let binary = TestHarness::new().run(&app, command(), ["app", "download"]);
    binary.assert_success();
    assert_eq!(binary.binary(), Some((&[1, 2, 3][..], "data.bin")));

    let structured = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        command(),
        ["app", "show-all"],
    );
    structured.assert_success();
    assert_eq!(structured.stdout(), "{\n  \"name\": \"Ada\"\n}");
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SkipCommands {
    List,
    #[dispatch(skip)]
    Hidden,
}

#[test]
fn test_skip_attribute_compiles() {
    let _ = SkipCommands::dispatch_config();
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum DefaultCommands {
    #[dispatch(default)]
    List,
    Add,
}

#[test]
fn test_default_command_compiles() {
    let _ = DefaultCommands::dispatch_config();
}

#[test]
fn test_default_command_sets_default() {
    let builder = DefaultCommands::dispatch_config()(GroupBuilder::new());
    assert_eq!(builder.get_default_command(), Some("list"));
}

#[test]
fn test_default_command_registers_commands() {
    let builder = DefaultCommands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("list"));
    assert!(builder.contains("add"));
}

mod input_handlers {
    use super::*;
    use clap::ArgMatches;
    use standout::cli::{CommandConfig, CommandContextInput};
    use standout::input::{ArgSource, InputChain, StdinSource};

    pub fn note_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H> {
        config.input(
            "note",
            InputChain::<String>::new()
                .try_source(ArgSource::new("note"))
                .try_source(StdinSource::new())
                .validate(
                    |note: &String| !note.trim().is_empty(),
                    "note cannot be empty",
                ),
        )
    }

    pub fn write(_matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
        let note: &String = ctx.input("note").expect("note is resolved before dispatch");
        Ok(Output::Render(json!({ "note": note })))
    }
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = input_handlers)]
enum InputCommands {
    #[dispatch(inputs = input_handlers::note_inputs, structured_only)]
    Write { note: Option<String> },
}

fn input_command() -> clap::Command {
    clap::Command::new("app")
        .subcommand(clap::Command::new("write").arg(clap::Arg::new("note").long("note")))
}

#[test]
fn inputs_attribute_resolves_a_chain_for_a_derive_registered_command() {
    let app = App::builder()
        .commands(InputCommands::dispatch_config())
        .unwrap()
        .build()
        .unwrap();

    let from_arg = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        input_command(),
        ["app", "write", "--note", "from the argument"],
    );
    from_arg.assert_success();
    assert!(from_arg.stdout().contains("from the argument"));

    let from_stdin = TestHarness::new()
        .output_mode(Representation::Json)
        .piped_stdin("from stdin\n")
        .run(&app, input_command(), ["app", "write"]);
    from_stdin.assert_success();
    assert!(from_stdin.stdout().contains("from stdin"));

    let rejected = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        input_command(),
        ["app", "write", "--note", "   "],
    );
    rejected.assert_error_contains("note cannot be empty");
}

mod hook_handlers {
    use super::*;
    use clap::ArgMatches;
    use standout::cli::hooks::HookError;
    use std::cell::RefCell;

    thread_local! {
        static ORDER: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    fn record(step: &'static str) {
        ORDER.with(|order| order.borrow_mut().push(step));
    }

    pub fn reset() {
        ORDER.with(|order| order.borrow_mut().clear());
    }

    pub fn steps() -> Vec<&'static str> {
        ORDER.with(|order| order.borrow().clone())
    }

    pub fn first(_matches: &ArgMatches, _ctx: &mut CommandContext) -> Result<(), HookError> {
        record("first");
        Ok(())
    }

    pub fn second(_matches: &ArgMatches, _ctx: &mut CommandContext) -> Result<(), HookError> {
        record("second");
        Ok(())
    }

    pub fn chained(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        record("handler");
        Ok(Output::Silent)
    }

    pub fn single(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        record("handler");
        Ok(Output::Silent)
    }
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = hook_handlers)]
enum HookCommands {
    #[dispatch(pre_dispatch(hook_handlers::first, hook_handlers::second), silent)]
    Chained,
    #[dispatch(pre_dispatch = hook_handlers::first, silent)]
    Single,
}

fn hook_command() -> clap::Command {
    clap::Command::new("app")
        .subcommand(clap::Command::new("chained"))
        .subcommand(clap::Command::new("single"))
}

fn hook_app() -> App {
    App::builder()
        .commands(HookCommands::dispatch_config())
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn a_pre_dispatch_list_runs_every_hook_in_written_order() {
    let app = hook_app();
    hook_handlers::reset();
    TestHarness::new()
        .run(&app, hook_command(), ["app", "chained"])
        .assert_success();
    assert_eq!(hook_handlers::steps(), ["first", "second", "handler"]);
}

#[test]
fn the_single_path_pre_dispatch_spelling_registers_that_one_hook() {
    let app = hook_app();
    hook_handlers::reset();
    TestHarness::new()
        .run(&app, hook_command(), ["app", "single"])
        .assert_success();
    assert_eq!(hook_handlers::steps(), ["first", "handler"]);
}
