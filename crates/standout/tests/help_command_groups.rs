use clap::Command;
use standout::cli::{
    render_help, validate_command_groups, App, CommandGroup, HelpConfig, HelpResult,
};
use standout::OutputMode;

#[test]
fn test_grouped_help_renders_titles() {
    let cmd = Command::new("myapp")
        .about("My application")
        .subcommand(Command::new("init").about("Initialize"))
        .subcommand(Command::new("list").about("List items"))
        .subcommand(Command::new("delete").about("Delete items"))
        .subcommand(Command::new("config").about("Configuration"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        command_groups: Some(vec![
            CommandGroup {
                title: "Commands".into(),
                help: None,
                commands: vec![Some("init".into()), Some("list".into())],
            },
            CommandGroup {
                title: "Danger Zone".into(),
                help: Some("These commands are destructive.".into()),
                commands: vec![Some("delete".into())],
            },
        ]),
        ..Default::default()
    };

    let output = render_help(&cmd, Some(config)).unwrap();

    // Group titles appear uppercased
    assert!(output.contains("COMMANDS"), "output:\n{output}");
    assert!(output.contains("DANGER ZONE"), "output:\n{output}");

    // Group help text renders
    assert!(
        output.contains("These commands are destructive."),
        "output:\n{output}"
    );

    // Ungrouped command auto-appended to "Other"
    assert!(output.contains("OTHER"), "output:\n{output}");
    assert!(output.contains("config"), "output:\n{output}");

    // Commands appear in the right order
    assert!(output.contains("init"), "output:\n{output}");
    assert!(output.contains("list"), "output:\n{output}");
    assert!(output.contains("delete"), "output:\n{output}");
}

#[test]
fn test_separators_produce_blank_lines() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("open").about("Open a pad"))
        .subcommand(Command::new("view").about("View pads"))
        .subcommand(Command::new("pin").about("Pin pads"))
        .subcommand(Command::new("unpin").about("Unpin pads"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        command_groups: Some(vec![CommandGroup {
            title: "Per Pad".into(),
            help: None,
            commands: vec![
                Some("open".into()),
                Some("view".into()),
                None, // separator
                Some("pin".into()),
                Some("unpin".into()),
            ],
        }]),
        ..Default::default()
    };

    let output = render_help(&cmd, Some(config)).unwrap();

    // All commands appear
    assert!(output.contains("open"), "output:\n{output}");
    assert!(output.contains("view"), "output:\n{output}");
    assert!(output.contains("pin"), "output:\n{output}");
    assert!(output.contains("unpin"), "output:\n{output}");

    // The separator produces a blank line between "view" line and "pin" line
    let lines: Vec<&str> = output.lines().collect();
    // Match the row's own name at the start of the cell — "pin" is a substring
    // of "unpin", which a bare `contains` would find first.
    let row = |name: &str| {
        lines
            .iter()
            .position(|l| l.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("no row for {name} in:\n{output}"))
    };
    let view_idx = row("view");
    let pin_idx = row("pin");
    // There should be a blank line between them
    assert!(
        pin_idx > view_idx + 1,
        "Expected blank line separator between view and pin, lines:\n{}",
        lines[view_idx..=pin_idx].join("\n")
    );
    let between_line = lines[view_idx + 1];
    assert!(
        between_line.trim().is_empty(),
        "Expected empty line between view and pin, got: {:?}",
        between_line
    );
}

#[test]
fn test_no_groups_backward_compat() {
    let cmd = Command::new("myapp")
        .about("My app")
        .subcommand(Command::new("foo").about("Foo cmd"))
        .subcommand(Command::new("bar").about("Bar cmd"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        ..Default::default()
    };

    let output = render_help(&cmd, Some(config)).unwrap();

    // Default "COMMANDS" header
    assert!(output.contains("COMMANDS"), "output:\n{output}");
    assert!(output.contains("foo"), "output:\n{output}");
    assert!(output.contains("bar"), "output:\n{output}");

    // No "OTHER" group when no groups are configured
    assert!(!output.contains("OTHER"), "output:\n{output}");
}

#[test]
fn test_all_grouped_no_other_section() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("a").about("A cmd"))
        .subcommand(Command::new("b").about("B cmd"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        command_groups: Some(vec![CommandGroup {
            title: "Everything".into(),
            help: None,
            commands: vec![Some("a".into()), Some("b".into())],
        }]),
        ..Default::default()
    };

    let output = render_help(&cmd, Some(config)).unwrap();

    assert!(output.contains("EVERYTHING"), "output:\n{output}");
    assert!(!output.contains("OTHER"), "output:\n{output}");
}

#[test]
fn test_validate_command_groups_passes_for_valid() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("init"))
        .subcommand(Command::new("list"));

    let groups = vec![CommandGroup {
        title: "Main".into(),
        help: None,
        commands: vec![Some("init".into()), Some("list".into())],
    }];

    assert!(validate_command_groups(&cmd, &groups).is_ok());
}

#[test]
fn test_validate_command_groups_fails_for_phantom() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("init"))
        .subcommand(Command::new("list"));

    let groups = vec![CommandGroup {
        title: "Main".into(),
        help: None,
        commands: vec![Some("init".into()), Some("typo".into())],
    }];

    let err = validate_command_groups(&cmd, &groups).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("typo"), "error: {msg}");
    assert!(msg.contains("does not exist"), "error: {msg}");
}

#[test]
fn test_multiple_groups_preserve_order() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("z_last").about("Last"))
        .subcommand(Command::new("a_first").about("First"))
        .subcommand(Command::new("m_middle").about("Middle"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        command_groups: Some(vec![
            CommandGroup {
                title: "Alpha".into(),
                help: None,
                commands: vec![Some("a_first".into())],
            },
            CommandGroup {
                title: "Zeta".into(),
                help: None,
                commands: vec![Some("z_last".into())],
            },
        ]),
        ..Default::default()
    };

    let output = render_help(&cmd, Some(config)).unwrap();

    // Alpha group appears before Zeta group
    let alpha_pos = output.find("ALPHA").unwrap();
    let zeta_pos = output.find("ZETA").unwrap();
    assert!(alpha_pos < zeta_pos, "output:\n{output}");

    // Ungrouped m_middle goes to Other
    let other_pos = output.find("OTHER").unwrap();
    assert!(zeta_pos < other_pos, "output:\n{output}");
    assert!(output.contains("m_middle"), "output:\n{output}");
}

#[test]
fn test_group_help_text_renders_below_title() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("view").about("View pads"))
        .subcommand(Command::new("edit").about("Edit pads"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        command_groups: Some(vec![CommandGroup {
            title: "Per Pad".into(),
            help: Some(
                "These commands accept one or more pad ids: <id> or ranges <id>-<id>".into(),
            ),
            commands: vec![Some("view".into()), Some("edit".into())],
        }]),
        ..Default::default()
    };

    let output = render_help(&cmd, Some(config)).unwrap();

    // Help text appears between title and first command
    let title_pos = output.find("PER PAD").unwrap();
    let help_pos = output.find("These commands accept").unwrap();
    let first_cmd_pos = output.find("  view").unwrap();

    assert!(
        title_pos < help_pos && help_pos < first_cmd_pos,
        "output:\n{output}"
    );
}

// =========================================================================
// Help handling opt-in and uniform interception tests
// =========================================================================

/// Helper: build an App with help_handling enabled and command groups.
fn app_with_groups() -> App {
    App::builder()
        .help_handling(true)
        .command_groups(vec![
            CommandGroup {
                title: "Core".into(),
                help: None,
                commands: vec![Some("status".into()), Some("list".into())],
            },
            CommandGroup {
                title: "Misc".into(),
                help: None,
                commands: vec![Some("help".into())],
            },
        ])
        .build()
        .unwrap()
}

fn test_cmd() -> Command {
    Command::new("myapp")
        .about("Test app")
        .subcommand(Command::new("status").about("Show status"))
        .subcommand(Command::new("list").about("List items"))
}

fn extract_help(result: HelpResult) -> String {
    match result {
        HelpResult::Help(h) => h,
        HelpResult::PagedHelp(h) => h,
        other => panic!("Expected Help, got: {other:?}"),
    }
}

#[test]
fn test_help_subcommand_renders_grouped() {
    let app = app_with_groups();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "help"]);
    let output = extract_help(result);
    assert!(output.contains("CORE"), "output:\n{output}");
    assert!(output.contains("status"), "output:\n{output}");
}

#[test]
fn test_help_flag_renders_grouped() {
    let app = app_with_groups();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "--help"]);
    let output = extract_help(result);
    assert!(output.contains("CORE"), "output:\n{output}");
    assert!(output.contains("status"), "output:\n{output}");
}

#[test]
fn test_help_short_flag_renders_grouped() {
    let app = app_with_groups();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "-h"]);
    let output = extract_help(result);
    assert!(output.contains("CORE"), "output:\n{output}");
    assert!(output.contains("status"), "output:\n{output}");
}

#[test]
fn test_all_help_forms_produce_same_output() {
    let cmd_factory = || test_cmd();

    let app = app_with_groups();
    let help_sub = extract_help(app.get_matches_from(cmd_factory(), ["myapp", "help"]));

    let app = app_with_groups();
    let help_long = extract_help(app.get_matches_from(cmd_factory(), ["myapp", "--help"]));

    let app = app_with_groups();
    let help_short = extract_help(app.get_matches_from(cmd_factory(), ["myapp", "-h"]));

    assert_eq!(help_sub, help_long, "help vs --help differ");
    assert_eq!(help_sub, help_short, "help vs -h differ");
}

#[test]
fn test_subcommand_help_flag_renders_subcommand_help() {
    let app = app_with_groups();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "status", "--help"]);
    let output = extract_help(result);
    assert!(output.contains("status"), "output:\n{output}");
    // Should show the subcommand's help, not the root help
    assert!(
        !output.contains("CORE"),
        "should not show root groups:\n{output}"
    );
}

#[test]
fn test_subcommand_help_short_flag() {
    let app = app_with_groups();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "status", "-h"]);
    let output = extract_help(result);
    assert!(output.contains("status"), "output:\n{output}");
}

#[test]
fn test_help_handling_off_does_not_intercept() {
    // Without help_handling, the "help" subcommand is NOT added by standout
    let app = App::builder().build().unwrap();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "status"]);
    // Should get normal matches, not help
    match result {
        HelpResult::Matches(m) => {
            assert_eq!(m.subcommand_name(), Some("status"));
        }
        other => panic!("Expected Matches, got: {other:?}"),
    }
}

#[test]
fn test_help_handling_off_help_flag_returns_clap_error() {
    // Without help_handling, --help goes through clap's error path
    let app = App::builder().build().unwrap();
    let cmd = test_cmd();
    let result = app.get_matches_from(cmd, ["myapp", "--help"]);
    match result {
        HelpResult::Error(e) => {
            assert_eq!(e.kind(), clap::error::ErrorKind::DisplayHelp);
        }
        other => panic!("Expected Error(DisplayHelp), got: {other:?}"),
    }
}

#[test]
fn test_build_errors_on_groups_without_help_handling() {
    let result = App::builder()
        .command_groups(vec![CommandGroup {
            title: "Core".into(),
            help: None,
            commands: vec![Some("init".into())],
        }])
        .build();
    match result {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("command_groups requires .help_handling(true)"),
                "error: {msg}"
            );
        }
        Ok(_) => panic!("Expected build to fail"),
    }
}

#[test]
fn test_build_errors_on_topics_without_help_handling() {
    use standout::topics::{Topic, TopicType};
    let result = App::builder()
        .add_topic(Topic::new(
            "Guide",
            "Some guide content here.",
            TopicType::Text,
            Some("guide".to_string()),
        ))
        .build();
    match result {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("topics requires .help_handling(true)"),
                "error: {msg}"
            );
        }
        Ok(_) => panic!("Expected build to fail"),
    }
}

#[test]
fn test_build_succeeds_with_help_handling_and_groups() {
    let app = App::builder()
        .help_handling(true)
        .command_groups(vec![CommandGroup {
            title: "Core".into(),
            help: None,
            commands: vec![Some("init".into())],
        }])
        .build();
    assert!(app.is_ok());
}

#[test]
fn test_build_succeeds_with_help_handling_and_topics() {
    use standout::topics::{Topic, TopicType};
    let app = App::builder()
        .help_handling(true)
        .add_topic(Topic::new(
            "Guide",
            "Some guide content here.",
            TopicType::Text,
            Some("guide".to_string()),
        ))
        .build();
    assert!(app.is_ok());
}

#[test]
fn test_help_flag_works_with_required_args() {
    // A subcommand with required positional args should still show help
    // when --help is passed (clap's native short-circuit behavior).
    let app = App::builder().help_handling(true).build().unwrap();
    let cmd = Command::new("myapp").subcommand(
        Command::new("greet")
            .about("Greet someone")
            .arg(clap::Arg::new("name").required(true)),
    );
    let result = app.get_matches_from(cmd, ["myapp", "greet", "--help"]);
    let output = extract_help(result);
    assert!(output.contains("greet"), "output:\n{output}");
}
