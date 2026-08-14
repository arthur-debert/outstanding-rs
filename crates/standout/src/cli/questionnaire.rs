//! Framework-owned questionnaire command surface.
//!
//! A command configured with a questionnaire gets reserved clap surface from
//! the app builder (`--answers`, `--yes`, and a `questions` subcommand). The
//! framework resolves and validates the filled application type before
//! dispatch, optionally lets the application render a review, runs the
//! attended confirmation gate, and then the handler reads the filled type from
//! [`CommandContextInput::questionnaire`](crate::cli::CommandContextInput::questionnaire).
//! File and stdin sheets keep non-fatal parse diagnostics visible by queuing
//! their `RawAnswers` warnings for the framework warning flush.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::{Arg, ArgAction, ArgMatches, Command};
use standout_input::env::DefaultStdin;
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, FormError, QuestionnaireInput, QuestionnaireInputError, RawAnswers,
};
use standout_input::{InputError, InputSourceKind, Inputs, ResolvedInput};

use crate::cli::dispatch::get_deepest_matches;
use crate::cli::handler::{CommandContext, RunError, RunErrorKind};
use crate::cli::hooks::HookError;
use crate::SetupError;

pub(crate) const QUESTIONNAIRE_INPUT_NAME: &str = "questionnaire";
pub(crate) const ANSWERS_ARG_ID: &str = "_standout_questionnaire_answers";
pub(crate) const YES_ARG_ID: &str = "_standout_questionnaire_yes";
pub(crate) const QUESTIONS_FILE_ARG_ID: &str = "_standout_questionnaire_questions_file";
pub(crate) const QUESTIONS_SUBCOMMAND: &str = "questions";

const CONFIRM_QUESTION: &str = "Continue? Type 'yes' to continue: ";

const NO_ATTENDED_TERMINAL: &str =
    "confirmation requires an attended terminal, but none is available; \
     rerun in a terminal to review and confirm, or pass --yes to continue \
     without a confirmation prompt; nothing was run";

#[cfg(debug_assertions)]
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
) -> Result<(), HookError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
{
    questionnaire_pre_dispatch_with::<T, _>(matches, ctx, |_| Vec::new())
}

pub(crate) fn questionnaire_pre_dispatch_with<T, F>(
    matches: &ArgMatches,
    ctx: &mut CommandContext,
    form: F,
) -> Result<(), HookError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
    F: FnOnce(&T) -> Vec<FormError>,
{
    questionnaire_pre_dispatch_with_review::<T, F, _>(matches, ctx, form, |_, _| Ok(()))
}

pub(crate) fn questionnaire_pre_dispatch_with_review<T, F, R>(
    matches: &ArgMatches,
    ctx: &mut CommandContext,
    form: F,
    review: R,
) -> Result<(), HookError>
where
    T: QuestionnaireInput + Clone + Send + Sync + 'static,
    F: FnOnce(&T) -> Vec<FormError>,
    R: FnOnce(&T, &mut dyn Write) -> anyhow::Result<()>,
{
    let sub_matches = get_deepest_matches(matches);
    let resolved = collect_questionnaire_with::<T, F>(sub_matches, form).map_err(|error| {
        HookError::pre_dispatch(format!(
            "questionnaire input `{QUESTIONNAIRE_INPUT_NAME}`: {error}"
        ))
    })?;

    let assume_yes = sub_matches.get_flag(YES_ARG_ID);
    {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        review(&resolved.value, &mut stdout)
            .map_err(|error| HookError::pre_dispatch(error.to_string()))?;
        stdout
            .flush()
            .map_err(|error| HookError::pre_dispatch(error.to_string()))?;
    }
    if !assume_yes
        && !confirm_attended_from_env()
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

/// Collect one questionnaire submission from exactly one source — the
/// `--answers` file, explicit stdin (`--answers -`), or interactive
/// prompting when the flag is absent — then decode it into the typed value.
///
/// Document sources surface their non-fatal parse warnings through the
/// framework warning flush, labeled by origin; fatal parse diagnostics
/// become one validation error carrying every accumulated diagnostic.
fn collect_questionnaire_with<T, F>(
    matches: &ArgMatches,
    form: F,
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
        push_raw_answer_warnings(label, raw.warnings());
        Ok(raw)
    };

    let (raw, source) = match matches
        .get_one::<String>(ANSWERS_ARG_ID)
        .map(String::as_str)
    {
        Some("-") => (
            read_document("from stdin".to_string(), &|| {
                questionnaire.read_answer_sheet_stdin_with(&DefaultStdin)
            })?,
            InputSourceKind::Stdin,
        ),
        Some(path) => {
            let path = PathBuf::from(path);
            let label = path.display().to_string();
            (
                read_document(label, &|| questionnaire.read_answer_sheet_file(&path))?,
                InputSourceKind::Flag,
            )
        }
        None => (
            questionnaire.collect_interactive()?,
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

fn push_raw_answer_warnings(label: String, diagnostics: &[AnswerSheetDiagnostic]) {
    for diagnostic in diagnostics {
        standout_render::warnings::push_warning(format!("answer sheet {label}: {diagnostic}"));
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

#[cfg(any(test, debug_assertions))]
struct ScriptedTerminal {
    attended: bool,
    replies: std::collections::VecDeque<String>,
}

#[cfg(any(test, debug_assertions))]
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

#[cfg(any(test, debug_assertions))]
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

fn confirm_attended_from_env() -> anyhow::Result<bool> {
    let mut terminal = attended_terminal_from_env()?;
    confirm_attended(terminal.as_mut())
}

fn attended_terminal_from_env() -> anyhow::Result<Box<dyn AttendedTerminal>> {
    #[cfg(debug_assertions)]
    match std::env::var_os(TERMINAL_SEAM_VAR) {
        None => Ok(Box::new(ControllingTerminal)),
        Some(value) if value == "absent" => Ok(Box::new(ScriptedTerminal::absent())),
        Some(path) => {
            let script = std::fs::read_to_string(&path).map_err(|error| {
                anyhow::anyhow!(
                    "failed to read the scripted terminal replies from {}: {error}",
                    Path::new(&path).display()
                )
            })?;
            Ok(Box::new(ScriptedTerminal::from_replies(
                script.lines().map(ToOwned::to_owned),
            )))
        }
    }
    #[cfg(not(debug_assertions))]
    Ok(Box::new(ControllingTerminal))
}

fn confirm_attended(terminal: &mut dyn AttendedTerminal) -> anyhow::Result<bool> {
    if !terminal.is_attended() {
        anyhow::bail!(NO_ATTENDED_TERMINAL);
    }
    let reply = terminal.ask(CONFIRM_QUESTION)?;
    Ok(reply.is_some_and(|line| line.trim() == "yes"))
}

pub(crate) fn augment_questionnaire_command(mut cmd: Command) -> Command {
    cmd = cmd.arg(
        Arg::new(ANSWERS_ARG_ID)
            .long("answers")
            .value_name("FILE")
            .action(ArgAction::Set)
            .help("Read questionnaire answers from a file, or '-' for piped stdin"),
    );
    cmd = cmd.arg(
        Arg::new(YES_ARG_ID)
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
) -> crate::cli::handler::RunResult {
    let sheet = match questionnaire.render_answer_sheet() {
        Ok(sheet) => sheet,
        Err(error) => return crate::cli::handler::RunResult::Error(error),
    };
    let sub_matches = get_deepest_matches(matches);
    if let Some(path) = sub_matches.get_one::<String>(QUESTIONS_FILE_ARG_ID) {
        if let Err(error) = std::fs::write(path, sheet) {
            return crate::cli::handler::RunResult::Error(RunError::new(
                format!("Error writing questionnaire answer sheet: {error}"),
                RunErrorKind::FinalWrite(crate::cli::handler::OutputKind::Text),
            ));
        }
        crate::cli::handler::RunResult::Handled(crate::cli::handler::RunOutput::command(
            String::new(),
        ))
    } else {
        crate::cli::handler::RunResult::Handled(crate::cli::handler::RunOutput::command(sheet))
    }
}
