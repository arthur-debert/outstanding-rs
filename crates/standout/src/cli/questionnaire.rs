use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;

use crate::cli::CommandContextInput;
use clap::{Arg, ArgAction, ArgMatches, Command};
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, AnswerSheetFormat, FormError, QuestionnaireInput,
    QuestionnaireInputError, RawAnswers, StandoutAnswerSheet,
};
use standout_input::{InputError, InputSourceKind, Inputs, ResolvedInput};

use crate::cli::dispatch::get_deepest_matches;
use crate::cli::handler::{CommandContext, RunError, RunErrorKind};
use crate::cli::hooks::HookError;
use crate::SetupError;

pub(crate) const QUESTIONNAIRE_INPUT_NAME: &str = "questionnaire";

pub const QUESTIONNAIRE_ANSWERS_ARG: &str = "_standout_questionnaire_answers";

pub const QUESTIONNAIRE_YES_ARG: &str = "_standout_questionnaire_yes";

pub(crate) const QUESTIONS_FILE_ARG_ID: &str = "_standout_questionnaire_questions_file";
pub(crate) const QUESTIONS_SUBCOMMAND: &str = "questions";

const CONFIRM_QUESTION: &str = "Continue? Type 'yes' to continue: ";

/// Reply and `Word` are trimmed before matching; a blank `Word` accepts nothing but a decline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationAcceptance {
    Word(String),
    YesOrY,
    Disabled,
}

/// `Stderr` is the default, so stdout stays the data channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStream {
    Stderr,
    Stdout,
}

/// The prompt goes to the controlling terminal, not to either standard stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    prompt: String,
    acceptance: ConfirmationAcceptance,
    review_stream: ReviewStream,
}

impl Default for Confirmation {
    fn default() -> Self {
        Self {
            prompt: CONFIRM_QUESTION.to_string(),
            acceptance: ConfirmationAcceptance::Word("yes".to_string()),
            review_stream: ReviewStream::Stderr,
        }
    }
}

impl Confirmation {
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    pub fn acceptance(mut self, acceptance: ConfirmationAcceptance) -> Self {
        self.acceptance = acceptance;
        self
    }

    pub fn review_stream(mut self, stream: ReviewStream) -> Self {
        self.review_stream = stream;
        self
    }

    fn accepts(&self, reply: &str) -> bool {
        let reply = reply.trim();
        match &self.acceptance {
            ConfirmationAcceptance::Word(word) => {
                let word = word.trim();
                !word.is_empty() && reply == word
            }
            ConfirmationAcceptance::YesOrY => {
                reply.eq_ignore_ascii_case("y") || reply.eq_ignore_ascii_case("yes")
            }
            ConfirmationAcceptance::Disabled => true,
        }
    }
}

pub(crate) struct QuestionnaireSettings {
    pub(crate) confirmation: Confirmation,
    pub(crate) format: Rc<dyn AnswerSheetFormat>,
}

impl Default for QuestionnaireSettings {
    fn default() -> Self {
        Self {
            confirmation: Confirmation::default(),
            format: Rc::new(StandoutAnswerSheet),
        }
    }
}

const NO_ATTENDED_TERMINAL: &str =
    "confirmation requires an attended terminal, but none is available; \
     rerun in a terminal to review and confirm, or pass --yes to continue \
     without a confirmation prompt; nothing was run";

#[cfg(feature = "test-support")]
const TERMINAL_SEAM_VAR: &str = "STANDOUT_QUESTIONNAIRE_TERMINAL";

#[derive(Clone)]
pub(crate) struct QuestionnaireCommand {
    render: Rc<dyn Fn() -> Result<String, String>>,
}

impl QuestionnaireCommand {
    pub(crate) fn new<T>() -> Self
    where
        T: QuestionnaireInput + Clone + Send + Sync + 'static,
    {
        Self {
            render: Rc::new(|| {
                T::questionnaire()
                    .map(|questionnaire| questionnaire.render_answer_sheet())
                    .map_err(|error| error.to_string())
            }),
        }
    }

    fn render_answer_sheet(&self) -> Result<String, RunError> {
        (self.render)().map_err(|error| {
            RunError::new(
                format!("questionnaire definition is invalid: {error}"),
                RunErrorKind::Handler,
            )
        })
    }
}

pub(crate) fn questionnaire_pre_dispatch<T>(
    matches: &ArgMatches,
    ctx: &mut CommandContext,
    settings: &QuestionnaireSettings,
) -> Result<(), HookError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
{
    questionnaire_pre_dispatch_with::<T, _>(matches, ctx, settings, |_| Vec::new())
}

pub(crate) fn questionnaire_pre_dispatch_with<T, F>(
    matches: &ArgMatches,
    ctx: &mut CommandContext,
    settings: &QuestionnaireSettings,
    form: F,
) -> Result<(), HookError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
    F: FnOnce(&T) -> Vec<FormError>,
{
    questionnaire_pre_dispatch_with_review::<T, F, _>(matches, ctx, settings, form, |_, _| Ok(()))
}

pub(crate) fn questionnaire_pre_dispatch_with_review<T, F, R>(
    matches: &ArgMatches,
    ctx: &mut CommandContext,
    settings: &QuestionnaireSettings,
    form: F,
    review: R,
) -> Result<(), HookError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
    F: FnOnce(&T) -> Vec<FormError>,
    R: FnOnce(&T, &mut dyn Write) -> anyhow::Result<()>,
{
    let sub_matches = get_deepest_matches(matches);
    let warnings = ctx
        .extensions
        .get::<standout_render::warnings::WarningBuffer>()
        .cloned()
        .unwrap_or_default();
    let resolved = collect_questionnaire_with::<T, F>(
        sub_matches,
        form,
        settings.format.as_ref(),
        ctx.input_sources(),
        &warnings,
    )
    .map_err(|error| {
        HookError::pre_dispatch(format!(
            "questionnaire input `{QUESTIONNAIRE_INPUT_NAME}`: {error}"
        ))
    })?;

    let assume_yes = sub_matches.get_flag(QUESTIONNAIRE_YES_ARG);
    {
        let mut stream: Box<dyn Write> = match settings.confirmation.review_stream {
            ReviewStream::Stderr => Box::new(io::stderr().lock()),
            ReviewStream::Stdout => Box::new(io::stdout().lock()),
        };
        review(&resolved.value, &mut stream)
            .map_err(|error| HookError::pre_dispatch(error.to_string()))?;
        stream
            .flush()
            .map_err(|error| HookError::pre_dispatch(error.to_string()))?;
    }
    if !assume_yes
        && !confirm_attended_from_env(&settings.confirmation)
            .map_err(|error| HookError::pre_dispatch(error.to_string()))?
    {
        return Err(HookError::pre_dispatch(
            "questionnaire confirmation declined; nothing was run",
        ));
    }

    if !ctx.extensions.contains::<Inputs>() {
        ctx.extensions.insert(Inputs::new());
    }
    let bag = ctx
        .extensions
        .get_mut::<Inputs>()
        .expect("Inputs just inserted");
    if let Some(source) = bag.source_of(QUESTIONNAIRE_INPUT_NAME) {
        return Err(HookError::pre_dispatch(format!(
            "questionnaire input `{QUESTIONNAIRE_INPUT_NAME}` conflicts with an input already resolved from {source}; `{QUESTIONNAIRE_INPUT_NAME}` is reserved for command questionnaires"
        )));
    }
    bag.insert(QUESTIONNAIRE_INPUT_NAME, resolved);
    Ok(())
}

fn collect_questionnaire_with<T, F>(
    matches: &ArgMatches,
    form: F,
    format: &dyn AnswerSheetFormat,
    sources: &standout_input::InputSources,
    warnings: &standout_render::warnings::WarningBuffer,
) -> Result<ResolvedInput<T>, InputError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
    F: FnOnce(&T) -> Vec<FormError>,
{
    let questionnaire = T::questionnaire()
        .map_err(|error| InputError::validation(format!("definition is invalid: {error}")))?;

    let read_document = |label: String,
                         read: &dyn Fn() -> Result<RawAnswers, Vec<AnswerSheetDiagnostic>>|
     -> Result<RawAnswers, InputError> {
        let raw = read().map_err(|diagnostics| {
            InputError::validation(format_diagnostics(label.clone(), &diagnostics))
        })?;
        push_raw_answer_warnings(label, raw.warnings(), warnings);
        Ok(raw)
    };

    let (raw, source) = match matches
        .get_one::<String>(QUESTIONNAIRE_ANSWERS_ARG)
        .map(String::as_str)
    {
        Some("-") => (
            read_document("from stdin".to_string(), &|| {
                questionnaire.read_answer_sheet_stdin(sources.stdin(), format)
            })?,
            InputSourceKind::Stdin,
        ),
        Some(path) => {
            let path = PathBuf::from(path);
            let label = path.display().to_string();
            (
                read_document(label, &|| {
                    questionnaire.read_answer_sheet_file(&path, format)
                })?,
                InputSourceKind::Flag,
            )
        }
        None => (
            questionnaire.collect_interactive_from(sources)?,
            InputSourceKind::Prompt,
        ),
    };

    let value = T::from_raw_answers_with(&raw, form).map_err(questionnaire_input_error)?;
    Ok(ResolvedInput { value, source })
}

fn questionnaire_input_error(error: QuestionnaireInputError) -> InputError {
    InputError::validation(error.to_string())
}

fn format_diagnostics(label: String, diagnostics: &[AnswerSheetDiagnostic]) -> String {
    let details = diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "answer sheet {label} has {} problem(s): {details}",
        diagnostics.len()
    )
}

fn push_raw_answer_warnings(
    label: String,
    diagnostics: &[AnswerSheetDiagnostic],
    warnings: &standout_render::warnings::WarningBuffer,
) {
    for diagnostic in diagnostics {
        warnings.push(format!("answer sheet {label}: {diagnostic}"));
    }
}

trait AttendedTerminal {
    fn is_attended(&self) -> bool;
    fn ask(&mut self, question: &str) -> anyhow::Result<Option<String>>;
}

struct ControllingTerminal;

#[cfg(unix)]
fn open_controlling_terminal() -> Option<(std::fs::File, std::fs::File)> {
    let read = std::fs::OpenOptions::new()
        .read(true)
        .open("/dev/tty")
        .ok()?;
    let write = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .ok()?;
    Some((read, write))
}

#[cfg(windows)]
fn open_controlling_terminal() -> Option<(std::fs::File, std::fs::File)> {
    let read = std::fs::OpenOptions::new().read(true).open("CONIN$").ok()?;
    let write = std::fs::OpenOptions::new()
        .write(true)
        .open("CONOUT$")
        .ok()?;
    Some((read, write))
}

impl AttendedTerminal for ControllingTerminal {
    fn is_attended(&self) -> bool {
        open_controlling_terminal().is_some()
    }

    fn ask(&mut self, question: &str) -> anyhow::Result<Option<String>> {
        let (read, mut write) =
            open_controlling_terminal().ok_or_else(|| anyhow::anyhow!(NO_ATTENDED_TERMINAL))?;
        write.write_all(question.as_bytes())?;
        write.flush()?;
        let mut line = String::new();
        if io::BufReader::new(read).read_line(&mut line)? == 0 {
            return Ok(None);
        }
        Ok(Some(line))
    }
}

#[cfg(feature = "test-support")]
struct ScriptedTerminal {
    attended: bool,
    replies: std::collections::VecDeque<String>,
}

#[cfg(feature = "test-support")]
impl ScriptedTerminal {
    fn absent() -> Self {
        Self {
            attended: false,
            replies: std::collections::VecDeque::new(),
        }
    }

    fn from_replies(replies: impl IntoIterator<Item = String>) -> Self {
        Self {
            attended: true,
            replies: replies.into_iter().collect(),
        }
    }
}

#[cfg(feature = "test-support")]
impl AttendedTerminal for ScriptedTerminal {
    fn is_attended(&self) -> bool {
        self.attended
    }

    fn ask(&mut self, question: &str) -> anyhow::Result<Option<String>> {
        print!("{question}");
        io::stdout().flush()?;
        Ok(self.replies.pop_front())
    }
}

fn confirm_attended_from_env(confirmation: &Confirmation) -> anyhow::Result<bool> {
    if confirmation.acceptance == ConfirmationAcceptance::Disabled {
        return Ok(true);
    }
    let mut terminal = attended_terminal_from_env()?;
    confirm_attended(terminal.as_mut(), confirmation)
}

fn attended_terminal_from_env() -> anyhow::Result<Box<dyn AttendedTerminal>> {
    #[cfg(feature = "test-support")]
    match std::env::var_os(TERMINAL_SEAM_VAR) {
        None => Ok(Box::new(ControllingTerminal)),
        Some(value) if value == "absent" => Ok(Box::new(ScriptedTerminal::absent())),
        Some(path) => {
            let script = std::fs::read_to_string(&path).map_err(|error| {
                anyhow::anyhow!(
                    "failed to read the scripted terminal replies from {}: {error}",
                    std::path::Path::new(&path).display()
                )
            })?;
            Ok(Box::new(ScriptedTerminal::from_replies(
                script.lines().map(ToOwned::to_owned),
            )))
        }
    }
    #[cfg(not(feature = "test-support"))]
    Ok(Box::new(ControllingTerminal))
}

fn confirm_attended(
    terminal: &mut dyn AttendedTerminal,
    confirmation: &Confirmation,
) -> anyhow::Result<bool> {
    if !terminal.is_attended() {
        anyhow::bail!(NO_ATTENDED_TERMINAL);
    }
    let reply = terminal.ask(&confirmation.prompt)?;
    Ok(reply.is_some_and(|line| confirmation.accepts(&line)))
}

pub(crate) fn augment_questionnaire_command(mut cmd: Command) -> Command {
    cmd = cmd.arg(
        Arg::new(QUESTIONNAIRE_ANSWERS_ARG)
            .long("answers")
            .value_name("FILE")
            .action(ArgAction::Set)
            .help("Read questionnaire answers from a file, or '-' for piped stdin"),
    );
    cmd = cmd.arg(
        Arg::new(QUESTIONNAIRE_YES_ARG)
            .long("yes")
            .action(ArgAction::SetTrue)
            .help("Bypass the attended confirmation prompt"),
    );
    cmd.subcommand(
        Command::new(QUESTIONS_SUBCOMMAND)
            .about("Render the blank questionnaire answer sheet")
            .arg(
                Arg::new(QUESTIONS_FILE_ARG_ID)
                    .long("file")
                    .value_name("FILE")
                    .action(ArgAction::Set)
                    .help("Write the answer sheet to a file instead of stdout"),
            ),
    )
}

pub(crate) fn validate_questionnaire_surface(cmd: &Command, path: &str) -> Result<(), SetupError> {
    let mut conflicts = Vec::new();
    for arg in cmd.get_arguments() {
        if let Some(long) = arg.get_long() {
            if long == "answers" || long == "yes" {
                conflicts.push(format!("--{long}"));
            }
        }
        if let Some(aliases) = arg.get_all_aliases() {
            for alias in aliases {
                if alias == "answers" || alias == "yes" {
                    conflicts.push(format!("--{alias}"));
                }
            }
        }
    }
    for subcommand in cmd.get_subcommands() {
        if subcommand.get_name() == QUESTIONS_SUBCOMMAND
            || subcommand
                .get_all_aliases()
                .any(|alias| alias == QUESTIONS_SUBCOMMAND)
        {
            conflicts.push(QUESTIONS_SUBCOMMAND.to_string());
        }
    }

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(SetupError::Config(format!(
            "questionnaire command `{path}` declares reserved name(s): {}; \
             --answers, --yes, and questions are injected by standout",
            conflicts.join(", ")
        )))
    }
}

pub(crate) fn render_questions_result(
    questionnaire: &QuestionnaireCommand,
    matches: &ArgMatches,
) -> crate::cli::handler::DispatchResult {
    let sheet = match questionnaire.render_answer_sheet() {
        Ok(sheet) => sheet,
        Err(error) => return crate::cli::handler::DispatchResult::Error(error),
    };
    let sub_matches = get_deepest_matches(matches);
    if let Some(path) = sub_matches.get_one::<String>(QUESTIONS_FILE_ARG_ID) {
        if let Err(error) = std::fs::write(path, sheet) {
            return crate::cli::handler::DispatchResult::Error(RunError::new(
                format!("Error writing questionnaire answer sheet: {error}"),
                RunErrorKind::FinalWrite(crate::cli::handler::OutputKind::Text),
            ));
        }
        crate::cli::handler::DispatchResult::Handled(crate::cli::handler::RunOutput::command(
            String::new(),
        ))
    } else {
        crate::cli::handler::DispatchResult::Handled(crate::cli::handler::RunOutput::command(sheet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingTerminal {
        asked: Vec<String>,
        reply: Option<String>,
    }

    impl RecordingTerminal {
        fn replying(reply: &str) -> Self {
            Self {
                asked: Vec::new(),
                reply: Some(reply.to_string()),
            }
        }
    }

    impl AttendedTerminal for RecordingTerminal {
        fn is_attended(&self) -> bool {
            true
        }

        fn ask(&mut self, question: &str) -> anyhow::Result<Option<String>> {
            self.asked.push(question.to_string());
            Ok(self.reply.clone())
        }
    }

    fn confirmed(reply: &str, confirmation: &Confirmation) -> bool {
        let mut terminal = RecordingTerminal::replying(reply);
        confirm_attended(&mut terminal, confirmation).unwrap()
    }

    #[test]
    fn the_default_gate_takes_only_the_word_yes() {
        let confirmation = Confirmation::default();
        assert!(confirmed("yes\n", &confirmation));
        assert!(!confirmed("y\n", &confirmation));
        assert!(!confirmed("YES\n", &confirmation));
    }

    #[test]
    fn the_y_or_yes_rule_ignores_case_and_takes_the_initial() {
        let confirmation = Confirmation::default().acceptance(ConfirmationAcceptance::YesOrY);
        assert!(confirmed("y\n", &confirmation));
        assert!(confirmed(" Yes \n", &confirmation));
        assert!(confirmed("YES\n", &confirmation));
        assert!(!confirmed("no\n", &confirmation));
        assert!(!confirmed("\n", &confirmation));
    }

    #[test]
    fn an_app_word_replaces_yes() {
        let confirmation =
            Confirmation::default().acceptance(ConfirmationAcceptance::Word("proceed".to_string()));
        assert!(confirmed("proceed\n", &confirmation));
        assert!(!confirmed("yes\n", &confirmation));
    }

    #[test]
    fn a_padded_app_word_matches_the_word_it_names() {
        let confirmation = Confirmation::default()
            .acceptance(ConfirmationAcceptance::Word(" proceed ".to_string()));
        assert!(confirmed("proceed\n", &confirmation));
        assert!(!confirmed("\n", &confirmation));
    }

    #[test]
    fn an_empty_app_word_accepts_nothing() {
        for word in ["", "   "] {
            let confirmation =
                Confirmation::default().acceptance(ConfirmationAcceptance::Word(word.to_string()));
            assert!(!confirmed("\n", &confirmation));
            assert!(!confirmed("   \n", &confirmation));
            assert!(!confirmed("yes\n", &confirmation));
        }
    }

    #[test]
    fn the_prompt_is_the_apps_wording() {
        let confirmation = Confirmation::default().prompt("Ship it? [y/N] ");
        let mut terminal = RecordingTerminal::replying("yes\n");

        confirm_attended(&mut terminal, &confirmation).unwrap();

        assert_eq!(terminal.asked, ["Ship it? [y/N] "]);
    }

    #[test]
    fn the_review_dump_defaults_to_stderr() {
        assert_eq!(Confirmation::default().review_stream, ReviewStream::Stderr);
        assert_eq!(
            Confirmation::default()
                .review_stream(ReviewStream::Stdout)
                .review_stream,
            ReviewStream::Stdout
        );
    }
}
