//! Framework template definitions.
//!
//! Templates are stored as `(name, content)` pairs for registration
//! with the template registry.

/// Framework-supplied templates.
///
/// Each entry is `(name_with_extension, content)`.
/// The registry will make them available both with and without extension.
pub const FRAMEWORK_TEMPLATES: &[(&str, &str)] = &[
    ("standout/list-view.jinja", LIST_VIEW_TEMPLATE),
    ("standout/empty-list.jinja", EMPTY_LIST_TEMPLATE),
    ("standout/filter-summary.jinja", FILTER_SUMMARY_TEMPLATE),
];

/// Default list view template.
///
/// This template renders `ListViewResult<T>` with support for:
/// - Introduction text (header)
/// - Items (tabular or custom rendering)
/// - Ending text (footer)
/// - Filter summary
/// - Status messages
///
/// Template variables:
/// - `items`: The items to display
/// - `intro`: Optional header text
/// - `ending`: Optional footer text
/// - `messages`: Status messages (level, text)
/// - `total_count`: Total before filtering (for "showing X of Y")
/// - `filter_summary`: Description of applied filters
/// - `empty_message`: Custom message when list is empty
const LIST_VIEW_TEMPLATE: &str = r#"{% if intro %}
{{ intro }}

{% endif %}
{% if items | length == 0 %}
{{ empty_message | default("No items found.") }}
{% else %}
{% for item in items %}
{{ item }}
{% endfor %}
{% endif %}
{% if ending %}

{{ ending }}
{% endif %}
{% if total_count and items | length < total_count %}
[standout-muted](Showing {{ items | length }} of {{ total_count }}{% if filter_summary %}, {{ filter_summary }}{% endif %})[/standout-muted]
{% elif filter_summary %}
[standout-muted]({{ filter_summary }})[/standout-muted]
{% endif %}
{% for msg in messages %}
[standout-{{ msg.level }}]{{ msg.text }}[/standout-{{ msg.level }}]
{% endfor %}
"#;

/// Template for empty list message.
const EMPTY_LIST_TEMPLATE: &str = r#"{{ message | default("No items found.") }}
"#;

/// Template for filter summary display.
const FILTER_SUMMARY_TEMPLATE: &str = r#"[standout-muted]{{ summary }}[/standout-muted]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framework_templates_not_empty() {
        assert!(!FRAMEWORK_TEMPLATES.is_empty());
    }

    #[test]
    fn test_all_templates_have_extension() {
        for (name, _) in FRAMEWORK_TEMPLATES {
            assert!(
                name.ends_with(".jinja"),
                "Template {} should have .jinja extension",
                name
            );
        }
    }

    #[test]
    fn test_all_templates_in_standout_namespace() {
        for (name, _) in FRAMEWORK_TEMPLATES {
            assert!(
                name.starts_with("standout/"),
                "Template {} should be in standout/ namespace",
                name
            );
        }
    }
}
