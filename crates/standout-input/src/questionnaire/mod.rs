//! Questionnaire answer sheets: render a prose questionnaire, collect
//! answers interactively or from a document, and decode them by stable
//! identity through one shared validation pipeline.
//!
//! Long questionnaires are awkward as a sequence of terminal prompts. This
//! module renders an application-defined questionnaire as a prose *answer
//! sheet* — a document that reads as questions and answers, not as a
//! configuration format — and collects answers from any of three sources:
//! interactive prompts, a named answer file, or explicitly requested stdin.
//! Every source normalizes into the same [`RawAnswers`] representation and
//! decodes through the same field decoders and validators, so equivalent
//! answers behave identically no matter how they arrived. Sources never
//! merge: one submission comes from exactly one source.
//!
//! # Ownership boundary
//!
//! `standout-input` owns the reusable machinery: definition validation,
//! deterministic rendering, parsing, collection adapters, shared field
//! decoding and validation, and diagnostics. The application owns everything
//! else — its questionnaire definition, whole-form rules (supplied as a
//! closure to [`Questionnaire::decode_answers_with`]), conversion of decoded
//! [`Answers`] into domain types, interactive flow, review, confirmation,
//! and side effects.
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
//! 2. License. [project.license] (mit, bsd, or gpl)
//! -> mit
//!
//! 3. Add any notes. [project.notes] (text, optional)
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
//! Declared defaults render pre-filled on the marker line.
//!
//! # Decoding: defaults, omission, conditions
//!
//! Decoding a submission ([`Questionnaire::decode_answers`]) applies one
//! blank rule everywhere: a blank answer resolves to the declared default
//! first; without a default, a blank optional field is an omission and a
//! blank required field is a missing-value error. A conditional field
//! ([`ScalarField::active_when`]) is asked and enforced only while its
//! controller holds the expected value; an inactive field may stay blank
//! (or keep its untouched pre-filled default), while a *populated* inactive
//! field is an error — stale intent is never silently discarded.
//!
//! Interactive collection gives immediate feedback: a failed answer
//! re-prompts that one question, keeping earlier answers. Batch collection
//! (file / stdin) reads the whole document and accumulates every
//! independent diagnostic — syntax, identity, missing values, conversion,
//! field validation, and the application's whole-form rules — in one pass,
//! so a sheet can be repaired in one edit.
//!
//! # Compatibility: exact match, no migration
//!
//! The preamble pins an answer-format version, the questionnaire ID, and a
//! semantic *fingerprint* of the definition. Parsing accepts only exact
//! matches of all three; a stale sheet gets a diagnostic asking for a
//! freshly rendered one, never a guessed field mapping. The fingerprint
//! covers every semantic property that changes accepted answers — IDs,
//! kinds, optionality, defaults, constraints, conditions, and declared
//! validator revisions — and ignores wording, numbering, and ordering. Copy
//! edits keep old sheets valid; semantic changes reliably invalidate them.
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
//! values; application validator and form messages should do the same.
//!
//! # Round-trip example
//!
//! ```
//! use standout_input::questionnaire::{FormError, Questionnaire, ScalarField, ScalarKind};
//!
//! // The application owns this definition; IDs are the stable contract.
//! let questionnaire = Questionnaire::new(
//!     "demo.profile",
//!     vec![
//!         ScalarField::new("project.name", "What is your project called?", ScalarKind::String),
//!         ScalarField::new("project.docker", "Use Docker?", ScalarKind::Bool)
//!             .with_default("no"),
//!         // Asked only when the controller above decodes to true.
//!         ScalarField::new("project.docker_image", "Base image?", ScalarKind::String)
//!             .active_when("project.docker", "yes"),
//!     ],
//! )
//! .unwrap();
//!
//! // Render the blank sheet, then simulate a user editing an answer in.
//! // The docker default is pre-filled; leaving it means "no", so the
//! // conditional image question may stay blank.
//! let sheet = questionnaire.render_answer_sheet();
//! let edited = sheet.replace(
//!     "[project.name] (string)\n->",
//!     "[project.name] (string)\n-> demo",
//! );
//!
//! let raw = questionnaire.parse_answer_sheet(&edited).unwrap();
//! let answers = questionnaire
//!     .decode_answers_with(&raw, |_answers| Vec::<FormError>::new())
//!     .unwrap();
//! assert_eq!(answers.get_text("project.name"), Some("demo"));
//! assert_eq!(answers.get_bool("project.docker"), Some(false));
//! assert_eq!(answers.get("project.docker_image"), None); // inactive
//! ```

mod collect;
mod decode;
mod definition;
mod fingerprint;
mod parse;
mod render;

pub use decode::{AnswerValue, Answers, FormError, ValidationDiagnostic};
pub use definition::{
    Condition, Constraint, FieldValidator, Questionnaire, QuestionnaireError, ScalarField,
    ScalarKind,
};
pub use parse::{AnswerSheetDiagnostic, RawAnswers};
