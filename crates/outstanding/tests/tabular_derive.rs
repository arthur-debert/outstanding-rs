//! Integration tests for the Tabular derive macro.
//!
//! These tests verify that the `#[derive(Tabular)]` macro generates correct
//! `TabularSpec` configurations from struct field annotations.

#![cfg(feature = "macros")]

use outstanding::tabular::{Align, Anchor, Overflow, Tabular, TabularRow, TruncateAt, Width};
use outstanding_macros::Tabular as DeriveTabular;
use serde::Serialize;

// =============================================================================
// Basic derive tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct BasicTask {
    id: String,
    title: String,
    status: String,
}

#[test]
fn test_basic_derive_compiles() {
    let spec = BasicTask::tabular_spec();
    assert_eq!(spec.columns.len(), 3);
}

#[test]
fn test_basic_derive_field_names() {
    let spec = BasicTask::tabular_spec();
    assert_eq!(spec.columns[0].name.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].name.as_deref(), Some("title"));
    assert_eq!(spec.columns[2].name.as_deref(), Some("status"));
}

#[test]
fn test_basic_derive_default_keys() {
    let spec = BasicTask::tabular_spec();
    // Keys default to field names
    assert_eq!(spec.columns[0].key.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].key.as_deref(), Some("title"));
    assert_eq!(spec.columns[2].key.as_deref(), Some("status"));
}

#[test]
fn test_basic_derive_default_headers() {
    let spec = BasicTask::tabular_spec();
    // Headers default to field names
    assert_eq!(spec.columns[0].header.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].header.as_deref(), Some("title"));
    assert_eq!(spec.columns[2].header.as_deref(), Some("status"));
}

// =============================================================================
// Width attribute tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct WidthTask {
    #[col(width = 8)]
    id: String,

    #[col(width = "fill")]
    title: String,

    #[col(width = "2fr")]
    description: String,

    #[col(min = 10, max = 30)]
    status: String,
}

#[test]
fn test_width_fixed() {
    let spec = WidthTask::tabular_spec();
    assert_eq!(spec.columns[0].width, Width::Fixed(8));
}

#[test]
fn test_width_fill() {
    let spec = WidthTask::tabular_spec();
    assert_eq!(spec.columns[1].width, Width::Fill);
}

#[test]
fn test_width_fraction() {
    let spec = WidthTask::tabular_spec();
    assert_eq!(spec.columns[2].width, Width::Fraction(2));
}

#[test]
fn test_width_bounded() {
    let spec = WidthTask::tabular_spec();
    assert_eq!(
        spec.columns[3].width,
        Width::Bounded {
            min: Some(10),
            max: Some(30)
        }
    );
}

// =============================================================================
// Alignment and anchor tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct AlignTask {
    #[col(align = "left")]
    left_aligned: String,

    #[col(align = "right")]
    right_aligned: String,

    #[col(align = "center")]
    center_aligned: String,

    #[col(anchor = "right")]
    right_anchored: String,
}

#[test]
fn test_align_left() {
    let spec = AlignTask::tabular_spec();
    assert_eq!(spec.columns[0].align, Align::Left);
}

#[test]
fn test_align_right() {
    let spec = AlignTask::tabular_spec();
    assert_eq!(spec.columns[1].align, Align::Right);
}

#[test]
fn test_align_center() {
    let spec = AlignTask::tabular_spec();
    assert_eq!(spec.columns[2].align, Align::Center);
}

#[test]
fn test_anchor_right() {
    let spec = AlignTask::tabular_spec();
    assert_eq!(spec.columns[3].anchor, Anchor::Right);
}

// =============================================================================
// Overflow tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct OverflowTask {
    #[col(overflow = "wrap")]
    wrapped: String,

    #[col(overflow = "clip")]
    clipped: String,

    #[col(overflow = "expand")]
    expanded: String,

    #[col(overflow = "truncate", truncate_at = "middle")]
    truncated_middle: String,

    #[col(truncate_at = "start")]
    truncated_start: String,
}

#[test]
fn test_overflow_wrap() {
    let spec = OverflowTask::tabular_spec();
    assert_eq!(spec.columns[0].overflow, Overflow::Wrap { indent: 0 });
}

#[test]
fn test_overflow_clip() {
    let spec = OverflowTask::tabular_spec();
    assert_eq!(spec.columns[1].overflow, Overflow::Clip);
}

#[test]
fn test_overflow_expand() {
    let spec = OverflowTask::tabular_spec();
    assert_eq!(spec.columns[2].overflow, Overflow::Expand);
}

#[test]
fn test_overflow_truncate_middle() {
    let spec = OverflowTask::tabular_spec();
    assert_eq!(
        spec.columns[3].overflow,
        Overflow::Truncate {
            at: TruncateAt::Middle,
            marker: "…".to_string()
        }
    );
}

#[test]
fn test_overflow_truncate_start() {
    let spec = OverflowTask::tabular_spec();
    assert_eq!(
        spec.columns[4].overflow,
        Overflow::Truncate {
            at: TruncateAt::Start,
            marker: "…".to_string()
        }
    );
}

// =============================================================================
// Style tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct StyleTask {
    #[col(style = "muted")]
    styled: String,

    #[col(style_from_value)]
    dynamic_style: String,
}

#[test]
fn test_style() {
    let spec = StyleTask::tabular_spec();
    assert_eq!(spec.columns[0].style.as_deref(), Some("muted"));
    assert!(!spec.columns[0].style_from_value);
}

#[test]
fn test_style_from_value() {
    let spec = StyleTask::tabular_spec();
    assert!(spec.columns[1].style_from_value);
}

// =============================================================================
// Header and null_repr tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct HeaderTask {
    #[col(header = "Task ID")]
    id: String,

    #[col(null_repr = "N/A")]
    optional_field: String,

    #[col(key = "nested.value")]
    custom_key: String,
}

#[test]
fn test_custom_header() {
    let spec = HeaderTask::tabular_spec();
    assert_eq!(spec.columns[0].header.as_deref(), Some("Task ID"));
}

#[test]
fn test_null_repr() {
    let spec = HeaderTask::tabular_spec();
    assert_eq!(spec.columns[1].null_repr, "N/A");
}

#[test]
fn test_custom_key() {
    let spec = HeaderTask::tabular_spec();
    assert_eq!(spec.columns[2].key.as_deref(), Some("nested.value"));
}

// =============================================================================
// Skip attribute tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
struct SkipTask {
    id: String,

    #[col(skip)]
    internal_state: u32,

    title: String,
}

#[test]
fn test_skip_field() {
    let spec = SkipTask::tabular_spec();
    // Should only have 2 columns (id and title), not 3
    assert_eq!(spec.columns.len(), 2);
    assert_eq!(spec.columns[0].name.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].name.as_deref(), Some("title"));
}

// =============================================================================
// Container attribute tests
// =============================================================================

#[derive(Serialize, DeriveTabular)]
#[tabular(separator = " │ ")]
struct SeparatorTask {
    id: String,
    title: String,
}

#[test]
fn test_custom_separator() {
    let spec = SeparatorTask::tabular_spec();
    assert_eq!(spec.decorations.column_sep, " │ ");
}

#[derive(Serialize, DeriveTabular)]
#[tabular(prefix = "│ ", suffix = " │")]
struct PrefixSuffixTask {
    id: String,
}

#[test]
fn test_prefix_suffix() {
    let spec = PrefixSuffixTask::tabular_spec();
    assert_eq!(spec.decorations.row_prefix, "│ ");
    assert_eq!(spec.decorations.row_suffix, " │");
}

// =============================================================================
// Combined attributes test
// =============================================================================

#[derive(Serialize, DeriveTabular)]
#[tabular(separator = " │ ")]
struct CompleteTask {
    #[col(width = 8, style = "muted", header = "ID")]
    id: String,

    #[col(width = "fill", overflow = "wrap")]
    title: String,

    #[col(width = 12, align = "right", style_from_value)]
    status: String,

    #[col(skip)]
    internal: String,

    #[col(width = 10, anchor = "right", truncate_at = "middle")]
    due: String,
}

#[test]
fn test_complete_task_columns() {
    let spec = CompleteTask::tabular_spec();
    // Should have 4 columns (internal is skipped)
    assert_eq!(spec.columns.len(), 4);
}

#[test]
fn test_complete_task_id_column() {
    let spec = CompleteTask::tabular_spec();
    let col = &spec.columns[0];
    assert_eq!(col.name.as_deref(), Some("id"));
    assert_eq!(col.width, Width::Fixed(8));
    assert_eq!(col.style.as_deref(), Some("muted"));
    assert_eq!(col.header.as_deref(), Some("ID"));
}

#[test]
fn test_complete_task_title_column() {
    let spec = CompleteTask::tabular_spec();
    let col = &spec.columns[1];
    assert_eq!(col.name.as_deref(), Some("title"));
    assert_eq!(col.width, Width::Fill);
    assert_eq!(col.overflow, Overflow::Wrap { indent: 0 });
}

#[test]
fn test_complete_task_status_column() {
    let spec = CompleteTask::tabular_spec();
    let col = &spec.columns[2];
    assert_eq!(col.name.as_deref(), Some("status"));
    assert_eq!(col.width, Width::Fixed(12));
    assert_eq!(col.align, Align::Right);
    assert!(col.style_from_value);
}

#[test]
fn test_complete_task_due_column() {
    let spec = CompleteTask::tabular_spec();
    let col = &spec.columns[3];
    assert_eq!(col.name.as_deref(), Some("due"));
    assert_eq!(col.width, Width::Fixed(10));
    assert_eq!(col.anchor, Anchor::Right);
    assert_eq!(
        col.overflow,
        Overflow::Truncate {
            at: TruncateAt::Middle,
            marker: "…".to_string()
        }
    );
}

#[test]
fn test_complete_task_decorations() {
    let spec = CompleteTask::tabular_spec();
    assert_eq!(spec.decorations.column_sep, " │ ");
}

// =============================================================================
// TabularRow derive tests
// =============================================================================

use outstanding_macros::TabularRow as DeriveTabularRow;

#[derive(DeriveTabularRow)]
struct BasicRow {
    id: String,
    title: String,
    status: String,
}

#[test]
fn test_tabular_row_basic() {
    let row = BasicRow {
        id: "TSK-001".to_string(),
        title: "Implement feature".to_string(),
        status: "pending".to_string(),
    };
    let values = row.to_row();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "TSK-001");
    assert_eq!(values[1], "Implement feature");
    assert_eq!(values[2], "pending");
}

#[derive(DeriveTabularRow)]
struct NumericRow {
    id: i32,
    count: u64,
    value: f64,
}

#[test]
fn test_tabular_row_numeric() {
    let row = NumericRow {
        id: 42,
        count: 100,
        value: 1.23,
    };
    let values = row.to_row();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "42");
    assert_eq!(values[1], "100");
    assert_eq!(values[2], "1.23");
}

#[derive(DeriveTabularRow)]
struct SkipRow {
    id: String,

    #[col(skip)]
    #[allow(dead_code)]
    internal: u32,

    title: String,
}

#[test]
fn test_tabular_row_skip() {
    let row = SkipRow {
        id: "TSK-001".to_string(),
        internal: 42,
        title: "Task title".to_string(),
    };
    let values = row.to_row();
    // Should have 2 fields (internal is skipped)
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], "TSK-001");
    assert_eq!(values[1], "Task title");
}

#[derive(DeriveTabularRow)]
struct BoolRow {
    active: bool,
    name: String,
}

#[test]
fn test_tabular_row_bool() {
    let row = BoolRow {
        active: true,
        name: "Test".to_string(),
    };
    let values = row.to_row();
    assert_eq!(values[0], "true");
    assert_eq!(values[1], "Test");
}

// Test that both macros can be used together
#[derive(Serialize, DeriveTabular, DeriveTabularRow)]
#[tabular(separator = " | ")]
struct CombinedTask {
    #[col(width = 8)]
    id: String,

    #[col(width = "fill")]
    title: String,

    #[col(skip)]
    internal: u32,

    #[col(width = 12, align = "right")]
    status: String,
}

#[test]
fn test_combined_macros_spec() {
    let spec = CombinedTask::tabular_spec();
    // Should have 3 columns (internal is skipped)
    assert_eq!(spec.columns.len(), 3);
    assert_eq!(spec.columns[0].name.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].name.as_deref(), Some("title"));
    assert_eq!(spec.columns[2].name.as_deref(), Some("status"));
}

#[test]
fn test_combined_macros_row() {
    let task = CombinedTask {
        id: "TSK-001".to_string(),
        title: "Implement feature".to_string(),
        internal: 42,
        status: "pending".to_string(),
    };
    let values = task.to_row();
    // Should have 3 values (internal is skipped)
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "TSK-001");
    assert_eq!(values[1], "Implement feature");
    assert_eq!(values[2], "pending");
}

#[test]
fn test_combined_row_matches_spec_columns() {
    // Verify that the number of row values matches the number of spec columns
    let spec = CombinedTask::tabular_spec();
    let task = CombinedTask {
        id: "TSK-001".to_string(),
        title: "Implement feature".to_string(),
        internal: 42,
        status: "pending".to_string(),
    };
    let values = task.to_row();

    assert_eq!(spec.columns.len(), values.len());
}

// =============================================================================
// Integration with TabularFormatter tests
// =============================================================================

use outstanding::tabular::{BorderStyle, Table, TabularFormatter};

#[test]
fn test_formatter_from_type() {
    // TabularFormatter::from_type<T> should create a formatter using the derived spec
    let formatter = TabularFormatter::from_type::<CombinedTask>(80);

    // Should have 3 columns (internal is skipped)
    assert_eq!(formatter.num_columns(), 3);
}

#[test]
fn test_formatter_row_from_trait() {
    let formatter = TabularFormatter::from_type::<CombinedTask>(80);
    let task = CombinedTask {
        id: "TSK-001".to_string(),
        title: "Implement feature".to_string(),
        internal: 42,
        status: "pending".to_string(),
    };

    let row = formatter.row_from_trait(&task);

    // Row should contain the field values
    assert!(row.contains("TSK-001"));
    assert!(row.contains("Implement feature"));
    assert!(row.contains("pending"));
    // Internal field should not be present
    assert!(!row.contains("42"));
}

#[test]
fn test_formatter_row_lines_from_trait() {
    let formatter = TabularFormatter::from_type::<CombinedTask>(80);
    let task = CombinedTask {
        id: "TSK-001".to_string(),
        title: "Implement feature".to_string(),
        internal: 42,
        status: "pending".to_string(),
    };

    let lines = formatter.row_lines_from_trait(&task);

    // Should have at least one line
    assert!(!lines.is_empty());
    // First line should contain the values
    assert!(lines[0].contains("TSK-001"));
}

#[test]
fn test_table_from_type() {
    // Table::from_type<T> should create a table using the derived spec
    let table = Table::from_type::<CombinedTask>(80)
        .header_from_columns()
        .border(BorderStyle::Light);

    // Should have 3 columns
    assert_eq!(table.num_columns(), 3);
}

#[test]
fn test_table_row_from_trait() {
    let table = Table::from_type::<CombinedTask>(80).border(BorderStyle::Light);
    let task = CombinedTask {
        id: "TSK-001".to_string(),
        title: "Implement feature".to_string(),
        internal: 42,
        status: "pending".to_string(),
    };

    let row = table.row_from_trait(&task);

    // Row should have border characters
    assert!(row.starts_with('│'));
    assert!(row.ends_with('│'));

    // Row should contain the field values
    assert!(row.contains("TSK-001"));
    assert!(row.contains("Implement feature"));
    assert!(row.contains("pending"));
}

#[test]
fn test_table_header_from_columns_with_derived_spec() {
    // The CompleteTask struct has explicit headers defined
    let table = Table::from_type::<CompleteTask>(80).header_from_columns();

    let header = table.header_row();

    // "ID" is explicitly set as header for the id field
    assert!(header.contains("ID"));
}

#[test]
fn test_full_table_workflow_with_macros() {
    // Demonstrate the complete workflow: define struct, derive macros, create table, render rows
    let table = Table::from_type::<CombinedTask>(80)
        .header_from_columns()
        .border(BorderStyle::Light);

    let tasks = vec![
        CombinedTask {
            id: "TSK-001".to_string(),
            title: "First task".to_string(),
            internal: 1,
            status: "pending".to_string(),
        },
        CombinedTask {
            id: "TSK-002".to_string(),
            title: "Second task".to_string(),
            internal: 2,
            status: "done".to_string(),
        },
    ];

    // Render all rows using the trait
    let mut output = Vec::new();
    output.push(table.top_border());
    output.push(table.header_row());
    output.push(table.separator_row());
    for task in &tasks {
        output.push(table.row_from_trait(task));
    }
    output.push(table.bottom_border());

    let rendered = output.join("\n");

    // Verify the complete table structure
    assert!(rendered.contains("TSK-001"));
    assert!(rendered.contains("TSK-002"));
    assert!(rendered.contains("First task"));
    assert!(rendered.contains("Second task"));
    assert!(rendered.contains("pending"));
    assert!(rendered.contains("done"));
}

// =============================================================================
// Template integration tests
// =============================================================================

use minijinja::{context, Environment};
use outstanding::tabular::filters::{
    formatter_from_type, register_tabular_filters, table_from_type,
};

// Define a struct for template tests
#[derive(Serialize, DeriveTabular, DeriveTabularRow)]
#[tabular(separator = "  ")]
struct DemoTask {
    #[col(width = 10, header = "Task ID")]
    id: String,

    #[col(width = "fill", header = "Title")]
    title: String,

    #[col(width = 8, align = "right", header = "Status")]
    status: String,
}

fn setup_template_env() -> Environment<'static> {
    let mut env = Environment::new();
    register_tabular_filters(&mut env);
    env
}

#[test]
fn test_helper_formatter_from_type() {
    // Create a formatter from the derived spec
    let formatter = formatter_from_type::<DemoTask>(60);

    // Use it in a template
    let mut env = setup_template_env();
    env.add_template(
        "test",
        r#"{{ fmt.row(["TSK-001", "Implement feature", "pending"]) }}"#,
    )
    .unwrap();

    let result = env
        .get_template("test")
        .unwrap()
        .render(context!(fmt => formatter))
        .unwrap();

    assert!(result.contains("TSK-001"));
    assert!(result.contains("Implement feature"));
    assert!(result.contains("pending"));
}

#[test]
fn test_helper_table_from_type_with_border() {
    // Create a table from the derived spec with border
    let table = table_from_type::<DemoTask>(80, BorderStyle::Light, true);

    // Use it in a template
    let mut env = setup_template_env();
    env.add_template(
        "test",
        r#"{{ tbl.header_row() }}
{{ tbl.separator_row() }}
{{ tbl.row(["TSK-001", "Test task", "done"]) }}"#,
    )
    .unwrap();

    let result = env
        .get_template("test")
        .unwrap()
        .render(context!(tbl => table))
        .unwrap();

    // Should have header with our custom names
    assert!(result.contains("Task ID"));
    assert!(result.contains("Title"));
    assert!(result.contains("Status"));

    // Should have border characters
    assert!(result.contains("│"));
    assert!(result.contains("─"));

    // Should have our data
    assert!(result.contains("TSK-001"));
}

#[test]
fn test_helper_table_from_type_without_headers() {
    // Create a table without headers
    let table = table_from_type::<DemoTask>(80, BorderStyle::None, false);

    let mut env = setup_template_env();
    env.add_template("test", r#"{{ tbl.header_row() }}"#)
        .unwrap();

    let result = env
        .get_template("test")
        .unwrap()
        .render(context!(tbl => table))
        .unwrap();

    // Header should be empty when not requested
    assert!(result.is_empty());
}

#[test]
fn test_helper_full_template_workflow() {
    // Demonstrate the complete workflow with derived macros and templates
    let table = table_from_type::<DemoTask>(80, BorderStyle::Light, true);

    let mut env = setup_template_env();
    env.add_template(
        "tasks_list",
        r#"{{ tbl.top_border() }}
{{ tbl.header_row() }}
{{ tbl.separator_row() }}
{% for task in tasks %}{{ tbl.row([task.id, task.title, task.status]) }}
{% endfor %}{{ tbl.bottom_border() }}"#,
    )
    .unwrap();

    let tasks = vec![
        context!(id => "TSK-001", title => "First task", status => "pending"),
        context!(id => "TSK-002", title => "Second task", status => "done"),
    ];

    let result = env
        .get_template("tasks_list")
        .unwrap()
        .render(context!(tbl => table, tasks => tasks))
        .unwrap();

    // Verify complete table structure
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines.len() >= 6); // top, header, sep, 2 rows, bottom

    // Top border
    assert!(lines[0].starts_with('┌'));
    // Header row
    assert!(lines[1].contains("Task ID"));
    // Separator
    assert!(lines[2].starts_with('├'));
    // Data rows
    assert!(lines[3].contains("TSK-001"));
    assert!(lines[4].contains("TSK-002"));
    // Bottom border
    assert!(lines[5].starts_with('└'));
}

#[test]
fn test_spec_columns_match_derived_demo_task() {
    // Verify the derived spec has correct column configuration
    let spec = DemoTask::tabular_spec();

    assert_eq!(spec.columns.len(), 3);
    assert_eq!(spec.columns[0].width, Width::Fixed(10));
    assert_eq!(spec.columns[0].header.as_deref(), Some("Task ID"));
    assert_eq!(spec.columns[1].width, Width::Fill);
    assert_eq!(spec.columns[1].header.as_deref(), Some("Title"));
    assert_eq!(spec.columns[2].header.as_deref(), Some("Status"));
}

#[test]
fn test_row_extraction_matches_derived_demo_task() {
    // Verify TabularRow generates correct values
    let task = DemoTask {
        id: "TSK-001".to_string(),
        title: "Test".to_string(),
        status: "pending".to_string(),
    };

    let row = task.to_row();
    assert_eq!(row.len(), 3);
    assert_eq!(row[0], "TSK-001");
    assert_eq!(row[1], "Test");
    assert_eq!(row[2], "pending");
}

// =============================================================================
// Option field tests (Regression test for TabularRow derive)
// =============================================================================

#[derive(DeriveTabularRow)]
struct OptionRow {
    id: String,

    // This should work (None -> empty string, Some(s) -> s)
    description: Option<String>,

    // This should also work with non-string options
    score: Option<i32>,
}

#[test]
fn test_tabular_row_option() {
    let row = OptionRow {
        id: "TSK-001".to_string(),
        description: Some("desc".to_string()),
        score: None,
    };
    let values = row.to_row();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "TSK-001");
    assert_eq!(values[1], "desc");
    // Default behavior for None is empty string
    assert_eq!(values[2], "");
}
