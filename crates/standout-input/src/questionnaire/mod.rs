//! Questionnaire answer sheets: render a prose questionnaire, let a human
//! edit it as text, and parse the answers back by stable identity.
//!
//! Long questionnaires are awkward as a sequence of terminal prompts. This
//! module renders an application-defined questionnaire as a prose *answer
//! sheet* — a document that reads as questions and answers, not as a
//! configuration format — and parses an edited sheet back into raw answers.
//!
//! # Ownership boundary
//!
//! `standout-input` owns the reusable machinery: definition validation,
//! deterministic rendering, parsing, and diagnostics. The application owns
//! everything else — its questionnaire definition, decoding raw answers into
//! domain types, whole-form validation, interactive flow, review,
//! confirmation, and side effects. Parsing stops at [`RawAnswers`]: trimmed
//! answer text keyed by stable field ID, deliberately short of any
//! application model.
//!
//! # The rendered format
//!
//! ```text
//! #! standout-answers 1
//! #! questionnaire: demo.profile
//! #! fingerprint: sha256:…
//!
//! 1. What is your project called? [project.name] (string)
//! ->
//!
//! 2. Add any notes. [project.notes] (text, optional)
//! ->
//! ```
//!
//! The bracketed token is the stable machine identity. Everything else in a
//! question block — the display number, the wording, indentation, and the
//! parenthesized type hint — is cosmetic: a user (or a later release of the
//! application) may reword, renumber, or re-indent freely without changing
//! what the document means. A field is recognized only by a
//! schema-recognized bracketed ID whose next line begins with the `->`
//! answer marker; ordinary bracketed prose inside an answer never opens a
//! field. Answers keep internal line breaks and lose only outer whitespace.
//!
//! # Compatibility: exact match, no migration
//!
//! The preamble pins an answer-format version, the questionnaire ID, and a
//! semantic *fingerprint* of the definition. Parsing accepts only exact
//! matches of all three; a stale sheet gets a diagnostic asking for a
//! freshly rendered one, never a guessed field mapping. The fingerprint
//! covers the semantic definition (IDs, kinds, optionality) and ignores
//! wording, numbering, and ordering — so copy edits keep old sheets valid,
//! while semantic changes reliably invalidate them.
//!
//! The fingerprint is a compatibility checksum only. It does not
//! authenticate a document, detect tampering, or protect its content.
//!
//! # Sensitive content
//!
//! Answer sheets are plain text files that may hold whatever the questions
//! ask for — including private or sensitive values. Treat a saved sheet with
//! the same care as the answers themselves: keep it out of version control
//! and world-readable locations, and delete it when done. Diagnostics from
//! this module identify fields by ID and line number without echoing answer
//! values.
//!
//! # Round-trip example
//!
//! ```
//! use standout_input::questionnaire::{Questionnaire, ScalarField, ScalarKind};
//!
//! // The application owns this definition; IDs are the stable contract.
//! let questionnaire = Questionnaire::new(
//!     "demo.profile",
//!     vec![
//!         ScalarField::new("project.name", "What is your project called?", ScalarKind::String),
//!         ScalarField::new("project.notes", "Add any notes.", ScalarKind::Text).optional(),
//!     ],
//! )
//! .unwrap();
//!
//! // Render the blank sheet, then simulate a user editing answers in.
//! let sheet = questionnaire.render_answer_sheet();
//! let edited = sheet
//!     .replace(
//!         "1. What is your project called? [project.name] (string)\n->",
//!         "1. What is your project called? [project.name] (string)\n-> demo",
//!     )
//!     .replace(
//!         "2. Add any notes. [project.notes] (text, optional)\n->",
//!         "2. Add any notes. [project.notes] (text, optional)\n-> Spans two lines,\nlike [this] one.",
//!     );
//!
//! let answers = questionnaire.parse_answer_sheet(&edited).unwrap();
//! assert_eq!(answers.get("project.name"), Some("demo"));
//! assert_eq!(answers.get("project.notes"), Some("Spans two lines,\nlike [this] one."));
//! ```

mod definition;
mod fingerprint;
mod parse;
mod render;

pub use definition::{Questionnaire, QuestionnaireError, ScalarField, ScalarKind};
pub use parse::{AnswerSheetDiagnostic, RawAnswers};
