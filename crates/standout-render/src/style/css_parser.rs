//! Parses a subset of CSS level 3 (via `cssparser`, the tokenizer used by
//! Firefox) into the same [`ThemeVariants`] that the YAML parser produces, so
//! themes can be authored in whichever syntax is more convenient.
//!
//! Class selectors (`.error`) map to style names. `@media
//! (prefers-color-scheme: dark|light) { ... }` blocks define light/dark
//! overrides the same way YAML's `light:`/`dark:` sections do. Supported
//! properties: `color`/`fg`, `background-color`/`bg`, `font-weight: bold`,
//! `font-style: italic`, `text-decoration: underline|line-through`,
//! `visibility: hidden`, and direct `bold`/`italic`/`dim`/`blink`/`reverse`/
//! `hidden`/`strikethrough` flags.

use std::collections::HashMap;

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, Token,
};

use super::attributes::StyleAttributes;
use super::color::ColorDef;
use super::definition::StyleDefinition;
use super::error::StylesheetError;
use super::parser::{build_variants, ThemeVariants};

pub fn parse_css(
    css: &str,
    palette: Option<&crate::colorspace::ThemePalette>,
) -> Result<ThemeVariants, StylesheetError> {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);

    let mut css_parser = StyleSheetParser {
        definitions: HashMap::new(),
        current_mode: None,
    };

    let rule_list_parser = cssparser::StyleSheetParser::new(&mut parser, &mut css_parser);

    for result in rule_list_parser {
        if let Err(e) = result {
            return Err(StylesheetError::Parse {
                path: None,
                message: format!("CSS Parse Error: {:?}", e),
            });
        }
    }

    build_variants(&css_parser.definitions, palette)
}

struct StyleSheetParser {
    definitions: HashMap<String, StyleDefinition>,
    current_mode: Option<Mode>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Light,
    Dark,
}

impl<'i> QualifiedRuleParser<'i> for StyleSheetParser {
    type Prelude = Vec<String>;
    type QualifiedRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let mut names = Vec::new();

        while let Ok(token) = input.next() {
            match token {
                Token::Delim('.') => {
                    let name = input.expect_ident()?;
                    names.push(name.as_ref().to_string());
                }
                Token::Comma | Token::WhiteSpace(_) => continue,
                _ => {}
            }
        }

        if names.is_empty() {
            return Err(input.new_custom_error::<(), ()>(()));
        }
        Ok(names)
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        let mut decl_parser = StyleDeclarationParser;
        let rule_parser = RuleBodyParser::new(input, &mut decl_parser);

        let mut attributes = StyleAttributes::new();

        for (_prop, val) in rule_parser.flatten() {
            if let Some(c) = val.fg {
                attributes.fg = Some(c);
            }
            if let Some(c) = val.bg {
                attributes.bg = Some(c);
            }
            if let Some(b) = val.bold {
                attributes.bold = Some(b);
            }
            if let Some(v) = val.dim {
                attributes.dim = Some(v);
            }
            if let Some(v) = val.italic {
                attributes.italic = Some(v);
            }
            if let Some(v) = val.underline {
                attributes.underline = Some(v);
            }
            if let Some(v) = val.blink {
                attributes.blink = Some(v);
            }
            if let Some(v) = val.reverse {
                attributes.reverse = Some(v);
            }
            if let Some(v) = val.hidden {
                attributes.hidden = Some(v);
            }
            if let Some(v) = val.strikethrough {
                attributes.strikethrough = Some(v);
            }
        }

        for name in prelude {
            let def = self
                .definitions
                .entry(name)
                .or_insert(StyleDefinition::Attributes {
                    base: StyleAttributes::new(),
                    light: None,
                    dark: None,
                });

            if let StyleDefinition::Attributes {
                ref mut base,
                ref mut light,
                ref mut dark,
            } = def
            {
                match self.current_mode {
                    None => *base = base.merge(&attributes),
                    Some(Mode::Light) => {
                        let l = light.get_or_insert(StyleAttributes::new());
                        *l = l.merge(&attributes);
                    }
                    Some(Mode::Dark) => {
                        let d = dark.get_or_insert(StyleAttributes::new());
                        *d = d.merge(&attributes);
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for StyleSheetParser {
    type Prelude = Mode;
    type AtRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        if name.as_ref() == "media" {
            let mut found_mode: Option<Mode> = None;

            loop {
                match input.next() {
                    Ok(Token::ParenthesisBlock) => {
                        let nested_res = input.parse_nested_block(|input| {
                            input.expect_ident_matching("prefers-color-scheme")?;
                            input.expect_colon()?;
                            let val = input.expect_ident()?;
                            match val.as_ref() {
                                "dark" => Ok(Mode::Dark),
                                "light" => Ok(Mode::Light),
                                _ => Err(input.new_custom_error::<(), ()>(())),
                            }
                        });
                        if let Ok(m) = nested_res {
                            found_mode = Some(m);
                        }
                    }
                    Ok(Token::WhiteSpace(_)) | Ok(Token::Comment(_)) => continue,
                    Err(_) => break,
                    Ok(_) => {}
                }
            }

            if let Some(m) = found_mode {
                return Ok(m);
            }

            Err(input.new_custom_error::<(), ()>(()))
        } else {
            Err(input.new_custom_error::<(), ()>(()))
        }
    }

    fn parse_block<'t>(
        &mut self,
        mode: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::AtRule, ParseError<'i, Self::Error>> {
        let old_mode = self.current_mode;
        self.current_mode = Some(mode);

        let list_parser = cssparser::StyleSheetParser::new(input, self);
        for _ in list_parser {}

        self.current_mode = old_mode;
        Ok(())
    }
}

struct StyleDeclarationParser;

impl<'i> DeclarationParser<'i> for StyleDeclarationParser {
    type Declaration = (String, StyleAttributes);
    type Error = ();

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Declaration, ParseError<'i, Self::Error>> {
        let mut attrs = StyleAttributes::new();
        match name.as_ref() {
            "fg" | "color" => {
                attrs.fg = Some(parse_color(input)?);
            }
            "bg" | "background" | "background-color" => {
                attrs.bg = Some(parse_color(input)?);
            }
            "bold" => {
                if parse_bool_or_flag(input)? {
                    attrs.bold = Some(true);
                }
            }
            "dim" => {
                if parse_bool_or_flag(input)? {
                    attrs.dim = Some(true);
                }
            }
            "italic" => {
                if parse_bool_or_flag(input)? {
                    attrs.italic = Some(true);
                }
            }
            "underline" => {
                if parse_bool_or_flag(input)? {
                    attrs.underline = Some(true);
                }
            }
            "blink" => {
                if parse_bool_or_flag(input)? {
                    attrs.blink = Some(true);
                }
            }
            "reverse" => {
                if parse_bool_or_flag(input)? {
                    attrs.reverse = Some(true);
                }
            }
            "hidden" => {
                if parse_bool_or_flag(input)? {
                    attrs.hidden = Some(true);
                }
            }
            "strikethrough" => {
                if parse_bool_or_flag(input)? {
                    attrs.strikethrough = Some(true);
                }
            }

            "font-weight" => {
                let val = input.expect_ident()?;
                if val.as_ref() == "bold" {
                    attrs.bold = Some(true);
                }
            }
            "font-style" => {
                let val = input.expect_ident()?;
                if val.as_ref() == "italic" {
                    attrs.italic = Some(true);
                }
            }
            "text-decoration" => {
                let val = input.expect_ident()?;
                match val.as_ref() {
                    "underline" => attrs.underline = Some(true),
                    "line-through" => attrs.strikethrough = Some(true),
                    _ => {}
                }
            }
            "visibility" => {
                let val = input.expect_ident()?;
                if val.as_ref() == "hidden" {
                    attrs.hidden = Some(true);
                }
            }

            _ => return Err(input.new_custom_error::<(), ()>(())),
        }
        Ok((name.as_ref().to_string(), attrs))
    }
}

impl<'i> AtRuleParser<'i> for StyleDeclarationParser {
    type Prelude = ();
    type AtRule = (String, StyleAttributes);
    type Error = ();
}

impl<'i> QualifiedRuleParser<'i> for StyleDeclarationParser {
    type Prelude = ();
    type QualifiedRule = (String, StyleAttributes);
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, (String, StyleAttributes), ()> for StyleDeclarationParser {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

fn parse_color<'i, 't>(input: &mut Parser<'i, 't>) -> Result<ColorDef, ParseError<'i, ()>> {
    let token = match input.next() {
        Ok(t) => t,
        Err(_) => return Err(input.new_custom_error::<(), ()>(())),
    };

    match token {
        Token::Function(ref name) if name.as_ref() == "cube" => input
            .parse_nested_block(|input| {
                let r = input.expect_percentage()?;
                input.expect_comma()?;
                let g = input.expect_percentage()?;
                input.expect_comma()?;
                let b = input.expect_percentage()?;
                crate::colorspace::CubeCoord::from_percentages(
                    r as f64 * 100.0,
                    g as f64 * 100.0,
                    b as f64 * 100.0,
                )
                .map(ColorDef::Cube)
                .map_err(|_| input.new_custom_error::<(), ()>(()))
            })
            .map_err(|_: ParseError<'i, ()>| input.new_custom_error::<(), ()>(())),
        Token::Ident(name) => {
            ColorDef::parse_string(name.as_ref()).map_err(|_| input.new_custom_error::<(), ()>(()))
        }
        Token::Hash(val) | Token::IDHash(val) => ColorDef::parse_string(&format!("#{}", val))
            .map_err(|_| input.new_custom_error::<(), ()>(())),
        _ => Err(input.new_custom_error::<(), ()>(())),
    }
}

fn parse_bool_or_flag<'i, 't>(input: &mut Parser<'i, 't>) -> Result<bool, ParseError<'i, ()>> {
    match input.expect_ident() {
        Ok(val) => Ok(val.as_ref() == "true"),
        Err(_) => Err(input.new_custom_error::<(), ()>(())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColorMode, StyleValue};

    #[test]
    fn test_parse_simple() {
        let css = ".error { color: red; font-weight: bold; }";
        let variants = parse_css(css, None).unwrap();
        let base = variants.base();

        assert!(base.contains_key("error"));

        let style = base.get("error").unwrap().clone().force_styling(true);
        let styled = style.apply_to("text").to_string();
        assert!(styled.contains("\x1b[31m"));
        assert!(styled.contains("\x1b[1m"));
    }

    #[test]
    fn test_parse_adaptive() {
        let css =
            ".text { color: red; } @media (prefers-color-scheme: dark) { .text { color: white; } }";
        let variants = parse_css(css, None).unwrap();

        let light = variants.resolve(Some(ColorMode::Light));
        let dark = variants.resolve(Some(ColorMode::Dark));

        if let StyleValue::Concrete(s) = light.get("text").unwrap() {
            let out = s.clone().force_styling(true).apply_to("x").to_string();
            assert!(out.contains("\x1b[31m"));
        } else {
            panic!("Expected Concrete style for light mode");
        }

        if let StyleValue::Concrete(s) = dark.get("text").unwrap() {
            let out = s.clone().force_styling(true).apply_to("x").to_string();
            assert!(out.contains("\x1b[37m"));
        } else {
            panic!("Expected Concrete style for dark mode");
        }
    }

    #[test]
    fn test_multiple_selectors() {
        let css = ".a, .b { color: blue; }";
        let variants = parse_css(css, None).unwrap();
        let base = variants.base();
        assert!(base.contains_key("a"));
        assert!(base.contains_key("b"));
    }

    #[test]
    fn test_all_properties() {
        let css = r#"
        .all-props {
            fg: red;
            bg: blue;
            bold: true;
            dim: true;
            italic: true;
            underline: true;
            blink: true;
            reverse: true;
            hidden: true;
            strikethrough: true;
        }
        "#;
        let variants = parse_css(css, None).unwrap();
        let base = variants.base();
        assert!(base.contains_key("all-props"));

        let style = base.get("all-props").unwrap().clone().force_styling(true);
        let out = style.apply_to("text").to_string();

        assert!(out.contains("\x1b[31m"));
        assert!(out.contains("\x1b[44m"));
        assert!(out.contains("\x1b[1m"));
        assert!(out.contains("\x1b[2m"));
        assert!(out.contains("\x1b[3m"));
        assert!(out.contains("\x1b[4m"));
        assert!(out.contains("\x1b[5m"));
        assert!(out.contains("\x1b[7m"));
        assert!(out.contains("\x1b[8m"));
        assert!(out.contains("\x1b[9m"));
    }

    #[test]
    fn test_css_aliases() {
        let css = r#"
        .aliases {
            background-color: green;
            font-weight: bold;
            font-style: italic;
            text-decoration: underline;
            visibility: hidden;
        }
        "#;
        let variants = parse_css(css, None).unwrap();
        let base = variants.base();
        let style = base.get("aliases").unwrap().clone().force_styling(true);
        let out = style.apply_to("text").to_string();

        assert!(out.contains("\x1b[42m"));
        assert!(out.contains("\x1b[1m"));
        assert!(out.contains("\x1b[3m"));
        assert!(out.contains("\x1b[4m"));
        assert!(out.contains("\x1b[8m"));
    }

    #[test]
    fn test_text_decoration_line_through() {
        let css = ".strike { text-decoration: line-through; }";
        let variants = parse_css(css, None).unwrap();
        let style = variants
            .base()
            .get("strike")
            .unwrap()
            .clone()
            .force_styling(true);
        let out = style.apply_to("text").to_string();
        assert!(out.contains("\x1b[9m"));
    }

    #[test]
    fn test_invalid_syntax_recovery() {
        let css = r#"
        .broken {
            color: ;
            unknown: prop;
            bold: not-a-bool;
        }
        .valid { color: cyan; }
        "#;

        let variants = parse_css(css, None).unwrap();
        assert!(variants.base().contains_key("valid"));
    }

    #[test]
    fn test_empty_selector_error() {
        let css = ". { color: red; }";
        let res = parse_css(css, None);
        assert!(res.is_err());
    }

    #[test]
    fn test_no_dot_selector() {
        let css = "body { color: red; }";
        let res = parse_css(css, None);
        assert!(res.is_err());
    }

    #[test]
    fn test_invalid_color() {
        let css = ".bad-color { color: not-a-color; }";
        let variants = parse_css(css, None).unwrap();
        assert!(variants.base().contains_key("bad-color"));
    }

    #[test]
    fn test_hex_colors() {
        let css = ".hex { color: #ff0000; bg: #00ff00; }";
        let variants = parse_css(css, None).unwrap();
        let style = variants.base().get("hex").unwrap();
        let out = style.apply_to("x").to_string();
        assert!(!out.is_empty());
    }

    #[test]
    fn test_comments() {
        let css = r#"
        /* This is a comment */
        .commented {
            color: red; /* Inline comment */
        }
        "#;
        let variants = parse_css(css, None).unwrap();
        assert!(variants.base().contains_key("commented"));
    }

    #[test]
    fn test_css_cube_color() {
        let css = ".warm { color: cube(60%, 20%, 0%); }";
        let variants = parse_css(css, None).unwrap();
        assert!(variants.base().contains_key("warm"));
    }

    #[test]
    fn test_css_cube_color_bg() {
        let css = ".panel { background-color: cube(10%, 10%, 50%); }";
        let variants = parse_css(css, None).unwrap();
        assert!(variants.base().contains_key("panel"));
    }

    #[test]
    fn test_css_cube_with_other_props() {
        let css = ".styled { color: cube(80%, 30%, 0%); font-weight: bold; }";
        let variants = parse_css(css, None).unwrap();
        let style = variants
            .base()
            .get("styled")
            .unwrap()
            .clone()
            .force_styling(true);
        let out = style.apply_to("text").to_string();
        assert!(out.contains("\x1b[1m"));
    }

    #[test]
    fn test_css_cube_adaptive() {
        let css = r#"
        .text { color: cube(50%, 50%, 50%); }
        @media (prefers-color-scheme: dark) {
            .text { color: cube(80%, 80%, 80%); }
        }
        "#;
        let variants = parse_css(css, None).unwrap();
        assert!(variants.base().contains_key("text"));
        assert!(variants.dark().contains_key("text"));
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_random_css_input_no_panic(s in "\\PC*") {
            let _ = parse_css(&s, None);
        }

        #[test]
        fn test_valid_structure_random_values(
            color in "[a-zA-Z]+",
            bool_val in "true|false",
            prop_name in "[a-z-]+"
        ) {
            let css = format!(".prop {{ color: {}; bold: {}; {}: {}; }}", color, bool_val, prop_name, bool_val);
            let _ = parse_css(&css, None);
        }
    }
}
