//! Checks that a rendered help page actually states what the `clap::Command`
//! declares.
//!
//! [`clap_facts`] walks a `Command` and produces one [`Fact`] per
//! declared detail (an arg's spelling, metavar, default, help text, a
//! subcommand's name/about/alias, ...). [`assert_page_states_clap_facts`]
//! then finds each fact's row in the page (via [`crate::page`]) and asserts
//! its text is present — catching a metavar or default that clap knows
//! about but the rendered help silently drops. [`Exemption`] opts specific
//! facts out where a framework-level convention (not clap's) legitimately
//! changes what's shown.

use crate::page::{
    candidate_metavars, contains_flag_token, contains_token, declared_metavars, find_row,
    flag_spellings, normalize, positional_row, rows, takes_values, value_placeholders, ClapJoint,
    Row,
};
use crate::TestResult;
use clap::{Arg, Command};
use standout::cli::HelpLength;
use std::collections::HashSet;
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactKind {
    Purpose,
    SubcommandName,
    SubcommandAbout,
    SubcommandAlias,
    ArgSpelling,
    ArgMetavar,
    ArgHelp,
    ArgLongHelp,
    ArgDefault,
    ArgPossibleValue,
    ArgAlias,
    Classification,
}
impl FactKind {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Command(String),
    Argument(String),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Stated,
    Suppressed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    kind: FactKind,
    subject: Subject,
    expected: String,
    presence: Presence,
    generated: bool,
}
impl Fact {
    pub fn kind(&self) -> FactKind {
        self.kind
    }
    pub fn subject(&self) -> &Subject {
        &self.subject
    }
    pub fn expected(&self) -> &str {
        &self.expected
    }
    pub fn presence(&self) -> Presence {
        self.presence
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Omission {
    ClapGeneratedSubcommands,
    Kind(FactKind),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exemption {
    pub omission: Omission,
    pub reason: &'static str,
}
pub const DELIBERATE_OMISSIONS: &[Exemption] = &[
    Exemption {
        omission: Omission::ClapGeneratedSubcommands,
        reason: "Standout installs its own `help` word and calls \
                 `disable_help_subcommand`, so a subcommand that appears only \
                 once clap builds the command is standout's own machinery \
                 rather than a destination the application declared. The \
                 extractor drops those by provenance and lists every declared \
                 subcommand, including an application's own `help`. Clap's \
                 generated *arguments* are not exempt: `-h/--help` and \
                 `-V/--version` are rows on the page.",
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
    fn covers(&self, fact: &Fact) -> bool {
        match self.omission {
            Omission::ClapGeneratedSubcommands => {
                fact.generated && matches!(fact.subject, Subject::Subcommand(_))
            }
            Omission::Kind(kind) => fact.kind == kind,
        }
    }
}
pub fn clap_facts(cmd: &Command, length: HelpLength) -> Vec<Fact> {
    let declared: HashSet<String> = cmd
        .get_arguments()
        .map(|arg| arg.get_id().to_string())
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
        let generated = clap_generates_subcommand(&built, sub);
        facts.extend(
            subcommand_facts(sub)
                .into_iter()
                .map(|fact| fact.generated(generated)),
        );
    }
    for arg in built.get_arguments() {
        let generated = clap_generates_argument(&built, arg);
        facts.extend(
            argument_facts(arg, length)
                .into_iter()
                .map(|fact| fact.generated(generated)),
        );
    }
    if classifiable(&built, &declared, length) {
        facts.push(Fact::new(
            FactKind::Classification,
            Subject::Command(name),
            "",
        ));
    }
    facts
}
/// The parent's `disable_help_subcommand` setting decides, whatever build state it is in.
fn clap_generates_subcommand(parent: &Command, sub: &Command) -> bool {
    sub.get_name() == "help" && !parent.is_disable_help_subcommand_set()
}
/// `-h/--help` unless `disable_help_flag`; `-V/--version` unless `disable_version_flag`.
fn clap_generates_argument(parent: &Command, arg: &Arg) -> bool {
    match arg.get_id().as_str() {
        "help" => !parent.is_disable_help_flag_set(),
        "version" => !parent.is_disable_version_flag_set(),
        _ => false,
    }
}
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
fn argument_facts(arg: &Arg, length: HelpLength) -> Vec<Fact> {
    let subject = Subject::Argument(arg.get_id().to_string());
    if !visible_at(arg, length) {
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
fn visible_at(arg: &Arg, length: HelpLength) -> bool {
    if arg.is_hide_set() {
        return false;
    }
    let long = matches!(length, HelpLength::Long);
    (long && !arg.is_hide_long_help_set())
        || (!long && !arg.is_hide_short_help_set())
        || arg.is_next_line_help_set()
}
fn spellings(arg: &Arg) -> Vec<String> {
    if arg.is_positional() {
        candidate_metavars(arg)
    } else {
        flag_spellings(arg)
    }
}
fn classifiable(built: &Command, declared: &HashSet<String>, length: HelpLength) -> bool {
    let mut positionals = false;
    let mut options = false;
    for arg in built
        .get_arguments()
        .filter(|arg| visible_at(arg, length) && default_headed(arg))
    {
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
fn default_headed(arg: &Arg) -> bool {
    arg.get_help_heading().is_none()
}
#[track_caller]
pub fn assert_states_clap_facts(result: &TestResult, cmd: &Command, length: HelpLength) {
    assert_page_states_clap_facts(&result.stdout_plain(), cmd, length);
}
#[track_caller]
pub fn assert_page_states_clap_facts(page: &str, cmd: &Command, length: HelpLength) {
    assert_page_states_clap_facts_with(page, cmd, length, DELIBERATE_OMISSIONS);
}
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
        if let Err(detail) = check(fact, &built, &rows, page, length) {
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
fn check(
    fact: &Fact,
    built: &Command,
    rows: &[Row<'_>],
    page: &str,
    length: HelpLength,
) -> Result<(), String> {
    match (&fact.subject, fact.kind) {
        (_, FactKind::Classification) => classification(built, rows, length),
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
fn argument_check(fact: &Fact, built: &Command, rows: &[Row<'_>]) -> Result<(), String> {
    let Subject::Argument(id) = &fact.subject else {
        return Err("the fact is not about an argument".to_string());
    };
    let Some(arg) = built.get_arguments().find(|arg| arg.get_id() == id) else {
        return Err(format!("`{id}` is not an argument of the command"));
    };
    if fact.kind == FactKind::ArgSpelling {
        let listed = if arg.is_positional() {
            rows.iter()
                .filter(|row| positional_row(row.label))
                .any(|row| {
                    value_placeholders(row.label)
                        .iter()
                        .any(|shown| shown.eq_ignore_ascii_case(&fact.expected))
                })
        } else {
            rows.iter()
                .any(|row| contains_flag_token(row.label, &fact.expected, arg))
        };
        return present(listed, fact, "no row is listed under that spelling");
    }
    let Some(row) = find_row(rows, arg) else {
        return match fact.presence {
            Presence::Suppressed => Ok(()),
            Presence::Stated => Err("the argument has no row on the page".to_string()),
        };
    };
    match fact.kind {
        FactKind::ArgMetavar => {
            let placeholders = value_placeholders(row.label);
            let shown = match declared_metavars(arg) {
                Some(_) => placeholders.iter().any(|shown| *shown == fact.expected),
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
            let stated = row.labelled_values("default:", ClapJoint::Spaces);
            present(
                stated.contains(&fact.expected),
                fact,
                &if stated.is_empty() {
                    format!(
                        "the row states no `default:` at all: {:?}",
                        row.block_text()
                    )
                } else {
                    format!("the row's stated default(s) are {stated:?}")
                },
            )
        }
        FactKind::ArgPossibleValue => {
            let names = row.possible_value_names(arg);
            present(
                names.contains(&fact.expected),
                fact,
                &if names.is_empty() {
                    format!(
                        "the row states no possible values at all: {:?}",
                        row.block_text()
                    )
                } else {
                    format!("the row's possible values are {names:?}")
                },
            )
        }
        FactKind::ArgAlias => present(
            contains_token(&row.block_text(), &fact.expected),
            fact,
            &format!("the row reads {:?}", row.block_text()),
        ),
        kind => Err(format!("no check is defined for {}", kind.label())),
    }
}
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
fn classification(built: &Command, rows: &[Row<'_>], length: HelpLength) -> Result<(), String> {
    let mut positional_sections: Vec<(String, &str)> = Vec::new();
    let mut option_sections: Vec<(String, &str)> = Vec::new();
    for arg in built
        .get_arguments()
        .filter(|arg| visible_at(arg, length) && default_headed(arg))
    {
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
