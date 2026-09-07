use super::super::{CommandInput, InputCardinality, InputSource, InputValueType};
use super::formatting::{quote, MAX_WIDTH};

pub(in crate::new_project) fn command_syntax_fragment(input: &CommandInput) -> String {
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

impl CommandInput {
    /// Reaches the command through an `InputChain`, not a typed `#[handler]` parameter.
    pub(super) fn is_chain(&self) -> bool {
        self.sources != [InputSource::Argument]
    }

    pub(super) fn chain_expr(&self, indent: usize) -> String {
        use unicode_width::UnicodeWidthStr;

        let sources = self
            .sources
            .iter()
            .map(|source| match source {
                InputSource::Argument => {
                    format!(".try_source(ArgSource::new({}))", quote(&self.name))
                }
                InputSource::File => format!(
                    ".try_source(FileSource::new({}))",
                    quote(&format!("{}_file", self.name))
                ),
                InputSource::Stdin => ".try_source(StdinSource::new())".to_string(),
            })
            .collect::<Vec<_>>();
        let inline = format!("InputChain::<String>::new(){}", sources.concat());
        // A trailing comma follows the chain on its line.
        if indent + inline.width() < MAX_WIDTH {
            return inline;
        }
        let pad = " ".repeat(indent + 4);
        let lines = sources
            .iter()
            .map(|source| format!("{pad}{source}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("InputChain::<String>::new()\n{lines}")
    }

    /// An underscored name is spelled out: clap ids after the field, `#[handler]` hyphenates.
    pub(super) fn handler_param(&self) -> String {
        let attribute = if self.cardinality == InputCardinality::Boolean {
            "flag"
        } else {
            "arg"
        };
        let attribute = if self.name.contains('_') {
            format!("#[{attribute}(name = {})]", quote(&self.name))
        } else {
            format!("#[{attribute}]")
        };
        format!("{attribute} {}: {}", self.name, self.rust_type())
    }

    pub(super) fn handler_test_value(&self, value: &str) -> String {
        match (self.value_type, self.cardinality) {
            (InputValueType::String, InputCardinality::Required) => {
                format!("{}.to_string()", quote(value))
            }
            (InputValueType::String, InputCardinality::Optional) => {
                format!("Some({}.to_string())", quote(value))
            }
            (InputValueType::String, InputCardinality::Repeated) => {
                format!("vec![{}.to_string(), \"extra\".to_string()]", quote(value))
            }
            (InputValueType::Bool, InputCardinality::Boolean) => "true".into(),
            (InputValueType::Path, InputCardinality::Required) => {
                format!("std::path::PathBuf::from({})", quote(value))
            }
            (InputValueType::Path, InputCardinality::Optional) => {
                format!("Some(std::path::PathBuf::from({}))", quote(value))
            }
            (InputValueType::Path, InputCardinality::Repeated) => format!(
                "vec![std::path::PathBuf::from({}), std::path::PathBuf::from(\"extra.toml\")]",
                quote(value)
            ),
            _ => unreachable!("validated input combinations are renderable"),
        }
    }

    pub(super) fn rust_type(&self) -> &'static str {
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

    pub(super) fn cli_arg(&self) -> String {
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

    pub(super) fn sample_args_for_source(
        &self,
        source: InputSource,
        primary_value: &str,
    ) -> Vec<String> {
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

    pub(super) fn sample_file_name(&self) -> String {
        format!("{}-input.txt", self.name.replace('_', "-"))
    }

    pub(super) fn readme_args(&self, value: &str) -> Vec<String> {
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

    pub(super) fn core_call_arg(&self) -> String {
        if self.is_chain() {
            format!("{}.clone()", self.name)
        } else {
            self.name.clone()
        }
    }

    pub(super) fn core_validation(&self) -> Option<String> {
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
}
