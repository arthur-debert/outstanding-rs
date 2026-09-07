use super::{CommandInput, InputCardinality, InputSource, InputValueType, ResultShape};
use anyhow::{bail, Result};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

impl CommandInput {
    pub(super) fn validate(&self) -> Result<()> {
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
        if self.cardinality == InputCardinality::Optional
            && self
                .sources
                .iter()
                .any(|source| *source != InputSource::Argument)
        {
            bail!("{} optional inputs only support argument source", self.name);
        }
        Ok(())
    }
}

pub(super) fn validate_crate_name(value: &str, label: &str) -> Result<()> {
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

pub(super) fn validate_ident(value: &str, label: &str) -> Result<()> {
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

pub(super) fn parse_record_fields(value: &str) -> Result<Vec<String>> {
    let fields: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    validate_result_fields(ResultShape::Record, &fields)?;
    Ok(fields)
}

pub(super) fn parse_input_sources(value: &str) -> Result<Vec<InputSource>> {
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

pub(super) fn validate_result_fields(shape: ResultShape, fields: &[String]) -> Result<()> {
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

pub(super) fn validate_generated_flags(inputs: &[CommandInput]) -> Result<()> {
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

pub(super) fn pascal_case(value: &str) -> String {
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
