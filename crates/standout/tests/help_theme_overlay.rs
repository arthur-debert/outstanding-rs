//! The application theme overlays the default help theme (#303).
//!
//! An app theme carries the app's OUTPUT vocabulary — lookma's declares
//! `node`, `added`, `deleted`, … — and shares nothing with the help
//! template's tags (`header`, `item`, `about`, …). Handing it to the help
//! renderer as a replacement left every help tag unresolved, so a terminal
//! reader saw literal `[about?]…[/about?]` markup on the whole page.
//!
//! The contract asserted here: the default help (and topic) theme is the
//! base, the configured theme overlays it — an entry the app names wins,
//! every tag it leaves alone still resolves to its default style.
//!
//! Assertions run in `Term` mode on purpose: unresolved tags render as
//! `[tag?]` markup only when styling is on, so a `Text`-mode test (or piped
//! manual check) cannot see this defect — which is how it shipped. Term mode
//! still defers to console's global color switch, so each test flips it on
//! (`set_colors_enabled(true)`) to get ANSI output without a TTY. The switch
//! is process-global and stays set, which is safe here: every test in this
//! file wants it on, and each file under `tests/` compiles to its own binary,
//! so the flag never reaches another test file's process.

use clap::{Arg, ArgAction, Command};
use console::{set_colors_enabled, Style};
use standout::cli::{render_help, App, HelpConfig, HelpResult};
use standout::topics::{render_topic, Topic, TopicRenderConfig, TopicType};
use standout::{OutputMode, Theme};

const BOLD: &str = "\u{1b}[1m";
const CYAN: &str = "\u{1b}[36m";

/// The reported shape: an app theme whose classes are its output
/// vocabulary, with no help tag among them.
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

/// Every tag the help template emits, as its opening `?` marker.
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

// ---------------------------------------------------------------------------
// The direct render path
// ---------------------------------------------------------------------------

/// The reported symptom: an unrelated app theme left every help tag
/// unresolved, so the page rendered as literal markup.
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

/// Deliberate restyling still works: a tag the app theme names takes the
/// app's style, and the tags it leaves alone keep their defaults.
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

// ---------------------------------------------------------------------------
// The builder path — how a real app hits this
// ---------------------------------------------------------------------------

fn themed_app() -> App {
    App::builder()
        .help_handling(true)
        .help_word(true)
        .theme(change_tree())
        .build()
        .unwrap()
}

/// `.theme(..)` is how lookma got here: the builder hands the app theme to
/// every help render.
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

/// The topic renderer takes the same app theme through the same entry
/// points, so it overlays the same way.
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
