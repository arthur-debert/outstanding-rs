pub const HELP_TEMPLATE_NAME: &str = "standout/help";
pub const TOPIC_TEMPLATE_NAME: &str = "standout/topic";
pub const TOPICS_LIST_TEMPLATE_NAME: &str = "standout/topics-list";

pub const FRAMEWORK_TEMPLATES: &[(&str, &str)] = &[
    ("standout/list-view.jinja", LIST_VIEW_TEMPLATE),
    ("standout/empty-list.jinja", EMPTY_LIST_TEMPLATE),
    ("standout/filter-summary.jinja", FILTER_SUMMARY_TEMPLATE),
    (
        "standout/help.jinja",
        include_str!("../cli/help/template.txt"),
    ),
    (
        "standout/topic.jinja",
        include_str!("../topic_template.txt"),
    ),
    (
        "standout/topics-list.jinja",
        include_str!("../topics_list_template.txt"),
    ),
];

const LIST_VIEW_TEMPLATE: &str = r#"{% if intro %}
{{ intro }}

{% endif %}
{% if items | length == 0 %}
{{ empty_message | default("No items found.") }}
{% else %}
{% if tabular_spec %}
{% set t = tabular(tabular_spec) %}
{% for item in items %}
{{ t.row_from(item) }}
{% endfor %}
{% else %}
{% for item in items %}
{{ item }}
{% endfor %}
{% endif %}
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
{% if msg.level == "error" -%}
[standout-error]{{ msg.text }}[/standout-error]
{% elif msg.level == "warning" -%}
[standout-warning]{{ msg.text }}[/standout-warning]
{% elif msg.level == "success" -%}
[standout-success]{{ msg.text }}[/standout-success]
{% else -%}
[standout-info]{{ msg.text }}[/standout-info]
{% endif %}
{% endfor %}
"#;

const EMPTY_LIST_TEMPLATE: &str = r#"{{ message | default("No items found.") }}
"#;

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
