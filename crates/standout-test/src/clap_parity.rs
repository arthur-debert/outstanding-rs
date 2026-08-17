//! The clap-parity differential: themed help against clap's own metadata.
//!
//! Every other help assertion in the workspace checks standout's rendering
//! against standout's data model. That is the shape of the themed-help defect
//! cluster (#292–#303): the extractor copies the clap fields someone thought
//! of into a template context, and everything clap knows but the extractor did
//! not copy — `long_about`, defaults, possible values, metavars, the
//! positional-vs-option split — vanishes with nothing to notice. Before the
//! #298 fix `OptionData` had no `default` field at all, so its ten unit tests
//! could not have failed on its absence: they asserted counts and ordering of
//! a struct that was itself the bug.
//!
//! This module states the missing contract, and states it against an oracle
//! outside standout:
//!
//! > **Themed help renders every user-facing fact clap's own formatter would
//! > render for the same [`Command`] — or exempts it explicitly.**
//!
//! [`clap_facts`] walks a command's clap metadata and derives that fact list;
//! [`assert_states_clap_facts`] checks each fact against a rendered page.
//! Dropping a field from the extractor deletes rows from the page and fails
//! here, naming the argument and the value that went missing.
//!
//! # Presence of facts, never layout
//!
//! The differential asserts that a fact **is on the page**, never that the
//! page looks like clap's. Themed help exists to look different: standout
//! renders `default: brief` where clap renders `[default: brief]`, spells its
//! sections `OPTIONS` where clap spells them `Options:`, and drops clap's
//! requiredness brackets in favour of a `[metavar]` tag. Chasing byte parity
//! with clap's formatter is the trap this test must not fall into, so every
//! check here is a search for a value inside the row that owns it.
//!
//! # The allowlist
//!
//! [`DELIBERATE_OMISSIONS`] is the single place a "we do not render this"
//! lives. An exemption is visible in review, carries its reason, and is
//! *provably load-bearing*: the suite asserts that clap's own page states each
//! exempted fact (so nothing is exempted that clap would not have rendered)
//! and that removing the entry turns the page red.
//!
//! # What grounds the derivation
//!
//! The fact list is only an oracle if clap agrees it is clap's. Run the same
//! assertion over [`Command::render_long_help`]'s output with no exemptions
//! and it passes — that is the suite's grounding test, and it is what stops
//! this module from drifting into asserting standout's habits back at itself.

use std::collections::HashSet;
use std::fmt;

use clap::{Arg, Command};
use standout::cli::HelpLength;

use crate::page::{
    candidate_metavars, contains_token, declared_metavars, find_row, flag_spellings, metavar_text,
    normalize, rows, takes_values, value_placeholders, Row,
};
use crate::TestResult;

// ---------------------------------------------------------------------------
// The fact model
// ---------------------------------------------------------------------------

/// The kind of user-facing fact a [`Fact`] carries.
///
/// A kind decides *where* the fact is looked for — a possible value belongs on
/// its argument's possible-values line, an about paragraph anywhere on the
/// page — and it is the granularity [`DELIBERATE_OMISSIONS`] exempts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactKind {
    /// The command's `about` (`-h`) or `long_about` (`--help`, the `help`
    /// word), which is what makes the two help lengths differ at all.
    Purpose,
    /// A subcommand is listed, under the name the user types.
    SubcommandName,
    /// A subcommand's `about`.
    SubcommandAbout,
    /// A subcommand's visible alias.
    SubcommandAlias,
    /// An argument is listed, under a spelling the user types (`-c`,
    /// `--color`, or a positional's metavar).
    ArgSpelling,
    /// A value-taking argument's value name, so the row says it takes a value.
    ArgMetavar,
    /// An argument's `help`.
    ArgHelp,
    /// An argument's `long_help`, which clap renders in place of `help` under
    /// `--help`.
    ArgLongHelp,
    /// An argument's default value.
    ArgDefault,
    /// One of an argument's possible values.
    ArgPossibleValue,
    /// An argument's visible alias.
    ArgAlias,
    /// Positionals are listed apart from options, the way clap splits
    /// `Arguments:` from `Options:` — the classification #302's cluster made
    /// invisible by rendering a valued option as if it were a flag.
    Classification,
}

impl FactKind {
    /// The kind's name as an exemption or a failure spells it.
    fn label(self) -> &'static str {
        match self {
            FactKind::Purpose => "about",
            FactKind::SubcommandName => "subcommand",
            FactKind::SubcommandAbout => "subcommand about",
            FactKind::SubcommandAlias => "subcommand alias",
            FactKind::ArgSpelling => "spelling",
            FactKind::ArgMetavar => "metavar",
            FactKind::ArgHelp => "help",
            FactKind::ArgLongHelp => "long help",
            FactKind::ArgDefault => "default value",
            FactKind::ArgPossibleValue => "possible value",
            FactKind::ArgAlias => "alias",
            FactKind::Classification => "positional/option classification",
        }
    }
}

/// What a fact is about: the command itself, one of its arguments, or one of
/// its subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// The command whose page was rendered.
    Command(String),
    /// An argument, by clap id.
    Argument(String),
    /// A subcommand, by name.
    Subcommand(String),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::Command(name) => write!(f, "command `{name}`"),
            Subject::Argument(id) => write!(f, "argument `{id}`"),
            Subject::Subcommand(name) => write!(f, "subcommand `{name}`"),
        }
    }
}

/// Whether the page must state a fact or must not.
///
/// Clap's formatter suppresses as deliberately as it prints: no possible
/// values and no default for an argument that takes no value (`--staged true`
/// is not syntax the user may type — #301), nothing at all for a hidden
/// argument or a hidden possible value. A differential that only checked
/// presence would call a page that prints all of them parity-clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Clap's formatter would print this; themed help must too.
    Stated,
    /// Clap's formatter suppresses this; themed help must not print it.
    Suppressed,
}

/// One user-facing fact clap's own formatter would state — or suppress — for a
/// command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    kind: FactKind,
    subject: Subject,
    expected: String,
    presence: Presence,
    generated: bool,
}

impl Fact {
    /// What kind of fact this is.
    pub fn kind(&self) -> FactKind {
        self.kind
    }

    /// What the fact is about.
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The text the page must state (or must not).
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// Whether clap would print this fact or suppress it.
    pub fn presence(&self) -> Presence {
        self.presence
    }

    /// Whether clap generated the subject itself while building the command —
    /// its `-h/--help` and `-V/--version` arguments, which no application
    /// declared.
    pub fn is_clap_generated(&self) -> bool {
        self.generated
    }

    fn new(kind: FactKind, subject: Subject, expected: impl Into<String>) -> Self {
        Self {
            kind,
            subject,
            expected: expected.into(),
            presence: Presence::Stated,
            generated: false,
        }
    }

    fn suppressed(mut self) -> Self {
        self.presence = Presence::Suppressed;
        self
    }

    fn generated(mut self, generated: bool) -> Self {
        self.generated = generated;
        self
    }
}

impl fmt::Display for Fact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.subject, self.kind.label())?;
        if !self.expected.is_empty() {
            write!(f, " {:?}", self.expected)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The allowlist
// ---------------------------------------------------------------------------

/// What one [`Exemption`] covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Omission {
    /// Every fact owned by a subject clap generated while building the command
    /// — its `-h/--help` and `-V/--version` arguments and its `help`
    /// subcommand — rather than one the application declared.
    ClapGeneratedSubjects,
    /// Every fact of one kind.
    Kind(FactKind),
}

/// One deliberate omission, with the reason it is deliberate.
///
/// The reason is the point: an unexplained exemption is indistinguishable from
/// a forgotten field, which is the failure mode this differential exists to
/// end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exemption {
    /// What is not asserted.
    pub omission: Omission,
    /// Why it is not asserted.
    pub reason: &'static str,
}

/// The single allowlist of facts themed help deliberately does not render.
///
/// Everything clap's formatter prints and standout's does not is either a bug
/// or an entry here. Each entry is proved load-bearing by the suite: clap's
/// own page states the fact (so the exemption is a real difference, not an
/// invented one), and removing the entry turns standout's page red (so the
/// exemption is doing work rather than describing something already
/// rendered).
pub const DELIBERATE_OMISSIONS: &[Exemption] = &[
    Exemption {
        omission: Omission::ClapGeneratedSubjects,
        reason: "Standout renders no row for clap's own `-h/--help` and \
                 `-V/--version`: its help affordances are the `help` word in \
                 COMMANDS and the flags clap short-circuits on before standout \
                 renders anything. The page therefore never tells a reader the \
                 flags exist — a real gap, exempted here rather than silently \
                 unasserted, and left to the work that is allowed to change \
                 rendered bytes. Clap's generated `help` *subcommand* is exempt \
                 for a second, deliberate reason: standout drops a COMMANDS \
                 section whose only entry is the word that printed it, because \
                 that is a section about standout rather than about the \
                 application (see `only_the_help_word` in the extractor).",
    },
    Exemption {
        omission: Omission::Kind(FactKind::ArgLongHelp),
        reason: "The extractor copies `Arg::get_help()` only, so an argument's \
                 `long_help` never reaches the page and `--help` shows the same \
                 rows as `-h`. The command-level half of the distinction — \
                 `about` versus `long_about` — is asserted, so what is exempt \
                 is the per-argument half alone.",
    },
    Exemption {
        omission: Omission::Kind(FactKind::ArgAlias),
        reason: "Clap prints `[aliases: -t, --thr]` on an argument's row; \
                 standout's `OptionData` carries the canonical spelling only. \
                 An alias is discoverability, not syntax the canonical spelling \
                 cannot express.",
    },
    Exemption {
        omission: Omission::Kind(FactKind::SubcommandAlias),
        reason: "Clap prints `[alias: st]` beside a subcommand's about; \
                 standout's COMMANDS rows carry the canonical name only, for \
                 the same reason as argument aliases.",
    },
];

impl Exemption {
    /// Whether this exemption covers `fact`.
    fn covers(&self, fact: &Fact) -> bool {
        match self.omission {
            Omission::ClapGeneratedSubjects => fact.generated,
            Omission::Kind(kind) => fact.kind == kind,
        }
    }
}

// ---------------------------------------------------------------------------
// Deriving the facts from clap
// ---------------------------------------------------------------------------

/// Every user-facing fact clap's own formatter would state for `cmd` at
/// `length`, plus the ones it deliberately suppresses.
///
/// The command is built first (clap resolves value parsers, arities and its
/// own `--help` argument during build), so the derivation reads the same
/// metadata clap's formatter reads rather than a half-populated declaration.
/// Facts owned by arguments that appear only after the build are marked
/// [`Fact::is_clap_generated`], which is how the allowlist can exempt clap's
/// machinery without naming `--help` in a string.
pub fn clap_facts(cmd: &Command, length: HelpLength) -> Vec<Fact> {
    let declared: HashSet<String> = cmd
        .get_arguments()
        .map(|arg| arg.get_id().to_string())
        .collect();
    let declared_subcommands: HashSet<String> = cmd
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();

    let mut built = cmd.clone();
    built.build();

    let mut facts = Vec::new();
    let name = built.get_name().to_string();

    let purpose = match length {
        HelpLength::Long => built.get_long_about().or_else(|| built.get_about()),
        HelpLength::Short => built.get_about(),
    };
    if let Some(about) = purpose {
        facts.push(Fact::new(
            FactKind::Purpose,
            Subject::Command(name.clone()),
            about.to_string(),
        ));
    }

    for sub in built.get_subcommands() {
        let generated = !declared_subcommands.contains(sub.get_name());
        facts.extend(
            subcommand_facts(sub)
                .into_iter()
                .map(|fact| fact.generated(generated)),
        );
    }

    for arg in built.get_arguments() {
        let generated = !declared.contains(arg.get_id().as_str());
        facts.extend(
            argument_facts(arg, length)
                .into_iter()
                .map(|fact| fact.generated(generated)),
        );
    }

    if classifiable(&built, &declared) {
        facts.push(Fact::new(
            FactKind::Classification,
            Subject::Command(name),
            "",
        ));
    }

    facts
}

/// The facts a parent page states about one subcommand.
fn subcommand_facts(sub: &Command) -> Vec<Fact> {
    let name = sub.get_name().to_string();
    let subject = Subject::Subcommand(name.clone());

    if sub.is_hide_set() {
        return vec![Fact::new(FactKind::SubcommandName, subject, name).suppressed()];
    }

    let mut facts = vec![Fact::new(
        FactKind::SubcommandName,
        subject.clone(),
        name.clone(),
    )];
    if let Some(about) = sub.get_about() {
        facts.push(Fact::new(
            FactKind::SubcommandAbout,
            subject.clone(),
            about.to_string(),
        ));
    }
    for alias in sub.get_visible_aliases() {
        facts.push(Fact::new(FactKind::SubcommandAlias, subject.clone(), alias));
    }
    facts
}

/// The facts a page states about one argument, at one help length.
fn argument_facts(arg: &Arg, length: HelpLength) -> Vec<Fact> {
    let subject = Subject::Argument(arg.get_id().to_string());

    if arg.is_hide_set() {
        return spellings(arg)
            .into_iter()
            .map(|spelling| {
                Fact::new(FactKind::ArgSpelling, subject.clone(), spelling).suppressed()
            })
            .collect();
    }

    let mut facts: Vec<Fact> = spellings(arg)
        .into_iter()
        .map(|spelling| Fact::new(FactKind::ArgSpelling, subject.clone(), spelling))
        .collect();

    if takes_values(arg) {
        for metavar in candidate_metavars(arg) {
            facts.push(Fact::new(FactKind::ArgMetavar, subject.clone(), metavar));
        }
    }

    // Clap renders `long_help` *instead of* `help` under `--help`, so the two
    // are one fact whose kind says which text the page owes the reader.
    let (kind, text) = match length {
        HelpLength::Long => match arg.get_long_help() {
            Some(long) => (FactKind::ArgLongHelp, Some(long)),
            None => (FactKind::ArgHelp, arg.get_help()),
        },
        HelpLength::Short => (FactKind::ArgHelp, arg.get_help()),
    };
    if let Some(text) = text {
        facts.push(Fact::new(kind, subject.clone(), text.to_string()));
    }

    // Clap suppresses a default and a possible-values list for an argument
    // that takes no value: they name syntax the user cannot type.
    let shows_values = takes_values(arg);

    for default in arg.get_default_values() {
        let fact = Fact::new(
            FactKind::ArgDefault,
            subject.clone(),
            default.to_string_lossy().into_owned(),
        );
        facts.push(if shows_values && !arg.is_hide_default_value_set() {
            fact
        } else {
            fact.suppressed()
        });
    }

    for value in arg.get_possible_values() {
        let fact = Fact::new(
            FactKind::ArgPossibleValue,
            subject.clone(),
            value.get_name(),
        );
        facts.push(
            if shows_values && !arg.is_hide_possible_values_set() && !value.is_hide_set() {
                fact
            } else {
                fact.suppressed()
            },
        );
    }

    if let Some(aliases) = arg.get_visible_aliases() {
        for alias in aliases {
            facts.push(Fact::new(
                FactKind::ArgAlias,
                subject.clone(),
                format!("--{alias}"),
            ));
        }
    }
    if let Some(aliases) = arg.get_visible_short_aliases() {
        for alias in aliases {
            facts.push(Fact::new(
                FactKind::ArgAlias,
                subject.clone(),
                format!("-{alias}"),
            ));
        }
    }

    facts
}

/// Every spelling the page may list an argument under: a positional's value
/// name, or a flag's short and long forms.
fn spellings(arg: &Arg) -> Vec<String> {
    if arg.is_positional() {
        candidate_metavars(arg)
    } else {
        flag_spellings(arg)
    }
}

/// Whether the command has both a visible positional and a visible declared
/// option, which is what makes "listed apart" a statement about it at all.
fn classifiable(built: &Command, declared: &HashSet<String>) -> bool {
    let mut positionals = false;
    let mut options = false;
    for arg in built.get_arguments().filter(|arg| !arg.is_hide_set()) {
        if !declared.contains(arg.get_id().as_str()) {
            continue;
        }
        if arg.is_positional() {
            positionals = true;
        } else {
            options = true;
        }
    }
    positionals && options
}

// ---------------------------------------------------------------------------
// Checking the facts against a page
// ---------------------------------------------------------------------------

/// Panics unless the run's page states every fact clap's own formatter would
/// state for `cmd`, save the [`DELIBERATE_OMISSIONS`].
///
/// `length` is the help length the entry point asked for: [`HelpLength::Short`]
/// for `-h`, [`HelpLength::Long`] for `--help` and the `help` word. It is not
/// a cosmetic axis — it decides whether the page owes `about` or `long_about`,
/// which is one of the facts the extractor could drop unnoticed.
#[track_caller]
pub fn assert_states_clap_facts(result: &TestResult, cmd: &Command, length: HelpLength) {
    assert_page_states_clap_facts(&result.stdout_plain(), cmd, length);
}

/// [`assert_states_clap_facts`] against a page directly.
#[track_caller]
pub fn assert_page_states_clap_facts(page: &str, cmd: &Command, length: HelpLength) {
    assert_page_states_clap_facts_with(page, cmd, length, DELIBERATE_OMISSIONS);
}

/// [`assert_page_states_clap_facts`] with an explicit allowlist.
///
/// Pass `&[]` to assert full parity — that is how the suite grounds the
/// derivation against clap's own rendered page, and how it proves each entry
/// of [`DELIBERATE_OMISSIONS`] is load-bearing rather than decorative.
#[track_caller]
pub fn assert_page_states_clap_facts_with(
    page: &str,
    cmd: &Command,
    length: HelpLength,
    exemptions: &[Exemption],
) {
    let facts = clap_facts(cmd, length);
    let mut built = cmd.clone();
    built.build();

    let rows = rows(page);
    let mut failures: Vec<String> = Vec::new();
    let mut exempted = 0usize;

    for fact in &facts {
        if exemptions.iter().any(|exemption| exemption.covers(fact)) {
            exempted += 1;
            continue;
        }
        if let Err(detail) = check(fact, &built, &rows, page) {
            failures.push(format!("  - {fact}: {detail}"));
        }
    }

    if failures.is_empty() {
        return;
    }

    panic!(
        "the rendered page drops {} of the {} fact(s) clap states for `{}` \
         ({} exempted by the allowlist):\n{}\n--- page ---\n{}\n------------",
        failures.len(),
        facts.len(),
        built.get_name(),
        exempted,
        failures.join("\n"),
        page
    );
}

/// Checks one fact against the parsed page, describing the failure.
///
/// Where a fact is looked for follows its subject: a paragraph belongs to the
/// page, everything else to the row that lists its owner. Reading an
/// argument's default off the whole page would let another argument's row
/// satisfy it.
fn check(fact: &Fact, built: &Command, rows: &[Row<'_>], page: &str) -> Result<(), String> {
    match (&fact.subject, fact.kind) {
        (_, FactKind::Classification) => classification(built, rows),
        (Subject::Command(_), FactKind::Purpose) => present(
            normalize(page).contains(&normalize(&fact.expected)),
            fact,
            "the page does not carry the paragraph",
        ),
        (Subject::Subcommand(name), kind) => subcommand_check(fact, name, kind, rows),
        (Subject::Argument(_), _) => argument_check(fact, built, rows),
        (subject, kind) => Err(format!(
            "no check is defined for {subject} {}",
            kind.label()
        )),
    }
}

/// Checks a fact owned by a subcommand, against the row that lists it.
fn subcommand_check(
    fact: &Fact,
    name: &str,
    kind: FactKind,
    rows: &[Row<'_>],
) -> Result<(), String> {
    let row = rows.iter().find(|row| contains_token(row.label, name));

    if kind == FactKind::SubcommandName {
        return present(row.is_some(), fact, "no row is listed under that name");
    }

    let Some(row) = row else {
        // The name is the fact that fails first; a subcommand with no row
        // cannot state its about or its aliases either, and saying so twice
        // buries the one failure worth reading.
        return Ok(());
    };

    let found = match kind {
        FactKind::SubcommandAlias => contains_token(&row.block_text(), &fact.expected),
        _ => row.block_text().contains(&normalize(&fact.expected)),
    };
    present(
        found,
        fact,
        &format!("the row reads {:?}", row.block_text()),
    )
}

/// Checks a fact owned by an argument, which is looked for inside that
/// argument's own row rather than anywhere on the page: a value that appears
/// under some *other* argument is not this argument's row stating it.
fn argument_check(fact: &Fact, built: &Command, rows: &[Row<'_>]) -> Result<(), String> {
    let Subject::Argument(id) = &fact.subject else {
        return Err("the fact is not about an argument".to_string());
    };
    let Some(arg) = built.get_arguments().find(|arg| arg.get_id() == id) else {
        return Err(format!("`{id}` is not an argument of the command"));
    };

    // A spelling is what identifies a row, so it is checked against every
    // row's label rather than through the row lookup it would otherwise be
    // assumed by.
    if fact.kind == FactKind::ArgSpelling {
        let listed = rows.iter().any(|row| {
            contains_token(row.label, &fact.expected)
                || row
                    .label
                    .split_whitespace()
                    .next()
                    .map(metavar_text)
                    .is_some_and(|first| first.eq_ignore_ascii_case(&fact.expected))
        });
        return present(listed, fact, "no row is listed under that spelling");
    }

    let Some(row) = find_row(rows, arg) else {
        // A suppressed fact is satisfied by the row's absence; a stated one
        // cannot be, and the missing row is the more useful failure.
        return match fact.presence {
            Presence::Suppressed => Ok(()),
            Presence::Stated => Err("the argument has no row on the page".to_string()),
        };
    };

    match fact.kind {
        FactKind::ArgMetavar => {
            let placeholders = value_placeholders(row.label);
            let shown = match declared_metavars(arg) {
                // A declared value name must appear as declared: an app writes
                // `value_name = "RATIO"` precisely so the user reads `RATIO`.
                Some(_) => placeholders.iter().any(|shown| *shown == fact.expected),
                // Clap's fallback uppercases the id and standout's does not,
                // and neither spelling loses the reader.
                None => placeholders
                    .iter()
                    .any(|shown| shown.eq_ignore_ascii_case(&fact.expected)),
            };
            present(
                shown,
                fact,
                &format!("the row's value placeholders are {placeholders:?}"),
            )
        }
        FactKind::ArgHelp | FactKind::ArgLongHelp => {
            let text = row.block_text();
            present(
                text.contains(&normalize(&fact.expected)),
                fact,
                &format!("the row reads {text:?}"),
            )
        }
        FactKind::ArgDefault => {
            labelled(row, "default:", fact, |line| line.contains(&fact.expected))
        }
        FactKind::ArgPossibleValue => labelled(row, "possible values:", fact, |line| {
            contains_token(line, &fact.expected)
        }),
        FactKind::ArgAlias => present(
            contains_token(&row.block_text(), &fact.expected),
            fact,
            &format!("the row reads {:?}", row.block_text()),
        ),
        kind => Err(format!("no check is defined for {}", kind.label())),
    }
}

/// Checks a fact stated under a label in the row's block (`default: brief`,
/// `possible values: brief, full, none`).
///
/// Reading the labelled remainder rather than the whole row is what keeps the
/// check honest in both directions: it cannot pass because the word appears in
/// the description, and it cannot fail because the page spells its label
/// differently from clap's `[default: …]`. The label carries its colon for the
/// same reason — a description that says "the default profile" is prose about
/// a default, not the row stating one.
fn labelled(
    row: &Row<'_>,
    label: &str,
    fact: &Fact,
    matches: impl Fn(&str) -> bool,
) -> Result<(), String> {
    let lines = row.labelled(label);
    let found = lines.iter().any(|line| matches(line));
    present(
        found,
        fact,
        &if lines.is_empty() {
            format!("the row states no `{label}` at all: {:?}", row.block_text())
        } else {
            format!("the row's `{label}` reads {lines:?}")
        },
    )
}

/// Turns a boolean observation into a result, respecting the fact's polarity.
fn present(found: bool, fact: &Fact, detail: &str) -> Result<(), String> {
    match (fact.presence, found) {
        (Presence::Stated, true) | (Presence::Suppressed, false) => Ok(()),
        (Presence::Stated, false) => {
            Err(format!("clap states it and the page does not — {detail}"))
        }
        (Presence::Suppressed, true) => Err(format!(
            "clap suppresses it and the page states it anyway — {detail}"
        )),
    }
}

/// Checks that positionals and options are listed apart.
///
/// The sections are matched by identity, never by name: standout spells them
/// `ARGUMENTS`/`OPTIONS` and clap `Arguments:`/`Options:`, and asserting
/// either spelling would be asserting layout. What has to hold is that a
/// reader can tell which arguments are typed by position and which by flag —
/// the distinction a single merged list destroys.
fn classification(built: &Command, rows: &[Row<'_>]) -> Result<(), String> {
    let mut positional_sections: Vec<(String, &str)> = Vec::new();
    let mut option_sections: Vec<(String, &str)> = Vec::new();

    for arg in built.get_arguments().filter(|arg| !arg.is_hide_set()) {
        let Some(row) = find_row(rows, arg) else {
            continue;
        };
        let entry = (arg.get_id().to_string(), row.section);
        if arg.is_positional() {
            positional_sections.push(entry);
        } else {
            option_sections.push(entry);
        }
    }

    let (Some((_, positional_section)), Some((_, option_section))) =
        (positional_sections.first(), option_sections.first())
    else {
        return Ok(());
    };

    if let Some((id, section)) = positional_sections
        .iter()
        .find(|(_, section)| section != positional_section)
    {
        return Err(format!(
            "positional `{id}` is listed under {section:?} while another is under \
             {positional_section:?}"
        ));
    }
    if let Some((id, section)) = option_sections
        .iter()
        .find(|(_, section)| section != option_section)
    {
        return Err(format!(
            "option `{id}` is listed under {section:?} while another is under \
             {option_section:?}"
        ));
    }
    if positional_section == option_section {
        return Err(format!(
            "positionals and options share the section {positional_section:?}, so the \
             page does not say which arguments are typed by position"
        ));
    }

    Ok(())
}
