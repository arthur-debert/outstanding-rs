use clap::{Arg, ArgAction, Command};
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{render_help, App, HelpConfig, HelpLength, Output};
use standout::EmbeddedTemplates;
use standout::Representation;
use standout_fixtures::downstream;
use standout_test::clap_parity::{
    assert_page_states_clap_facts, assert_page_states_clap_facts_with, assert_states_clap_facts,
    clap_facts, Exemption, Fact, FactKind, Omission, Presence, Subject, DELIBERATE_OMISSIONS,
};
use standout_test::invariants::{
    assert_descriptions_aligned, assert_every_tag_resolved, assert_metavar_for_valued_args,
    assert_no_possible_values_for_valueless_args, assert_no_unresolved_tag_markers,
};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[("stat", "stat")];
const ENTRY_POINTS: [(&str, HelpLength); 3] = [
    ("-h", HelpLength::Short),
    ("--help", HelpLength::Long),
    ("help", HelpLength::Long),
];
#[test]
#[serial]
fn every_matrix_cell_states_every_clap_fact() {
    for flat in [false, true] {
        let fixture = if flat {
            downstream().flat().build()
        } else {
            downstream().build()
        };
        for (entry, length) in ENTRY_POINTS {
            for styled in [false, true] {
                let harness = if styled {
                    TestHarness::new().color_capable_terminal()
                } else {
                    TestHarness::new().stdout_is_terminal(false)
                };
                let result = harness.run(fixture.app(), fixture.command(), ["lookma", entry]);
                result.assert_success();
                assert_states_clap_facts(&result, &fixture.command(), length);
                assert_every_tag_resolved(&result);
                assert_no_unresolved_tag_markers(&result);
                assert_no_possible_values_for_valueless_args(&result, &fixture.command());
                assert_metavar_for_valued_args(&result, &fixture.command());
                assert_descriptions_aligned(&result);
            }
        }
    }
}
#[test]
#[serial]
fn a_subcommand_page_states_the_subcommands_clap_facts() {
    let fixture = downstream().build();
    let review = fixture
        .command()
        .find_subcommand("review")
        .expect("the fixture's review subcommand")
        .clone();
    for (entry, length) in ENTRY_POINTS {
        if entry == "help" {
            continue;
        }
        let result = TestHarness::new().stdout_is_terminal(false).run(
            fixture.app(),
            fixture.command(),
            ["lookma", "review", entry],
        );
        result.assert_success();
        assert_states_clap_facts(&result, &review, length);
    }
}
#[test]
#[serial]
fn the_help_word_targeting_a_subcommand_states_its_facts() {
    let fixture = downstream().build();
    let review = fixture
        .command()
        .find_subcommand("review")
        .expect("the fixture's review subcommand")
        .clone();
    let result = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "review"],
    );
    result.assert_success();
    assert_states_clap_facts(&result, &review, HelpLength::Long);
}
#[test]
fn clap_states_every_fact_the_oracle_derives() {
    for flat in [false, true] {
        let fixture = if flat {
            downstream().flat().build()
        } else {
            downstream().build()
        };
        for length in [HelpLength::Short, HelpLength::Long] {
            let page = clap_page(fixture.command(), length);
            assert_page_states_clap_facts_with(&page, &fixture.command(), length, &[]);
        }
    }
}
#[test]
fn the_derivation_covers_the_metadata_the_cluster_lost() {
    let fixture = downstream().build();
    let facts = clap_facts(&fixture.command(), HelpLength::Long);
    let stated = |kind: FactKind, subject: Subject, expected: &str| {
        assert!(
            facts.iter().any(|fact| fact.kind() == kind
                && *fact.subject() == subject
                && fact.expected() == expected
                && fact.presence() == Presence::Stated),
            "no stated {:?} fact {expected:?} for {subject}",
            kind
        );
    };
    let suppressed = |kind: FactKind, subject: Subject, expected: &str| {
        assert!(
            facts.iter().any(|fact| fact.kind() == kind
                && *fact.subject() == subject
                && fact.expected() == expected
                && fact.presence() == Presence::Suppressed),
            "no suppressed {:?} fact {expected:?} for {subject}",
            kind
        );
    };
    let arg = |id: &str| Subject::Argument(id.to_string());
    stated(
        FactKind::Purpose,
        Subject::Command("lookma".into()),
        "Diff a git range.\n\nNames a change the way a human would.",
    );
    stated(FactKind::ArgMetavar, arg("range"), "RANGE");
    stated(FactKind::ArgMetavar, arg("threshold"), "RATIO");
    stated(FactKind::ArgMetavar, arg("pattern"), "pattern");
    stated(FactKind::ArgDefault, arg("summary"), "brief");
    for value in ["brief", "full", "none"] {
        stated(FactKind::ArgPossibleValue, arg("summary"), value);
    }
    suppressed(FactKind::ArgDefault, arg("staged"), "false");
    stated(FactKind::ArgPossibleValue, arg("color"), "true");
    suppressed(FactKind::ArgPossibleValue, arg("color"), "yes");
    for name in ["review", "stat", "export"] {
        stated(
            FactKind::SubcommandName,
            Subject::Subcommand(name.into()),
            name,
        );
    }
    assert!(
        facts
            .iter()
            .any(|fact| fact.kind() == FactKind::Classification),
        "the fixture has both a positional and options, so classification is a fact about it"
    );
    stated(FactKind::ArgSpelling, arg("help"), "--help");
    stated(FactKind::ArgSpelling, arg("help"), "-h");
    assert!(
        facts
            .iter()
            .any(|fact| fact.is_clap_generated() && fact.expected() == "--help"),
        "clap's generated help argument is still marked generated, even though \
         the page states it"
    );
    assert!(
        facts
            .iter()
            .filter(|fact| *fact.subject() == arg("summary"))
            .all(|fact| !fact.is_clap_generated()),
        "an argument the application declared is not clap's"
    );
}
#[test]
#[serial]
fn a_dropped_default_fails() {
    let page = drop_lines(&rendered("--help"), "default: brief");
    fails_naming(&["summary", "brief"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn a_default_replaced_by_a_superstring_fails() {
    let page = rendered("--help").replace("default: brief", "default: briefly");
    fails_naming(&["summary", "brief"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn a_dropped_possible_value_fails() {
    let page = rendered("--help").replace(
        "possible values: brief, full, none",
        "possible values: brief, none",
    );
    fails_naming(&["summary", "full"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn a_dropped_metavar_fails() {
    let page = rendered("--help").replace("--threshold <RATIO>", "--threshold        ");
    fails_naming(&["threshold", "RATIO"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn a_dropped_argument_row_fails() {
    let page = drop_lines(&rendered("--help"), "--staged");
    fails_naming(&["staged"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn a_dropped_subcommand_row_fails() {
    let page = drop_lines(&rendered("--help"), "Summarize a range by file");
    fails_naming(&["stat"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn a_dropped_help_text_fails() {
    let page = rendered("--help").replace("Move/rename similarity threshold", "");
    fails_naming(&["threshold", "Move/rename similarity threshold"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn the_short_about_does_not_satisfy_the_long_page() {
    let page = rendered("-h");
    fails_naming(&["Names a change the way a human would"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn merging_the_positional_and_option_sections_fails() {
    let page = rendered("--help").replace("\nARGUMENTS\n", "\nOPTIONS\n");
    fails_naming(&["classification"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn stating_a_fact_clap_suppresses_fails() {
    let page = rendered("--help").replace(
        "  --staged                   Diff the staged changes",
        "  --staged                   Diff the staged changes\n\
         \x20                            default: false",
    );
    fails_naming(&["staged", "false"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}
#[test]
#[serial]
fn the_page_lists_the_help_flag_clap_generates() {
    let fixture = downstream().build();
    let page = rendered("--help");
    assert_page_states_clap_facts(&page, &fixture.command(), HelpLength::Long);
    assert!(
        page.contains("-h, --help"),
        "clap accepts `-h`/`--help`, so the page states them:\n{page}"
    );
}
#[test]
#[serial]
fn the_page_lists_the_version_flag_clap_generates() {
    let (app, cmd) = versioned();
    let result =
        TestHarness::new()
            .stdout_is_terminal(false)
            .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
    assert!(
        page.contains("-V, --version"),
        "clap accepts `-V`/`--version`, so the page states them:\n{page}"
    );
    assert!(
        clap_facts(&cmd, HelpLength::Long)
            .iter()
            .any(|fact| fact.is_clap_generated() && fact.expected() == "--version"),
        "clap's generated version argument is still marked generated"
    );
}
#[test]
#[serial]
fn the_clap_generated_subcommand_exemption_is_load_bearing() {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .subcommand(Command::new("stat").about("Summarize the notes"));
    let page = themed_page(&cmd);
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
    assert!(
        !page.contains("Print this message"),
        "clap's own help word is standout's machinery, not an application \
         destination:\n{page}"
    );
    fails_naming(&["subcommand `help`"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::ClapGeneratedSubcommands),
        )
    });
}
#[test]
#[serial]
fn an_already_built_command_states_the_same_facts() {
    let mut cmd = Command::new("notes")
        .about("Keep short notes")
        .subcommand(Command::new("stat").about("Summarize the notes"));
    cmd.build();
    let page = themed_page(&cmd);
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
    assert!(
        clap_facts(&cmd, HelpLength::Long)
            .iter()
            .any(|fact| fact.is_clap_generated()
                && *fact.subject() == Subject::Subcommand("help".into())),
        "clap's help word is generated however built the caller's command is"
    );
}
#[test]
#[serial]
fn the_argument_and_subcommand_exemptions_are_load_bearing() {
    let (app, cmd) = decorated();
    let result =
        TestHarness::new()
            .stdout_is_terminal(false)
            .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();
    assert_page_states_clap_facts_with(
        &clap_page(cmd.clone(), HelpLength::Long),
        &cmd,
        HelpLength::Long,
        &[],
    );
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
    fails_naming(&["long help", "as a ratio"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::Kind(FactKind::ArgLongHelp)),
        )
    });
    fails_naming(&["threshold", "--thr"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::Kind(FactKind::ArgAlias)),
        )
    });
    fails_naming(&["stat", "st"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::Kind(FactKind::SubcommandAlias)),
        )
    });
}
#[test]
fn every_exemption_states_a_reason() {
    for exemption in DELIBERATE_OMISSIONS {
        assert!(
            exemption.reason.split_whitespace().count() >= 10,
            "{:?} is exempt without a reason a reviewer could weigh",
            exemption.omission
        );
    }
}
#[test]
#[serial]
fn hidden_metadata_stays_off_the_page() {
    let (app, cmd) = concealing();
    let result =
        TestHarness::new()
            .stdout_is_terminal(false)
            .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
    assert!(
        !page.contains("--secret") && !page.contains("vault"),
        "the fixture only tests the oracle if the page really hides them:\n{page}"
    );
    let leaked = page.replace("OPTIONS", "OPTIONS\n  --secret <TOKEN>           A secret");
    fails_naming(&["--secret"], || {
        assert_page_states_clap_facts(&leaked, &cmd, HelpLength::Long)
    });
}
#[test]
fn the_derivation_follows_length_specific_hides() {
    let cmd = length_concealing();
    let short = clap_facts(&cmd, HelpLength::Short);
    let long = clap_facts(&cmd, HelpLength::Long);
    let spelling = |facts: &[Fact], id: &str, presence: Presence| {
        assert!(
            facts.iter().any(|fact| fact.kind() == FactKind::ArgSpelling
                && *fact.subject() == Subject::Argument(id.to_string())
                && fact.presence() == presence),
            "`{id}` should have a {presence:?} spelling fact"
        );
    };
    spelling(&short, "verbose", Presence::Suppressed);
    spelling(&long, "verbose", Presence::Stated);
    spelling(&short, "terse", Presence::Stated);
    spelling(&long, "terse", Presence::Suppressed);
    spelling(&short, "insistent", Presence::Stated);
    spelling(&long, "insistent", Presence::Stated);
}
#[test]
fn length_specific_hides_ground_against_clap() {
    let cmd = length_concealing();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn an_argument_hidden_from_one_length_leaking_onto_that_page_fails() {
    let cmd = length_concealing();
    let leaked = clap_page(cmd.clone(), HelpLength::Long).replace(
        "Options:",
        "Options:\n      --terse\n          Only the short page lists this\n",
    );
    fails_naming(&["--terse"], || {
        assert_page_states_clap_facts_with(&leaked, &cmd, HelpLength::Long, &[])
    });
}
#[test]
fn a_help_less_short_flag_does_not_swallow_the_long_flag_below_it() {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .arg(Arg::new("force").short('f').action(ArgAction::SetTrue))
        .arg(
            Arg::new("long-only")
                .long("long-only")
                .action(ArgAction::SetTrue)
                .help("Spelled out in full"),
        );
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn helped_and_quoted_values_ground_against_clap() {
    let cmd = quoted_and_helped();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn a_dropped_possible_value_bullet_fails() {
    let cmd = quoted_and_helped();
    let page = drop_lines(&clap_page(cmd.clone(), HelpLength::Long), "- json");
    fails_naming(&["format", "json"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}
#[test]
fn a_dropped_quoted_possible_value_fails() {
    let cmd = quoted_and_helped();
    let page = clap_page(cmd.clone(), HelpLength::Short).replace("\"plain text\", ", "");
    fails_naming(&["format", "plain text"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Short, &[])
    });
}
#[test]
#[serial]
fn a_whitespace_value_on_standouts_page_is_one_value() {
    let (app, cmd) = whitespace_valued();
    let result =
        TestHarness::new()
            .stdout_is_terminal(false)
            .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();
    assert!(
        page.contains("possible values: plain text, json"),
        "the page only tests the decoder if it spells the list unquoted:\n{page}"
    );
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
}
#[test]
fn a_possible_value_naming_a_separator_grounds_against_clap() {
    let cmd = colon_valued();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn a_dropped_separator_carrying_bullet_fails() {
    let cmd = colon_valued();
    let page = drop_lines(&clap_page(cmd.clone(), HelpLength::Long), "- key: value");
    fails_naming(&["format", "key: value"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}
#[test]
fn a_multi_name_positional_grounds_against_clap() {
    let cmd = multi_name_positional();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn a_positional_losing_its_second_value_name_fails() {
    let cmd = multi_name_positional();
    let page = clap_page(cmd.clone(), HelpLength::Long).replace("[SRC] [DEST]", "[SRC]");
    fails_naming(&["paths", "DEST"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}
#[test]
fn an_options_placeholder_does_not_answer_for_a_positional() {
    let cmd = multi_name_positional().arg(
        Arg::new("into")
            .long("into")
            .value_name("DEST")
            .action(ArgAction::Set)
            .help("Where to copy"),
    );
    let page = clap_page(cmd.clone(), HelpLength::Long).replace("[SRC] [DEST]", "[SRC] [TARGET]");
    fails_naming(&["argument `paths` spelling \"DEST\""], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}
#[test]
fn a_repeating_positional_grounds_against_clap() {
    for cmd in [optional_repeating(), required_repeating()] {
        for length in [HelpLength::Short, HelpLength::Long] {
            let page = clap_page(cmd.clone(), length);
            assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
        }
    }
}
#[test]
fn a_repeating_positional_filed_under_its_id_fails() {
    for (cmd, label) in [
        (optional_repeating(), "[FILE]..."),
        (required_repeating(), "<FILE>..."),
    ] {
        let page = clap_page(cmd.clone(), HelpLength::Long).replace(label, "[files]...");
        fails_naming(&["argument `files` metavar \"FILE\""], || {
            assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
        });
    }
}
#[test]
fn a_spaced_value_name_grounds_against_clap() {
    let cmd = spaced_value_names();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn a_spaced_value_name_rendered_by_halves_fails() {
    let cmd = spaced_value_names();
    let page = clap_page(cmd.clone(), HelpLength::Long).replace("--file <P Q>", "--file <P>");
    fails_naming(&["argument `file` metavar \"P Q\""], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}
#[test]
fn custom_help_headings_ground_against_clap() {
    let cmd = custom_headed();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}
#[test]
fn merging_the_default_headed_sections_fails_beside_custom_headings() {
    let cmd = custom_headed();
    let page = clap_page(cmd.clone(), HelpLength::Long).replace("\nArguments:\n", "\nOptions:\n");
    fails_naming(&["classification"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}
fn length_concealing() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .action(ArgAction::SetTrue)
                .hide_short_help(true)
                .help("Only the long page lists this"),
        )
        .arg(
            Arg::new("terse")
                .long("terse")
                .action(ArgAction::SetTrue)
                .hide_long_help(true)
                .help("Only the short page lists this"),
        )
        .arg(
            Arg::new("insistent")
                .long("insistent")
                .action(ArgAction::SetTrue)
                .hide_long_help(true)
                .next_line_help(true)
                .help("Next-line help overrides the length hide"),
        )
}
fn quoted_and_helped() -> Command {
    Command::new("notes").about("Keep short notes").arg(
        Arg::new("format")
            .long("format")
            .value_name("FORMAT")
            .action(ArgAction::Set)
            .default_value("plain text")
            .value_parser([
                clap::builder::PossibleValue::new("plain text"),
                clap::builder::PossibleValue::new("json").help("One note per line"),
            ])
            .help("How to print the notes"),
    )
}
fn colon_valued() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .action(ArgAction::Set)
                .value_parser([
                    clap::builder::PossibleValue::new("key: value"),
                    clap::builder::PossibleValue::new("plain"),
                    clap::builder::PossibleValue::new("json").help("One note per line"),
                ])
                .help("How to print the notes"),
        )
}
fn multi_name_positional() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(
            Arg::new("paths")
                .value_names(["SRC", "DEST"])
                .num_args(2)
                .help("Copy from SRC to DEST"),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .action(ArgAction::SetTrue)
                .help("Overwrite the destination"),
        )
}
fn optional_repeating() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(
            Arg::new("files")
                .value_name("FILE")
                .num_args(1..)
                .help("Files to read"),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .action(ArgAction::SetTrue)
                .help("Overwrite the destination"),
        )
}
fn required_repeating() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(
            Arg::new("files")
                .value_name("FILE")
                .num_args(1..)
                .required(true)
                .help("Files to read"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .action(ArgAction::Count)
                .help("Say more"),
        )
}
fn spaced_value_names() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(Arg::new("range").value_name("A B").help("A range"))
        .arg(
            Arg::new("file")
                .long("file")
                .value_name("P Q")
                .action(ArgAction::Set)
                .help("Notes file to read"),
        )
}
fn custom_headed() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("force")
                .long("force")
                .action(ArgAction::SetTrue)
                .help("Overwrite the destination"),
        )
        .arg(
            Arg::new("path")
                .value_name("PATH")
                .help("A path")
                .help_heading("Selection"),
        )
        .arg(
            Arg::new("since")
                .long("since")
                .value_name("WHEN")
                .action(ArgAction::Set)
                .help("Only notes since then")
                .help_heading("Selection"),
        )
        .arg(
            Arg::new("quiet")
                .long("quiet")
                .action(ArgAction::SetTrue)
                .help("Say less")
                .help_heading("Output"),
        )
}
fn whitespace_valued() -> (App, Command) {
    let cmd = quoted_and_helped().subcommand(Command::new("stat").about("Summarize the notes"));
    (notes_app(), cmd)
}
fn rendered(entry: &str) -> String {
    let fixture = downstream().build();
    let result = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", entry],
    );
    result.assert_success();
    result.stdout_plain()
}
fn themed_page(cmd: &Command) -> String {
    render_help(
        cmd,
        Some(HelpConfig {
            output_mode: Some(Representation::Human),
            length: HelpLength::Long,
            ..Default::default()
        }),
    )
    .expect("the themed page renders")
}
fn clap_page(cmd: Command, length: HelpLength) -> String {
    let mut cmd = cmd;
    cmd.build();
    match length {
        HelpLength::Short => cmd.render_help().to_string(),
        HelpLength::Long => cmd.render_long_help().to_string(),
    }
}
fn without(omission: Omission) -> Vec<Exemption> {
    DELIBERATE_OMISSIONS
        .iter()
        .filter(|exemption| exemption.omission != omission)
        .copied()
        .collect()
}
fn drop_lines(page: &str, needle: &str) -> String {
    page.lines()
        .filter(|line| !line.contains(needle))
        .collect::<Vec<_>>()
        .join("\n")
}
#[track_caller]
fn fails_naming(needles: &[&str], assertion: impl FnOnce()) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(assertion));
    let payload = match outcome {
        Err(payload) => payload,
        Ok(()) => panic!("expected the assertion to fail, naming {needles:?}"),
    };
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("<panic payload was not a string>")
        .to_string();
    for needle in needles {
        assert!(
            message.contains(needle),
            "the failure must name {needle:?}; it said:\n{message}"
        );
    }
}
fn notes_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "stat",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn versioned() -> (App, Command) {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .version("1.2.3")
        .subcommand(Command::new("stat").about("Summarize the notes"));
    (notes_app(), cmd)
}
fn decorated() -> (App, Command) {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .long_about("Keep short notes.\n\nOne file, one line each.")
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("threshold")
                .long("threshold")
                .visible_alias("thr")
                .visible_short_alias('t')
                .value_name("RATIO")
                .action(ArgAction::Set)
                .help("Similarity threshold")
                .long_help("Similarity threshold, as a ratio between 0 and 1"),
        )
        .subcommand(
            Command::new("stat")
                .about("Summarize the notes")
                .visible_alias("st"),
        );
    (notes_app(), cmd)
}
fn concealing() -> (App, Command) {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("secret")
                .long("secret")
                .value_name("TOKEN")
                .action(ArgAction::Set)
                .hide(true)
                .help("A secret"),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .action(ArgAction::Set)
                .value_parser([
                    clap::builder::PossibleValue::new("text"),
                    clap::builder::PossibleValue::new("json"),
                    clap::builder::PossibleValue::new("vault").hide(true),
                ])
                .help("How to print the notes"),
        )
        .subcommand(Command::new("stat").about("Summarize the notes").hide(true));
    (notes_app(), cmd)
}
