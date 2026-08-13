use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use minijinja::context;
use standout_input::env::{DefaultStdin, StdinReader};
use standout_input::questionnaire::{
    AnswerSheetDiagnostic, AnswerValue, Answers, FieldValidator, FormError, Group, Item,
    Questionnaire, RawAnswers, ScalarField, ScalarKind,
};
use standout_render::template::new_environment;

#[derive(Parser)]
#[command(name = "standout", about = "Standout project tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the smallest runnable Standout workspace.
    NewProject {
        /// Generate from a completed answer sheet instead of interactive
        /// prompts: a named file, or `-` to read exactly one complete sheet
        /// from piped stdin. Either form replaces question collection
        /// entirely; the review and confirmation gate still follows, and
        /// submitting a sheet never implies consent to generate.
        #[arg(long, value_name = "FILE")]
        answers: Option<PathBuf>,

        /// Skip the confirmation prompt (for automation). Parsing,
        /// validation, the review output, and atomic publication all still
        /// run; only the final "type 'yes'" gate is bypassed.
        #[arg(long)]
        yes: bool,

        #[command(subcommand)]
        command: Option<NewProjectCommand>,
    },
}

/// `new-project` subcommands that never generate a project.
#[derive(Subcommand)]
enum NewProjectCommand {
    /// Render the blank questionnaire answer sheet to stdout (or --file).
    Questions {
        /// Write the answer sheet to this file instead of stdout.
        #[arg(long, value_name = "FILE")]
        file: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::NewProject {
            answers,
            yes,
            command,
        } => match (command, answers, yes) {
            (Some(NewProjectCommand::Questions { file }), None, false) => {
                render_questions(file.as_deref())
            }
            (Some(NewProjectCommand::Questions { .. }), _, _) => {
                bail!(
                    "`questions` renders the blank answer sheet and cannot be combined \
                     with --answers or --yes; run `standout new-project --answers FILE` \
                     to generate from a completed sheet"
                )
            }
            (None, answers, yes) => run_new_project(answers.as_deref(), yes),
        },
    }
}

/// The answer-sheet source an `--answers` value selects: a named file, or
/// exactly one complete sheet read from piped stdin (`--answers -`). Sheets
/// never merge with prompts or with each other.
enum SheetSource {
    File(PathBuf),
    Stdin,
}

impl SheetSource {
    fn from_flag(value: &Path) -> Self {
        if value == Path::new("-") {
            Self::Stdin
        } else {
            Self::File(value.to_path_buf())
        }
    }

    /// The source's name in diagnostics ("answer sheet <label> has …").
    fn label(&self) -> String {
        match self {
            Self::File(path) => path.display().to_string(),
            Self::Stdin => "from stdin".to_string(),
        }
    }

    fn answers(&self) -> Result<WizardAnswers, Vec<String>> {
        match self {
            Self::File(path) => sheet_answers(path),
            Self::Stdin => stdin_sheet_answers(&DefaultStdin),
        }
    }
}

/// Run the wizard end to end: collect [`WizardAnswers`] from exactly one
/// source — interactive prompts, or a completed answer sheet named by
/// `answers_file` (a file path, or `-` for piped stdin; sources never
/// merge) — then take every mode through the same review, confirmation,
/// and atomic publication gate.
///
/// Confirmation policy: submitting a sheet never implies consent, and no
/// stdin byte or EOF can ever confirm. Interactive runs confirm on the
/// prompt stream they were already attending. Sheet runs without
/// `assume_yes` confirm through the attended-terminal seam
/// ([`AttendedTerminal`]) — independent of stdin by construction — and fail
/// with guidance when no attended terminal exists. `assume_yes` (`--yes`)
/// skips only that final gate. Every collection or validation failure, a
/// missing terminal, a rejected confirmation, and cancellation all return
/// before any destination file is written.
fn run_new_project(answers_file: Option<&Path>, assume_yes: bool) -> Result<()> {
    let source = answers_file.map(SheetSource::from_flag);
    let answers = match &source {
        Some(sheet) => match sheet.answers() {
            Ok(answers) => answers,
            Err(diagnostics) => {
                for diagnostic in &diagnostics {
                    eprintln!("{diagnostic}");
                }
                bail!(
                    "answer sheet {} has {} problem(s); nothing was generated",
                    sheet.label(),
                    diagnostics.len()
                );
            }
        },
        None => match prompt_answers(&mut io::stdin().lock(), &mut io::stdout()) {
            Ok(answers) => answers,
            Err(error) if error.is::<Cancelled>() => {
                println!("Generation cancelled.");
                return Ok(());
            }
            Err(error) => return Err(error),
        },
    };
    let spec = ProjectSpec::from_answers(answers)?;
    write_review(&spec, &mut io::stdout())?;
    let confirmed = if assume_yes {
        true
    } else if source.is_some() {
        confirm_attended(attended_terminal_from_env()?.as_mut())?
    } else {
        confirm(&mut io::stdin().lock(), &mut io::stdout())?
    };
    if !confirmed {
        println!("Generation cancelled.");
        return Ok(());
    }
    publish_project(&spec)?;
    println!("Created {}", spec.destination.display());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WizardAnswers {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    result_shape: ResultShape,
    record_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSpec {
    project_name: String,
    executable_name: String,
    command_name: String,
    command_description: String,
    inputs: Vec<CommandInput>,
    result_shape: ResultShape,
    record_fields: Vec<String>,
    lib_crate: String,
    operation_name: String,
    view_name: String,
    destination: PathBuf,
    standout_version: String,
    local_patch_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandInput {
    name: String,
    value_type: InputValueType,
    cardinality: InputCardinality,
    sources: Vec<InputSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InputValueType {
    String,
    Bool,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InputCardinality {
    Required,
    Optional,
    Repeated,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSource {
    Argument,
    File,
    Stdin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultShape {
    Message,
    Record,
}

impl ResultShape {
    fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Record => "record",
        }
    }
}

impl ProjectSpec {
    fn from_answers(answers: WizardAnswers) -> Result<Self> {
        validate_crate_name(&answers.project_name, "project name")?;
        validate_crate_name(&answers.executable_name, "executable name")?;
        validate_ident(&answers.command_name.replace('-', "_"), "command name")?;
        if answers.inputs.is_empty() {
            bail!("at least one command input is required");
        }
        for input in &answers.inputs {
            input.validate()?;
        }
        validate_generated_flags(&answers.inputs)?;
        validate_result_fields(answers.result_shape, &answers.record_fields)?;
        if answers.command_description.trim().is_empty() {
            bail!("command description cannot be empty");
        }

        let lib_crate = format!("{}lib", answers.project_name.replace('-', "_"));
        let command_ident = answers.command_name.replace('-', "_");
        let operation_name = format!("process_{command_ident}");
        let view_name = format!("{}View", pascal_case(&command_ident));
        Ok(Self {
            destination: PathBuf::from(&answers.project_name),
            project_name: answers.project_name,
            executable_name: answers.executable_name,
            command_name: answers.command_name,
            command_description: answers.command_description,
            inputs: answers.inputs,
            result_shape: answers.result_shape,
            record_fields: answers.record_fields,
            lib_crate,
            operation_name,
            view_name,
            standout_version: env!("CARGO_PKG_VERSION").to_string(),
            local_patch_root: None,
        })
    }

    fn tree(&self) -> Vec<String> {
        vec![
            "Cargo.toml".into(),
            format!("crates/{}/Cargo.toml", self.lib_crate),
            format!("crates/{}/src/lib.rs", self.lib_crate),
            format!("crates/{}/Cargo.toml", self.executable_name),
            format!("crates/{}/README.md", self.executable_name),
            format!("crates/{}/src/main.rs", self.executable_name),
            format!("crates/{}/src/cli.rs", self.executable_name),
            format!("crates/{}/src/handlers.rs", self.executable_name),
            format!(
                "crates/{}/src/templates/{}.jinja",
                self.executable_name, self.command_name
            ),
            format!(
                "crates/{}/src/styles/{}.css",
                self.executable_name, self.project_name
            ),
        ]
    }
}

impl CommandInput {
    fn validate(&self) -> Result<()> {
        validate_ident(&self.name, "input name")?;
        if self.sources.is_empty() {
            bail!("{} must allow at least one input source", self.name);
        }
        if self.cardinality == InputCardinality::Boolean {
            if self.value_type != InputValueType::Bool {
                bail!("{} uses boolean cardinality but is not bool", self.name);
            }
            if self.sources != [InputSource::Argument] {
                bail!("{} boolean flags only support argument source", self.name);
            }
        }
        if self.value_type == InputValueType::Bool && self.cardinality != InputCardinality::Boolean
        {
            bail!("{} bool inputs must use boolean cardinality", self.name);
        }
        if self.value_type == InputValueType::Path
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} path inputs only support argument source", self.name);
        }
        if self.cardinality == InputCardinality::Repeated
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} repeated inputs only support argument source", self.name);
        }
        Ok(())
    }

    fn policy_sentence(&self) -> String {
        let sources = self
            .sources
            .iter()
            .map(|source| match source {
                InputSource::Argument if self.cardinality == InputCardinality::Boolean => {
                    format!("--{}", self.name.replace('_', "-"))
                }
                InputSource::Argument => format!("--{}", self.name.replace('_', "-")),
                InputSource::File => format!("--{}-file", self.name.replace('_', "-")),
                InputSource::Stdin => "piped stdin".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", then ");
        format!("{} comes from {sources}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeneratedFiles {
    files: BTreeMap<PathBuf, String>,
}

impl GeneratedFiles {
    fn render(spec: &ProjectSpec) -> Result<Self> {
        let mut env = new_environment();
        for (name, source) in TEMPLATE_CATALOG {
            env.add_template(name, source)
                .with_context(|| format!("template {name} is malformed"))?;
        }

        let mut files = BTreeMap::new();
        for (path_template, template_name) in FILE_MAP {
            let path = render_inline(path_template, spec)?;
            let mut body = env
                .get_template(template_name)?
                .render(model(spec))
                .with_context(|| format!("template {template_name} is missing model data"))?;
            if !body.ends_with('\n') {
                body.push('\n');
            }
            files.insert(PathBuf::from(path), body);
        }
        Ok(Self { files })
    }
}

/// Control-flow marker raised when the user answers an exact uppercase `X`
/// at any questionnaire prompt. The caller reports a successful,
/// non-mutating cancellation instead of surfacing an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("generation cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// The exact answer that cancels the questionnaire at any prompt.
const CANCEL_ANSWER: &str = "X";

/// Collects and validates every questionnaire answer, re-prompting the
/// smallest coherent unit — the current scalar question, the record-field
/// list, or the current input block — on an invalid answer instead of
/// discarding the session. An exact uppercase `X` at any prompt raises
/// [`Cancelled`]; exhausted input aborts with an error.
fn prompt_answers(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<WizardAnswers> {
    let project_name = prompt_retrying(input, output, "Project name", None, |value| {
        validate_crate_name(value, "project name")?;
        Ok(value.to_string())
    })?;

    let executable_name = prompt_retrying(
        input,
        output,
        "Executable name",
        Some(&project_name),
        |value| {
            validate_crate_name(value, "executable name")?;
            Ok(value.to_string())
        },
    )?;

    let command_name = prompt_retrying(input, output, "Initial command name", None, |value| {
        validate_ident(&value.replace('-', "_"), "command name")?;
        Ok(value.to_string())
    })?;

    let command_description = prompt_retrying(
        input,
        output,
        "One-sentence command description",
        None,
        |value| Ok(value.to_string()),
    )?;

    let inputs = prompt_command_inputs(input, output)?;
    let result_shape = prompt_result_shape(input, output)?;
    let record_fields = match result_shape {
        ResultShape::Message => Vec::new(),
        ResultShape::Record => prompt_record_fields(input, output)?,
    };

    Ok(WizardAnswers {
        project_name,
        executable_name,
        command_name,
        command_description,
        inputs,
        result_shape,
        record_fields,
    })
}

/// Asks one question until the answer parses, printing each validation
/// error before re-prompting. An empty answer takes `default` when one
/// exists and otherwise re-prompts. Cancellation ([`CANCEL_ANSWER`]) and
/// end of input abort the questionnaire instead of retrying.
fn prompt_retrying<T>(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
    default: Option<&str>,
    parse: impl Fn(&str) -> Result<T>,
) -> Result<T> {
    loop {
        match default {
            Some(default) => write!(output, "{label} [{default}]: ")?,
            None => write!(output, "{label}: ")?,
        }
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            bail!("input ended before the questionnaire completed");
        }
        let mut value = line.trim();
        if value == CANCEL_ANSWER {
            return Err(Cancelled.into());
        }
        if value.is_empty() {
            match default {
                Some(default) => value = default,
                None => {
                    writeln!(output, "{label} cannot be empty")?;
                    continue;
                }
            }
        }
        match parse(value) {
            Ok(parsed) => return Ok(parsed),
            Err(error) => writeln!(output, "{error}")?,
        }
    }
}

/// Collects the requested number of command inputs. Scalar answers inside
/// a block retry individually; a cross-answer constraint failure — an
/// unsupported type/cardinality/source combination or a generated-flag
/// collision with an earlier input — retries the whole current block.
fn prompt_command_inputs(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Vec<CommandInput>> {
    let count = prompt_retrying(
        input,
        output,
        "Number of command inputs",
        Some("1"),
        |value| {
            let count: usize = value
                .parse()
                .context("number of command inputs must be an integer")?;
            if count == 0 {
                bail!("at least one command input is required");
            }
            Ok(count)
        },
    )?;

    let mut inputs: Vec<CommandInput> = Vec::with_capacity(count);
    for index in 1..=count {
        loop {
            writeln!(output, "Input {index}:")?;
            let candidate = prompt_command_input(input, output)?;
            let cross_answer_check = candidate.validate().and_then(|()| {
                let mut trial = inputs.clone();
                trial.push(candidate.clone());
                validate_generated_flags(&trial)
            });
            if let Err(error) = cross_answer_check {
                writeln!(output, "{error}")?;
                continue;
            }
            inputs.push(candidate);
            break;
        }
    }
    Ok(inputs)
}

/// Collects one input block: name, value type, cardinality, and sources.
/// Each scalar answer validates and retries on its own; cross-answer
/// constraints are enforced by the caller, which retries the whole block.
fn prompt_command_input(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<CommandInput> {
    let name = prompt_retrying(input, output, "  Name", None, |value| {
        validate_ident(value, "input name")?;
        Ok(value.to_string())
    })?;
    let value_type = prompt_retrying(
        input,
        output,
        "  Type (string/bool/path)",
        Some("string"),
        parse_value_type,
    )?;
    let cardinality_default = if value_type == InputValueType::Bool {
        "boolean"
    } else {
        "required"
    };
    let cardinality = prompt_retrying(
        input,
        output,
        "  Cardinality (required/optional/repeated/boolean)",
        Some(cardinality_default),
        parse_cardinality,
    )?;
    let sources_default = if value_type == InputValueType::String
        && matches!(
            cardinality,
            InputCardinality::Required | InputCardinality::Optional
        ) {
        "argument,file,stdin"
    } else {
        "argument"
    };
    let sources = prompt_retrying(
        input,
        output,
        "  Sources in precedence order (argument,file,stdin)",
        Some(sources_default),
        parse_input_sources,
    )?;
    Ok(CommandInput {
        name,
        value_type,
        cardinality,
        sources,
    })
}

/// The confirmation question every mode asks; only an exact trimmed `yes`
/// reply confirms, everything else — including EOF — cancels.
const CONFIRM_QUESTION: &str = "Generate this project? Type 'yes' to continue: ";

/// Interactive-mode confirmation on the prompt stream the user is already
/// attending. Answer-sheet runs never come here — they confirm through
/// [`confirm_attended`], independent of stdin.
fn confirm(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<bool> {
    write!(output, "{CONFIRM_QUESTION}")?;
    output.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(line.trim() == "yes")
}

/// The attended-terminal seam answer-sheet runs confirm through.
///
/// By construction it is independent of stdin: the production
/// implementation talks to the controlling terminal device, so nothing an
/// answer sheet pipes in — content, trailing bytes, or EOF — can ever reach
/// the confirmation gate.
trait AttendedTerminal {
    /// Whether a human-attended terminal is present to prompt.
    fn is_attended(&self) -> bool;

    /// Show `question` on the terminal and read one reply line.
    /// `Ok(None)` is end of input — the caller treats it as a rejection,
    /// never as consent.
    fn ask(&mut self, question: &str) -> Result<Option<String>>;
}

/// The guidance for a missing attended terminal at confirmation time —
/// shared by [`confirm_attended`]'s upfront check and
/// [`ControllingTerminal::ask`] (the terminal can disappear between the
/// two), so the same user-visible condition always gets the same
/// rerun-or-`--yes` advice.
const NO_ATTENDED_TERMINAL: &str =
    "confirmation requires an attended terminal, but none is available; \
     rerun in a terminal to review and confirm, or pass --yes to generate \
     without a confirmation prompt; nothing was generated";

/// Ask the attended confirmation question through `terminal`. Fails with
/// actionable guidance when no attended terminal exists — an absent
/// terminal is an error, never implicit approval — and confirms only on an
/// exact trimmed `yes` reply.
fn confirm_attended(terminal: &mut dyn AttendedTerminal) -> Result<bool> {
    if !terminal.is_attended() {
        bail!(NO_ATTENDED_TERMINAL);
    }
    let reply = terminal.ask(CONFIRM_QUESTION)?;
    Ok(reply.is_some_and(|line| line.trim() == "yes"))
}

/// The production [`AttendedTerminal`]: the process's controlling terminal
/// device (`/dev/tty` on Unix, `CONIN$`/`CONOUT$` on Windows), opened at
/// confirmation time. Opening the device fails exactly when no attended
/// terminal exists, whatever stdin/stdout are redirected to.
struct ControllingTerminal;

#[cfg(unix)]
fn open_controlling_terminal() -> Option<(fs::File, fs::File)> {
    let read = fs::OpenOptions::new().read(true).open("/dev/tty").ok()?;
    let write = fs::OpenOptions::new().write(true).open("/dev/tty").ok()?;
    Some((read, write))
}

#[cfg(windows)]
fn open_controlling_terminal() -> Option<(fs::File, fs::File)> {
    // Minimal access only: broader access (e.g. GENERIC_WRITE on CONIN$) is
    // needed solely for console-mode changes this gate never makes, and
    // requesting it can fail on consoles a read/write-only open accepts.
    let read = fs::OpenOptions::new().read(true).open("CONIN$").ok()?;
    let write = fs::OpenOptions::new().write(true).open("CONOUT$").ok()?;
    Some((read, write))
}

impl AttendedTerminal for ControllingTerminal {
    fn is_attended(&self) -> bool {
        open_controlling_terminal().is_some()
    }

    fn ask(&mut self, question: &str) -> Result<Option<String>> {
        let (read, mut write) =
            open_controlling_terminal().ok_or_else(|| anyhow!(NO_ATTENDED_TERMINAL))?;
        write.write_all(question.as_bytes())?;
        write.flush()?;
        let mut line = String::new();
        if io::BufReader::new(read).read_line(&mut line)? == 0 {
            return Ok(None);
        }
        Ok(Some(line))
    }
}

/// A scripted [`AttendedTerminal`] for tests that must run the production
/// binary without a real terminal: either no terminal at all, or a fixed
/// reply script. Questions echo to stdout so CLI transcripts can assert the
/// prompt appeared without a terminal device in the loop. Compiled for
/// debug builds (which honor the [`TERMINAL_SEAM_VAR`] seam) and tests
/// only — a release binary carries no scripted terminal.
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

    fn ask(&mut self, question: &str) -> Result<Option<String>> {
        print!("{question}");
        io::stdout().flush()?;
        Ok(self.replies.pop_front())
    }
}

/// The env var that swaps the attended-terminal seam for tests. This is a
/// TEST SEAM, not user configuration: unset means the real controlling
/// terminal; `absent` simulates a missing terminal; any other value names a
/// file whose lines are the scripted terminal replies. Debug builds only —
/// a release binary never consults it, so an inherited environment cannot
/// auto-confirm or block generation in production.
#[cfg(debug_assertions)]
const TERMINAL_SEAM_VAR: &str = "STANDOUT_NEW_PROJECT_TERMINAL";

/// Resolve the [`AttendedTerminal`] the confirmation gate uses.
///
/// Debug builds honor the [`TERMINAL_SEAM_VAR`] test seam (integration
/// tests exercise the debug-profile binary); release builds always use the
/// real controlling terminal and never read the env var.
fn attended_terminal_from_env() -> Result<Box<dyn AttendedTerminal>> {
    #[cfg(debug_assertions)]
    match std::env::var_os(TERMINAL_SEAM_VAR) {
        None => Ok(Box::new(ControllingTerminal)),
        Some(value) if value == "absent" => Ok(Box::new(ScriptedTerminal::absent())),
        Some(path) => {
            let script = fs::read_to_string(&path).with_context(|| {
                format!(
                    "failed to read the scripted terminal replies from {}",
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

fn prompt_result_shape(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<ResultShape> {
    prompt_retrying(
        input,
        output,
        "Result shape (message/record)",
        Some("record"),
        parse_result_shape,
    )
}

/// Asks for the record-field list, retrying the whole comma-separated list
/// when any field is invalid or duplicated.
fn prompt_record_fields(input: &mut dyn BufRead, output: &mut dyn Write) -> Result<Vec<String>> {
    prompt_retrying(
        input,
        output,
        "Record fields (comma-separated)",
        Some("summary,count"),
        parse_record_fields,
    )
}

fn write_review(spec: &ProjectSpec, output: &mut dyn Write) -> Result<()> {
    writeln!(output, "\nReview")?;
    writeln!(output, "Destination: {}", spec.destination.display())?;
    writeln!(output, "Generated tree:")?;
    for path in spec.tree() {
        writeln!(output, "  {path}")?;
    }
    writeln!(
        output,
        "Command syntax: {} {} {}",
        spec.executable_name,
        spec.command_name,
        spec.inputs
            .iter()
            .map(command_syntax_fragment)
            .collect::<Vec<_>>()
            .join(" ")
    )?;
    writeln!(output, "Input policy:")?;
    for input in &spec.inputs {
        writeln!(output, "  - {}.", input.policy_sentence())?;
    }
    writeln!(
        output,
        "Core operation: {}::{}({})",
        spec.lib_crate,
        spec.operation_name,
        spec.inputs
            .iter()
            .map(core_signature_fragment)
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    writeln!(
        output,
        "Output shape: {} {} renders human output and serializes as JSON.",
        spec.result_shape.as_str(),
        spec.view_name
    )?;
    writeln!(
        output,
        "Generated tests: core validation, typed handler mapping, TestHarness pipeline."
    )?;
    Ok(())
}

fn publish_project(spec: &ProjectSpec) -> Result<()> {
    if spec.destination.exists() {
        if !spec.destination.is_dir() {
            bail!(
                "destination {} already exists and is not a directory",
                spec.destination.display()
            );
        }
        if fs::read_dir(&spec.destination)?.next().is_some() {
            bail!(
                "destination {} already exists and is not empty",
                spec.destination.display()
            );
        }
    }

    let generated = GeneratedFiles::render(spec)?;
    let parent = spec.destination.parent().unwrap_or_else(|| Path::new("."));
    let name = spec
        .destination
        .file_name()
        .ok_or_else(|| anyhow!("destination must have a final path component"))?;
    let staging = parent.join(format!(
        ".{}.standout-new-{}-{}",
        name.to_string_lossy(),
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    if let Err(error) = write_generated_files(&staging, &generated) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if spec.destination.exists() {
        fs::remove_dir(&spec.destination).with_context(|| {
            format!(
                "destination {} must be empty before publish",
                spec.destination.display()
            )
        })?;
    }

    match fs::rename(&staging, &spec.destination) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error).with_context(|| {
                format!(
                    "failed to publish staged project to {}",
                    spec.destination.display()
                )
            })
        }
    }
}

fn write_generated_files(root: &Path, generated: &GeneratedFiles) -> Result<()> {
    for (path, body) in &generated.files {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, body)?;
    }
    Ok(())
}

fn validate_crate_name(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{label} cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("{label} must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        bail!("{label} may only contain letters, numbers, underscores, or hyphens");
    }
    Ok(())
}

fn validate_ident(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        bail!("{label} cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("{label} must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        bail!("{label} may only contain letters, numbers, or underscores");
    }
    if is_rust_keyword(value) {
        bail!("{label} cannot be a reserved Rust keyword");
    }
    Ok(())
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

/// Decode a value-type answer. Shared by the interactive prompt and the
/// answer-sheet path so both accept exactly the same vocabulary.
fn parse_value_type(value: &str) -> Result<InputValueType> {
    match value {
        "string" => Ok(InputValueType::String),
        "bool" => Ok(InputValueType::Bool),
        "path" => Ok(InputValueType::Path),
        _ => bail!("input type must be string, bool, or path"),
    }
}

/// Decode a cardinality answer. Shared by the interactive prompt and the
/// answer-sheet path so both accept exactly the same vocabulary.
fn parse_cardinality(value: &str) -> Result<InputCardinality> {
    match value {
        "required" => Ok(InputCardinality::Required),
        "optional" => Ok(InputCardinality::Optional),
        "repeated" => Ok(InputCardinality::Repeated),
        "boolean" => Ok(InputCardinality::Boolean),
        _ => bail!("input cardinality must be required, optional, repeated, or boolean"),
    }
}

/// Decode a result-shape answer. Shared by the interactive prompt and the
/// answer-sheet path so both accept exactly the same vocabulary.
fn parse_result_shape(value: &str) -> Result<ResultShape> {
    match value {
        "message" => Ok(ResultShape::Message),
        "record" => Ok(ResultShape::Record),
        _ => bail!("result shape must be message or record"),
    }
}

fn parse_record_fields(value: &str) -> Result<Vec<String>> {
    let fields: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    validate_result_fields(ResultShape::Record, &fields)?;
    Ok(fields)
}

fn parse_input_sources(value: &str) -> Result<Vec<InputSource>> {
    let mut sources = Vec::new();
    for source in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let parsed = match source {
            "argument" | "arg" => InputSource::Argument,
            "file" => InputSource::File,
            "stdin" | "piped stdin" => InputSource::Stdin,
            _ => bail!("input source must be argument, file, or stdin"),
        };
        if sources.contains(&parsed) {
            bail!("input source {source} is declared more than once");
        }
        sources.push(parsed);
    }
    if sources.is_empty() {
        bail!("at least one input source is required");
    }
    Ok(sources)
}

fn validate_result_fields(shape: ResultShape, fields: &[String]) -> Result<()> {
    match shape {
        ResultShape::Message if !fields.is_empty() => {
            bail!("message results cannot declare record fields");
        }
        ResultShape::Message => {}
        ResultShape::Record => {
            if fields.is_empty() {
                bail!("record results must declare at least one field");
            }
            let mut seen = std::collections::BTreeSet::new();
            for field in fields {
                validate_ident(field, "record field")?;
                if !seen.insert(field) {
                    bail!("record field {field} is declared more than once");
                }
            }
        }
    }
    Ok(())
}

fn validate_generated_flags(inputs: &[CommandInput]) -> Result<()> {
    const RESERVED_FLAGS: &[&str] = &["help", "output", "output-file-path"];

    let mut flags = BTreeMap::new();
    for input in inputs {
        let logical_flag = input.name.replace('_', "-");
        if RESERVED_FLAGS.contains(&logical_flag.as_str()) {
            bail!(
                "input {} generates reserved framework/Clap flag --{logical_flag}",
                input.name
            );
        }
        if let Some(owner) = flags.insert(logical_flag.clone(), input.name.as_str()) {
            bail!(
                "input {} generates --{logical_flag}, which conflicts with input {owner}",
                input.name
            );
        }
        if input.sources.contains(&InputSource::File) {
            let file_flag = format!("{logical_flag}-file");
            if RESERVED_FLAGS.contains(&file_flag.as_str()) {
                bail!(
                    "input {} generates reserved framework/Clap flag --{file_flag}",
                    input.name
                );
            }
            if let Some(owner) = flags.insert(file_flag.clone(), input.name.as_str()) {
                bail!(
                    "input {} generates --{file_flag}, which conflicts with input {owner}",
                    input.name
                );
            }
        }
    }
    Ok(())
}

/// The bootstrap questionnaire's stable identity, pinned in every rendered
/// answer sheet's preamble. Together with the semantic fingerprint it makes
/// stale sheets fail with a regeneration message instead of guessed answers.
const QUESTIONNAIRE_ID: &str = "standout.new-project";

/// Wrap an application text rule as a questionnaire field validator.
///
/// The rule runs in `standout-input`'s shared decode stage, so interactive
/// and answer-sheet submissions are judged by the same functions. `revision`
/// enters the sheet fingerprint: bump it whenever the rule's accepted values
/// change so previously rendered sheets are invalidated like any other
/// semantic change.
fn text_rule(
    revision: &str,
    check: impl Fn(&str) -> Result<()> + Send + Sync + 'static,
) -> FieldValidator {
    FieldValidator::new(revision, move |value: &AnswerValue| {
        check(value.as_text().unwrap_or_default()).map_err(|error| error.to_string())
    })
}

/// The wizard's complete static questionnaire.
///
/// The bracketed IDs are the stable contract: prompts, numbering, and type
/// hints on a rendered sheet are cosmetic, while renaming an ID or changing
/// kinds, defaults, constraints, conditions, or validator revisions is a
/// semantic change that invalidates previously rendered sheets. Field
/// validators reuse the exact functions the interactive prompts run, so both
/// collection paths accept the same values; cross-answer rules that span
/// several fields live in [`wizard_form_rules`].
fn wizard_questionnaire() -> Questionnaire {
    Questionnaire::new(
        QUESTIONNAIRE_ID,
        vec![
            Item::from(
                ScalarField::new(
                    "project.name",
                    "What is the project name? It is also the destination directory.",
                    ScalarKind::String,
                )
                .with_validator(text_rule("crate-name.v1", |text| {
                    validate_crate_name(text, "project name")
                })),
            ),
            Item::from(
                ScalarField::new(
                    "project.executable",
                    "What is the executable name? Leave blank to reuse the project name.",
                    ScalarKind::String,
                )
                .optional()
                .with_validator(text_rule("crate-name.v1", |text| {
                    validate_crate_name(text, "executable name")
                })),
            ),
            Item::from(Group::new(
                "command",
                "Describe the initial command.",
                vec![
                    Item::from(
                        ScalarField::new(
                            "command.name",
                            "What is the command name?",
                            ScalarKind::String,
                        )
                        .with_validator(text_rule("command-name.v1", |text| {
                            validate_ident(&text.replace('-', "_"), "command name")
                        })),
                    ),
                    Item::from(ScalarField::new(
                        "command.description",
                        "Describe the command in a sentence or two.",
                        ScalarKind::Text,
                    )),
                    Item::from(
                        Group::new(
                            "command.inputs",
                            "Describe a command input.",
                            vec![
                                ScalarField::new(
                                    "command.inputs.name",
                                    "What is its name?",
                                    ScalarKind::String,
                                )
                                .with_validator(text_rule("input-name.v1", |text| {
                                    validate_ident(text, "input name")
                                })),
                                ScalarField::new(
                                    "command.inputs.value_type",
                                    "What type of value is it?",
                                    ScalarKind::String,
                                )
                                .one_of(["string", "bool", "path"])
                                .with_default("string"),
                                ScalarField::new(
                                    "command.inputs.cardinality",
                                    "How many values does it take?",
                                    ScalarKind::String,
                                )
                                .one_of(["required", "optional", "repeated", "boolean"])
                                .with_default("required"),
                                ScalarField::new(
                                    "command.inputs.sources",
                                    "Where can its value come from, in precedence order \
                                     (comma-separated: argument, file, stdin)?",
                                    ScalarKind::String,
                                )
                                .with_default("argument")
                                .with_validator(text_rule("input-sources.v1", |text| {
                                    parse_input_sources(text).map(|_| ())
                                })),
                            ],
                        )
                        .repeatable(1),
                    ),
                ],
            )),
            Item::from(
                ScalarField::new(
                    "result.shape",
                    "Should the result be a message or a record?",
                    ScalarKind::String,
                )
                .one_of(["message", "record"])
                .with_default("record"),
            ),
            Item::from(
                ScalarField::new(
                    "result.fields",
                    "Which fields should the record carry (comma-separated)?",
                    ScalarKind::String,
                )
                .with_default("summary,count")
                .active_when("result.shape", "record")
                .with_validator(text_rule("record-fields.v1", |text| {
                    parse_record_fields(text).map(|_| ())
                })),
            ),
        ],
    )
    .expect("the bootstrap questionnaire definition is statically valid")
}

/// The wizard's whole-form rules for an answer-sheet submission: the same
/// cross-answer constraints the interactive flow enforces per input block —
/// unsupported type/cardinality/source combinations and generated-flag
/// collisions or reservations. Independent violations accumulate, each
/// pointing at the occurrence block it came from.
fn wizard_form_rules(answers: &Answers) -> Vec<FormError> {
    let mut errors = Vec::new();
    let inputs = sheet_inputs(answers);
    for (index, input) in inputs.iter().enumerate() {
        if let Err(error) = input.validate() {
            errors.push(FormError::new(
                [format!("command.inputs[{index}]")],
                error.to_string(),
            ));
        }
    }
    if let Err(error) = validate_generated_flags(&inputs) {
        errors.push(FormError::new(["command.inputs.name"], error.to_string()));
    }
    errors
}

/// Rebuild the wizard's [`CommandInput`] blocks from decoded answer-sheet
/// values. Field-level decoding already succeeded, so per-field conversion
/// cannot fail here; cross-answer rules run in [`wizard_form_rules`].
fn sheet_inputs(answers: &Answers) -> Vec<CommandInput> {
    (0..answers.occurrence_count("command.inputs"))
        .map(|index| {
            let text = |field: &str| {
                answers
                    .get_text(&format!("command.inputs[{index}].{field}"))
                    .expect("decoded input fields are answered or defaulted")
            };
            CommandInput {
                name: text("name").to_string(),
                value_type: parse_value_type(text("value_type"))
                    .expect("value_type is constrained to its choices"),
                cardinality: parse_cardinality(text("cardinality"))
                    .expect("cardinality is constrained to its choices"),
                sources: parse_input_sources(text("sources"))
                    .expect("sources passed field validation"),
            }
        })
        .collect()
}

/// Convert a fully decoded and form-validated submission into the same
/// [`WizardAnswers`] the interactive flow produces. A blank executable name
/// reuses the project name, exactly like the interactive default.
fn wizard_answers_from(answers: &Answers) -> WizardAnswers {
    let project_name = answers
        .get_text("project.name")
        .expect("project.name is required")
        .to_string();
    let executable_name = answers
        .get_text("project.executable")
        .unwrap_or(&project_name)
        .to_string();
    let result_shape = parse_result_shape(
        answers
            .get_text("result.shape")
            .expect("result.shape is defaulted"),
    )
    .expect("result.shape is constrained to its choices");
    let record_fields = match result_shape {
        ResultShape::Message => Vec::new(),
        ResultShape::Record => parse_record_fields(
            answers
                .get_text("result.fields")
                .expect("result.fields is required while the record shape is active"),
        )
        .expect("result.fields passed field validation"),
    };
    WizardAnswers {
        command_name: answers
            .get_text("command.name")
            .expect("command.name is required")
            .to_string(),
        command_description: answers
            .get_text("command.description")
            .expect("command.description is required")
            .to_string(),
        inputs: sheet_inputs(answers),
        project_name,
        executable_name,
        result_shape,
        record_fields,
    }
}

/// Collect [`WizardAnswers`] from one completed answer sheet named by path.
///
/// Reading, parsing, decoding, and the wizard's whole-form rules all run
/// before anything else happens; failures return every accumulated
/// diagnostic, already rendered for display. This path never merges other
/// sources and performs no generation itself — the caller still runs the
/// review, confirmation, and publication gate ([`stdin_sheet_answers`] is
/// the `--answers -` counterpart sharing the same [`decode_sheet`] tail).
fn sheet_answers(path: &Path) -> Result<WizardAnswers, Vec<String>> {
    let questionnaire = wizard_questionnaire();
    let raw = questionnaire.read_answer_sheet_file(path);
    decode_sheet(&questionnaire, raw)
}

/// [`sheet_answers`] for `--answers -`: read exactly one complete answer
/// sheet from `reader` (piped stdin in production, an injected
/// [`StdinReader`] in tests) to end of input, then decode it through the
/// identical parse, decode, and whole-form pipeline as a named file — the
/// two sources produce identical validated answers for identical documents.
/// Interactive-terminal stdin is a collection error, and nothing read here
/// ever reaches the confirmation gate.
fn stdin_sheet_answers(reader: &dyn StdinReader) -> Result<WizardAnswers, Vec<String>> {
    let questionnaire = wizard_questionnaire();
    let raw = questionnaire.read_answer_sheet_stdin_with(reader);
    decode_sheet(&questionnaire, raw)
}

/// The collection-source-independent tail of sheet reading: decode raw
/// sheet answers with the wizard's whole-form rules and convert them to
/// [`WizardAnswers`], rendering any accumulated diagnostics for display.
/// Parser warnings (answer text containing a `<id:` tag fragment) print to
/// stderr without failing the submission.
fn decode_sheet(
    questionnaire: &Questionnaire,
    raw: Result<RawAnswers, Vec<AnswerSheetDiagnostic>>,
) -> Result<WizardAnswers, Vec<String>> {
    fn rendered(diagnostics: Vec<impl ToString>) -> Vec<String> {
        diagnostics.iter().map(ToString::to_string).collect()
    }
    let raw = raw.map_err(rendered)?;
    for warning in raw.warnings() {
        eprintln!("{warning}");
    }
    let answers = questionnaire
        .decode_answers_with(&raw, wizard_form_rules)
        .map_err(rendered)?;
    Ok(wizard_answers_from(&answers))
}

/// Render the blank bootstrap answer sheet to stdout, or to `file` when one
/// is named. Rendering is deterministic and never generates project files.
/// A stdout consumer that closes the pipe early (`… | head`) ends rendering
/// successfully rather than failing the command.
fn render_questions(file: Option<&Path>) -> Result<()> {
    let sheet = wizard_questionnaire().render_answer_sheet();
    match file {
        None => {
            if let Err(error) = io::stdout().write_all(sheet.as_bytes()) {
                if error.kind() != io::ErrorKind::BrokenPipe {
                    return Err(error).context("failed to write the answer sheet to stdout");
                }
            }
        }
        Some(path) => fs::write(path, &sheet)
            .with_context(|| format!("failed to write the answer sheet to {}", path.display()))?,
    }
    Ok(())
}

fn pascal_case(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn command_syntax_fragment(input: &CommandInput) -> String {
    let long = input.name.replace('_', "-");
    let sources = input
        .sources
        .iter()
        .map(|source| match source {
            InputSource::Argument => match input.cardinality {
                InputCardinality::Boolean => format!("--{long}"),
                _ => format!("--{long} <{}>", input.name),
            },
            InputSource::File => format!("--{long}-file <PATH>"),
            InputSource::Stdin => "<piped stdin>".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" | ");
    match input.cardinality {
        InputCardinality::Required => {
            if input.sources.len() == 1 {
                sources
            } else {
                format!("({sources})")
            }
        }
        InputCardinality::Repeated => format!("[{sources}]..."),
        _ => format!("[{sources}]"),
    }
}

fn core_signature_fragment(input: &CommandInput) -> String {
    format!("{}: {}", input.name, input.rust_type())
}

fn core_fn_signature(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!("({})", core_signature_fragment(&spec.inputs[0]))
    } else {
        let params = spec
            .inputs
            .iter()
            .map(|input| format!("    {},", core_signature_fragment(input)))
            .collect::<Vec<_>>()
            .join("\n");
        format!("(\n{params}\n)")
    }
}

fn core_primary_value(spec: &ProjectSpec) -> String {
    let primary = &spec.inputs[0];
    match (primary.value_type, primary.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            format!("{}.trim().to_string()", primary.name)
        }
        _ => format!("format!(\"{{:?}}\", &{})", primary.name),
    }
}

fn core_unused_inputs(spec: &ProjectSpec) -> String {
    let names = spec
        .inputs
        .iter()
        .skip(1)
        .map(|input| format!("&{}", input.name))
        .collect::<Vec<_>>();
    if names.is_empty() {
        String::new()
    } else if names.len() == 1 {
        format!("let _ = {};", names[0])
    } else {
        format!("let _ = ({});", names.join(", "))
    }
}

fn result_fields(spec: &ProjectSpec, visibility: &str) -> String {
    match spec.result_shape {
        ResultShape::Message => format!("    {visibility}message: String,"),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| format!("    {visibility}{field}: String,"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn result_init(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "        message: format!(\"Processed {primary}\"),".into(),
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => "        summary: format!(\"Processed {primary}\"),".into(),
                "count" | "length" => {
                    format!("        {field}: primary.chars().count().to_string(),")
                }
                other => format!("        {other}: primary.clone(),"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn result_field_names(spec: &ProjectSpec) -> Vec<String> {
    match spec.result_shape {
        ResultShape::Message => vec!["message".into()],
        ResultShape::Record => spec.record_fields.clone(),
    }
}

fn view_from_fields(spec: &ProjectSpec) -> String {
    result_field_names(spec)
        .into_iter()
        .map(|field| format!("            {field}: value.{field},"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn core_valid_assertions(spec: &ProjectSpec) -> String {
    let primary = match (spec.inputs[0].value_type, spec.inputs[0].cardinality) {
        (InputValueType::String, InputCardinality::Required) => "Standout".to_string(),
        (InputValueType::String, InputCardinality::Optional) => "Some(\"optional\")".to_string(),
        (InputValueType::String, InputCardinality::Repeated) => "[\"alpha\", \"beta\"]".to_string(),
        (InputValueType::Bool, InputCardinality::Boolean) => "true".to_string(),
        (InputValueType::Path, InputCardinality::Required) => "\"config.toml\"".to_string(),
        (InputValueType::Path, InputCardinality::Optional) => "Some(\"config.toml\")".to_string(),
        (InputValueType::Path, InputCardinality::Repeated) => "[\"one.toml\"]".to_string(),
        _ => unreachable!("validated input combinations are renderable"),
    };
    match spec.result_shape {
        ResultShape::Message => {
            let expected = format!("Processed {primary}");
            format!("        assert_eq!(result.message, {});", quote(&expected))
        }
        ResultShape::Record => spec
            .record_fields
            .iter()
            .map(|field| match field.as_str() {
                "summary" => format!(
                    "        assert_eq!(result.summary, {});",
                    quote(&format!("Processed {primary}"))
                ),
                "count" | "length" => format!(
                    "        assert_eq!(result.{field}, {});",
                    quote(&primary.chars().count().to_string())
                ),
                other => format!("        assert_eq!(result.{other}, {});", quote(&primary)),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn core_test_call(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        return format!(
            "{}({})",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        );
    }
    let args = spec
        .inputs
        .iter()
        .map(|input| format!("            {},", core_test_arg(input, false)))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}(\n{args}\n        )", spec.operation_name)
}

fn core_sample_result(spec: &ProjectSpec) -> String {
    if spec.inputs.len() == 1 {
        format!(
            "        let result = {}({}).unwrap();",
            spec.operation_name,
            core_test_arg(&spec.inputs[0], false)
        )
    } else {
        format!(
            "        let result = {}\n        .unwrap();",
            core_test_call(spec)
        )
    }
}

fn core_invalid_test(spec: &ProjectSpec) -> String {
    let Some(index) = spec.inputs.iter().position(|input| {
        input.value_type == InputValueType::String
            && input.cardinality == InputCardinality::Required
    }) else {
        let call = core_test_call(spec);
        let result = if spec.inputs.len() == 1 {
            format!("        let result = {call}.unwrap();")
        } else {
            format!("        let result = {call}\n        .unwrap();")
        };
        return format!(
            r#"    #[test]
    fn configured_inputs_are_accepted_by_the_core() {{
{result}

        assert_eq!(result, result.clone());
    }}"#
        );
    };

    if spec.inputs.len() == 1 {
        return format!(
            r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!(
            {}({}),
            Err(CoreError::EmptyInput {{ field: {} }})
        );
    }}"#,
            spec.operation_name,
            core_test_arg(&spec.inputs[0], true),
            quote(&spec.inputs[index].name)
        );
    }

    let args = spec
        .inputs
        .iter()
        .enumerate()
        .map(|(input_index, input)| {
            format!(
                "                {},",
                core_test_arg(input, input_index == index)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"    #[test]
    fn invalid_required_string_is_rejected_by_the_core() {{
        assert_eq!(
            {}(
{args}
            ),
            Err(CoreError::EmptyInput {{ field: {} }})
        );
    }}"#,
        spec.operation_name,
        quote(&spec.inputs[index].name)
    )
}

fn core_test_arg(input: &CommandInput, blank: bool) -> String {
    match (input.value_type, input.cardinality) {
        (InputValueType::String, InputCardinality::Required) => {
            if blank {
                "\"\".to_string()".into()
            } else {
                "\"Standout\".to_string()".into()
            }
        }
        (InputValueType::String, InputCardinality::Optional) => {
            if blank {
                "Some(\"\".to_string())".into()
            } else {
                "Some(\"optional\".to_string())".into()
            }
        }
        (InputValueType::String, InputCardinality::Repeated) => {
            "vec![\"alpha\".to_string(), \"beta\".to_string()]".into()
        }
        (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
        (InputValueType::Path, InputCardinality::Required) => {
            "std::path::PathBuf::from(\"config.toml\")".into()
        }
        (InputValueType::Path, InputCardinality::Optional) => {
            "Some(std::path::PathBuf::from(\"config.toml\"))".into()
        }
        (InputValueType::Path, InputCardinality::Repeated) => {
            "vec![std::path::PathBuf::from(\"one.toml\")]".into()
        }
        _ => unreachable!("validated input combinations are renderable"),
    }
}

fn handler_expected_fields(spec: &ProjectSpec) -> String {
    let expected = expected_first_field(spec, "hello");
    result_field_names(spec)
        .into_iter()
        .map(|field| match field.as_str() {
            "message" | "summary" => {
                format!("                {field}: {}.into(),", quote(&expected.1))
            }
            "count" | "length" => {
                let value = expected_primary_text(&spec.inputs[0], "hello")
                    .chars()
                    .count()
                    .to_string();
                format!("                {field}: {}.into(),", quote(&value))
            }
            _ => {
                let value = expected_primary_text(&spec.inputs[0], "hello");
                format!("                {field}: {}.into(),", quote(&value))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn template_body(spec: &ProjectSpec) -> String {
    match spec.result_shape {
        ResultShape::Message => "[title]{{ message }}[/title]\n".into(),
        ResultShape::Record => {
            let mut lines = vec![format!(
                "[title]{{{{ {} }}}}[/title]",
                spec.record_fields[0]
            )];
            for field in &spec.record_fields {
                lines.push(format!("{}: {{{{ {} }}}}", pascal_case(field), field));
            }
            lines.join("\n") + "\n"
        }
    }
}

fn expected_first_field(spec: &ProjectSpec, input: &str) -> (String, String) {
    let field = result_field_names(spec).remove(0);
    let input = expected_primary_text(&spec.inputs[0], input);
    let value = match field.as_str() {
        "message" | "summary" => format!("Processed {input}"),
        "count" | "length" => input.chars().count().to_string(),
        _ => input,
    };
    (field, value)
}

fn expected_primary_text(input: &CommandInput, value: &str) -> String {
    match (input.value_type, input.cardinality) {
        (InputValueType::String, InputCardinality::Required) => value.to_string(),
        (InputValueType::String, InputCardinality::Optional) => format!("Some(\"{value}\")"),
        (InputValueType::String, InputCardinality::Repeated) => {
            format!("[\"{value}\", \"extra\"]")
        }
        (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
        (InputValueType::Path, InputCardinality::Required) => format!("\"{value}\""),
        (InputValueType::Path, InputCardinality::Optional) => format!("Some(\"{value}\")"),
        (InputValueType::Path, InputCardinality::Repeated) => {
            format!("[\"{value}\", \"extra.toml\"]")
        }
        _ => unreachable!("validated input combinations are renderable"),
    }
}

fn sample_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![
        quote(&spec.executable_name),
        quote(&spec.command_name),
        quote("--output"),
        quote("text"),
    ];
    args.extend(sample_command_args(spec, primary_value));
    args
}

fn sample_json_cli_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = vec![
        quote(&spec.executable_name),
        quote(&spec.command_name),
        quote("--output"),
        quote("json"),
    ];
    args.extend(sample_command_args(spec, primary_value));
    args
}

fn sample_handler_args(spec: &ProjectSpec) -> Vec<String> {
    let mut args = vec![
        format!("{}.to_string()", quote(&spec.executable_name)),
        format!("{}.to_string()", quote(&spec.command_name)),
    ];
    for (index, input) in spec.inputs.iter().enumerate() {
        let value = if index == 0 { "hello" } else { "sample" };
        if input.sources[0] == InputSource::File {
            args.push(format!(
                "{}.to_string()",
                quote(&format!("--{}-file", input.name.replace('_', "-")))
            ));
            args.push(format!("{}_test_file.display().to_string()", input.name));
        } else {
            args.extend(
                input
                    .sample_args_for_source(input.sources[0], value)
                    .into_iter()
                    .map(|arg| format!("{arg}.to_string()")),
            );
        }
    }
    args
}

fn sample_command_args(spec: &ProjectSpec, primary_value: &str) -> Vec<String> {
    let mut args = Vec::new();
    for (index, input) in spec.inputs.iter().enumerate() {
        args.extend(input.sample_args_for_source(
            input.sources[0],
            if index == 0 { primary_value } else { "sample" },
        ));
    }
    args
}

fn harness_for_samples(spec: &ProjectSpec, primary_value: &str) -> String {
    let has_source_setup = spec
        .inputs
        .iter()
        .any(|input| input.sources[0] != InputSource::Argument);
    let mut harness = if has_source_setup {
        "TestHarness::new()\n            .no_color()".to_string()
    } else {
        "TestHarness::new().no_color()".to_string()
    };
    for (index, input) in spec.inputs.iter().enumerate() {
        let value = if index == 0 { primary_value } else { "sample" };
        match input.sources[0] {
            InputSource::File => harness.push_str(&format!(
                "\n            .fixture({}, {})",
                quote(&input.sample_file_name()),
                quote(value)
            )),
            InputSource::Stdin => {
                harness.push_str(&format!(
                    "\n            .piped_stdin({})",
                    quote(&format!("{value}\n"))
                ));
            }
            InputSource::Argument => {}
        }
    }
    harness
}

fn generated_harness_run(spec: &ProjectSpec, primary_value: &str, args: &[String]) -> String {
    let harness = harness_for_samples(spec, primary_value);
    if harness.contains('\n') {
        let args = rust_array(args, 20, 63);
        format!(
            r#"        let result = {harness}
            .run(
                &app,
                cli::command(),
                {args},
            );"#
        )
    } else {
        let args = rust_array(args, 16, 63);
        format!(
            r#"        let result = {harness}.run(
            &app,
            cli::command(),
            {args},
        );"#
        )
    }
}

/// Generates panic-safe setup for file and stdin sources in the handler test.
fn handler_sample_setup(spec: &ProjectSpec) -> String {
    let mut lines = Vec::new();
    if spec
        .inputs
        .iter()
        .any(|input| input.sources[0] == InputSource::Stdin)
    {
        lines.push(
            r#"        struct StdinGuard;

        impl StdinGuard {
            fn install(reader: standout_input::MockStdin) -> Self {
                standout_input::env::set_default_stdin_reader(std::sync::Arc::new(reader));
                Self
            }
        }

        impl Drop for StdinGuard {
            fn drop(&mut self) {
                standout_input::env::reset_default_stdin_reader();
            }
        }"#
            .to_string(),
        );
    }
    for (index, input) in spec.inputs.iter().enumerate() {
        let value = if index == 0 { "hello" } else { "sample" };
        match input.sources[0] {
            InputSource::File => {
                lines.push(format!(
                    "        let {0}_test_file =\n            std::env::temp_dir().join(format!(\"{1}-{0}-{{}}.txt\", std::process::id()));",
                    input.name, spec.executable_name
                ));
                lines.push(format!(
                    "        std::fs::write(&{}_test_file, {}).unwrap();",
                    input.name,
                    quote(value)
                ));
            }
            InputSource::Stdin => lines.push(format!(
                "        let _stdin = StdinGuard::install(standout_input::MockStdin::piped({}));",
                quote(&format!("{value}\n"))
            )),
            InputSource::Argument => {}
        }
    }
    lines.join("\n")
}

/// Generates explicit cleanup for temporary files used by the handler test.
fn handler_sample_cleanup(spec: &ProjectSpec) -> String {
    let mut lines = Vec::new();
    for input in &spec.inputs {
        if input.sources[0] == InputSource::File {
            lines.push(format!(
                "        std::fs::remove_file({}_test_file).unwrap();",
                input.name
            ));
        }
    }
    lines.join("\n")
}

fn generated_json_pipeline_test(spec: &ProjectSpec) -> String {
    let primary_value = match spec.inputs[0].value_type {
        InputValueType::Path => "config.toml",
        _ => "Grace",
    };
    let (field, expected) = expected_first_field(spec, primary_value);
    let args = sample_json_cli_args(spec, primary_value);
    let run = generated_harness_run(spec, primary_value, &args);
    format!(
        r#"    #[test]
    #[serial]
    fn pipeline_serializes_json_for_configured_inputs() {{
        let app = build_app().unwrap();

{run}

        result.assert_success();
        let value: Value = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["{field}"], {expected});
    }}"#,
        expected = quote(&expected)
    )
}

fn readme_input_policy(spec: &ProjectSpec) -> String {
    spec.inputs
        .iter()
        .map(|input| format!("- {}.", input.policy_sentence()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn readme_validation_note(spec: &ProjectSpec) -> String {
    let inputs = spec
        .inputs
        .iter()
        .filter(|input| {
            input.value_type == InputValueType::String
                && input.cardinality == InputCardinality::Required
        })
        .map(|input| format!("`{}`", input.name))
        .collect::<Vec<_>>();
    match inputs.as_slice() {
        [] => String::new(),
        [input] => {
            format!("Blank values for the required string input {input} are rejected by the core operation.")
        }
        _ => format!(
            "Blank values for the required string inputs {} are rejected by the core operation.",
            inputs.join(", ")
        ),
    }
}

fn handler_imports(spec: &ProjectSpec) -> String {
    let mut imports = [
        "use clap::ArgMatches;".to_string(),
        format!("use {} as core;", spec.lib_crate),
        "use serde::Serialize;".to_string(),
        "use standout::cli::{CommandContext, Output};".to_string(),
        "use standout::handler;".to_string(),
    ];
    imports.sort();
    imports.join("\n")
}

fn readme_examples(spec: &ProjectSpec) -> String {
    let command = format!(
        "cargo run -p {} -- {}",
        spec.executable_name, spec.command_name
    );
    let args = spec
        .inputs
        .iter()
        .enumerate()
        .flat_map(|(index, input)| input.readme_args(if index == 0 { "VALUE" } else { "SAMPLE" }))
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = Vec::new();
    for input in &spec.inputs {
        if input.sources[0] == InputSource::File {
            lines.push(format!("printf '%s' VALUE > {}", input.sample_file_name()));
        }
    }
    let invocation = format!("{command} {args}").trim().to_string();
    if spec
        .inputs
        .iter()
        .any(|input| input.sources[0] == InputSource::Stdin)
    {
        lines.push(format!("printf '%s\\n' VALUE | {invocation}"));
        lines.push(format!("printf '%s\\n' VALUE | {invocation} --output json"));
    } else {
        lines.push(invocation.clone());
        lines.push(format!("{invocation} --output json"));
    }
    lines.join("\n")
}

fn quote(value: &str) -> String {
    format!("{value:?}")
}

/// Width budget rustfmt's default `attr_fn_like_width` heuristic grants a
/// function-like attribute's argument list before splitting it across lines.
const ATTR_FN_LIKE_WIDTH: usize = 70;

/// Renders the generated CLI's `#[command(...)]` attribute exactly as the
/// pinned rustfmt would format it: inline while the argument list fits
/// [`ATTR_FN_LIKE_WIDTH`], one argument per line without a trailing comma
/// once it does not. The budget is measured in Unicode display width
/// (via `unicode-width`, the same crate rustfmt's heuristics use), not
/// char count, so wide characters in a description (CJK, emoji) trip the
/// split at the same point rustfmt would. Emitting the formatted shape
/// keeps generated projects `cargo fmt --check`-clean without invoking a
/// formatter at generation time, even for long command descriptions.
fn cli_command_attribute(spec: &ProjectSpec) -> String {
    use unicode_width::UnicodeWidthStr;

    let name = quote(&spec.executable_name);
    let about = quote(&spec.command_description);
    let arguments = format!("name = {name}, about = {about}");
    if arguments.width() <= ATTR_FN_LIKE_WIDTH {
        format!("#[command({arguments})]")
    } else {
        format!("#[command(\n    name = {name},\n    about = {about}\n)]")
    }
}

/// Escapes a path for interpolation inside a TOML basic string.
fn toml_basic_string_content(path: &Path) -> String {
    let mut escaped = String::new();
    for character in path.to_string_lossy().chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\u{000C}' => escaped.push_str("\\f"),
            '\r' => escaped.push_str("\\r"),
            character if character <= '\u{001F}' || character == '\u{007F}' => {
                escaped.push_str(&format!("\\u{:04X}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn rust_array(items: &[String], indent: usize, max_inline_len: usize) -> String {
    let inline = format!("[{}]", items.join(", "));
    if inline.len() <= max_inline_len {
        return inline;
    }
    let spaces = " ".repeat(indent);
    let mut output = String::from("[\n");
    for item in items {
        output.push_str(&spaces);
        output.push_str(item);
        output.push_str(",\n");
    }
    output.push_str(&" ".repeat(indent.saturating_sub(4)));
    output.push(']');
    output
}

impl CommandInput {
    fn rust_type(&self) -> &'static str {
        match (self.value_type, self.cardinality) {
            (InputValueType::String, InputCardinality::Required) => "String",
            (InputValueType::String, InputCardinality::Optional) => "Option<String>",
            (InputValueType::String, InputCardinality::Repeated) => "Vec<String>",
            (InputValueType::Bool, InputCardinality::Boolean) => "bool",
            (InputValueType::Path, InputCardinality::Required) => "std::path::PathBuf",
            (InputValueType::Path, InputCardinality::Optional) => "Option<std::path::PathBuf>",
            (InputValueType::Path, InputCardinality::Repeated) => "Vec<std::path::PathBuf>",
            _ => "String",
        }
    }

    fn cli_arg(&self) -> String {
        let long = self.name.replace('_', "-");
        let mut args = Vec::new();
        let argument = match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                Some(format!("#[arg(long = \"{long}\", action = clap::ArgAction::SetTrue)]\n        {}: bool,", self.name))
            }
            (InputValueType::Path, InputCardinality::Required) => {
                Some(format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: std::path::PathBuf,", self.name))
            }
            (InputValueType::Path, InputCardinality::Optional) => {
                Some(format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Option<std::path::PathBuf>,", self.name))
            }
            (InputValueType::Path, InputCardinality::Repeated) => {
                Some(format!("#[arg(long = \"{long}\", value_name = \"PATH\")]\n        {}: Vec<std::path::PathBuf>,", self.name))
            }
            (_, InputCardinality::Required) => {
                if !self.sources.contains(&InputSource::Argument) {
                    None
                } else if self.sources == [InputSource::Argument] {
                    Some(format!("#[arg(long = \"{long}\")]\n        {}: String,", self.name))
                } else {
                    Some(format!(
                        "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                        self.name
                    ))
                }
            }
            (_, InputCardinality::Optional) => {
                self.sources.contains(&InputSource::Argument).then(|| format!(
                    "#[arg(long = \"{long}\")]\n        {}: Option<String>,",
                    self.name
                ))
            }
            (_, InputCardinality::Repeated) => {
                Some(format!(
                    "#[arg(long = \"{long}\")]\n        {}: Vec<String>,",
                    self.name
                ))
            }
            _ => unreachable!("validated input combinations are renderable"),
        };
        if let Some(argument) = argument {
            args.push(argument);
        }
        if self.value_type == InputValueType::String && self.sources.contains(&InputSource::File) {
            args.push(format!(
                "#[arg(long = \"{long}-file\", value_name = \"PATH\")]\n        {0}_file: Option<std::path::PathBuf>,",
                self.name
            ));
        }
        args.join("\n        ")
    }

    fn sample_args_for_source(&self, source: InputSource, primary_value: &str) -> Vec<String> {
        let long = format!("--{}", self.name.replace('_', "-"));
        match source {
            InputSource::File => {
                return vec![
                    quote(&format!("{long}-file")),
                    quote(&self.sample_file_name()),
                ];
            }
            InputSource::Stdin => return Vec::new(),
            InputSource::Argument => {}
        }
        match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => vec![quote(&long)],
            (InputValueType::String, InputCardinality::Required | InputCardinality::Optional) => {
                if self.sources.contains(&InputSource::Argument) {
                    vec![quote(&long), quote(primary_value)]
                } else {
                    Vec::new()
                }
            }
            (InputValueType::String, InputCardinality::Repeated) => vec![
                quote(&long),
                quote(primary_value),
                quote(&long),
                quote("extra"),
            ],
            (InputValueType::Path, InputCardinality::Required | InputCardinality::Optional) => {
                vec![quote(&long), quote(primary_value)]
            }
            (InputValueType::Path, InputCardinality::Repeated) => vec![
                quote(&long),
                quote(primary_value),
                quote(&long),
                quote("extra.toml"),
            ],
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    fn sample_file_name(&self) -> String {
        format!("{}-input.txt", self.name.replace('_', "-"))
    }

    fn readme_args(&self, value: &str) -> Vec<String> {
        let long = format!("--{}", self.name.replace('_', "-"));
        match self.sources[0] {
            InputSource::File => vec![format!("{long}-file"), self.sample_file_name()],
            InputSource::Stdin => Vec::new(),
            InputSource::Argument => match (self.value_type, self.cardinality) {
                (InputValueType::Bool, InputCardinality::Boolean) => vec![long],
                (_, InputCardinality::Repeated) => {
                    vec![long.clone(), value.into(), long, "EXTRA".into()]
                }
                _ => vec![long, value.into()],
            },
        }
    }

    fn core_call_arg(&self) -> String {
        self.name.clone()
    }

    fn core_validation(&self) -> Option<String> {
        if self.value_type == InputValueType::String
            && self.cardinality == InputCardinality::Required
        {
            Some(format!(
                "if {}.trim().is_empty() {{\n        return Err(CoreError::EmptyInput {{ field: {} }});\n    }}",
                self.name,
                quote(&self.name)
            ))
        } else {
            None
        }
    }

    fn resolve_statement(&self) -> String {
        match (self.value_type, self.cardinality) {
            (InputValueType::Bool, InputCardinality::Boolean) => {
                format!("let {} = matches.get_flag(\"{}\");", self.name, self.name)
            }
            (InputValueType::Path, InputCardinality::Required) => format!(
                "let {} = matches\n        .get_one::<std::path::PathBuf>(\"{}\")\n        .cloned()\n        .ok_or_else(|| anyhow::anyhow!(\"{} is required\"))?;",
                self.name, self.name, self.name
            ),
            (InputValueType::Path, InputCardinality::Optional) => format!(
                "let {} = matches.get_one::<std::path::PathBuf>(\"{}\").cloned();",
                self.name, self.name
            ),
            (InputValueType::Path, InputCardinality::Repeated) => format!(
                "let {} = matches\n        .get_many::<std::path::PathBuf>(\"{}\")\n        .map(|values| values.cloned().collect())\n        .unwrap_or_default();",
                self.name, self.name
            ),
            (InputValueType::String, InputCardinality::Repeated) => format!(
                "let {} = matches\n        .get_many::<String>(\"{}\")\n        .map(|values| values.cloned().collect())\n        .unwrap_or_default();",
                self.name, self.name
            ),
            (InputValueType::String, InputCardinality::Optional) => {
                self.string_resolver(false)
            }
            (InputValueType::String, InputCardinality::Required) => {
                self.string_resolver(true)
            }
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    fn string_resolver(&self, required: bool) -> String {
        let mut lines = vec![format!("let mut {} = None;", self.name)];
        for source in &self.sources {
            match source {
                InputSource::Argument => lines.push(format!(
                    "if {0}.is_none() {{\n        {0} = matches.get_one::<String>(\"{0}\").cloned();\n    }}",
                    self.name
                )),
                InputSource::File => lines.push(format!(
                    "if {0}.is_none() {{\n        if let Some(path) = matches.get_one::<std::path::PathBuf>(\"{0}_file\") {{\n            {0} =\n                Some(std::fs::read_to_string(path).map_err(|error| {{\n                    anyhow::anyhow!(\"failed to read {{}}: {{error}}\", path.display())\n                }})?);\n        }}\n    }}",
                    self.name
                )),
                InputSource::Stdin => lines.push(format!(
                    "if {0}.is_none() {{\n        {0} = standout_input::read_if_piped()?;\n    }}",
                    self.name
                )),
            }
        }
        if required {
            lines.push(format!(
                "let {} = match {} {{\n        Some(value) => value,\n        None => return Err(anyhow::anyhow!(\"{} is required\")),\n    }};",
                self.name, self.name, self.name
            ));
        }
        lines.join("\n    ")
    }
}

fn render_inline(template: &str, spec: &ProjectSpec) -> Result<String> {
    new_environment()
        .template_from_str(template)?
        .render(model(spec))
        .with_context(|| format!("path template {template} is missing model data"))
}

fn model(spec: &ProjectSpec) -> minijinja::Value {
    let primary = &spec.inputs[0];
    context! {
        project_name => spec.project_name,
        executable_name => spec.executable_name,
        command_name => spec.command_name,
        command_ident => spec.command_name.replace('-', "_"),
        command_variant => pascal_case(&spec.command_name.replace('-', "_")),
        command_description => spec.command_description,
        cli_command_attribute => cli_command_attribute(spec),
        input_name => primary.name,
        inputs => spec.inputs.iter().map(|input| {
            context! {
                name => input.name,
                cli_arg => input.cli_arg(),
                core_call_arg => input.core_call_arg(),
                rust_type => input.rust_type(),
                policy => input.policy_sentence(),
            }
        }).collect::<Vec<_>>(),
        core_params => spec.inputs.iter().map(core_signature_fragment).collect::<Vec<_>>().join(", "),
        core_fn_signature => core_fn_signature(spec),
        core_call_args => spec.inputs.iter().map(CommandInput::core_call_arg).collect::<Vec<_>>().join(", "),
        core_validations => spec.inputs.iter().filter_map(CommandInput::core_validation).collect::<Vec<_>>().join("\n    "),
        core_unused_inputs => core_unused_inputs(spec),
        cli_args => spec.inputs.iter().map(CommandInput::cli_arg).collect::<Vec<_>>().join("\n        "),
        resolve_inputs => spec.inputs.iter().map(CommandInput::resolve_statement).collect::<Vec<_>>().join("\n    "),
        lib_crate => spec.lib_crate,
        lib_package => spec.lib_crate.replace('_', "-"),
        operation_name => spec.operation_name,
        view_name => spec.view_name,
        result_shape => spec.result_shape.as_str(),
        core_primary_value => core_primary_value(spec),
        core_result_fields => result_fields(spec, "pub "),
        cli_view_fields => result_fields(spec, "pub(crate) "),
        result_init => result_init(spec),
        view_from_fields => view_from_fields(spec),
        core_valid_assertions => core_valid_assertions(spec),
        core_sample_result => core_sample_result(spec),
        core_invalid_test => core_invalid_test(spec),
        handler_expected_fields => handler_expected_fields(spec),
        handler_imports => handler_imports(spec),
        pipeline_human_run => generated_harness_run(spec, "Ada", &sample_cli_args(spec, "Ada")),
        pipeline_json_test => generated_json_pipeline_test(spec),
        handler_args => rust_array(&sample_handler_args(spec), 16, 60),
        handler_sample_setup => handler_sample_setup(spec),
        handler_sample_cleanup => handler_sample_cleanup(spec),
        template_body => template_body(spec),
        human_expected => quote(&expected_first_field(spec, "Ada").1),
        readme_input_policy => readme_input_policy(spec),
        readme_validation_note => readme_validation_note(spec),
        readme_examples => readme_examples(spec),
        command_syntax => spec.inputs.iter().map(command_syntax_fragment).collect::<Vec<_>>().join(" "),
        standout_version => spec.standout_version,
        local_patch_root => spec.local_patch_root.as_deref().map(toml_basic_string_content),
    }
}

const FILE_MAP: &[(&str, &str)] = &[
    ("Cargo.toml", "workspace"),
    ("crates/{{ lib_crate }}/Cargo.toml", "core_manifest"),
    ("crates/{{ lib_crate }}/src/lib.rs", "core_lib"),
    ("crates/{{ executable_name }}/Cargo.toml", "cli_manifest"),
    ("crates/{{ executable_name }}/README.md", "readme"),
    ("crates/{{ executable_name }}/src/main.rs", "main"),
    ("crates/{{ executable_name }}/src/cli.rs", "cli"),
    ("crates/{{ executable_name }}/src/handlers.rs", "handlers"),
    (
        "crates/{{ executable_name }}/src/templates/{{ command_name }}.jinja",
        "template",
    ),
    (
        "crates/{{ executable_name }}/src/styles/{{ project_name }}.css",
        "style",
    ),
];

const TEMPLATE_CATALOG: &[(&str, &str)] = &[
    (
        "workspace",
        r#"[workspace]
resolver = "2"
members = [
    "crates/{{ lib_crate }}",
    "crates/{{ executable_name }}",
]

{%- if local_patch_root %}
[patch.crates-io]
standout = { path = "{{ local_patch_root }}/crates/standout" }
standout-test = { path = "{{ local_patch_root }}/crates/standout-test" }
standout-dispatch = { path = "{{ local_patch_root }}/crates/standout-dispatch" }
standout-input = { path = "{{ local_patch_root }}/crates/standout-input" }
{%- endif %}
"#,
    ),
    (
        "core_manifest",
        r#"[package]
name = "{{ lib_package }}"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "2"
"#,
    ),
    (
        "core_lib",
        r#"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct {{ view_name }} {
{{ core_result_fields }}
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("{field} cannot be empty")]
    EmptyInput { field: &'static str },
}

/// Runs the CLI-free core operation for the generated command.
///
/// The caller supplies explicit values. This crate deliberately has no Clap,
/// Standout, template, terminal, environment, or CLI-view dependencies.
pub fn {{ operation_name }}{{ core_fn_signature }} -> Result<{{ view_name }}, CoreError> {
{%- if core_validations %}
    {{ core_validations }}
{%- endif %}
{%- if core_unused_inputs %}
    {{ core_unused_inputs }}
{%- endif %}
    let primary = {{ core_primary_value }};
    Ok({{ view_name }} {
{{ result_init }}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_returns_a_typed_result() {
{{ core_sample_result }}

{{ core_valid_assertions }}
    }

{{ core_invalid_test }}
}
"#,
    ),
    (
        "cli_manifest",
        r#"[package]
name = "{{ executable_name }}"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
standout = "{{ standout_version }}"
standout-dispatch = "{{ standout_version }}"
standout-input = "{{ standout_version }}"
{{ lib_crate }} = { package = "{{ lib_package }}", path = "../{{ lib_crate }}" }

[dev-dependencies]
serde_json = "1"
serial_test = "3"
standout-test = "{{ standout_version }}"
"#,
    ),
    (
        "main",
        r#"mod cli;
mod handlers;

use anyhow::Result;
use standout::{embed_styles, embed_templates};

fn main() -> Result<()> {
    let app = build_app()?;
    app.run(cli::command(), std::env::args());
    Ok(())
}

fn build_app() -> Result<standout::cli::App> {
    Ok(standout::cli::App::builder()
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("{{ project_name }}")
        .command_with("{{ command_name }}", handlers::{{ command_ident }}__handler, |config| {
            config.template("{{ command_name }}.jinja")
        })?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use serial_test::serial;
    use standout_test::TestHarness;

    #[test]
    #[serial]
    fn pipeline_renders_human_output_from_argument() {
        let app = build_app().unwrap();

{{ pipeline_human_run }}

        result.assert_success();
        result.assert_stdout_contains({{ human_expected }});
    }

{{ pipeline_json_test }}
}
"#,
    ),
    (
        "cli",
        r#"use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
{{ cli_command_attribute }}
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// {{ command_description }}
    #[command(name = "{{ command_name }}")]
    {{ command_variant }} {
        {{ cli_args }}
    },
}

pub(crate) fn command() -> clap::Command {
    Cli::command()
}
"#,
    ),
    (
        "handlers",
        r#"#![allow(non_snake_case)]

{{ handler_imports }}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct {{ view_name }} {
{{ cli_view_fields }}
}

impl From<core::{{ view_name }}> for {{ view_name }} {
    fn from(value: core::{{ view_name }}) -> Self {
        Self {
{{ view_from_fields }}
        }
    }
}

/// Adapts typed shell input into the CLI-free core operation.
///
/// The handler owns CLI-only source resolution, including file-content reads,
/// then returns data for Standout to render or serialize.
#[handler]
pub(crate) fn {{ command_ident }}(
    #[matches] matches: &ArgMatches,
    #[ctx] _ctx: &CommandContext,
) -> Result<Output<{{ view_name }}>, anyhow::Error> {
    {{ resolve_inputs }}
    let result = core::{{ operation_name }}({{ core_call_args }})?;
    Ok(Output::Render(result.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn typed_handler_maps_input_to_core_and_view() {
{%- if handler_sample_setup %}
{{ handler_sample_setup }}
{%- endif %}
        let matches = crate::cli::command()
            .try_get_matches_from({{ handler_args }})
            .unwrap();
        let (_, matches) = matches.subcommand().unwrap();
        let ctx = CommandContext::default();

        let Output::Render(view) = {{ command_ident }}(matches, &ctx).unwrap() else {
            panic!("expected rendered data");
        };
{%- if handler_sample_cleanup %}
{{ handler_sample_cleanup }}
{%- endif %}

        assert_eq!(
            view,
            {{ view_name }} {
{{ handler_expected_fields }}
            }
        );
    }
}
"#,
    ),
    ("template", r#"{{ template_body }}"#),
    (
        "style",
        r#".title {
  font-weight: bold;
  color: cyan;
}
"#,
    ),
    (
        "readme",
        r#"# {{ executable_name }}

This generated project is a Standout architecture starter, not a finished
application. The reusable `{{ lib_crate }}` crate owns the CLI-free operation
and validation. This binary crate owns Clap declarations, Standout wiring,
input policy, handlers, serializable view types, templates, styles, and process
execution.

Run the generated command:

```sh
{{ readme_examples }}
```

Command syntax:

```text
{{ executable_name }} {{ command_name }} {{ command_syntax }}
```

Input policy:

{{ readme_input_policy }}

{%- if readme_validation_note %}

{{ readme_validation_note }}
{%- endif %}

The generated `{{ result_shape }}` result is intentionally small. The handler
maps resolved shell input into `{{ lib_crate }}::{{ operation_name }}` and maps
the core result into the CLI-owned `{{ view_name }}`.

Verify the project with:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
```
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn required_string(name: impl Into<String>) -> CommandInput {
        CommandInput {
            name: name.into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
        }
    }

    fn sample_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "hello-tool".into(),
            executable_name: "hello-tool".into(),
            command_name: "greet".into(),
            command_description: "Greet one value".into(),
            inputs: vec![required_string("name")],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into()],
        })
        .unwrap();
        spec.destination = root.join("hello-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("standout crate lives under crates/standout in the repository")
            .to_path_buf()
    }

    #[test]
    fn exhausted_input_aborts_instead_of_looping() {
        let mut input = io::Cursor::new("1bad\n");
        let mut output = Vec::new();

        let error = prompt_answers(&mut input, &mut output).unwrap_err();

        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("project name must start"));
        assert!(error
            .to_string()
            .contains("input ended before the questionnaire completed"));
    }

    #[test]
    fn invalid_scalar_answers_reprompt_the_current_question() {
        let answers = [
            "1bad",
            "hello-tool",
            "",
            "fn",
            "greet",
            "Greet one value",
            "two",
            "0",
            "1",
            "name",
            "strang",
            "string",
            "sometimes",
            "required",
            "carrier,argument",
            "argument",
            "list",
            "message",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let answers = prompt_answers(&mut input, &mut output).unwrap();

        assert_eq!(answers.project_name, "hello-tool");
        assert_eq!(answers.command_name, "greet");
        assert_eq!(answers.inputs.len(), 1);
        assert_eq!(answers.result_shape, ResultShape::Message);
        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("project name must start"));
        assert!(transcript.contains("reserved Rust keyword"));
        assert!(transcript.contains("number of command inputs must be an integer"));
        assert!(transcript.contains("at least one command input is required"));
        assert!(transcript.contains("input type must be string, bool, or path"));
        assert!(transcript.contains("input cardinality must be required"));
        assert!(transcript.contains("input source must be argument, file, or stdin"));
        assert!(transcript.contains("result shape must be message or record"));
    }

    #[test]
    fn invalid_record_field_answers_reprompt_without_discarding_progress() {
        let answers = [
            "hello-tool",
            "",
            "greet",
            "Greet one value",
            "1",
            "name",
            "string",
            "required",
            "argument",
            "record",
            "docker-volume,summary",
            "docker_volume,summary",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let answers = prompt_answers(&mut input, &mut output).unwrap();

        assert_eq!(answers.record_fields, vec!["docker_volume", "summary"]);
        let transcript = String::from_utf8(output).unwrap();
        assert!(
            transcript.contains("record field may only contain letters, numbers, or underscores")
        );
        assert_eq!(
            transcript
                .matches("Record fields (comma-separated)")
                .count(),
            2
        );
    }

    #[test]
    fn cross_answer_input_constraint_retries_the_input_block() {
        let answers = [
            "demo",
            "",
            "inspect",
            "Inspect one value",
            "1",
            "config",
            "path",
            "required",
            "argument,file",
            "config",
            "path",
            "required",
            "argument",
            "message",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let answers = prompt_answers(&mut input, &mut output).unwrap();

        assert_eq!(answers.inputs.len(), 1);
        assert_eq!(answers.inputs[0].sources, vec![InputSource::Argument]);
        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("path inputs only support argument source"));
        assert_eq!(transcript.matches("Input 1:").count(), 2);
    }

    #[test]
    fn colliding_generated_flags_retry_the_conflicting_input_block() {
        let answers = [
            "demo",
            "",
            "inspect",
            "Inspect one value",
            "2",
            "document",
            "string",
            "required",
            "argument,file",
            "document_file",
            "path",
            "optional",
            "argument",
            "note",
            "string",
            "optional",
            "argument",
            "message",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let answers = prompt_answers(&mut input, &mut output).unwrap();

        assert_eq!(answers.inputs.len(), 2);
        assert_eq!(answers.inputs[1].name, "note");
        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("--document-file"));
        assert!(transcript.contains("conflicts"));
        assert_eq!(transcript.matches("Input 2:").count(), 2);
    }

    #[test]
    fn uppercase_x_cancels_after_earlier_valid_answers() {
        let answers = ["hello-tool", "", "greet", "X"].join("\n") + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let error = prompt_answers(&mut input, &mut output).unwrap_err();

        assert!(error.is::<Cancelled>());
    }

    #[test]
    fn questionnaire_collects_supported_input_matrix() {
        let answers = [
            "inspect-tool",
            "",
            "inspect",
            "Inspect document input",
            "4",
            "document",
            "string",
            "required",
            "argument,file,stdin",
            "verbose",
            "bool",
            "boolean",
            "argument",
            "tag",
            "string",
            "repeated",
            "argument",
            "config",
            "path",
            "optional",
            "argument",
            "record",
            "summary,count,echo",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut output = Vec::new();

        let answers = prompt_answers(&mut input, &mut output).unwrap();
        let spec = ProjectSpec::from_answers(answers).unwrap();

        assert_eq!(spec.executable_name, "inspect-tool");
        assert_eq!(spec.inputs.len(), 4);
        assert_eq!(
            spec.inputs[0].sources,
            vec![InputSource::Argument, InputSource::File, InputSource::Stdin,]
        );
        assert_eq!(spec.inputs[1].value_type, InputValueType::Bool);
        assert_eq!(spec.inputs[2].cardinality, InputCardinality::Repeated);
        assert_eq!(spec.inputs[3].value_type, InputValueType::Path);
    }

    #[test]
    fn input_source_aliases_cannot_declare_the_same_source_twice() {
        let duplicate = parse_input_sources("argument,arg").unwrap_err();

        assert!(duplicate.to_string().contains("declared more than once"));
    }

    #[test]
    fn project_spec_is_private_validated_model() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        assert_eq!(spec.lib_crate, "demolib");
        assert_eq!(spec.operation_name, "process_inspect");
        assert_eq!(spec.view_name, "InspectView");
    }

    #[test]
    fn render_builds_files_in_memory_before_publish() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());

        let generated = GeneratedFiles::render(&spec).unwrap();

        assert!(generated.files.contains_key(Path::new("Cargo.toml")));
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello_toollib/src/lib.rs")));
        assert!(generated
            .files
            .contains_key(Path::new("crates/hello-tool/README.md")));
        assert!(!spec.destination.exists());
    }

    #[test]
    fn generated_manifests_only_depend_on_publishable_workspace_crates() {
        let dir = TempDir::new().unwrap();
        let mut spec = sample_spec(dir.path());
        // [patch.crates-io] would redirect the family deps to local paths and
        // hide what a user's clean registry resolution actually sees.
        spec.local_patch_root = None;
        let generated = GeneratedFiles::render(&spec).unwrap();
        write_generated_files(&spec.destination, &generated).unwrap();

        let emitted = generated_family_crates_io_deps(&spec.destination.join("Cargo.toml"));
        assert!(
            !emitted.is_empty(),
            "the generated project is expected to depend on the standout family"
        );

        let publishable = workspace_crates_io_publishable();
        let stranded: Vec<&String> = emitted
            .iter()
            .filter(|name| publishable.get(*name) != Some(&true))
            .collect();
        assert!(
            stranded.is_empty(),
            "the wizard emits crates.io dependencies on workspace crates that are not \
             published to crates.io: {stranded:?}. A generated project cannot resolve \
             them at any version this workspace pins. Publish those crates or stop \
             generating a dependency on them.\nemitted: {emitted:?}\npublishable: {publishable:?}"
        );
    }

    #[test]
    fn result_fields_and_generated_identifiers_are_validated() {
        let duplicate = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "summary".into()],
        })
        .unwrap_err();
        let keyword = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "match".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(duplicate.to_string().contains("declared more than once"));
        assert!(keyword.to_string().contains("reserved Rust keyword"));
    }

    #[test]
    fn generated_flags_cannot_collide_across_inputs() {
        let answers_with = |inputs| WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs,
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        };

        for reserved in ["output", "output_file_path", "help"] {
            let error = ProjectSpec::from_answers(answers_with(vec![required_string(reserved)]))
                .unwrap_err();
            assert!(error.to_string().contains("reserved framework/Clap flag"));
            assert!(error
                .to_string()
                .contains(&format!("--{}", reserved.replace('_', "-"))));
        }

        let derived_collision = ProjectSpec::from_answers(answers_with(vec![
            required_string("document"),
            CommandInput {
                name: "document_file".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        ]))
        .unwrap_err();
        assert!(derived_collision.to_string().contains("--document-file"));
        assert!(derived_collision.to_string().contains("conflicts"));

        let questionnaire = [
            "reserved-tool",
            "",
            "inspect",
            "Inspect one value",
            "1",
            "output",
            "string",
            "required",
            "argument",
            "document",
            "string",
            "required",
            "argument",
            "message",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(questionnaire);
        let mut output = Vec::new();
        let answers = prompt_answers(&mut input, &mut output).unwrap();
        assert_eq!(answers.inputs[0].name, "document");
        let transcript = String::from_utf8(output).unwrap();
        assert!(transcript.contains("reserved framework/Clap flag --output"));
    }

    #[test]
    fn render_omits_local_patch_paths_by_default() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(!manifest.contains("[patch.crates-io]"));
        assert!(!manifest.contains(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn local_patch_paths_are_escaped_as_toml_basic_string_content() {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.local_patch_root = Some(PathBuf::from(r#"C:\Users\Ada "Q"\standout"#));

        let generated = GeneratedFiles::render(&spec).unwrap();
        let manifest = generated.files.get(Path::new("Cargo.toml")).unwrap();

        assert!(manifest
            .contains(r#"standout = { path = "C:\\Users\\Ada \"Q\"\\standout/crates/standout" }"#));
        assert!(!manifest.contains(r#"path = "C:\Users"#));
    }

    #[test]
    fn validates_supported_typed_cardinality_source_combinations() {
        let rich_inputs = vec![
            CommandInput {
                name: "document".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument, InputSource::File, InputSource::Stdin],
            },
            CommandInput {
                name: "verbose".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Boolean,
                sources: vec![InputSource::Argument],
            },
            CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        ];

        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: rich_inputs,
            result_shape: ResultShape::Record,
            record_fields: vec!["summary".into(), "count".into()],
        })
        .unwrap();

        assert_eq!(
            spec.inputs[0].policy_sentence(),
            "document comes from --document, then --document-file, then piped stdin"
        );
    }

    #[test]
    fn generated_sources_commands_and_validation_preserve_the_validated_model() {
        let file_only = CommandInput {
            name: "document".into(),
            value_type: InputValueType::String,
            cardinality: InputCardinality::Required,
            sources: vec![InputSource::File],
        };
        assert_eq!(
            command_syntax_fragment(&file_only),
            "--document-file <PATH>"
        );
        assert_eq!(
            command_syntax_fragment(&CommandInput {
                name: "tag".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Repeated,
                sources: vec![InputSource::Argument],
            }),
            "[--tag <tag>]..."
        );

        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "send_email".into(),
            command_description: "Send one message".into(),
            inputs: vec![
                file_only,
                CommandInput {
                    name: "subject".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Required,
                    sources: vec![InputSource::Stdin],
                },
            ],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let cli = generated
            .files
            .get(Path::new("crates/demo/src/cli.rs"))
            .unwrap();
        let handlers = generated
            .files
            .get(Path::new("crates/demo/src/handlers.rs"))
            .unwrap();
        let core = generated
            .files
            .get(Path::new("crates/demolib/src/lib.rs"))
            .unwrap();
        let readme = generated
            .files
            .get(Path::new("crates/demo/README.md"))
            .unwrap();

        assert!(cli.contains("#[command(name = \"send_email\")]"));
        assert!(cli.contains("long = \"document-file\""));
        assert!(!cli.contains("long = \"document\""));
        assert!(!cli.contains("long = \"subject\""));
        assert!(handlers.contains("use serial_test::serial;"));
        assert!(handlers.contains("#[serial]\n    fn typed_handler_maps_input_to_core_and_view()"));
        assert!(core.contains("CoreError::EmptyInput { field: \"document\" }"));
        assert!(core.contains("CoreError::EmptyInput { field: \"subject\" }"));
        assert!(readme.contains("demo send_email --document-file <PATH> <piped stdin>"));
    }

    #[test]
    fn rejects_unsupported_input_combinations_before_rendering() {
        let error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "enabled".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("bool inputs must use boolean cardinality"));

        let error = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument, InputSource::File],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("path inputs only support argument source"));
    }

    #[test]
    fn path_input_rendering_does_not_emit_string_validation() {
        let spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: "Inspect one value".into(),
            inputs: vec![CommandInput {
                name: "config".into(),
                value_type: InputValueType::Path,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::Argument],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();

        let generated = GeneratedFiles::render(&spec).unwrap();
        let core = generated
            .files
            .get(Path::new("crates/demolib/src/lib.rs"))
            .unwrap();

        assert!(core.contains("config: std::path::PathBuf"));
        assert!(!core.contains("config.trim()"));
    }

    #[test]
    fn publish_refuses_non_empty_destination_without_partial_staging() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        fs::create_dir_all(&spec.destination).unwrap();
        fs::write(spec.destination.join("keep.txt"), "existing").unwrap();

        let error = publish_project(&spec).unwrap_err();

        assert!(error.to_string().contains("not empty"));
        assert_eq!(
            fs::read_to_string(spec.destination.join("keep.txt")).unwrap(),
            "existing"
        );
        let staged: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
            .collect();
        assert!(staged.is_empty());
    }

    #[test]
    fn publish_refuses_file_destination_with_clear_error() {
        let dir = TempDir::new().unwrap();
        let spec = sample_spec(dir.path());
        fs::write(&spec.destination, "existing").unwrap();

        let error = publish_project(&spec).unwrap_err();

        assert!(error.to_string().contains("not a directory"));
        assert_eq!(fs::read_to_string(&spec.destination).unwrap(), "existing");
        let staged: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("standout-new"))
            .collect();
        assert!(staged.is_empty());
    }

    #[test]
    fn declined_confirmation_is_a_non_mutating_cancel() {
        let mut input = io::Cursor::new("no\n");
        let mut output = Vec::new();

        assert!(!confirm(&mut input, &mut output).unwrap());
    }

    fn spec_with_description(description: &str) -> ProjectSpec {
        ProjectSpec::from_answers(WizardAnswers {
            project_name: "demo".into(),
            executable_name: "demo".into(),
            command_name: "inspect".into(),
            command_description: description.into(),
            inputs: vec![required_string("document")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap()
    }

    #[test]
    fn command_attribute_splits_exactly_at_the_rustfmt_width_boundary() {
        // The rendered argument list is `name = "demo", about = "..."`:
        // 25 characters of scaffolding around the description text.
        let at_limit = spec_with_description(&"a".repeat(ATTR_FN_LIKE_WIDTH - 25));
        assert_eq!(
            cli_command_attribute(&at_limit),
            format!(
                "#[command(name = \"demo\", about = \"{}\")]",
                "a".repeat(ATTR_FN_LIKE_WIDTH - 25)
            )
        );

        let over_limit = spec_with_description(&"a".repeat(ATTR_FN_LIKE_WIDTH - 24));
        assert_eq!(
            cli_command_attribute(&over_limit),
            format!(
                "#[command(\n    name = \"demo\",\n    about = \"{}\"\n)]",
                "a".repeat(ATTR_FN_LIKE_WIDTH - 24)
            )
        );
    }

    #[test]
    fn command_attribute_measures_display_width_not_char_count() {
        // rustfmt's width heuristic counts Unicode display width, so CJK
        // characters (width 2) hit the budget at half the char count. The
        // scaffolding `name = "demo", about = ""` contributes 25 columns.
        // 22 CJK chars: width 25 + 44 = 69 <= 70 -> inline (47 chars total).
        let inline = spec_with_description(&"検".repeat(22));
        assert_eq!(
            cli_command_attribute(&inline),
            format!(
                "#[command(name = \"demo\", about = \"{}\")]",
                "検".repeat(22)
            )
        );

        // 23 CJK chars: width 25 + 46 = 71 > 70 -> split, even though the
        // argument list is only 48 chars (well under the budget by count).
        let split = spec_with_description(&"検".repeat(23));
        assert_eq!(
            cli_command_attribute(&split),
            format!(
                "#[command(\n    name = \"demo\",\n    about = \"{}\"\n)]",
                "検".repeat(23)
            )
        );
    }

    #[test]
    fn long_description_generated_project_is_rustfmt_clean() {
        let dir = TempDir::new().unwrap();
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "provisioning-tool".into(),
            executable_name: "provisioning-tool".into(),
            command_name: "provision".into(),
            command_description: "Provisions pinned env either container or bare metal".into(),
            inputs: vec![required_string("target")],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = dir.path().join("provisioning-tool");
        spec.local_patch_root = Some(workspace_root());

        publish_project(&spec).unwrap();

        let cli = fs::read_to_string(spec.destination.join("crates/provisioning-tool/src/cli.rs"))
            .unwrap();
        assert!(cli.contains(
            "#[command(\n    name = \"provisioning-tool\",\n    \
             about = \"Provisions pinned env either container or bare metal\"\n)]"
        ));
        run_cargo(&spec.destination, ["fmt", "--check"]);
    }

    fn rich_questionnaire_spec(root: &Path) -> ProjectSpec {
        let answers = [
            "inspect-tool",
            "",
            "inspect",
            "Inspect document input",
            "4",
            "document",
            "string",
            "required",
            "argument,file,stdin",
            "verbose",
            "bool",
            "boolean",
            "argument",
            "tag",
            "string",
            "repeated",
            "argument",
            "config",
            "path",
            "optional",
            "argument",
            "record",
            "summary,count,echo",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(answers);
        let mut review = Vec::new();
        let answers = prompt_answers(&mut input, &mut review).unwrap();
        let mut spec = ProjectSpec::from_answers(answers).unwrap();
        spec.destination = root.join("inspect-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn file_only_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "file-tool".into(),
            executable_name: "file-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect file contents".into(),
            inputs: vec![CommandInput {
                name: "document".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Required,
                sources: vec![InputSource::File],
            }],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join("file-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn single_input_spec(root: &Path, project_name: &str, input: CommandInput) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: project_name.into(),
            executable_name: project_name.into(),
            command_name: "inspect".into(),
            command_description: "Inspect configured input".into(),
            inputs: vec![input],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join(project_name);
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    fn path_first_spec(root: &Path) -> ProjectSpec {
        let mut spec = ProjectSpec::from_answers(WizardAnswers {
            project_name: "config-tool".into(),
            executable_name: "config-tool".into(),
            command_name: "inspect".into(),
            command_description: "Inspect a config path".into(),
            inputs: vec![
                CommandInput {
                    name: "config".into(),
                    value_type: InputValueType::Path,
                    cardinality: InputCardinality::Required,
                    sources: vec![InputSource::Argument],
                },
                CommandInput {
                    name: "note".into(),
                    value_type: InputValueType::String,
                    cardinality: InputCardinality::Optional,
                    sources: vec![InputSource::Argument],
                },
            ],
            result_shape: ResultShape::Message,
            record_fields: Vec::new(),
        })
        .unwrap();
        spec.destination = root.join("config-tool");
        spec.local_patch_root = Some(workspace_root());
        spec
    }

    #[test]
    fn generated_project_matrix_formats_checks_tests_and_runs() {
        let dir = TempDir::new().unwrap();
        let mut message = sample_spec(dir.path());
        message.result_shape = ResultShape::Message;
        message.record_fields.clear();
        let rich = rich_questionnaire_spec(dir.path());
        let path_first = path_first_spec(dir.path());
        let file_only = file_only_spec(dir.path());
        let bool_first = single_input_spec(
            dir.path(),
            "bool-tool",
            CommandInput {
                name: "verbose".into(),
                value_type: InputValueType::Bool,
                cardinality: InputCardinality::Boolean,
                sources: vec![InputSource::Argument],
            },
        );
        let optional_first = single_input_spec(
            dir.path(),
            "optional-tool",
            CommandInput {
                name: "note".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Optional,
                sources: vec![InputSource::Argument],
            },
        );
        let repeated_first = single_input_spec(
            dir.path(),
            "repeated-tool",
            CommandInput {
                name: "tag".into(),
                value_type: InputValueType::String,
                cardinality: InputCardinality::Repeated,
                sources: vec![InputSource::Argument],
            },
        );

        for spec in [
            &message,
            &rich,
            &path_first,
            &file_only,
            &bool_first,
            &optional_first,
            &repeated_first,
        ] {
            publish_project(spec).unwrap();
            run_cargo(&spec.destination, ["fmt", "--check"]);
            run_cargo(&spec.destination, ["check", "--workspace"]);
            run_cargo(&spec.destination, ["test", "--workspace"]);
        }

        let file_readme =
            fs::read_to_string(file_only.destination.join("crates/file-tool/README.md")).unwrap();
        assert!(file_readme.contains("--document-file document-input.txt"));
        assert!(!file_readme.contains("--document VALUE"));
        assert!(file_readme
            .contains("Blank values for the required string input `document` are rejected"));
        let bool_readme =
            fs::read_to_string(bool_first.destination.join("crates/bool-tool/README.md")).unwrap();
        assert!(bool_readme.contains("inspect --verbose"));
        assert!(!bool_readme.contains("Blank values"));
        let path_readme =
            fs::read_to_string(path_first.destination.join("crates/config-tool/README.md"))
                .unwrap();
        assert!(!path_readme.contains("Blank values"));
        let optional_readme = fs::read_to_string(
            optional_first
                .destination
                .join("crates/optional-tool/README.md"),
        )
        .unwrap();
        assert!(!optional_readme.contains("Blank values"));
        let repeated_readme = fs::read_to_string(
            repeated_first
                .destination
                .join("crates/repeated-tool/README.md"),
        )
        .unwrap();
        assert!(!repeated_readme.contains("Blank values"));

        let file_input = file_only.destination.join("document.txt");
        fs::write(&file_input, "File only").unwrap();
        let file_only_run = run_binary(
            &file_only.destination,
            [
                "run",
                "-q",
                "-p",
                "file-tool",
                "--",
                "inspect",
                "--document-file",
                file_input.to_str().unwrap(),
            ],
        );
        assert!(String::from_utf8(file_only_run.stdout)
            .unwrap()
            .contains("Processed File only"));

        let message_human = run_binary(
            &message.destination,
            [
                "run",
                "-q",
                "-p",
                "hello-tool",
                "--",
                "greet",
                "--name",
                "Ada",
            ],
        );
        let stdout = String::from_utf8(message_human.stdout).unwrap();
        assert!(stdout.contains("Processed Ada"));

        let human = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Ada",
                "--verbose",
                "--tag",
                "alpha",
                "--tag",
                "beta",
                "--config",
                "settings.toml",
            ],
        );
        let stdout = String::from_utf8(human.stdout).unwrap();
        assert!(stdout.contains("Ada"));
        assert!(stdout.contains("Summary:"));
        assert!(stdout.contains("Echo: Ada"));

        let json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Ada",
                "--output",
                "json",
            ],
        );
        let value = json_value(&json);
        assert_eq!(value["summary"], "Processed Ada");
        assert_eq!(value["count"], "3");
        assert_eq!(value["echo"], "Ada");

        let input_file = rich.destination.join("input.txt");
        fs::write(&input_file, "File Ada").unwrap();
        let file_json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        let value = json_value(&file_json);
        assert_eq!(value["summary"], "Processed File Ada");
        assert_eq!(value["count"], "8");

        let stdin_json = run_binary_with_stdin(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--output",
                "json",
            ],
            "Pipe Ada\n",
        );
        let value = json_value(&stdin_json);
        assert_eq!(value["summary"], "Processed Pipe Ada");
        assert_eq!(value["count"], "8");

        let precedence_json = run_binary(
            &rich.destination,
            [
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "Arg Ada",
                "--document-file",
                input_file.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        let value = json_value(&precedence_json);
        assert_eq!(value["summary"], "Processed Arg Ada");
        assert_eq!(value["count"], "7");

        let invalid = Command::new("cargo")
            .current_dir(&rich.destination)
            .args([
                "run",
                "-q",
                "-p",
                "inspect-tool",
                "--",
                "inspect",
                "--document",
                "   ",
            ])
            .output()
            .unwrap();
        assert!(!invalid.status.success());
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("document cannot be empty"));

        let missing = Command::new("cargo")
            .current_dir(&rich.destination)
            .args(["run", "-q", "-p", "inspect-tool", "--", "inspect"])
            .output()
            .unwrap();
        assert!(!missing.status.success());
        assert!(String::from_utf8_lossy(&missing.stderr).contains("document is required"));
    }

    fn cargo_metadata(manifest: &Path) -> serde_json::Value {
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--offline",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(manifest)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo metadata failed for {}\nstderr:\n{}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    /// How `cargo metadata` spells the source of a dependency that resolves
    /// from the default registry.
    const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

    /// Names of the crates.io `standout*` dependencies declared anywhere in the
    /// workspace rooted at `manifest`, across every dependency kind.
    fn generated_family_crates_io_deps(manifest: &Path) -> std::collections::BTreeSet<String> {
        let metadata = cargo_metadata(manifest);
        let mut names = std::collections::BTreeSet::new();
        for package in metadata["packages"].as_array().unwrap() {
            for dependency in package["dependencies"].as_array().unwrap() {
                let name = dependency["name"].as_str().unwrap();
                let from_crates_io = dependency["source"].as_str() == Some(CRATES_IO_SOURCE);
                if from_crates_io && name.starts_with("standout") {
                    names.insert(name.to_string());
                }
            }
        }
        names
    }

    /// Every member of this repository's workspace, mapped to whether `cargo
    /// publish` would send it to crates.io.
    fn workspace_crates_io_publishable() -> std::collections::BTreeMap<String, bool> {
        let metadata = cargo_metadata(&workspace_root().join("Cargo.toml"));
        metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|package| {
                let registries = &package["publish"];
                let publishable = match registries.as_array() {
                    None => true,
                    Some(registries) => registries
                        .iter()
                        .any(|registry| registry.as_str() == Some("crates-io")),
                };
                (package["name"].as_str().unwrap().to_string(), publishable)
            })
            .collect()
    }

    fn run_cargo<const N: usize>(cwd: &Path, args: [&str; N]) {
        let output = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo command failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_binary<const N: usize>(cwd: &Path, args: [&str; N]) -> std::process::Output {
        let output = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "binary run failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn run_binary_with_stdin<const N: usize>(
        cwd: &Path,
        args: [&str; N],
        stdin: &str,
    ) -> std::process::Output {
        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new("cargo")
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "binary run with stdin failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn json_value(output: &std::process::Output) -> serde_json::Value {
        serde_json::from_slice(&output.stdout).unwrap()
    }

    /// Set `value` as the answer text below the `nth` (zero-based) question
    /// line ending with `<id:...>`, replacing a pre-filled default line when
    /// one is rendered. Panics when the sheet has no such question line, so
    /// a test cannot silently leave a question unanswered.
    fn fill_nth(sheet: &str, id: &str, value: &str, nth: usize) -> String {
        let tag = format!("<id:{id}>");
        let lines: Vec<&str> = sheet.lines().collect();
        let mut out: Vec<String> = Vec::new();
        let mut seen = 0;
        let mut done = false;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            out.push(line.to_string());
            i += 1;
            if line.trim_end().ends_with(&tag) {
                if seen == nth {
                    // A non-blank line right below the question is a
                    // pre-filled default: the answer replaces it.
                    if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                        i += 1;
                    }
                    out.push(value.to_string());
                    done = true;
                }
                seen += 1;
            }
        }
        assert!(done, "answer sheet has no occurrence {nth} of {tag}");
        out.join("\n") + "\n"
    }

    fn fill(sheet: &str, id: &str, value: &str) -> String {
        fill_nth(sheet, id, value, 0)
    }

    /// Simulate copy-the-block editing: duplicate the complete last
    /// `command.inputs` block (its group tag line through its last
    /// question's answer line) below itself, exactly as a user adding an
    /// item would.
    fn duplicate_inputs_block(sheet: &str) -> String {
        let lines: Vec<&str> = sheet.lines().collect();
        let start = lines
            .iter()
            .rposition(|line| line.trim_end().ends_with("<id:command.inputs>"))
            .expect("sheet renders the repeatable inputs group tag line");
        let sources = lines
            .iter()
            .rposition(|line| line.trim_end().ends_with("<id:command.inputs.sources>"))
            .expect("sheet renders the sources question");
        let end = sources + 1; // the sources answer line (default or filled)
        let mut copied: Vec<&str> = lines[..=end].to_vec();
        copied.push("");
        copied.extend(&lines[start..=end]);
        copied.extend(&lines[end + 1..]);
        copied.join("\n") + "\n"
    }

    /// Decode one completed sheet through the exact production pipeline —
    /// parse, shared decoding, wizard whole-form rules, domain conversion.
    fn decode_sheet(sheet: &str) -> Result<WizardAnswers, Vec<String>> {
        let questionnaire = wizard_questionnaire();
        let raw = questionnaire.parse_answer_sheet(sheet);
        super::decode_sheet(&questionnaire, raw)
    }

    #[test]
    fn answer_sheet_and_interactive_submissions_decode_identically() {
        // Nested (command group), repeated (two input blocks), defaulted
        // (result.shape stays "record"), and a blank executable reusing the
        // project name — answered once through the sheet and once through
        // the interactive prompts.
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "inspect-tool");
        let sheet = fill(&sheet, "command.name", "inspect");
        let sheet = fill(&sheet, "command.description", "Inspect document input");
        let sheet = fill(&sheet, "command.inputs.name", "document");
        let sheet = fill(&sheet, "command.inputs.sources", "argument,file,stdin");
        let sheet = duplicate_inputs_block(&sheet);
        let sheet = fill_nth(&sheet, "command.inputs.name", "verbose", 1);
        let sheet = fill_nth(&sheet, "command.inputs.value_type", "bool", 1);
        let sheet = fill_nth(&sheet, "command.inputs.cardinality", "boolean", 1);
        let sheet = fill_nth(&sheet, "command.inputs.sources", "argument", 1);
        let sheet = fill(&sheet, "result.fields", "summary,count,echo");

        let from_sheet = decode_sheet(&sheet).unwrap();

        let script = [
            "inspect-tool",
            "",
            "inspect",
            "Inspect document input",
            "2",
            "document",
            "string",
            "required",
            "argument,file,stdin",
            "verbose",
            "bool",
            "boolean",
            "argument",
            "record",
            "summary,count,echo",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let from_prompts = prompt_answers(&mut input, &mut output).unwrap();

        assert_eq!(from_sheet, from_prompts);
        assert_eq!(
            ProjectSpec::from_answers(from_sheet).unwrap(),
            ProjectSpec::from_answers(from_prompts).unwrap()
        );
    }

    #[test]
    fn sheet_defaults_resolve_like_interactive_defaults() {
        // Untouched pre-filled defaults on the sheet (value type,
        // cardinality, result shape, record fields) and blank interactive
        // answers taking the same defaults produce equal answers; the
        // sources answer is explicit on both sides because its interactive
        // default is context-dependent while the sheet default is static.
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "name");

        let from_sheet = decode_sheet(&sheet).unwrap();

        let script = [
            "demo",
            "",
            "greet",
            "Greet one value",
            "1",
            "name",
            "",
            "",
            "argument",
            "",
            "",
        ]
        .join("\n")
            + "\n";
        let mut input = io::Cursor::new(script);
        let mut output = Vec::new();
        let from_prompts = prompt_answers(&mut input, &mut output).unwrap();

        assert_eq!(from_sheet, from_prompts);
        assert_eq!(from_sheet.executable_name, "demo");
        assert_eq!(from_sheet.record_fields, vec!["summary", "count"]);
    }

    #[test]
    fn multiline_description_keeps_internal_line_breaks() {
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(
            &sheet,
            "command.description",
            "Greet one value.\nIt spans two lines.",
        );
        let sheet = fill(&sheet, "command.inputs.name", "name");

        let answers = decode_sheet(&sheet).unwrap();

        assert_eq!(
            answers.command_description,
            "Greet one value.\nIt spans two lines."
        );
        assert!(ProjectSpec::from_answers(answers).is_ok());
    }

    #[test]
    fn populated_inactive_record_fields_are_rejected() {
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "name");
        let sheet = fill(&sheet, "result.shape", "message");
        let sheet = fill(&sheet, "result.fields", "summary,extra");

        let errors = decode_sheet(&sheet).unwrap_err();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("[result.fields]"));
        assert!(errors[0].contains("does not apply"));
    }

    #[test]
    fn inactive_record_fields_may_keep_their_untouched_default() {
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "name");
        let sheet = fill(&sheet, "result.shape", "message");

        let answers = decode_sheet(&sheet).unwrap();

        assert_eq!(answers.result_shape, ResultShape::Message);
        assert_eq!(answers.record_fields, Vec::<String>::new());
    }

    #[test]
    fn sheet_field_and_form_failures_accumulate_per_stage() {
        // Field stage: every independent field diagnostic reports together.
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "9bad");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "name");
        let sheet = fill(&sheet, "command.inputs.value_type", "integer");
        let sheet = fill(&sheet, "command.inputs.sources", "argument,teleport");

        let errors = decode_sheet(&sheet).unwrap_err();

        assert_eq!(errors.len(), 3, "unexpected diagnostics: {errors:?}");
        assert!(errors.iter().any(|e| e.contains("[project.name]")));
        assert!(errors
            .iter()
            .any(|e| e.contains("[command.inputs[0].value_type]")));
        assert!(errors
            .iter()
            .any(|e| e.contains("[command.inputs[0].sources]")));

        // Form stage: cross-answer rules over field-valid values accumulate
        // too — an unsupported combination and a generated-flag collision.
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "demo");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        let sheet = fill(&sheet, "command.inputs.name", "document");
        let sheet = fill(&sheet, "command.inputs.value_type", "path");
        let sheet = fill(&sheet, "command.inputs.sources", "file");
        let sheet = duplicate_inputs_block(&sheet);
        let sheet = fill_nth(&sheet, "command.inputs.name", "tag", 1);
        let sheet = fill_nth(&sheet, "command.inputs.value_type", "string", 1);
        let sheet = fill_nth(&sheet, "command.inputs.sources", "argument", 1);
        let mut sheet = duplicate_inputs_block(&sheet);
        // The third block collides with the second's generated --tag flag.
        for (id, value) in [
            ("command.inputs.name", "tag"),
            ("command.inputs.value_type", "string"),
            ("command.inputs.sources", "argument"),
        ] {
            sheet = fill_nth(&sheet, id, value, 2);
        }

        let errors = decode_sheet(&sheet).unwrap_err();

        assert_eq!(errors.len(), 2, "unexpected diagnostics: {errors:?}");
        assert!(errors
            .iter()
            .any(|e| e.contains("path inputs only support argument source")
                && e.contains("command.inputs[0]")));
        assert!(errors.iter().any(|e| e.contains("conflicts with input")));
    }

    #[test]
    fn stale_fingerprint_rejects_with_regeneration_guidance() {
        let sheet = wizard_questionnaire().render_answer_sheet();
        let stale = sheet.replacen("#! fingerprint: sha256:", "#! fingerprint: sha256:00", 1);

        let errors = decode_sheet(&stale).unwrap_err();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("render a fresh answer sheet"));
    }

    /// One minimal valid `hello-tool` sheet for source-equivalence tests.
    fn minimal_sheet() -> String {
        let sheet = wizard_questionnaire().render_answer_sheet();
        let sheet = fill(&sheet, "project.name", "hello-tool");
        let sheet = fill(&sheet, "command.name", "greet");
        let sheet = fill(&sheet, "command.description", "Greet one value");
        fill(&sheet, "command.inputs.name", "name")
    }

    #[test]
    fn file_and_stdin_sheets_decode_to_identical_answers_and_specs() {
        let sheet = minimal_sheet();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("answers.txt");
        fs::write(&path, &sheet).unwrap();

        let from_file = sheet_answers(&path).unwrap();
        let from_stdin = stdin_sheet_answers(&standout_input::MockStdin::piped(&sheet)).unwrap();

        assert_eq!(from_file, from_stdin);
        assert_eq!(
            ProjectSpec::from_answers(from_file).unwrap(),
            ProjectSpec::from_answers(from_stdin).unwrap()
        );
    }

    #[test]
    fn terminal_stdin_cannot_supply_an_answer_sheet() {
        let errors = stdin_sheet_answers(&standout_input::MockStdin::terminal()).unwrap_err();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("interactive terminal"));
    }

    #[test]
    fn attended_confirmation_accepts_only_an_exact_yes() {
        let confirmed = |reply: &str| {
            confirm_attended(&mut ScriptedTerminal::from_replies([reply.to_string()])).unwrap()
        };

        assert!(confirmed("yes"));
        assert!(confirmed("  yes  "));
        assert!(!confirmed("no"));
        assert!(!confirmed("YES please"));
        assert!(!confirmed(""));
    }

    #[test]
    fn attended_end_of_input_is_a_rejection_not_consent() {
        let mut terminal = ScriptedTerminal::from_replies(Vec::<String>::new());

        assert!(!confirm_attended(&mut terminal).unwrap());
    }

    #[test]
    fn missing_attended_terminal_fails_with_guidance() {
        let error = confirm_attended(&mut ScriptedTerminal::absent()).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("attended terminal"));
        assert!(message.contains("--yes"));
        assert!(message.contains("nothing was generated"));
    }
}
