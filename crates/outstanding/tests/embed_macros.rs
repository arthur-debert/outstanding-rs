//! Integration tests for the embed macros.
//!
//! These tests verify that the `embed_templates!` and `embed_styles!` macros
//! correctly walk directories at compile time and embed resources.

#![cfg(feature = "macros")]

use outstanding::{embed_styles, embed_templates, ResolvedTemplate};

#[test]
fn test_embed_templates_simple() {
    let templates = embed_templates!("tests/fixtures/templates");

    // Should be able to get the simple template
    let resolved = templates
        .get("simple")
        .expect("simple template should exist");

    // Embedded templates should be Inline variant
    match resolved {
        ResolvedTemplate::Inline(content) => {
            assert!(content.contains("Hello"));
            assert!(content.contains("{{ name }}"));
        }
        _ => panic!("Expected Inline template"),
    }
}

#[test]
fn test_embed_templates_nested() {
    let templates = embed_templates!("tests/fixtures/templates");

    // Should be able to get nested templates
    let resolved = templates
        .get("nested/report")
        .expect("nested/report template should exist");

    match resolved {
        ResolvedTemplate::Inline(content) => {
            assert!(content.contains("Report:"));
            assert!(content.contains("{{ title }}"));
        }
        _ => panic!("Expected Inline template"),
    }
}

#[test]
fn test_embed_styles_simple() {
    let mut styles = embed_styles!("tests/fixtures/styles");

    // Should be able to get the default stylesheet
    let theme = styles.get("default").expect("default style should exist");
    let resolved = theme.resolve_styles(None);
    assert!(resolved.has("header"));
    assert!(resolved.has("muted"));
}

#[test]
fn test_embed_styles_nested() {
    let mut styles = embed_styles!("tests/fixtures/styles");

    // Should be able to get nested stylesheets
    let theme = styles
        .get("themes/dark")
        .expect("themes/dark style should exist");
    let resolved = theme.resolve_styles(None);
    assert!(resolved.has("header"));
    assert!(resolved.has("panel"));
}

#[test]
fn test_embed_templates_names() {
    let templates = embed_templates!("tests/fixtures/templates");

    let names: Vec<&str> = templates.names().collect();
    assert!(names.contains(&"simple"));
    assert!(names.contains(&"nested/report"));
}

#[test]
fn test_embed_styles_names() {
    let styles = embed_styles!("tests/fixtures/styles");

    let names: Vec<&str> = styles.names().collect();
    assert!(names.contains(&"default"));
    assert!(names.contains(&"themes/dark"));
}
