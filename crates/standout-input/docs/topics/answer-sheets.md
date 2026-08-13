# Questionnaire Answer Sheets

Long questionnaires are awkward as a sequence of terminal prompts: you cannot see the whole thing before answering, edit long answers comfortably, or keep a sheet around for a repeatable workflow. The `questionnaire` module renders an application-defined questionnaire as a prose *answer sheet* — a document that reads as questions and answers — collects answers interactively or from a document, and decodes every submission through one shared validation pipeline.

```text
#! standout-answers 1
#! questionnaire: demo.profile
#! fingerprint: sha256:2a4c…

1. What is your project called? (string) <id:project.name>
wizard-question-generator

2. License. (mit, bsd, or gpl) <id:project.license>
mit

3. Add any notes. (text, optional) <id:project.notes>
This answer may span several lines.
Internal line breaks remain part of the answer.
```

---

## Who owns what

The boundary is deliberate and narrow:

| `standout-input` owns                              | Your application owns                             |
| -------------------------------------------------- | ------------------------------------------------- |
| Definition validation (IDs, defaults, conditions)  | The questionnaire definition itself               |
| Deterministic rendering of the answer sheet        | Converting decoded `Answers` into domain types    |
| Parsing edited sheets into `RawAnswers`            | Whole-form rule *content* (a closure you supply)  |
| Collection adapters (interactive, file, stdin)     | Field-validator rule *content* (with a revision)  |
| Shared field decoding, constraints, blank rules    | Interactive flow, review, confirmation            |
| Compatibility checking (version, ID, fingerprint)  | All side effects (file writes, generation, etc.)  |
| Diagnostics with occurrence paths and line numbers |                                                   |

Every collection path stops at the same two waypoints: `RawAnswers` (trimmed answer text keyed by *occurrence path* — the stable field ID, with a zero-based index per enclosing repeatable-group occurrence, as in `command.inputs[1].name`) and, after `decode_answers`, typed `Answers`. The library never sees your domain model, and your domain model never leaks into the format.

The table above describes the standalone `standout-input` boundary. When a
questionnaire is attached to a `standout` command, the framework also owns the
standard command surface (`questions`, `--answers`, `--yes`), typed
pre-dispatch resolution, optional review callback, and attended confirmation
gate. See [Derived Questionnaires](../../../guides/derived-questionnaires.md)
for that command-integration path.

## Defining scalar fields

A `ScalarField` declares everything semantic about one question:

- **Kind** — `String` (single line), `Text` (multiline), `Bool` (decodes `true`/`false`/`yes`/`no`/`y`/`n`, case-insensitive), `Path` (single line, no filesystem checks at decode time).
- **Optionality** — `.optional()`: a blank answer without a default means omission rather than an error.
- **Default** — `.with_default("mit")`: rendered pre-filled as the answer text below the question line; during decoding, any blank answer resolves to the default *before* optionality is considered. Defaults must decode cleanly themselves.
- **Constraint** — `.one_of(["mit", "bsd", "gpl"])`: the decoded answer must be one of the choices. Enforced by the shared decoder on every path.
- **Conditional applicability** — `.active_when("project.docker", "yes")`: the field is asked and enforced only while the (earlier-declared) controller holds the expected value.
- **Application validator** — `.with_validator(FieldValidator::new("name-rules-1", …))`: your closure runs inside the shared decode stage; the *revision string* is its semantic identity (see fingerprinting below).

## Collecting answers

Three adapters, one representation — every path normalizes to `RawAnswers` and uses the same decoders and validators, so equivalent answers behave identically everywhere. Sources never merge: one submission comes from exactly one source.

- **Interactive** — `collect_interactive()` walks applicable fields through the existing prompt abstractions (and the `PromptResponder` test seam, so tests need no TTY). A decode or validation failure is local and retryable: the one question re-prompts with the diagnostic, and previously accepted answers are kept. Inactive conditional fields are skipped without prompting. EOF / Ctrl+D cancels the collection.
- **Named file** — `read_answer_sheet_file(path)` reads one complete document.
- **Explicit stdin** — `read_answer_sheet_stdin()` reads one complete document from piped stdin (for an `--answers -` style flag). An interactive terminal on stdin is an error, not a hang.

## Decoding and batch diagnostics

`decode_answers` (or `decode_answers_with`, which also runs your whole-form closure) applies one blank rule everywhere: blank → declared default → otherwise omission if optional, missing-value error if required. Conditional fields must be answered when active; an inactive field may stay blank (or keep its untouched pre-filled default), while a *populated* inactive field is an error — stale intent is never silently discarded.

Batch submissions accumulate every independent diagnostic — syntax and identity problems from parsing, then missing values, conversion failures, constraint violations, field-validator rejections, and your whole-form errors — so a sheet can be repaired in one editing pass. Diagnostics identify fields by occurrence path — the stable ID, indexed per repeatable-group occurrence — and never echo submitted values.

## Stable identity

The line-terminal tag (`<id:project.name>`) is the *only* machine identity in a question block. Display numbers, wording, indentation, and the parenthesized type hint are cosmetic — a user may reword, renumber, or re-indent a sheet freely (hints may contain any characters), and a later release of your application may copy-edit its questions without invalidating sheets already in the wild.

Recognition is one rule: a line is a **question line if and only if it ends with a schema-known `<id:...>` tag** — the tag must be the last non-whitespace content on the line, and any trailing non-blank character (even a period) demotes the line to ordinary prose. Everything between a question line and the next question line (or end of file) is that field's answer — outer whitespace trimmed, internal line breaks preserved. Bracketed prose, `->` bullets, and mid-line tag mentions inside an answer (`see <id:project.name> above`) are inert answer content.

Question lines that carry an *unknown* tag ID, or repeat a known one, are diagnostics rather than silently ignored text.

One limitation is accepted by design: an answer line that itself **ends** with a schema-valid `<id:...>` tag is read as a question line — there is no escaping mechanism, because the shape is rare in real prose. As a guard, accepted answer text containing `<id:` anywhere raises a warning-level diagnostic (`RawAnswers::warnings`), which also catches mangled or half-deleted tags, without failing the submission.

## Compatibility: exact match, no migration

Every sheet's preamble pins three things:

- the answer-format version (`standout-answers 1`),
- the questionnaire ID,
- a semantic **fingerprint** of the definition.

Parsing accepts only exact matches of all three. A stale or foreign sheet gets an actionable diagnostic asking for a freshly rendered sheet — never a guessed mapping from old fields to new ones.

The fingerprint covers every semantic property that changes which answers are accepted: field IDs, kinds, optionality, defaults, constraint choices, conditions, and declared validator revisions. It ignores wording, help text, display numbers, presentation order, and choice order, so cosmetic edits keep old sheets valid while semantic changes reliably invalidate them. Because a validator closure's behavior cannot be observed, its revision string stands in for it — bump the revision whenever the validator's accepted values change.

**The fingerprint is a compatibility checksum, not authentication.** It does not detect tampering and does not protect the document's content.

## Sensitive content

Answer sheets are plain text files holding whatever your questions ask for — possibly credentials, internal names, or personal data. Treat a saved sheet with the same care as the answers themselves: keep it out of version control and world-readable locations, and delete it when no longer needed. Diagnostics identify fields by occurrence path and line number without echoing answer values; your validator and form messages should follow the same rule.

## Example

```rust
use standout_input::questionnaire::{
    FormError, Questionnaire, ScalarField, ScalarKind,
};

// Application-owned definition. The IDs are the stable contract;
// the wording is yours to edit at any time.
let questionnaire = Questionnaire::new(
    "demo.profile",
    vec![
        ScalarField::new("project.name", "What is your project called?", ScalarKind::String),
        ScalarField::new("project.license", "License.", ScalarKind::String)
            .one_of(["mit", "bsd", "gpl"])
            .with_default("mit"),
        ScalarField::new("project.docker", "Use Docker?", ScalarKind::Bool)
            .with_default("no"),
        ScalarField::new("project.docker_image", "Base image?", ScalarKind::String)
            .active_when("project.docker", "yes"),
        ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
    ],
)
.unwrap();

// Render a blank sheet for the user to edit (defaults pre-filled)…
let sheet = questionnaire.render_answer_sheet();

// …and later, collect from wherever the caller chose — a file, piped
// stdin, or interactive prompts — then decode through the shared pipeline
// plus your whole-form rules.
let edited_text = sheet.replace(
    "<id:project.name>\n",
    "<id:project.name>\nwizard-question-generator\n",
);

// Stage 1: document → raw answers (syntax, identity, compatibility).
let raw = match questionnaire.parse_answer_sheet(&edited_text) {
    Ok(raw) => raw,
    Err(diagnostics) => {
        for diagnostic in diagnostics {
            eprintln!("{diagnostic}");
        }
        return;
    }
};

// Stage 2: raw answers → typed values (defaults, kinds, constraints,
// conditions, your field validators, your whole-form rules).
match questionnaire.decode_answers_with(&raw, |answers| {
    let mut errors = Vec::new();
    if answers.get_text("project.name") == Some("reserved") {
        errors.push(FormError::new(["project.name"], "that name is reserved"));
    }
    errors
}) {
    Ok(answers) => {
        // Typed values by stable ID; domain conversion is yours.
        let name = answers.get_text("project.name");
        let docker = answers.get_bool("project.docker");
    }
    Err(diagnostics) => {
        for diagnostic in diagnostics {
            eprintln!("{diagnostic}");
        }
    }
}
```

Rendering is deterministic: the same definition always produces the same bytes, fingerprint included, so sheets are diffable and cache-friendly.

## Current scope

This topic walks scalar fields end to end: definition, rendering with defaults, interactive / file / stdin collection, shared decoding with constraints, conditions, and application validators, and accumulated batch diagnostics. Questionnaires also support nested and repeatable groups on this same compatibility model; a submitted item inside a repeatable group is addressed by its occurrence path — the stable ID with a zero-based index per enclosing occurrence, as in `command.inputs[1].name` — never by numbering or wording. See the `questionnaire` module documentation for the group format and copy-the-block editing.
