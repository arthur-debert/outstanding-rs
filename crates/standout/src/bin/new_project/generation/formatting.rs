use super::super::ProjectSpec;
use super::handlers::has_chain_inputs;
use std::path::Path;

#[cfg(test)]
mod tests;

pub(super) fn quote(value: &str) -> String {
    format!("{value:?}")
}

// rustfmt's defaults: generated code is laid out as rustfmt would, so a fresh project is clean.
const ATTR_FN_LIKE_WIDTH: usize = 70;
pub(super) const MAX_WIDTH: usize = 100;
pub(super) const FN_CALL_WIDTH: usize = 60;

fn attribute(name: &str, arguments: &[String], indent: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let inline = arguments.join(", ");
    if inline.width() <= ATTR_FN_LIKE_WIDTH.saturating_sub(indent) {
        return format!("#[{name}({inline})]");
    }
    let pad = " ".repeat(indent);
    let lines = arguments
        .iter()
        .map(|argument| format!("{pad}    {argument}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("#[{name}(\n{lines}\n{pad})]")
}

pub(super) fn dispatch_attribute(spec: &ProjectSpec) -> String {
    let mut arguments = vec!["pure".to_string(), "default".to_string()];
    // The derive registers the kebab-case name; only an underscore spelling needs `name`.
    if spec.command_name.contains('_') {
        arguments.push(format!("name = {}", quote(&spec.command_name)));
    }
    if has_chain_inputs(spec) {
        arguments.push(format!(
            "inputs = crate::handlers::{}_inputs",
            spec.command_name.replace('-', "_")
        ));
    }
    attribute("dispatch", &arguments, 4)
}

pub(super) fn cli_command_attribute(spec: &ProjectSpec) -> String {
    let name = quote(&spec.executable_name);
    let about = quote(&spec.command_description);
    let arguments = [format!("name = {name}"), format!("about = {about}")];
    attribute("command", &arguments, 0)
}

pub(super) fn toml_basic_string_content(path: &Path) -> String {
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

pub(super) fn rust_array(items: &[String], indent: usize, max_inline_len: usize) -> String {
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
