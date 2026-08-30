//! The engine consumes exactly one of a template's trailing newlines.
//!
//! Jinja's rule, kept by MiniJinja, and observable in the bytes a script reads
//! once `App::run` appends its own newline at the process edge. Documented as a
//! contract in `docs/topics/templating.md`.

use serde::Serialize;
use standout_render::{render, Theme};

#[derive(Serialize)]
struct Data {
    name: String,
}

fn rendered(template: &str) -> String {
    render(
        template,
        &Data {
            name: "x".to_string(),
        },
        &Theme::new(),
    )
    .expect("template renders")
}

#[test]
fn exactly_one_trailing_newline_is_consumed() {
    assert_eq!(rendered("{{ name }}"), "x");
    assert_eq!(rendered("{{ name }}\n"), "x");
    assert_eq!(rendered("{{ name }}\n\n"), "x\n");
    assert_eq!(rendered("{{ name }}\n\n\n"), "x\n\n");
}
