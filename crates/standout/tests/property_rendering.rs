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
//!   template modes; structured modes may still refuse a data shape (#107 is
//!   the known XML case), so there the postcondition stays handled-or-error.
//! - Structured output must parse as what it claims to be.
//! - Where the theme defines every tag the template emits, the WS04
//!   invariants hold unconditionally: no `[tag?]` marker reaches the page,
//!   and the `Term` page, stripped of ANSI, is exactly the `Text` page.
//!
//! The unconditional form of those two invariants — no marker and no
//! mode-split *whatever* the theme defines — is the #303-shaped property, and
//! it fails on today's framework: an app template's unknown tag renders as a
//! `[tag?]` marker under `Term` while `Text` strips it silently, so the two
//! modes disagree about the same render. That test is below, `#[ignore]`d
//! with the issue that owns the fix (#318, loud-failures: themes merge and
//! unresolved tags degrade instead of leaking). Run it with
//! `cargo test -- --ignored` to watch the net catch the bug.

use clap::Command;
use console::Style;
use proptest::prelude::*;
use serde_json::{json, Value};
use standout::cli::{App, Output, RunResult};
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

/// A template variant, carrying the style tags it emits so a postcondition
/// can compare them against what the theme defines.
#[derive(Debug, Clone)]
struct TemplateCase {
    source: &'static str,
    tags: &'static [&'static str],
}

/// Template variations: the plain MiniJinja path, one styled tag, and nested
/// styled tags. The vocabulary is `title`/`highlight`, mirrored by the theme
/// strategy below.
fn template_strategy() -> impl Strategy<Value = TemplateCase> {
    prop_oneof![
        Just(TemplateCase {
            source: "{{ data }}",
            tags: &[],
        }),
        Just(TemplateCase {
            source: "[title]{{ data }}[/title]",
            tags: &["title"],
        }),
        Just(TemplateCase {
            source: "[highlight]Output: [title]{{ data }}[/title][/highlight]",
            tags: &["title", "highlight"],
        }),
    ]
}

/// A theme variant, carrying which of the template vocabulary it defines.
#[derive(Debug, Clone)]
struct ThemeCase {
    theme: Option<Theme>,
    defines: &'static [&'static str],
}

impl ThemeCase {
    /// Whether every tag `template` emits is defined here — the condition
    /// under which the tag invariants hold on today's framework.
    fn covers(&self, template: &TemplateCase) -> bool {
        template.tags.iter().all(|tag| self.defines.contains(tag))
    }
}

/// Themes from absent through incomplete to complete.
///
/// The incomplete cases are the point (and were missing before): a theme that
/// defines *some* of the vocabulary is the downstream shape — an app themes
/// what it knows about — and is exactly where #303-class defects live. The
/// search space deliberately contains the bug.
fn theme_strategy() -> impl Strategy<Value = ThemeCase> {
    prop_oneof![
        Just(ThemeCase {
            theme: None,
            defines: &[],
        }),
        Just(ThemeCase {
            theme: Some(Theme::new()),
            defines: &[],
        }),
        Just(ThemeCase {
            theme: Some(Theme::new().add("title", Style::new().bold())),
            defines: &["title"],
        }),
        Just(ThemeCase {
            theme: Some(Theme::new().add("highlight", Style::new().cyan())),
            defines: &["highlight"],
        }),
        Just(ThemeCase {
            theme: Some(
                Theme::new()
                    .add("title", Style::new().bold())
                    .add("highlight", Style::new().cyan())
                    .add("error", Style::new().red().bold())
            ),
            defines: &["title", "highlight"],
        }),
    ]
}

/// Arbitrary JSON data, wrapped by the caller as `{"data": …}` so the
/// templates have a name to reference.
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
fn dispatch(
    mode: OutputMode,
    theme: &ThemeCase,
    template: &TemplateCase,
    data: &Value,
) -> RunResult {
    let payload = json!({ "data": data });
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
    app.dispatch(matches, mode)
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
            // XML output must be non-empty and contain a root element
            assert!(!output.is_empty(), "XML output should not be empty");
            assert!(
                output.contains('<') && output.contains('>'),
                "XML output should contain tags: {}",
                output
            );
        }
        OutputMode::Csv => {
            // CSV output must be non-empty
            assert!(!output.is_empty(), "CSV output should not be empty");
        }
        _ => {}
    }
}

/// The `Term`/`Text` agreement invariant for one generated input: the `Term`
/// page with its escapes stripped is exactly the `Text` page.
fn assert_modes_agree(theme: &ThemeCase, template: &TemplateCase, data: &Value) {
    let term = dispatch(OutputMode::Term, theme, template, data);
    let text = dispatch(OutputMode::Text, theme, template, data);
    let (Some(term_page), Some(text_page)) = (term.output(), text.output()) else {
        panic!("both text modes must render the same input: term={term:?} text={text:?}");
    };
    assert_styling_preserves_layout_in_pages(&console::strip_ansi_codes(term_page), text_page);
}

// ---------------------------------------------------------------------------
// The properties
// ---------------------------------------------------------------------------

proptest! {
    /// The invariants that hold on today's framework, over the whole space —
    /// incomplete themes included.
    ///
    /// The tag invariants are conditioned on the theme covering the
    /// template's vocabulary, because an *uncovered* tag currently leaks a
    /// marker by design of `UnknownTagBehavior::Passthrough`; the
    /// unconditional form is the `#[ignore]`d test below. Everything else is
    /// unconditional: a valid template renders, structured output parses.
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
            // Structured serialization may refuse a data shape (#107 tracks
            // the XML case), but what it emits must parse.
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

            if theme.covers(&template) {
                let output = result.output().expect("a handled render has output");
                assert_no_unresolved_tag_markers_in_page(output);
                if matches!(mode, OutputMode::Term | OutputMode::Auto) {
                    assert_modes_agree(&theme, &template, &data);
                }
            }
        }
    }

    /// The unconditional tag invariants — the #303-shaped property.
    ///
    /// A tag the theme does not define must not corrupt the page, in any
    /// mode, and the text modes must agree about every input. Today both
    /// halves fail: under `Term`, an unknown tag renders as a `[tag?]` marker
    /// (template markup in user-facing output) while `Text` strips it
    /// silently — the exact divergence that let #303 ship. #318 (loud
    /// failures: themes merge, unresolved tags degrade to unstyled text) owns
    /// the fix; this net is what proves it.
    #[test]
    #[ignore = "red on current behavior: unknown tags leak [tag?] markers under Term and split \
                Term/Text; fix owned by #318 (loud failures: themes merge, never replace)"]
    fn no_theme_gap_corrupts_a_page_or_splits_the_modes(
        theme in theme_strategy(),
        template in template_strategy(),
        data in json_data_strategy()
    ) {
        let term = dispatch(OutputMode::Term, &theme, &template, &data);
        prop_assert!(term.is_handled(), "expected a render, got {:?}", term);
        assert_no_unresolved_tag_markers_in_page(term.output().unwrap());
        assert_modes_agree(&theme, &template, &data);
    }
}
