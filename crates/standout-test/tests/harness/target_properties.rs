use super::*;

#[derive(Clone, serde::Serialize)]
struct WidthSensitiveItem {
    name: &'static str,
}
fn build_framework_list_view_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_matches, _ctx| {
                let spec = standout::tabular::TabularSpec::builder()
                    .column(Column::new(Width::Fill).right().key("name"))
                    .build();
                Ok(Output::Render(
                    list_view(vec![WidthSensitiveItem { name: "cascade" }])
                        .tabular_spec(spec)
                        .build(),
                ))
            }),
            |config| config.template_name("standout/list-view"),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn list_command() -> Command {
    Command::new("app").subcommand(Command::new("list"))
}
#[test]
#[serial]
fn ambiguous_width_policy_can_be_injected_for_the_same_app_fixture() {
    let app = build_echo_app("echo-width");
    let narrow = TestHarness::new()
        .ambiguous_width(AmbiguousWidth::Narrow)
        .run(&app, echo_command(), ["app", "echo", "↦≈Δ"]);
    narrow.assert_stdout_eq("3");
    drop(narrow);
    let wide = TestHarness::new()
        .ambiguous_width(AmbiguousWidth::Wide)
        .run(&app, echo_command(), ["app", "echo", "↦≈Δ"]);
    wide.assert_stdout_eq("5");
}
#[test]
#[serial]
fn terminal_width_cascades_through_the_framework_list_view_template() {
    let app = build_framework_list_view_app();
    for width in [31, 37, 47] {
        let result =
            TestHarness::new()
                .terminal_width(width)
                .run(&app, list_command(), ["app", "list"]);
        result.assert_success();
        let row = result
            .stdout()
            .lines()
            .find(|line| line.contains("cascade"))
            .expect("framework list view should render its tabular row");
        assert_eq!(row.chars().count(), width);
        drop(result);
    }
}
#[test]
#[serial]
fn terminal_width_places_right_aligned_field_at_the_right_edge() {
    let app = build_framework_list_view_app();
    let field = "cascade";
    for width in [80, 120] {
        let result =
            TestHarness::new()
                .terminal_width(width)
                .run(&app, list_command(), ["app", "list"]);
        result.assert_success();
        let row = result
            .stdout()
            .lines()
            .find(|line| line.contains(field))
            .expect("framework list view should render its right-aligned field");
        assert_eq!(row.chars().count(), width);
        assert_eq!(row.find(field), Some(width - field.len()));
        assert!(row.ends_with(field));
        drop(result);
    }
}
#[test]
#[serial]
fn unknown_terminal_width_uses_the_framework_list_view_fallback() {
    let app = build_framework_list_view_app();
    let result = TestHarness::new()
        .no_terminal_width()
        .run(&app, list_command(), ["app", "list"]);
    result.assert_success();
    let row = result
        .stdout()
        .lines()
        .find(|line| line.contains("cascade"))
        .expect("framework list view should render its tabular row");
    assert_eq!(row.chars().count(), 80);
}
fn build_detectable_facts_app() -> App {
    let theme = Theme::new()
        .add_icon("mark", IconDefinition::new("CLASSIC").with_nerdfont("NERD"))
        .add_adaptive(
            "tone",
            Style::new(),
            Some(Style::new().green()),
            Some(Style::new().red()),
        );
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "list",
            FnHandler::new(|_matches, _ctx| {
                let spec = standout::tabular::TabularSpec::builder()
                    .column(Column::new(Width::Fill).right().key("name"))
                    .build();
                Ok(Output::Render(
                    list_view(vec![WidthSensitiveItem { name: "cascade" }])
                        .tabular_spec(spec)
                        .build(),
                ))
            }),
            |config| config.template_name("standout/list-view"),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn detectable_command() -> Command {
    Command::new("app")
        .subcommand(Command::new("say"))
        .subcommand(Command::new("list"))
}
#[test]
#[serial]
fn harness_run_is_independent_of_detected_process_facts() {
    let app = build_detectable_facts_app();
    let cmd = detectable_command();
    let baseline = || TestHarness::new().color_capable_terminal();
    let perturb = || {
        baseline()
            .env("COLUMNS", "37")
            .env("NERD_FONT", "1")
            .env("GTK_THEME", "Adwaita:light")
            .env("COLORFGBG", "0;15")
    };
    let (say_default, say_default_plain) = {
        let result = baseline().run(&app, cmd.clone(), ["app", "say"]);
        result.assert_success();
        (
            result.stdout().to_string(),
            result.stdout_plain().to_string(),
        )
    };
    let list_default = {
        let result = baseline().run(&app, cmd.clone(), ["app", "list"]);
        result.assert_success();
        result.stdout().to_string()
    };
    let say_perturbed = {
        let result = perturb().run(&app, cmd.clone(), ["app", "say"]);
        result.assert_success();
        result.stdout().to_string()
    };
    let list_perturbed = {
        let result = perturb().run(&app, cmd.clone(), ["app", "list"]);
        result.assert_success();
        result.stdout().to_string()
    };
    assert_eq!(say_default, say_perturbed);
    assert_eq!(list_default, list_perturbed);
    assert!(
        say_default_plain.contains("CLASSIC"),
        "unset icon_mode is Classic, got {say_default_plain:?}"
    );
    assert!(
        !say_default_plain.contains("NERD"),
        "NERD_FONT must not select the nerd variant: {say_default_plain:?}"
    );
    let row = list_default
        .lines()
        .find(|line| line.contains("cascade"))
        .expect("framework list view should render its tabular row");
    assert_eq!(
        row.chars().count(),
        80,
        "unset width is None, list-view fallback 80; got {row:?}"
    );
    let say_dark = {
        let result =
            baseline()
                .color_scheme(ColorMode::Dark)
                .run(&app, cmd.clone(), ["app", "say"]);
        result.stdout().to_string()
    };
    let say_light = {
        let result =
            baseline()
                .color_scheme(ColorMode::Light)
                .run(&app, cmd.clone(), ["app", "say"]);
        result.stdout().to_string()
    };
    assert_eq!(
        say_default, say_dark,
        "unset color_scheme is ColorMode::Dark"
    );
    assert_ne!(
        say_default, say_light,
        "Light vs Dark must be visible so scheme independence is meaningful"
    );
    let say_nerd = {
        let result =
            baseline()
                .icon_mode(IconMode::NerdFont)
                .run(&app, cmd.clone(), ["app", "say"]);
        result.stdout().to_string()
    };
    assert_ne!(
        say_default, say_nerd,
        "Classic vs NerdFont must be visible so NERD_FONT independence is meaningful"
    );
}
#[test]
#[serial]
fn rendering_pairs_match_the_separate_representation_and_color_setters() {
    let app = build_detectable_facts_app();
    let cmd = detectable_command();
    let pairs = [
        (Representation::Human, ColorPolicy::Never),
        (Representation::Human, ColorPolicy::Always),
        (Representation::TermDebug, ColorPolicy::Never),
        (Representation::Json, ColorPolicy::Never),
    ];
    let mut rendered: Vec<String> = Vec::new();
    for &(representation, color) in &pairs {
        let paired = TestHarness::new().rendering(representation, color).run(
            &app,
            cmd.clone(),
            ["app", "say"],
        );
        let separate = TestHarness::new()
            .output_mode(representation)
            .color(color)
            .run(&app, cmd.clone(), ["app", "say"]);
        paired.assert_success();
        separate.assert_success();
        assert_eq!(
            paired.output_mode(),
            separate.output_mode(),
            "{representation:?}/{color:?}"
        );
        assert_eq!(
            paired.stdout(),
            separate.stdout(),
            "{representation:?}/{color:?}"
        );
        rendered.push(paired.stdout().to_string());
    }
    for (i, left) in rendered.iter().enumerate() {
        for (j, right) in rendered.iter().enumerate().skip(i + 1) {
            assert_ne!(
                left, right,
                "{:?} and {:?} must render differently or the pairs prove nothing",
                pairs[i], pairs[j]
            );
        }
    }
}
#[test]
#[serial]
fn terminal_width_override_does_not_install_a_detector() {
    let app = build_echo_app("echo");
    let result = TestHarness::new()
        .terminal_width(42)
        .stdout_is_terminal(false)
        .run(&app, echo_command(), vec!["app", "echo", "hi"]);
    result.assert_stdout_eq("hi");
}
