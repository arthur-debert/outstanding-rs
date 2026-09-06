use std::{cmp::Ordering, sync::Arc};

use minijinja::{
    value::{DynObject, Enumerator, Object, ObjectRepr, ValueKind},
    Value,
};
use standout_bbparser::{StyledText, StyledTextEvent};
use standout_types::{
    FormattedText, PresentationNode, PresentationStyle, SgrColor, SgrParser, SgrStyle, SgrToken,
};

pub(crate) fn fragment(markup: String) -> Value {
    Value::from(parse_markup(&markup))
}

pub(crate) fn escape_text(text: &str) -> String {
    let text = crate::escape_control_characters(text.to_owned());
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '[' | ']') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

pub(crate) fn capture(value: Value) -> Value {
    if value.is_safe() {
        fragment(value.to_string())
    } else {
        value
    }
}

pub(crate) fn plain_if_formatted(value: Value) -> Value {
    if FormattedText::from_value(&value).is_some() {
        Value::from(value.to_string())
    } else {
        value
    }
}

#[derive(Debug)]
struct ComparisonValue(Value);

impl Object for ComparisonValue {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        match self.0.kind() {
            ValueKind::Map => ObjectRepr::Map,
            ValueKind::Seq => ObjectRepr::Seq,
            _ => ObjectRepr::Iterable,
        }
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        self.0.as_object()?.get_value(key).map(plain_for_comparison)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        let Ok(values) = self.0.try_iter() else {
            return Enumerator::NonEnumerable;
        };
        if self.0.kind() == ValueKind::Map {
            Enumerator::Iter(Box::new(values))
        } else {
            Enumerator::Iter(Box::new(values.map(plain_for_comparison)))
        }
    }

    fn enumerator_len(self: &Arc<Self>) -> Option<usize> {
        self.0.len()
    }

    fn custom_cmp(self: &Arc<Self>, other: &DynObject) -> Option<Ordering> {
        let other = other.downcast_ref::<Self>()?;
        minijinja::tests::is_sameas(&self.0, &other.0).then_some(Ordering::Equal)
    }
}

pub(crate) fn plain_for_comparison(value: Value) -> Value {
    if value.downcast_object_ref::<ComparisonValue>().is_some() {
        return value;
    }
    match value.kind() {
        ValueKind::Seq | ValueKind::Iterable | ValueKind::Map => {
            Value::from_object(ComparisonValue(value))
        }
        _ => plain_if_formatted(value),
    }
}

pub(crate) fn markup(value: &Value) -> String {
    if let Some(text) = FormattedText::from_value(value) {
        let mut output = String::new();
        render_nodes(text.nodes(), SgrStyle::default(), &mut output);
        output
    } else {
        escape_text(&super::spelling::stringify(value))
    }
}

fn render_nodes(nodes: &[PresentationNode], sgr: SgrStyle, output: &mut String) {
    for node in nodes {
        match node {
            PresentationNode::Text(text) => output.push_str(&escape_text(text)),
            PresentationNode::Styled {
                style: PresentationStyle::Semantic(name),
                children,
            } => {
                output.push_str(&format!("[{name}]"));
                render_nodes(children, sgr, output);
                output.push_str(&format!("[/{name}]"));
            }
            PresentationNode::Styled {
                style: PresentationStyle::Sgr(style),
                children,
            } => {
                output.push_str(&sgr_sequence(*style));
                render_nodes(children, *style, output);
                output.push_str(&sgr_sequence(sgr));
            }
        }
    }
}

fn sgr_sequence(style: SgrStyle) -> String {
    let mut codes = vec!["0".to_owned()];
    for (enabled, code) in [
        (style.bold, 1),
        (style.dim, 2),
        (style.italic, 3),
        (style.underline, 4),
        (style.blink, 5),
        (style.reverse, 7),
        (style.hidden, 8),
        (style.strikethrough, 9),
    ] {
        if enabled {
            codes.push(code.to_string());
        }
    }
    for (color, code) in [(style.foreground, 38), (style.background, 48)] {
        match color {
            Some(SgrColor::Indexed(index)) => codes.push(format!("{code};5;{index}")),
            Some(SgrColor::Rgb(r, g, b)) => codes.push(format!("{code};2;{r};{g};{b}")),
            None => {}
        }
    }
    format!("\x1b[{}m", codes.join(";"))
}

pub(crate) fn parse_markup(source: &str) -> FormattedText {
    let mut normalized = String::new();
    for token in SgrParser::default().parse(source) {
        match token {
            SgrToken::Text(text) => {
                normalized.push_str(&crate::escape_control_characters(text.to_owned()))
            }
            SgrToken::Control(text) => normalized.push_str(&escape_text(text)),
            SgrToken::Style(style) => normalized.push_str(&sgr_sequence(style)),
        }
    }
    let mut text = FormattedText::default();
    let mut stack: Vec<(String, FormattedText)> = Vec::new();
    let mut parser = SgrParser::default();
    let mut sgr = SgrStyle::default();
    StyledText::parse(&normalized).visit(|event| match event {
        StyledTextEvent::OpenTag(name) => stack.push((name.to_owned(), std::mem::take(&mut text))),
        StyledTextEvent::CloseTag(_) => {
            let (name, parent) = stack.pop().expect("styled text balances semantic tags");
            text = parent.append(
                std::mem::take(&mut text)
                    .styled(name)
                    .expect("styled text uses validated semantic names"),
            );
        }
        StyledTextEvent::Text(source) => {
            for token in parser.parse(&source) {
                match token {
                    SgrToken::Style(style) => sgr = style,
                    SgrToken::Text(source) | SgrToken::Control(source) => {
                        let span = if sgr == SgrStyle::default() {
                            FormattedText::text(source)
                        } else {
                            FormattedText::from_ansi_sgr(&format!("{}{source}", sgr_sequence(sgr)))
                        };
                        text = std::mem::take(&mut text).append(span);
                    }
                }
            }
        }
    });
    text
}

pub(crate) fn render_final(
    text: &FormattedText,
    styles: &std::collections::HashMap<String, console::Style>,
    mode: crate::output::StyleMode,
) -> String {
    fn walk(
        nodes: &[PresentationNode],
        styles: &std::collections::HashMap<String, console::Style>,
        mode: crate::output::StyleMode,
        stack: &mut Vec<console::Style>,
        output: &mut String,
    ) {
        for node in nodes {
            match node {
                PresentationNode::Text(text) => {
                    let mut text = crate::escape_control_characters(text.to_owned());
                    if mode.should_use_color() {
                        for style in stack.iter().rev() {
                            text = style.clone().force_styling(true).apply_to(text).to_string();
                        }
                    }
                    output.push_str(&text);
                }
                PresentationNode::Styled { style, children } => {
                    let resolved = match style {
                        PresentationStyle::Semantic(name) => {
                            if mode.is_debug() {
                                output.push_str(&format!("[{name}]"));
                            }
                            styles.get(name).cloned().unwrap_or_default()
                        }
                        PresentationStyle::Sgr(style) => console_style(style),
                    };
                    stack.push(resolved);
                    walk(children, styles, mode, stack, output);
                    stack.pop();
                    if mode.is_debug() {
                        if let PresentationStyle::Semantic(name) = style {
                            output.push_str(&format!("[/{name}]"));
                        }
                    }
                }
            }
        }
    }
    let mut output = String::new();
    walk(text.nodes(), styles, mode, &mut Vec::new(), &mut output);
    output
}

fn console_style(sgr: &SgrStyle) -> console::Style {
    fn color(color: SgrColor) -> console::Color {
        match color {
            SgrColor::Indexed(index) => console::Color::Color256(index),
            SgrColor::Rgb(r, g, b) => console::Color::TrueColor(r, g, b),
        }
    }
    let mut style = console::Style::new();
    if let Some(fg) = sgr.foreground {
        style = style.fg(color(fg));
    }
    if let Some(bg) = sgr.background {
        style = style.bg(color(bg));
    }
    for (enabled, attribute) in [
        (sgr.bold, console::Attribute::Bold),
        (sgr.dim, console::Attribute::Dim),
        (sgr.italic, console::Attribute::Italic),
        (sgr.underline, console::Attribute::Underlined),
        (sgr.blink, console::Attribute::Blink),
        (sgr.reverse, console::Attribute::Reverse),
        (sgr.hidden, console::Attribute::Hidden),
        (sgr.strikethrough, console::Attribute::StrikeThrough),
    ] {
        if enabled {
            style = style.attr(attribute);
        }
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StyleMode;
    use std::collections::HashMap;

    #[test]
    fn sgr_state_survives_semantic_boundaries() {
        let text = parse_markup("\x1b[31m[heading]X[/heading]Y\x1b[0mZ");
        let styles = HashMap::from([("heading".to_owned(), console::Style::new().bold())]);
        assert_eq!(render_final(&text, &styles, StyleMode::Plain), "XYZ");
        let rendered = render_final(&text, &styles, StyleMode::Ansi);
        let projected = FormattedText::from_ansi_sgr(&rendered);
        assert_eq!(projected.plain_text(), "XYZ");
        assert!(
            matches!(&projected.nodes()[0], PresentationNode::Styled {style: PresentationStyle::Sgr(style), ..} if style.bold && style.foreground == Some(SgrColor::Indexed(1)))
        );
        assert!(
            matches!(&projected.nodes()[1], PresentationNode::Styled {style: PresentationStyle::Sgr(style), ..} if !style.bold && style.foreground == Some(SgrColor::Indexed(1)))
        );
        assert!(matches!(&projected.nodes()[2], PresentationNode::Text(text) if text == "Z"));
        let debug = render_final(&text, &styles, StyleMode::Debug);
        assert_eq!(debug, "[heading]X[/heading]YZ");
    }

    #[test]
    fn unsupported_control_payloads_never_become_semantic_markup() {
        for source in [
            "\x1b]0;[heading]X[/heading]\x07",
            "\x1bP[heading]X[/heading]\x1b\\",
            "\u{9d}0;[heading]X[/heading]\u{9c}",
        ] {
            let text = parse_markup(source);
            let styles = HashMap::from([("heading".to_owned(), console::Style::new().bold())]);
            for mode in [StyleMode::Plain, StyleMode::Ansi, StyleMode::Debug] {
                let rendered = render_final(&text, &styles, mode);
                assert_eq!(
                    rendered,
                    crate::escape_control_characters(source.to_owned())
                );
                assert!(!rendered.contains('\x1b'));
            }
        }
    }

    #[test]
    fn empty_semantic_styles_do_not_remove_sgr_resets() {
        for styles in [
            HashMap::new(),
            HashMap::from([("empty".to_owned(), console::Style::new())]),
        ] {
            let text = FormattedText::from_ansi_sgr("\x1b[31mX\x1b[0m")
                .styled("empty")
                .unwrap()
                .append("Y");
            let rendered = render_final(
                &parse_markup(&markup(&Value::from(text))),
                &styles,
                StyleMode::Ansi,
            );
            let parsed = FormattedText::from_ansi_sgr(&rendered);
            assert!(
                matches!(&parsed.nodes()[1], PresentationNode::Text(text) if text == "Y"),
                "{rendered:?}"
            );
        }
    }
}
