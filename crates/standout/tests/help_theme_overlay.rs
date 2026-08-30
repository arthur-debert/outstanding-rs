use clap::{Arg, ArgAction, Command};
use console::{set_colors_enabled, Style};
use standout::cli::{render_help, App, HelpConfig, HelpResult};
use standout::topics::{render_topic, Topic, TopicRenderConfig, TopicType};
use standout::{OutputMode, Theme};

const BOLD: &str = "\u{1b}[1m";
const CYAN: &str = "\u{1b}[36m";

fn change_tree() -> Theme {
    Theme::new()
        .add("node", Style::new().bold())
        .add("added", Style::new().green())
        .add("deleted", Style::new().red())
}

fn lookma() -> Command {
    Command::new("lookma")
        .about("Diff a git range")
        .arg(
            Arg::new("range")
                .value_name("RANGE")
                .help("Git range to diff, e.g. main..HEAD"),
        )
        .arg(
            Arg::new("staged")
                .long("staged")
                .action(ArgAction::SetTrue)
                .help("Diff the staged changes"),
        )
}

const HELP_TAG_MARKERS: &[&str] = &[
    "[header?]",
    "[item?]",
    "[metavar?]",
    "[desc?]",
    "[default?]",
    "[values?]",
    "[usage?]",
    "[example?]",
    "[about?]",
];

fn assert_no_literal_tags(output: &str) {
    for marker in HELP_TAG_MARKERS {
        assert!(
            !output.contains(marker),
            "unresolved help tag {marker} leaked into output:\n{output}"
        );
    }
}

#[test]
fn unrelated_app_theme_leaves_no_literal_tags() {
    set_colors_enabled(true);
    let config = HelpConfig {
        theme: Some(change_tree()),
        output_mode: Some(OutputMode::Term),
        ..Default::default()
    };
    let output = render_help(&lookma(), Some(config)).unwrap();

    assert_no_literal_tags(&output);
    assert!(
        output.contains(BOLD),
        "default help styling (bold headers) must survive an app theme:\n{output:?}"
    );
}

#[test]
fn app_theme_overrides_the_tags_it_names_and_only_those() {
    set_colors_enabled(true);
    let config = HelpConfig {
        theme: Some(Theme::new().add("header", Style::new().cyan())),
        output_mode: Some(OutputMode::Term),
        ..Default::default()
    };
    let output = render_help(&lookma(), Some(config)).unwrap();

    assert_no_literal_tags(&output);
    assert!(
        output.contains(&format!("{CYAN}OPTIONS")),
        "a named tag must take the configured style:\n{output:?}"
    );
    assert!(
        output.contains(&format!("{BOLD}--staged")),
        "an unnamed tag must keep its default style:\n{output:?}"
    );
}

fn themed_app() -> App {
    App::builder()
        .help_handling(true)
        .help_word(true)
        .theme(change_tree())
        .build()
        .unwrap()
}

#[test]
fn app_with_own_theme_renders_clean_help() {
    set_colors_enabled(true);
    let output =
        match themed_app().get_matches_from(lookma(), ["lookma", "help", "--output", "term"]) {
            HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
            other => panic!("expected rendered help, got: {other:?}"),
        };

    assert_no_literal_tags(&output);
    assert!(
        output.contains(BOLD),
        "default help styling must survive the builder path:\n{output:?}"
    );
}

#[test]
fn topic_render_overlays_the_default_topic_theme() {
    set_colors_enabled(true);
    let topic = Topic::new(
        "Storage",
        "Where data is stored.",
        TopicType::Text,
        Some("storage".to_string()),
    );
    let config = TopicRenderConfig {
        theme: Some(change_tree()),
        output_mode: Some(OutputMode::Term),
        ..Default::default()
    };
    let output = render_topic(&topic, Some(config)).unwrap();

    assert!(
        !output.contains("[header?]"),
        "unresolved topic tag leaked into output:\n{output}"
    );
    assert!(
        output.contains(&format!("{BOLD}STORAGE")),
        "the default topic styling must survive an app theme:\n{output:?}"
    );
}
