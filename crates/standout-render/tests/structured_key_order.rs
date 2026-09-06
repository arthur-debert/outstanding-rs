use std::collections::HashMap;

use serde_json::json;
use standout_render::{
    default_template_engine, render_request_split, AmbiguousWidth, ColorMode, ColorPolicy,
    IconMode, RenderRequest, Representation, TargetProperties, TemplateRef, Theme,
};

fn request(format: Representation) -> RenderRequest {
    RenderRequest {
        // Declared out of alphabetical order (alphabetical would be
        // machine_type, name, status, zone) so a sort would be visible.
        data: json!({
            "name": "web-1",
            "zone": "us-east1-b",
            "machine_type": "n2-standard-2",
            "status": "RUNNING",
        })
        .into(),
        template: TemplateRef::Absent,
        theme: Theme::new(),
        format,
        color_policy: ColorPolicy::Never,
        target: TargetProperties {
            width: None,
            stdout_is_terminal: false,
            stderr_is_terminal: false,
            stdout_color_capability: false,
            stderr_color_capability: false,
            color_scheme: ColorMode::Dark,
            icon_mode: IconMode::Classic,
            ambiguous_width: AmbiguousWidth::Narrow,
        },
        engine: default_template_engine(),
        registry: None,
        context_registry: None,
        csv_projection: None,
        extras: HashMap::new(),
        warnings: None,
    }
}

fn assert_ascending(output: &str, needles: &[&str]) {
    let positions: Vec<usize> = needles
        .iter()
        .map(|n| {
            output
                .find(n)
                .unwrap_or_else(|| panic!("missing {n:?} in {output}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "expected {needles:?} in ascending order, got {output}"
    );
}

#[test]
fn json_keeps_declaration_order() {
    let result = render_request_split(&request(Representation::Json)).unwrap();
    assert_ascending(
        &result.formatted,
        &["\"name\"", "\"zone\"", "\"machine_type\"", "\"status\""],
    );
}

#[test]
fn yaml_keeps_declaration_order() {
    let result = render_request_split(&request(Representation::Yaml)).unwrap();
    assert_ascending(
        &result.formatted,
        &["name:", "zone:", "machine_type:", "status:"],
    );
}

#[test]
fn csv_keeps_declaration_order() {
    let result = render_request_split(&request(Representation::Csv)).unwrap();
    assert_eq!(
        result.formatted,
        "name,zone,machine_type,status\nweb-1,us-east1-b,n2-standard-2,RUNNING\n"
    );
}
