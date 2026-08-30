//! Questionnaire answer sheets: render a prose questionnaire, collect
//! answers interactively or from a document, and decode them by stable
//! identity through one shared validation pipeline. Structs that derive
//! `Questionnaire` lower to this same runtime model and fill themselves from
//! validated [`Answers`] without a serde boundary.
//!
//! `standout-input` owns the reusable machinery: definition validation,
//! deterministic rendering, parsing, collection adapters, shared field
//! decoding and validation, derive-support traits, and diagnostics. The
//! application owns its questionnaire definition, whole-form rules (a
//! closure passed to [`Questionnaire::decode_answers_with`]), interactive
//! flow, review, confirmation, and side effects.
//!
//! # Rendered format
//!
//! A rendered sheet is a preamble (format version, questionnaire ID,
//! fingerprint) followed by numbered questions, each ending in a stable
//! `<id:...>` tag. A line is a *question line* iff it ends with a tag as its
//! last non-whitespace content — any trailing content, even a period,
//! demotes it to prose. The answer is everything between a question line
//! and the next one (or EOF); everything before the tag (numbering,
//! wording, indentation, hint) is cosmetic and may be freely reworded.
//! Declared defaults render pre-filled. One limitation is accepted by
//! design: an answer that itself ends in a schema-valid tag reads as a
//! question line — there is no escaping, only a warning-level diagnostic
//! ([`RawAnswers::warnings`]) when answer text contains a stray `<id:`.
//!
//! # Nested and repeatable groups
//!
//! Alongside scalar fields, a questionnaire may declare [`Group`]s: nested
//! sections answered once, or *repeatable* sections answered once per
//! submitted item within [`Repeat`] bounds. Adding an item to a repeatable
//! group is copy-the-block editing — duplicate one rendered block below the
//! last and answer the copy; a group occurrence is counted as *a line
//! ending with the group's tag*, never by numbering or prose. Definition
//! IDs never change (every input's name field is `command.inputs.name`); a
//! submitted instance is addressed by its **occurrence path**, which
//! inserts a zero-based index per enclosing repeatable-group occurrence
//! (`command.inputs[1].name`). [`Answers`] and [`RawAnswers`] are keyed by
//! occurrence path and expose [`occurrence_count`](Answers::occurrence_count);
//! indexes belong to an answer instance, not the definition, so they never
//! enter the fingerprint.
//!
//! # Decoding
//!
//! One blank rule applies everywhere: a blank answer resolves to the
//! declared default first; without one, a blank optional field is an
//! omission and a blank required field is a missing-value error. A
//! conditional field ([`ScalarField::active_when`]) is asked and enforced
//! only while its controller holds the expected value; a populated
//! *inactive* field is an error rather than silently discarded. A default
//! may instead be dynamic ([`ScalarField::with_dynamic_default`]): a
//! closure over earlier decoded answers, paired with a mandatory revision
//! that enters the fingerprint in the static default's place (closures
//! can't be hashed). Interactive collection re-prompts only on a failed
//! *entered* answer, keeping earlier answers; batch collection (file/stdin)
//! accumulates every diagnostic from one pass instead.
//!
//! # Compatibility
//!
//! The preamble pins a format version, questionnaire ID, and a semantic
//! fingerprint; parsing requires an exact match on all three; a stale sheet
//! gets a diagnostic asking for a fresh one rather than a guessed mapping.
//! The fingerprint covers every property that changes accepted answers —
//! IDs, kinds, optionality, defaults, constraints, conditions, and
//! validator/dynamic-default revisions — and ignores wording, numbering,
//! and ordering. It is a compatibility checksum only, not an
//! authentication or tamper-detection mechanism.
//!
//! # Sensitive content
//!
//! Answer sheets are plain text and may hold private or sensitive values:
//! keep saved sheets out of version control and delete them when done.
//! Diagnostics identify fields by ID and line number without echoing
//! answer values; application validators and form messages should do the
//! same.

mod collect;
mod decode;
mod definition;
mod derive;
mod fingerprint;
mod parse;
mod render;

pub use decode::{AnswerValue, Answers, EarlierAnswers, FormError, ValidationDiagnostic};
pub use definition::{
    Condition, Constraint, DynamicDefault, FieldValidator, Group, Item, Questionnaire,
    QuestionnaireError, Repeat, ScalarField, ScalarKind,
};
pub use derive::{
    QuestionnaireChoiceParseError, QuestionnaireChoices, QuestionnaireInput,
    QuestionnaireInputError,
};
pub use parse::{AnswerSheetDiagnostic, RawAnswers};
