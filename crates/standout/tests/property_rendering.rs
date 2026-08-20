//! Property tests over the rendering pipeline: (mode × theme × template ×
//! data), with the WS04 invariants as postconditions.
//!
//! The original version of this file generated the exact shape of #303 — an
//! incomplete theme, a template using a styled tag, `Term` mode — and passed
//! it, twice over: its only postcondition was `is_handled() || is_error()`,
//! and its templates all spelled `{{ . }}`, which MiniJinja rejects as a
//! syntax error — so every generated case exercised the error path and the
//! "rendering" property test never rendered anything.
//!
//! Both are fixed here. The templates are valid, each one declares which
//! style tags it emits, and the theme strategy says which tags it defines —
//! including *incomplete* themes, which define some of the vocabulary but not
//! all of it. That lets the postconditions be real:
//!
//! - A valid template over generated data must **render**, not error, in the
//!   template modes.
//! - Structured modes see the *raw* generated value — top-level scalars and
//!   arrays included, not just objects. JSON and YAML serialize every JSON
//!   value, so they must **render**; XML and CSV may refuse a shape (#107 is
//!   the known XML case), but only as a `Render`-kind error — any other
//!   failure is a dispatch defect, not a serializer refusal.
//! - Structured output must parse as what it claims to be.
//! - The WS04 invariants hold unconditionally: no `[tag?]` marker reaches the
//!   page, and the `Term` page, stripped of ANSI, is exactly the `Text` page.
//!
//! The unconditional form of those two invariants is the #303-shaped property:
//! an incomplete theme, a styled template, and terminal output must still render
//! one clean page.

use clap::Command;
use console::Style;
use proptest::prelude::*;
use serde_json::{json, Value};
use standout::cli::{App, DispatchResult as RunResult, Output, RunErrorKind};
use standout::{OutputMode, Theme};
use standout_test::invariants::{
    assert_no_unresolved_tag_markers_in_page, assert_styling_preserves_layout_in_pages,
};

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// All 8 output modes, per design guidelines.
fn output_mode_strategy() -> impl Strategy<Value = OutputMode> {
    prop_oneof![
        Just(OutputMode::Auto),
        Just(OutputMode::Term),
        Just(OutputMode::Text),
        Just(OutputMode::TermDebug),
        Just(OutputMode::Json),
        Just(OutputMode::Yaml),
        Just(OutputMode::Xml),
        Just(OutputMode::Csv),
    ]
}

/// A template variant.
#[derive(Debug, Clone)]
struct TemplateCase {
    source: &'static str,
}

/// Template variations: the plain MiniJinja path, one styled tag, and nested
/// styled tags.
fn template_strategy() -> impl Strategy<Value = TemplateCase> {
    prop_oneof![
        Just(TemplateCase {
            source: "{{ data }}",
        }),
        Just(TemplateCase {
            source: "[title]{{ data }}[/title]",
        }),
        Just(TemplateCase {
            source: "[highlight]Output: [title]{{ data }}[/title][/highlight]",
        }),
    ]
}

/// A theme variant.
#[derive(Debug, Clone)]
struct ThemeCase {
    theme: Option<Theme>,
}

/// Themes from absent through incomplete to complete.
///
/// The incomplete cases are the point (and were missing before): a theme that
/// defines *some* of the vocabulary is the downstream shape — an app themes
/// what it knows about — and is exactly where #303-class defects live. The
/// search space deliberately contains the bug.
fn theme_strategy() -> impl Strategy<Value = ThemeCase> {
    prop_oneof![
        Just(ThemeCase { theme: None }),
        Just(ThemeCase {
            theme: Some(Theme::new()),
        }),
        Just(ThemeCase {
            theme: Some(Theme::new().add("title", Style::new().bold())),
        }),
        Just(ThemeCase {
            theme: Some(Theme::new().add("highlight", Style::new().cyan())),
        }),
        Just(ThemeCase {
            theme: Some(
                Theme::new()
                    .add("title", Style::new().bold())
                    .add("highlight", Style::new().cyan())
                    .add("error", Style::new().red().bold())
            ),
        }),
    ]
}

/// Arbitrary JSON data. [`dispatch`] hands it to structured modes as-is —
/// so serializers meet top-level scalars and arrays, not just objects — and
/// wraps it as `{"data": …}` for template modes, so the templates have a
/// name to reference.
fn json_data_strategy() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<f64>().prop_map(|f| json!(f)),
        "[a-zA-Z0-9]*".prop_map(Value::String),
    ];
    leaf.prop_recursive(
        4,  // 4 levels deep
        64, // Max size 64 nodes
        10, // Items per collection
        |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..10).prop_map(Value::Array),
                prop::collection::hash_map("[a-zA-Z0-9]*", inner, 0..10)
                    .prop_map(|m| { Value::Object(m.into_iter().collect()) })
            ]
        },
    )
}

// ---------------------------------------------------------------------------
// Dispatch plumbing
// ---------------------------------------------------------------------------

/// Builds the generated app and dispatches `test` under `mode`.
///
/// Structured modes serialize the handler's value directly and never read
/// the template, so they get the generated value untouched — wrapping it
/// would hide every top-level scalar and array from the serializers.
/// Template modes get it wrapped as `{"data": …}`, the name the templates
/// reference.
fn dispatch(
    mode: OutputMode,
    theme: &ThemeCase,
    template: &TemplateCase,
    data: &Value,
) -> RunResult {
    let payload = if mode.is_structured() {
        data.clone()
    } else {
        json!({ "data": data })
    };
    let builder = App::builder()
        .command(
            "test",
            move |_m, _ctx| Ok(Output::Render(payload.clone())),
            template.source,
        )
        .unwrap();

    let builder = match &theme.theme {
        Some(t) => builder.theme(t.clone()),
        None => builder,
    };

    let app = builder.build().expect("Failed to build app");
    let cmd = Command::new("app").subcommand(Command::new("test"));
    let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
    app.dispatch(matches, mode).into_outcome()
}

/// Structured output must parse as the format it claims to be.
fn validate_structured_output(output: &str, mode: OutputMode) {
    match mode {
        OutputMode::Json => {
            let parsed: Result<Value, _> = serde_json::from_str(output);
            assert!(
                parsed.is_ok(),
                "JSON output should be parseable: {}",
                output
            );
        }
        OutputMode::Yaml => {
            let parsed: Result<Value, _> = serde_yaml::from_str(output);
            assert!(
                parsed.is_ok(),
                "YAML output should be parseable: {}",
                output
            );
        }
        OutputMode::Xml => {
            // XML output must be non-empty and parse as XML: quick-xml (the
            // emitting crate) must read it back event by event without error.
            assert!(!output.is_empty(), "XML output should not be empty");
            let mut reader = quick_xml::Reader::from_str(output);
            loop {
                match reader.read_event() {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => {}
                    Err(err) => panic!("XML output should parse: {err}\n{output}"),
                }
            }
        }
        OutputMode::Csv => {
            // CSV output must be non-empty and parse as CSV: every record
            // reads cleanly and carries the header's width.
            assert!(!output.is_empty(), "CSV output should not be empty");
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(false)
                .from_reader(output.as_bytes());
            for record in reader.records() {
                record.unwrap_or_else(|err| panic!("CSV output should parse: {err}\n{output}"));
            }
        }
        _ => {}
    }
}

/// The mode-agreement invariant for one already-rendered page: `styled_page`
/// with its escapes stripped is exactly the `Text` page over the same input.
///
/// Takes the caller's page instead of re-dispatching it — the property case
/// already holds the `Term` (or `Auto`) render it is making a claim about,
/// so the only extra dispatch is the `Text` side of the comparison.
fn assert_agrees_with_text_mode(
    styled_page: &str,
    theme: &ThemeCase,
    template: &TemplateCase,
    data: &Value,
) {
    let text = dispatch(OutputMode::Text, theme, template, data);
    let Some(text_page) = text.output() else {
        panic!("Text mode must render the same input: {text:?}");
    };
    assert_styling_preserves_layout_in_pages(&console::strip_ansi_codes(styled_page), text_page);
}

// ---------------------------------------------------------------------------
// The properties
// ---------------------------------------------------------------------------

proptest! {
    /// The invariants that hold on today's framework, over the whole space —
    /// incomplete themes included.
    ///
    /// The tag invariants are unconditional: an uncovered tag degrades to
    /// unstyled text and must not change the rendered page beyond color.
    /// A valid template renders, JSON/YAML serialize any value, XML/CSV refuse
    /// a shape only as a `Render` error, and whatever a serializer emits parses.
    #[test]
    fn rendering_upholds_the_invariants(
        mode in output_mode_strategy(),
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let result = dispatch(mode, &theme, &template, &data);

        // Dispatch must never silently fall through to NoMatch/Silent.
        prop_assert!(
            result.is_handled() || result.is_error(),
            "expected Handled or Error, got {:?}",
            result
        );

        if mode.is_structured() {
            match mode {
                // JSON and YAML serialize every JSON value: an error here
                // is a regression, not a refusal.
                OutputMode::Json | OutputMode::Yaml => {
                    prop_assert!(
                        result.is_handled(),
                        "{:?} must serialize any generated value, got {:?}",
                        mode,
                        result
                    );
                }
                // XML (#107: quick-xml's root/shape limits) and CSV (shapes
                // that flatten to no columns) may refuse a shape — but only
                // as a Render error, the serializer-refusal kind. Any other
                // outcome is a dispatch defect hiding behind "structured
                // modes may error".
                _ => {
                    prop_assert!(
                        result.is_handled()
                            || result.error_kind() == Some(RunErrorKind::Render),
                        "{:?} must serialize or refuse with a Render error, got {:?}",
                        mode,
                        result
                    );
                }
            }
            // What a serializer does emit must parse as what it claims.
            if let Some(output) = result.output() {
                validate_structured_output(output, mode);
            }
        } else {
            // A valid template over valid data must render in every
            // template mode — the error path is not an acceptable rendering.
            prop_assert!(
                result.is_handled(),
                "a valid template must render, got {:?}",
                result
            );

            let output = result.output().expect("a handled render has output");
            assert_no_unresolved_tag_markers_in_page(output);
            if matches!(mode, OutputMode::Term | OutputMode::Auto) {
                assert_agrees_with_text_mode(output, &theme, &template, &data);
            }
        }
    }

    /// The unconditional tag invariants — the #303-shaped property.
    ///
    /// A tag the theme does not define must not corrupt the page, and `Term`
    /// must agree with `Text` after stripping ANSI.
    #[test]
    fn no_theme_gap_corrupts_a_page_or_splits_the_modes(
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let term = dispatch(OutputMode::Term, &theme, &template, &data);
        prop_assert!(term.is_handled(), "expected a render, got {:?}", term);
        let term_page = term.output().unwrap();
        assert_no_unresolved_tag_markers_in_page(term_page);
        assert_agrees_with_text_mode(term_page, &theme, &template, &data);
    }
}
