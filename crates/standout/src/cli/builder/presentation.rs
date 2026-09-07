use super::App;
use crate::ColorPolicy;
use crate::Representation;
use clap::parser::ValueSource;
use clap::ArgMatches;

impl App {
    pub fn extract_output_mode(&self, matches: &ArgMatches) -> Representation {
        self.typed_output_mode(matches)
            .unwrap_or(self.output_mode_fallback)
    }

    pub(crate) fn typed_output_mode(&self, matches: &ArgMatches) -> Option<Representation> {
        self.output_flag.as_ref()?;
        match matches.try_get_one::<String>(OUTPUT_MODE_ARG) {
            // A `DefaultValue` source means the user never typed `--output`.
            Ok(Some(value))
                if matches.value_source(OUTPUT_MODE_ARG) != Some(ValueSource::DefaultValue) =>
            {
                parse_output_mode_flag(value.as_str())
            }
            _ => None,
        }
    }

    /// The run's color policy, in precedence order: an explicit `--color`, the
    /// policy the caller named, `NO_COLOR`, the resolved `[term] color`, and
    /// last the destination, which `Auto` leaves to `resolve_style_mode`.
    /// `NO_COLOR` is read here only to outrank a configured `always`; below the
    /// key it is a destination fact the process edge already probes.
    pub(crate) fn resolve_color_policy(
        &self,
        typed: Option<ColorPolicy>,
        named: ColorPolicy,
        term: Option<&crate::TermSettings>,
    ) -> ColorPolicy {
        if let Some(policy) = typed {
            return policy;
        }
        if named != ColorPolicy::Auto {
            return named;
        }
        match term.and_then(|term| term.color).map(ColorPolicy::from) {
            Some(ColorPolicy::Always) if no_color_is_set() => ColorPolicy::Never,
            Some(policy) => policy,
            None => ColorPolicy::Auto,
        }
    }

    pub(crate) fn typed_color_policy(&self, matches: &ArgMatches) -> Option<ColorPolicy> {
        self.color_flag.as_ref()?;
        match matches.try_get_one::<String>(COLOR_ARG) {
            // A `DefaultValue` source means the user never typed `--color`.
            Ok(Some(value))
                if matches.value_source(COLOR_ARG) != Some(ValueSource::DefaultValue) =>
            {
                parse_color_flag(value.as_str())
            }
            _ => None,
        }
    }

    /// The pre-parse read the help and usage paths use, where no `ArgMatches` exists yet.
    pub(crate) fn typed_color_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> Option<ColorPolicy> {
        self.color_flag
            .as_deref()
            .and_then(|flag| last_unparsed_flag_value(flag, args))
            .and_then(parse_color_flag)
    }

    /// A named file is the run's destination whatever else the invocation asks
    /// for, so the page lands there, never in a pager.
    pub(crate) fn output_file_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> Option<std::path::PathBuf> {
        self.output_file_flag
            .as_deref()
            .and_then(|flag| last_unparsed_flag_value(flag, args))
            .map(std::path::PathBuf::from)
    }

    pub(crate) fn paging_is_suppressed(&self, args: &[std::ffi::OsString]) -> bool {
        self.pager_flag
            .as_deref()
            .is_some_and(|flag| unparsed_flag_is_present(flag, args))
    }

    pub(crate) fn extract_output_mode_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> Representation {
        let Some(flag) = self.output_flag.as_deref() else {
            return self.output_mode_fallback;
        };
        last_unparsed_flag_value(flag, args)
            .and_then(parse_output_mode_flag)
            .unwrap_or(self.output_mode_fallback)
    }
}

/// `--output` names a structured encoding and nothing else; the human
/// representation is what a bare invocation renders and has no spelling.
/// `term-debug` stays as the diagnostic view of the template's style tags.
pub(crate) const OUTPUT_MODE_FLAG_VALUES: [&str; 5] =
    ["json", "yaml", "csv", "ndjson", "term-debug"];

fn parse_output_mode_flag(value: &str) -> Option<Representation> {
    match value {
        "json" => Some(Representation::Json),
        "yaml" => Some(Representation::Yaml),
        "csv" => Some(Representation::Csv),
        "ndjson" => Some(Representation::Ndjson),
        "term-debug" => Some(Representation::TermDebug),
        _ => None,
    }
}

/// `None` for the human representation, which the flag cannot name.
pub(crate) fn output_mode_flag_spelling(representation: Representation) -> Option<&'static str> {
    OUTPUT_MODE_FLAG_VALUES
        .into_iter()
        .find(|value| parse_output_mode_flag(value) == Some(representation))
}

/// `--color` decides whether human text carries escape sequences, on its own
/// and whatever `--output` names.
pub(crate) const COLOR_FLAG_VALUES: [&str; 3] = ["auto", "always", "never"];

pub(crate) const COLOR_ARG: &str = "_color";

pub(crate) const NO_PAGER_ARG: &str = "_no_pager";

pub(crate) const OUTPUT_MODE_ARG: &str = "_output_mode";

pub(crate) const OUTPUT_FILE_ARG: &str = "_output_file_path";

pub(crate) const COLOR_FLAG_DEFAULT: &str = "auto";

/// The convention: set and non-empty asks for no color.
fn no_color_is_set() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

fn parse_color_flag(value: &str) -> Option<ColorPolicy> {
    match value {
        "auto" => Some(ColorPolicy::Auto),
        "always" => Some(ColorPolicy::Always),
        "never" => Some(ColorPolicy::Never),
        _ => None,
    }
}

fn unparsed_flag_is_present(flag: &str, args: &[std::ffi::OsString]) -> bool {
    let long = format!("--{flag}");
    args.iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == long)
}

fn last_unparsed_flag_value<'a>(flag: &str, args: &'a [std::ffi::OsString]) -> Option<&'a str> {
    let long = format!("--{flag}");
    let prefix = format!("--{flag}=");
    let mut found = None;
    let mut iter = args.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        let Some(arg) = arg.to_str() else {
            continue;
        };
        if arg == "--" {
            break;
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            found = Some(value);
            continue;
        }
        if arg == long {
            match iter.peek().and_then(|next| next.to_str()) {
                None => found = None,
                Some("--") => {
                    found = None;
                    break;
                }
                Some(next) if next.starts_with('-') => found = None,
                Some(_) => found = iter.next().and_then(|next| next.to_str()),
            }
        }
    }
    found
}
