mod templates;

pub use templates::{
    FRAMEWORK_TEMPLATES, HELP_TEMPLATE_NAME, TOPICS_LIST_TEMPLATE_NAME, TOPIC_TEMPLATE_NAME,
};

pub const FRAMEWORK_STYLES: &str = r#"
# Standout Framework Styles
# These can be overridden by user styles with the same name.

standout-muted:
  fg: gray

standout-error:
  fg: red

standout-warning:
  fg: yellow

standout-info:
  fg: blue

standout-success:
  fg: green

standout-header:
  bold: true
"#;
