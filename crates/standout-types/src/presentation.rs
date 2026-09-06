use std::{fmt, sync::Arc};

use minijinja::value::{Object, ObjectRepr, Value};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormattedText {
    nodes: Vec<PresentationNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationNode {
    Text(String),
    Styled {
        style: PresentationStyle,
        children: Vec<PresentationNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationStyle {
    Semantic(String),
    Sgr(SgrStyle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SgrColor {
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SgrStyle {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
    pub foreground: Option<SgrColor>,
    pub background: Option<SgrColor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStyleName(String);

impl fmt::Display for InvalidStyleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid style name: {:?}", self.0)
    }
}

impl std::error::Error for InvalidStyleName {}

impl FormattedText {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            nodes: vec![PresentationNode::Text(text.into())],
        }
    }

    pub fn styled(self, name: impl Into<String>) -> Result<Self, InvalidStyleName> {
        let name = name.into();
        let mut bytes = name.bytes();
        if !bytes
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c == b'_')
            || !bytes.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
        {
            return Err(InvalidStyleName(name));
        }
        Ok(Self {
            nodes: vec![PresentationNode::Styled {
                style: PresentationStyle::Semantic(name),
                children: self.nodes,
            }],
        })
    }

    pub fn append(mut self, other: impl Into<Self>) -> Self {
        self.nodes.extend(other.into().nodes);
        self
    }

    pub fn nodes(&self) -> &[PresentationNode] {
        &self.nodes
    }

    pub fn plain_text(&self) -> String {
        self.to_string()
    }

    pub fn from_value(value: &Value) -> Option<&Self> {
        value.downcast_object_ref()
    }

    pub fn from_ansi_sgr(input: &str) -> Self {
        let mut result = Self::default();
        let mut style = SgrStyle::default();
        for token in SgrParser::default().parse(input) {
            match token {
                SgrToken::Style(next) => style = next,
                SgrToken::Text(text) | SgrToken::Control(text) => {
                    result.push_span(text.to_owned(), style);
                }
            }
        }
        result
    }

    fn push_span(&mut self, text: String, style: SgrStyle) {
        if text.is_empty() {
            return;
        }
        let node = PresentationNode::Text(text);
        self.nodes.push(if style == SgrStyle::default() {
            node
        } else {
            PresentationNode::Styled {
                style: PresentationStyle::Sgr(style),
                children: vec![node],
            }
        });
    }
}

impl From<FormattedText> for Value {
    fn from(value: FormattedText) -> Self {
        Value::from_object(value)
    }
}

impl FormattedText {
    pub(crate) fn from_nodes(nodes: Vec<PresentationNode>) -> Result<Self, InvalidStyleName> {
        fn validate(nodes: &[PresentationNode]) -> Result<(), InvalidStyleName> {
            for node in nodes {
                if let PresentationNode::Styled { style, children } = node {
                    if let PresentationStyle::Semantic(name) = style {
                        FormattedText::default().styled(name)?;
                    }
                    validate(children)?;
                }
            }
            Ok(())
        }
        validate(&nodes)?;
        Ok(Self { nodes })
    }
}

impl From<String> for FormattedText {
    fn from(value: String) -> Self {
        Self::text(value)
    }
}

impl From<&str> for FormattedText {
    fn from(value: &str) -> Self {
        Self::text(value)
    }
}

impl fmt::Display for FormattedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut stack = vec![self.nodes.iter()];
        while let Some(nodes) = stack.last_mut() {
            match nodes.next() {
                Some(PresentationNode::Text(text)) => f.write_str(text)?,
                Some(PresentationNode::Styled { children, .. }) => stack.push(children.iter()),
                None => {
                    stack.pop();
                }
            }
        }
        Ok(())
    }
}

impl Serialize for FormattedText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if crate::render_data::serializer::is_presentation_serializer::<S>() {
            serializer.serialize_newtype_struct(
                crate::render_data::serializer::FORMATTED_TEXT,
                &self.nodes,
            )
        } else {
            serializer.collect_str(self)
        }
    }
}

impl Object for FormattedText {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn render(self: &Arc<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_ref(), f)
    }

    fn is_true(self: &Arc<Self>) -> bool {
        !self.plain_text().is_empty()
    }

    fn enumerator_len(self: &Arc<Self>) -> Option<usize> {
        Some(self.plain_text().chars().count())
    }
}

#[derive(Debug, Default)]
pub struct SgrParser {
    style: SgrStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SgrToken<'a> {
    Text(&'a str),
    Control(&'a str),
    Style(SgrStyle),
}

impl SgrParser {
    pub fn parse<'a>(&mut self, input: &'a str) -> Vec<SgrToken<'a>> {
        let mut result = Vec::new();
        let mut offset = 0;
        let mut plain_start = 0;
        while offset < input.len() {
            let remaining = &input[offset..];
            let character = remaining.chars().next().unwrap();
            let (prefix, string_control, osc) = match character {
                '\x1b' => match remaining.as_bytes().get(1) {
                    Some(b'[') => (2, false, false),
                    Some(b']') => (2, true, true),
                    Some(b'P' | b'X' | b'^' | b'_') => (2, true, false),
                    _ => (0, false, false),
                },
                '\u{9b}' => (character.len_utf8(), false, false),
                '\u{9d}' => (character.len_utf8(), true, true),
                '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => (character.len_utf8(), true, false),
                _ => {
                    offset += character.len_utf8();
                    continue;
                }
            };
            if plain_start < offset {
                result.push(SgrToken::Text(&input[plain_start..offset]));
            }
            let (length, next_style) = if prefix == 0 {
                (character.len_utf8(), None)
            } else if string_control {
                (string_control_length(remaining, prefix, osc), None)
            } else if let Some((end, final_character)) = remaining[prefix..]
                .char_indices()
                .find(|(_, c)| ('\u{40}'..='\u{7e}').contains(c))
            {
                let next = (final_character == 'm')
                    .then(|| parse_sgr(&remaining[prefix..prefix + end], self.style))
                    .flatten();
                (prefix + end + 1, next)
            } else {
                (remaining.len(), None)
            };
            if let Some(style) = next_style {
                self.style = style;
                result.push(SgrToken::Style(style));
            } else {
                result.push(SgrToken::Control(&remaining[..length]));
            }
            offset += length;
            plain_start = offset;
        }
        if plain_start < input.len() {
            result.push(SgrToken::Text(&input[plain_start..]));
        }
        result
    }
}

fn string_control_length(input: &str, prefix: usize, osc: bool) -> usize {
    for (offset, character) in input[prefix..].char_indices() {
        let offset = prefix + offset;
        if character == '\u{9c}' || (osc && character == '\x07') {
            return offset + character.len_utf8();
        }
        if character == '\x1b' && input.as_bytes().get(offset + 1) == Some(&b'\\') {
            return offset + 2;
        }
    }
    input.len()
}

fn parse_sgr(parameters: &str, mut style: SgrStyle) -> Option<SgrStyle> {
    if parameters.len() > 128 {
        return None;
    }
    let mut values = Vec::new();
    for part in parameters.split(';') {
        if values.len() == 32 || !part.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        values.push(if part.is_empty() {
            0
        } else {
            part.parse().ok()?
        });
    }
    let mut parameters = values.into_iter();
    while let Some(parameter) = parameters.next() {
        match parameter {
            0 => style = SgrStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            5 => style.blink = true,
            7 => style.reverse = true,
            8 => style.hidden = true,
            9 => style.strikethrough = true,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            25 => style.blink = false,
            27 => style.reverse = false,
            28 => style.hidden = false,
            29 => style.strikethrough = false,
            30..=37 => style.foreground = Some(SgrColor::Indexed((parameter - 30) as u8)),
            40..=47 => style.background = Some(SgrColor::Indexed((parameter - 40) as u8)),
            90..=97 => style.foreground = Some(SgrColor::Indexed((parameter - 90 + 8) as u8)),
            100..=107 => style.background = Some(SgrColor::Indexed((parameter - 100 + 8) as u8)),
            39 => style.foreground = None,
            49 => style.background = None,
            38 | 48 => {
                let color = parse_color(&mut parameters)?;
                if parameter == 38 {
                    style.foreground = Some(color);
                } else {
                    style.background = Some(color);
                }
            }
            _ => return None,
        }
    }
    Some(style)
}

fn parse_color(parameters: &mut impl Iterator<Item = u16>) -> Option<SgrColor> {
    match parameters.next()? {
        5 => Some(SgrColor::Indexed(parameters.next()?.try_into().ok()?)),
        2 => Some(SgrColor::Rgb(
            parameters.next()?.try_into().ok()?,
            parameters.next()?.try_into().ok()?,
            parameters.next()?.try_into().ok()?,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
