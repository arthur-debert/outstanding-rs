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
    ("standout/detail-view.jinja", DETAIL_VIEW_TEMPLATE),
    ("standout/create-view.jinja", CREATE_VIEW_TEMPLATE),
    ("standout/update-view.jinja", UPDATE_VIEW_TEMPLATE),
    ("standout/delete-view.jinja", DELETE_VIEW_TEMPLATE),
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
[standout-{{ msg.level }}]{{ msg.text }}[/standout-{{ msg.level }}]
{% endfor %}
"#;

/// Template for empty list message.
const EMPTY_LIST_TEMPLATE: &str = r#"{{ message | default("No items found.") }}
"#;

/// Template for filter summary display.
const FILTER_SUMMARY_TEMPLATE: &str = r#"[standout-muted]{{ summary }}[/standout-muted]
"#;

/// Detail view template for displaying a single item.
///
/// This template renders `DetailViewResult<T>` with support for:
/// - Title and subtitle header
/// - Item fields as key-value pairs
/// - Related items section
/// - Suggested actions
/// - Status messages
///
/// Template variables:
/// - `item`: The item to display (as JSON object)
/// - `title`: Optional title text
/// - `subtitle`: Optional subtitle text
/// - `related`: Map of related items by relationship name
/// - `actions`: List of suggested actions (label, command)
/// - `messages`: Status messages (level, text)
const DETAIL_VIEW_TEMPLATE: &str = r#"{% if title %}
[standout-header]{{ title }}[/standout-header]
{% endif %}
{% if subtitle %}
[standout-muted]{{ subtitle }}[/standout-muted]
{% endif %}
{% if title or subtitle %}

{% endif %}
{% for key, value in item %}
[standout-muted]{{ key }}:[/standout-muted] {{ value }}
{% endfor %}
{% if related | length > 0 %}

[standout-header]Related[/standout-header]
{% for name, value in related %}
[standout-muted]{{ name }}:[/standout-muted] {{ value }}
{% endfor %}
{% endif %}
{% if actions | length > 0 %}

[standout-muted]Actions:[/standout-muted]
{% for action in actions %}
  {{ action.label }}: [standout-muted]{{ action.command }}[/standout-muted]
{% endfor %}
{% endif %}
{% for msg in messages %}
[standout-{{ msg.level }}]{{ msg.text }}[/standout-{{ msg.level }}]
{% endfor %}
"#;

/// Create view template for displaying create operation results.
///
/// This template renders `CreateViewResult<T>` with support for:
/// - Dry-run mode indication
/// - Validation error display
/// - Created item preview
/// - Status messages
///
/// Template variables:
/// - `item`: The created item
/// - `dry_run`: Whether this was a dry-run
/// - `validation_errors`: List of validation errors (field, message)
/// - `messages`: Status messages (level, text)
const CREATE_VIEW_TEMPLATE: &str = r#"{% if dry_run %}
[standout-info]Dry run - no changes made[/standout-info]

{% endif %}
{% if validation_errors | length > 0 %}
[standout-error]Validation failed:[/standout-error]
{% for error in validation_errors %}
  [standout-muted]{{ error.field }}:[/standout-muted] {{ error.message }}
{% endfor %}
{% else %}
{% for key, value in item %}
[standout-muted]{{ key }}:[/standout-muted] {{ value }}
{% endfor %}
{% endif %}
{% for msg in messages %}
[standout-{{ msg.level }}]{{ msg.text }}[/standout-{{ msg.level }}]
{% endfor %}
"#;

/// Update view template for displaying update operation results.
///
/// This template renders `UpdateViewResult<T>` with support for:
/// - Dry-run mode indication
/// - Before/after comparison
/// - Changed fields highlighting
/// - Validation error display
/// - Status messages
///
/// Template variables:
/// - `before`: The item state before update (optional)
/// - `after`: The item state after update
/// - `changed_fields`: List of field names that changed
/// - `dry_run`: Whether this was a dry-run
/// - `validation_errors`: List of validation errors (field, message)
/// - `messages`: Status messages (level, text)
const UPDATE_VIEW_TEMPLATE: &str = r#"{% if dry_run %}
[standout-info]Dry run - no changes made[/standout-info]

{% endif %}
{% if validation_errors | length > 0 %}
[standout-error]Validation failed:[/standout-error]
{% for error in validation_errors %}
  [standout-muted]{{ error.field }}:[/standout-muted] {{ error.message }}
{% endfor %}
{% elif changed_fields | length == 0 %}
[standout-muted]No changes made[/standout-muted]
{% else %}
{% if before %}
[standout-muted]Changes:[/standout-muted]
{% for field in changed_fields %}
  {{ field }}: {{ before[field] | default("[none]") }} -> {{ after[field] }}
{% endfor %}
{% else %}
[standout-muted]Updated fields:[/standout-muted]
{% for field in changed_fields %}
  {{ field }}: {{ after[field] }}
{% endfor %}
{% endif %}
{% endif %}
{% for msg in messages %}
[standout-{{ msg.level }}]{{ msg.text }}[/standout-{{ msg.level }}]
{% endfor %}
"#;

/// Delete view template for displaying delete operation results.
///
/// This template renders `DeleteViewResult<T>` with support for:
/// - Confirmation status
/// - Soft-delete indication
/// - Undo command display
/// - Deleted item preview
/// - Status messages
///
/// Template variables:
/// - `item`: The deleted item
/// - `confirmed`: Whether deletion was confirmed
/// - `soft_deleted`: Whether this was a soft-delete
/// - `undo_command`: Command to undo the deletion (if available)
/// - `messages`: Status messages (level, text)
const DELETE_VIEW_TEMPLATE: &str = r#"{% if not confirmed %}
[standout-warning]Pending deletion:[/standout-warning]
{% for key, value in item %}
[standout-muted]{{ key }}:[/standout-muted] {{ value }}
{% endfor %}

[standout-muted]Use --confirm to proceed with deletion[/standout-muted]
{% else %}
{% if soft_deleted %}
[standout-info]Moved to trash[/standout-info]
{% endif %}
{% for key, value in item %}
[standout-muted]{{ key }}:[/standout-muted] {{ value }}
{% endfor %}
{% if undo_command %}

[standout-muted]Undo:[/standout-muted] {{ undo_command }}
{% endif %}
{% endif %}
{% for msg in messages %}
[standout-{{ msg.level }}]{{ msg.text }}[/standout-{{ msg.level }}]
{% endfor %}
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
