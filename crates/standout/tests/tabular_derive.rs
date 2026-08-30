#![cfg(feature = "macros")]

use serde::Serialize;
use standout::tabular::{Align, Anchor, Overflow, Tabular, TabularRow, TruncateAt, Width};
use standout_macros::Tabular as DeriveTabular;

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
    assert_eq!(spec.columns[0].key.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].key.as_deref(), Some("title"));
    assert_eq!(spec.columns[2].key.as_deref(), Some("status"));
}

#[test]
fn test_basic_derive_default_headers() {
    let spec = BasicTask::tabular_spec();
    assert_eq!(spec.columns[0].header.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].header.as_deref(), Some("title"));
    assert_eq!(spec.columns[2].header.as_deref(), Some("status"));
}

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
    assert_eq!(spec.columns.len(), 2);
    assert_eq!(spec.columns[0].name.as_deref(), Some("id"));
    assert_eq!(spec.columns[1].name.as_deref(), Some("title"));
}

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

use standout_macros::TabularRow as DeriveTabularRow;

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
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], "TSK-001");
    assert_eq!(values[1], "Implement feature");
    assert_eq!(values[2], "pending");
}

#[test]
fn test_combined_row_matches_spec_columns() {
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

use standout::tabular::{BorderStyle, Table, TabularFormatter};

#[test]
fn test_formatter_from_type() {
    let formatter = TabularFormatter::from_type::<CombinedTask>(80);

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

    assert!(row.contains("TSK-001"));
    assert!(row.contains("Implement feature"));
    assert!(row.contains("pending"));
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

    assert!(!lines.is_empty());
    assert!(lines[0].contains("TSK-001"));
}

#[test]
fn test_table_from_type() {
    let table = Table::from_type::<CombinedTask>(80)
        .header_from_columns()
        .border(BorderStyle::Light);

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

    assert!(row.starts_with('│'));
    assert!(row.ends_with('│'));

    assert!(row.contains("TSK-001"));
    assert!(row.contains("Implement feature"));
    assert!(row.contains("pending"));
}

#[test]
fn test_table_header_from_columns_with_derived_spec() {
    let table = Table::from_type::<CompleteTask>(80).header_from_columns();

    let header = table.header_row();

    assert!(header.contains("ID"));
}

#[test]
fn test_full_table_workflow_with_macros() {
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

    let mut output = Vec::new();
    output.push(table.top_border());
    output.push(table.header_row());
    output.push(table.separator_row());
    for task in &tasks {
        output.push(table.row_from_trait(task));
    }
    output.push(table.bottom_border());

    let rendered = output.join("\n");

    assert!(rendered.contains("TSK-001"));
    assert!(rendered.contains("TSK-002"));
    assert!(rendered.contains("First task"));
    assert!(rendered.contains("Second task"));
    assert!(rendered.contains("pending"));
    assert!(rendered.contains("done"));
}

use minijinja::{context, Environment};
use standout::tabular::filters::{
    formatter_from_type, formatter_from_type_with_ambiguous_width, register_tabular_filters,
    table_from_type, table_from_type_with_ambiguous_width,
};
use standout::{AmbiguousWidth, WidthCalculator};

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
    let formatter = formatter_from_type::<DemoTask>(60);

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
fn policy_aware_derive_helpers_use_wide_measurement() {
    let formatter = formatter_from_type_with_ambiguous_width::<DemoTask>(60, AmbiguousWidth::Wide);
    let table = table_from_type_with_ambiguous_width::<DemoTask>(
        61,
        BorderStyle::Light,
        false,
        AmbiguousWidth::Wide,
    );
    let mut env = setup_template_env();
    env.add_template(
        "policy_helpers",
        "{{ fmt.row(['A', '↦≈Δ', 'ok']) }}\n{{ tbl.row(['A', '↦≈Δ', 'ok']) }}",
    )
    .unwrap();
    let rendered = env
        .get_template("policy_helpers")
        .unwrap()
        .render(context!(fmt => formatter, tbl => table))
        .unwrap();
    let lines: Vec<_> = rendered.lines().collect();
    let calculator = WidthCalculator::new(AmbiguousWidth::Wide);

    assert_eq!(calculator.visible_width(lines[0]), 60);
    assert!(calculator.visible_width(lines[1]) <= 61);
    assert!(lines[1].starts_with('│') && lines[1].ends_with('│'));
}

#[test]
fn test_helper_table_from_type_with_border() {
    let table = table_from_type::<DemoTask>(80, BorderStyle::Light, true);

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

    assert!(result.contains("Task ID"));
    assert!(result.contains("Title"));
    assert!(result.contains("Status"));

    assert!(result.contains("│"));
    assert!(result.contains("─"));

    assert!(result.contains("TSK-001"));
}

#[test]
fn test_helper_table_from_type_without_headers() {
    let table = table_from_type::<DemoTask>(80, BorderStyle::None, false);

    let mut env = setup_template_env();
    env.add_template("test", r#"{{ tbl.header_row() }}"#)
        .unwrap();

    let result = env
        .get_template("test")
        .unwrap()
        .render(context!(tbl => table))
        .unwrap();

    assert!(result.is_empty());
}

#[test]
fn test_helper_full_template_workflow() {
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

    let lines: Vec<&str> = result.lines().collect();
    assert!(lines.len() >= 6);

    assert!(lines[0].starts_with('┌'));
    assert!(lines[1].contains("Task ID"));
    assert!(lines[2].starts_with('├'));
    assert!(lines[3].contains("TSK-001"));
    assert!(lines[4].contains("TSK-002"));
    assert!(lines[5].starts_with('└'));
}

#[test]
fn test_spec_columns_match_derived_demo_task() {
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

#[derive(DeriveTabularRow)]
struct OptionRow {
    id: String,

    description: Option<String>,

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
    assert_eq!(values[2], "");
}
