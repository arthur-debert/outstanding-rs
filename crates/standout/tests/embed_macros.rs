#![cfg(feature = "macros")]

use standout::{embed_styles, embed_templates, StylesheetRegistry, TemplateRegistry};

#[test]
fn test_embed_templates_simple() {
    let source = embed_templates!("tests/fixtures/templates");

    let templates: TemplateRegistry = source.into();

    let content = templates
        .get_content("simple")
        .expect("simple template should exist");

    assert!(content.contains("Hello"));
    assert!(content.contains("{{ name }}"));
}

#[test]
fn test_embed_templates_with_extension() {
    let templates: TemplateRegistry = embed_templates!("tests/fixtures/templates").into();

    let content = templates
        .get_content("simple.jinja")
        .expect("simple.jinja should exist");

    assert!(content.contains("Hello"));
}

#[test]
fn test_embed_templates_nested() {
    let templates: TemplateRegistry = embed_templates!("tests/fixtures/templates").into();

    let content = templates
        .get_content("nested/report")
        .expect("nested/report template should exist");

    assert!(content.contains("Report:"));
    assert!(content.contains("{{ title }}"));
}

#[test]
fn test_embed_templates_names() {
    let templates: TemplateRegistry = embed_templates!("tests/fixtures/templates").into();

    let names: Vec<&str> = templates.names().collect();

    assert!(names.contains(&"simple"));
    assert!(names.contains(&"simple.jinja"));
    assert!(names.contains(&"nested/report"));
    assert!(names.contains(&"nested/report.jinja"));
}

#[test]
fn test_embed_styles_simple() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let theme = styles.get("default").expect("default style should exist");
    let resolved = theme.resolve_styles(None);
    assert!(resolved.has("header"));
    assert!(resolved.has("muted"));
}

#[test]
fn test_embed_styles_with_extension() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let theme = styles
        .get("default.yaml")
        .expect("default.yaml should exist");
    let resolved = theme.resolve_styles(None);
    assert!(resolved.has("header"));
}

#[test]
fn test_embed_styles_nested() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let theme = styles
        .get("themes/dark")
        .expect("themes/dark style should exist");
    let resolved = theme.resolve_styles(None);
    assert!(resolved.has("header"));
    assert!(resolved.has("panel"));
}

#[test]
fn test_embed_styles_names() {
    let styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let names: Vec<&str> = styles.names().collect();

    assert!(names.contains(&"default"));
    assert!(names.contains(&"default.yaml"));
    assert!(names.contains(&"themes/dark"));
    assert!(names.contains(&"themes/dark.yaml"));
}

#[test]
fn test_embed_templates_extension_priority() {
    let templates: TemplateRegistry = embed_templates!("tests/fixtures/templates").into();

    assert!(templates.get("simple").is_ok());
}

#[test]
fn test_embed_styles_extension_priority() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();
    assert!(styles.get("default").is_ok());
}

#[test]
fn test_embed_styles_css_file() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let theme = styles.get("screen").expect("screen.css should load");
    let resolved = theme.resolve_styles(None);
    assert!(
        resolved.has("header"),
        "CSS .header class should be registered"
    );
    assert!(
        resolved.has("muted"),
        "CSS .muted class should be registered"
    );
}

#[test]
fn test_embed_styles_css_accessible_by_full_name() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let theme = styles
        .get("screen.css")
        .expect("screen.css should be accessible by full filename");
    let resolved = theme.resolve_styles(None);
    assert!(resolved.has("header"));
}

#[test]
fn test_embed_styles_css_beats_yaml_priority() {
    let mut styles: StylesheetRegistry = embed_styles!("tests/fixtures/styles").into();

    let theme = styles
        .get("themes/light")
        .expect("themes/light should resolve");
    let resolved = theme.resolve_styles(None);

    assert!(
        resolved.has("css_wins"),
        "CSS file must win priority — expected `css_wins` style from light.css"
    );
    assert!(
        !resolved.has("yaml_loses"),
        "YAML file must lose priority — `yaml_loses` should not be present"
    );
}

#[test]
fn test_embedded_source_has_entries() {
    let source = embed_templates!("tests/fixtures/templates");

    assert!(!source.entries().is_empty());

    assert!(source.source_path().ends_with("tests/fixtures/templates"));
}

#[test]
fn test_embedded_styles_source_has_entries() {
    let source = embed_styles!("tests/fixtures/styles");

    assert!(!source.entries().is_empty());

    assert!(source.source_path().ends_with("tests/fixtures/styles"));
}
