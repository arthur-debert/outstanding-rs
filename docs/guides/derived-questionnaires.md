# Derived Questionnaires

Derived questionnaires let an application describe a form once as Rust types and
let Standout provide the answer-sheet command surface around it.

Use this when a command needs a multi-question setup flow, an editable answer
sheet, or repeatable automation through `--answers FILE` and `--answers -`.
For lower-level control over the same runtime model, use the hand-built
`standout_input::questionnaire::Questionnaire` builder API directly.

## Define the Answer Type

Derive `Questionnaire` on named-field structs. The container
`#[question(id = "...")]` is the stable questionnaire identity written into
every answer sheet. Field doc comments become prompts; field names become
stable IDs unless `#[question(id = "...")]` overrides them.

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, standout::Questionnaire)]
#[question(id = "demo.import")]
struct ImportAnswers {
    /// What is the project name?
    #[question(validate = validate_name, revision = "project-name.v1")]
    project_name: String,

    /// Where is the manifest?
    manifest: PathBuf,

    /// Add release notes.
    #[question(prose)]
    notes: String,

    /// Which output format should be generated?
    #[question(choice, default = "json")]
    format: OutputFormat,
}
```

The derive lowers to `standout-input`'s public builder and implements typed
filling from decoded answers. It does not use `serde`, so serde field renames on
the same struct cannot change questionnaire IDs.

## Choice Enums

Derive `QuestionnaireChoices` on unit-variant enums used by
`#[question(choice)]` fields.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, standout::QuestionnaireChoices)]
enum OutputFormat {
    #[question(rename = "json")]
    Json,
    #[question(rename = "yaml")]
    Yaml,
    #[question(rename = "plain-text")]
    PlainText,
}
```

Every variant declares its user-facing spelling with
`#[question(rename = "...")]`; a variant without one is a compile error, so
the accepted answer strings are always explicit in the source. The enum is the
single source for the rendered hint, allowed choices, `FromStr`, and `Display`.

## Field Shapes

Supported scalar field types are `String`, `PathBuf`, and `bool`.
`Option<T>` makes a scalar or choice field optional. A nested questionnaire
struct lowers to a group, and `Vec<NestedStruct>` lowers to a repeatable group.

`Vec<T>` over a scalar element type (for example `Vec<String>`) is a compile
error: collect a list as a `String` field and split it in application code, or
model the items as a `Vec` of a nested questionnaire struct.

Useful field attributes:

| Attribute | Meaning |
| --- | --- |
| `id = "..."` | Override the stable field or group ID. |
| `default = "..."` | Static default answer text. |
| `default_with = path, revision = "..."` | Dynamic default from earlier answers. |
| `validate = path, revision = "..."` | Field validator hook. |
| `active_when(field = "...", is = "...")` | Conditional `Option<T>` field; the controller names a field of the same struct. |
| `choice` | Treat an enum as a choice field, not a nested struct. |
| `prose` | Treat a `String` as multiline text. |
| `min = N, max = M` | Bounds for repeatable groups. |

Static and dynamic defaults are mutually exclusive. `default_with` and
`validate` both require a non-empty `revision`; if both hooks are on the same
field, the one revision identifies both hook contracts for the fingerprint.
Bump it whenever either hook changes which answers are accepted or supplied.

Hook signatures are plain function paths:

```rust
use standout::input::questionnaire::{AnswerValue, EarlierAnswers};

fn default_name(answers: &EarlierAnswers<'_>) -> String {
    answers.get_text("project_name").unwrap_or("demo").to_string()
}

fn validate_name(value: &AnswerValue) -> Result<(), String> {
    let text = value.as_text().unwrap_or_default();
    text.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        .then_some(())
        .ok_or_else(|| "name may only contain letters, numbers, hyphens, or underscores".into())
}
```

`active_when` is intentionally bounded. It only applies to `Option<T>` fields,
and its controller must be an earlier scalar or choice field of the same
derived struct, named by its Rust field name; a name that does not resolve
within the struct is a compile error. The derive resolves the name through any
explicit `id` remapping, so renaming a group with `#[question(id = "project")]`
also remaps children to paths such as `project.name`.

## Wire a Command

With `#[derive(Dispatch)]`, attach the questionnaire to the command variant:

```rust
#[derive(clap::Subcommand, standout::cli::Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(questionnaire = ImportAnswers, template_name = "import")]
    Import,
}
```

The framework injects the reserved command surface:

```text
myapp import questions
myapp import questions --file answers.txt
myapp import --answers answers.txt
myapp import --answers - --yes
```

`questions` renders the blank answer sheet and has no side effects.
`--answers FILE` reads one completed sheet from a named file. `--answers -`
reads one completed sheet from piped stdin. With no `--answers`, the framework
collects the same questionnaire interactively.

Sources never merge. A file or stdin submission replaces interactive question
collection; after collection, every path goes through the same decoding,
dynamic defaults, validators, and whole-form rules.

The same configuration is available through `CommandConfig`:

```rust
use standout::cli::{App, FnHandler};

let app = App::builder()
    .command_with("import", FnHandler::new(handlers::import), |cfg| {
        cfg.template_name("import")
            .questionnaire_with_form_and_review::<ImportAnswers, _, _>(
                validate_form,
                write_review,
            )
    })?
    .build()?;
```

Use `questionnaire::<T>()` when field-level validation is enough.
Use `questionnaire_with_form::<T, _>(form)` for cross-field rules that return
`Vec<FormError>`. Use `questionnaire_with_form_and_review::<T, _, _>(form,
review)` when the user must see an application review before the confirmation
gate.

## Hook Order Around Questionnaire Resolution

`questionnaire::<T>()` and its two siblings register an ordinary pre-dispatch
hook. Pre-dispatch hooks run in the order they were registered on the
`CommandConfig`, so where you write the `questionnaire` call decides whether
your own hook sees the resolved answers:

```rust
cfg.pre_dispatch(require_answer_source)      // runs first: no answers yet
   .questionnaire::<ImportAnswers>()         // resolves, validates, confirms
   .pre_dispatch(record_submission)          // runs last: ctx.questionnaire() works
```

A hook registered *before* the questionnaire call runs before resolution and
cannot read `ctx.questionnaire()`; one registered *after* runs only if
resolution, whole-form rules and the confirmation gate all succeeded. Every
pre-dispatch hook receives the command's own `ArgMatches` — the deepest
subcommand's, the same the handler gets — so a hook can read the injected
`--answers` and `--yes` arguments directly.

Registering the same phase through both `CommandConfig` and
`AppBuilder::hooks(path, …)` is a configuration error naming the path and
phase, so one command's pre-dispatch order is always readable in one place.

## Read an Application's Own Sheet Format

`--answers` reads the preamble/fingerprint sheet `questions` renders. An
application whose own spec pins the shape of that file supplies an
`AnswerSheetFormat` instead:

```rust
use standout::input::questionnaire::{
    AnswerSheetDiagnostic, AnswerSheetFormat, Questionnaire, RawAnswers,
};

struct SpecSheet;

impl AnswerSheetFormat for SpecSheet {
    fn parse(
        &self,
        questionnaire: &Questionnaire,
        text: &str,
    ) -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>> {
        questionnaire.parse_answer_sheet_body(text)
    }
}
```

Wire it with `CommandConfig::answer_sheet_format`:

```rust
cfg.questionnaire::<ImportAnswers>()
    .answer_sheet_format(SpecSheet)
```

`parse_answer_sheet_body` reads the tagged body of a sheet without requiring
the preamble, which is the shortest way to accept a sheet the application
renders itself. A format that shares nothing with the rendered sheet fills a
`RawAnswers` directly (`set`, `set_occurrence_count`) and returns its own
diagnostics. Parsing is all the format owns: decoding, defaults, validators,
whole-form rules, review and confirmation run the same way afterwards.

## Read Answers in the Handler

Bring `CommandContextInput` into scope and read the typed questionnaire value:

```rust
use standout::cli::{CommandContext, CommandContextInput, HandlerResult, Output};

fn import(_matches: &clap::ArgMatches, ctx: &CommandContext) -> HandlerResult<serde_json::Value> {
    let answers: &ImportAnswers = ctx.questionnaire()?;
    Ok(Output::Render(serde_json::json!({
        "project": answers.project_name.as_str(),
    })))
}
```

The handler runs only after questionnaire resolution, field decoding,
whole-form rules, optional review, and confirmation have succeeded. Keep side
effects in the handler or later so a rejected confirmation writes nothing.

## Confirmation and Warnings

Questionnaire commands get `--yes` from the framework. Without it, Standout asks
for an exact `yes` on an attended controlling terminal after any configured
review. Piped stdin never confirms a run, EOF never confirms a run, and a missing
attended terminal is an error.

`CommandConfig::confirmation` makes the gate's three decisions the
application's:

```rust
use standout::cli::{Confirmation, ConfirmationAcceptance, ReviewStream};

cfg.questionnaire::<ImportAnswers>().confirmation(
    Confirmation::default()
        .prompt("Ship it? [y/N] ")
        .acceptance(ConfirmationAcceptance::YesOrY)
        .review_stream(ReviewStream::Stdout),
)
```

`ConfirmationAcceptance::Word(word)` takes that word alone and is the default
with `yes`; the reply and the word are both trimmed before they are compared, so
an empty or all-whitespace word accepts nothing and pressing Enter cannot
confirm. `YesOrY` takes `y` or `yes` in any case; `Disabled` runs without
asking, as `--yes` does. The prompt goes to the controlling terminal, and the
review a command writes goes to stderr unless `review_stream` says otherwise —
stdout is the data channel.

A hook or handler that reads `ArgMatches` for itself names the injected
arguments by their ids, `standout::cli::QUESTIONNAIRE_ANSWERS_ARG` and
`QUESTIONNAIRE_YES_ARG`.

Accepted answer sheets can still produce warnings, for example when answer text
contains a suspected `<id:` fragment. Standout queues these as framework
warnings: `App::run` renders them after the primary output, and
`standout-test::TestHarness` exposes them through `TestResult::warnings()`.

## Builder Alternative

The derive is sugar over the same public runtime model described in
[Questionnaire Answer Sheets](../crates/input/topics/answer-sheets.md). Use the
builder API when the definition is not known at compile time or when a
standalone library wants to render and decode answer sheets without depending
on `standout-macros` or `standout` command wiring.
