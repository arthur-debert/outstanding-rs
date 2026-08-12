# Questionnaire Answer Sheets

Long questionnaires are awkward as a sequence of terminal prompts: you cannot see the whole thing before answering, edit long answers comfortably, or keep a sheet around for a repeatable workflow. The `questionnaire` module renders an application-defined questionnaire as a prose *answer sheet* — a document that reads as questions and answers — and parses the edited document back into raw answers.

```text
#! standout-answers 1
#! questionnaire: demo.profile
#! fingerprint: sha256:2a4c…

1. What is your project called? [project.name] (string)
-> wizard-question-generator

2. Add any notes. [project.notes] (text, optional)
->
This answer may span several lines.
Internal line breaks remain part of the answer.
```

---

## Who owns what

The boundary is deliberate and narrow:

| `standout-input` owns                             | Your application owns                            |
| ------------------------------------------------- | ------------------------------------------------ |
| Definition validation (IDs, duplicates)           | The questionnaire definition itself              |
| Deterministic rendering of the answer sheet       | Decoding raw answers into domain types           |
| Parsing edited sheets into `RawAnswers`           | Whole-form validation and cross-field rules      |
| Compatibility checking (version, ID, fingerprint) | Interactive flow, review, confirmation           |
| Diagnostics with stable IDs and line numbers      | All side effects (file writes, generation, etc.) |

Parsing stops at `RawAnswers`: trimmed answer text keyed by stable field ID. The library never sees your domain model, and your domain model never leaks into the format.

## Stable identity

The bracketed token (`[project.name]`) is the *only* machine identity in a question block. Display numbers, wording, indentation, and the parenthesized type hint are cosmetic — a user may reword, renumber, or re-indent a sheet freely, and a later release of your application may copy-edit its questions without invalidating sheets already in the wild.

A field is recognized by the full header contract: a schema-recognized bracketed ID whose **next line begins with the `->` answer marker**. Everything after the marker, down to the next recognized header or end of file, is that field's answer — outer whitespace trimmed, internal line breaks preserved. Ordinary bracketed prose inside an answer (`see [project.name] above`) never opens a field, because it does not satisfy the header contract.

Header-shaped lines that carry an *unknown* ID, or repeat a known one, are diagnostics rather than silently ignored text.

## Compatibility: exact match, no migration

Every sheet's preamble pins three things:

- the answer-format version (`standout-answers 1`),
- the questionnaire ID,
- a semantic **fingerprint** of the definition.

Parsing accepts only exact matches of all three. A stale or foreign sheet gets an actionable diagnostic asking for a freshly rendered sheet — never a guessed mapping from old fields to new ones.

The fingerprint covers the *semantic* definition: field IDs, value kinds, and optionality. It ignores wording, help text, display numbers, and presentation order, so cosmetic edits keep old sheets valid while semantic changes reliably invalidate them.

**The fingerprint is a compatibility checksum, not authentication.** It does not detect tampering and does not protect the document's content.

## Sensitive content

Answer sheets are plain text files holding whatever your questions ask for — possibly credentials, internal names, or personal data. Treat a saved sheet with the same care as the answers themselves: keep it out of version control and world-readable locations, and delete it when no longer needed. Diagnostics identify fields by stable ID and line number without echoing answer values.

## Example

```rust
use standout_input::questionnaire::{Questionnaire, ScalarField, ScalarKind};

// Application-owned definition. The IDs are the stable contract;
// the wording is yours to edit at any time.
let questionnaire = Questionnaire::new(
    "demo.profile",
    vec![
        ScalarField::new("project.name", "What is your project called?", ScalarKind::String),
        ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
    ],
)?;

// Render a blank sheet for the user to edit…
let sheet = questionnaire.render_answer_sheet();

// …and later, parse the edited document back.
match questionnaire.parse_answer_sheet(&edited_text) {
    Ok(answers) => {
        // Raw text by stable ID; decoding into domain types is yours.
        let name = answers.get("project.name");
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

This first slice covers scalar fields (single- and multi-line prose answers). Nested and repeatable groups, shared decoding with interactive collection, and accumulated whole-form validation build on this same identity and compatibility model.
