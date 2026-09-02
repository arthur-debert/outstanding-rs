//! Questionnaire answer sheets: render a prose questionnaire, collect
//! answers interactively or from a document, and decode them by stable
//! identity through one shared validation pipeline. Structs that derive
//! `Questionnaire` lower to this same runtime model and fill themselves from
//! validated [`Answers`] without a serde boundary. The sheet format, the
//! compatibility contract and the sensitive-content guidance are in
//! `standout-input/docs/topics/answer-sheets.md`.
//!
//! `standout-input` owns the reusable machinery: definition validation,
//! deterministic rendering, parsing, collection adapters, shared field
//! decoding and validation, derive-support traits, and diagnostics. The
//! application owns its questionnaire definition, whole-form rules (a
//! closure passed to [`Questionnaire::decode_answers_with`]), interactive
//! flow, review, confirmation, and side effects.
//!
//! A line is a *question line* iff it ends with a stable `<id:...>` tag as
//! its last non-whitespace content; everything before the tag is cosmetic
//! and the answer is everything up to the next question line. There is no
//! escaping: an answer that itself ends in a schema-valid tag reads as a
//! question line, and a stray `<id:` in answer text only raises a warning
//! ([`RawAnswers::warnings`]). A [`Group`] occurrence is likewise counted
//! as a line ending with the group's tag; a repeatable group's submitted
//! instances are addressed by *occurrence path* (`command.inputs[1].name`),
//! and the index belongs to the answer, never to the definition or the
//! fingerprint.
//!
//! One blank rule applies everywhere: a blank answer resolves to the
//! declared default first; without one, a blank optional field is an
//! omission and a blank required field is a missing-value error. A
//! populated *inactive* conditional field ([`ScalarField::active_when`]) is
//! an error rather than silently discarded. A dynamic default
//! ([`ScalarField::with_dynamic_default`]) is a closure over earlier
//! answers paired with a mandatory revision, which enters the fingerprint
//! in the static default's place because a closure can't be hashed.
//! Interactive collection re-prompts only on a failed *entered* answer;
//! batch collection accumulates every diagnostic from one pass.
//!
//! The fingerprint covers every property that changes accepted answers and
//! ignores wording, numbering and ordering; it is a compatibility checksum,
//! not a tamper check. That is the contract of [`StandoutAnswerSheet`], the
//! default [`AnswerSheetFormat`]; an application whose own spec pins the
//! sheet's shape supplies another and shares at most the tagged body
//! ([`Questionnaire::parse_answer_sheet_body`]).

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
pub use parse::{AnswerSheetDiagnostic, AnswerSheetFormat, RawAnswers, StandoutAnswerSheet};
