use minijinja::context;

use crate::tabular::filters::test_data::setup_env;

const PR_ROWS: &[[&str; 3]] = &[
    ["#12", "open", "Add pagination"],
    ["#7", "merged", "Fix retry"],
];

fn render_with_rows(template: &str) -> String {
    let mut env = setup_env();
    env.add_template("test", template).unwrap();
    env.get_template("test")
        .unwrap()
        .render(context!(rows => PR_ROWS))
        .unwrap()
}

#[test]
fn function_tabular_without_rows_leaves_bounded_columns_unmeasured() {
    let widths = render_with_rows(
        r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": {"min": 0}}], separator="  ", width=60) %}{{ fmt.widths }}"#,
    );
    assert_eq!(standout_bbparser::strip_tags(&widths), "[0, 0, 56]");
}

#[test]
fn function_tabular_rows_sizes_bounded_columns_to_the_widest_cell() {
    let widths = render_with_rows(
        r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": {"min": 0}}], separator="  ", width=60, rows=rows) %}{{ fmt.widths }}"#,
    );
    assert_eq!(standout_bbparser::strip_tags(&widths), "[3, 6, 47]");
}

#[test]
fn function_tabular_rows_aligns_every_row_to_the_measured_columns() {
    let rendered = render_with_rows(
        r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": {"min": 0}}], separator="  ", width=60, rows=rows) %}{% for row in rows %}{{ fmt.row(row) }}
{% endfor %}"#,
    );
    let lines: Vec<&str> = rendered.lines().map(|line| line.trim_end()).collect();
    assert_eq!(
        lines,
        vec!["#12  open    Add pagination", "#7   merged  Fix retry"]
    );
}

#[test]
fn function_tabular_rows_respects_a_max_bound() {
    let widths = render_with_rows(
        r#"{% set fmt = tabular([{"width": {"min": 0, "max": 2}}, {"width": {"min": 0}}, {"width": "fill"}], separator="  ", width=60, rows=rows) %}{{ fmt.widths }}"#,
    );
    assert_eq!(standout_bbparser::strip_tags(&widths), "[2, 6, 48]");
}

#[test]
fn function_tabular_rows_leaves_a_sub_column_parent_unmeasured() {
    let widths = render_with_rows(
        r#"{% set fmt = tabular([{"width": {"min": 4}}, {"width": {"min": 0}, "sub_columns": {"columns": [{"width": "fill"}, {"width": 6}]}}], separator="  ", width=40, rows=rows) %}{{ fmt.widths }}"#,
    );
    assert_eq!(standout_bbparser::strip_tags(&widths), "[4, 34]");
}

#[test]
fn function_table_rows_measures_the_header_too() {
    let rendered = render_with_rows(
        r#"{% set t = table([{"width": {"min": 0}}, {"width": {"min": 0}}, {"width": "fill"}], separator="  ", width=60, header=["NUMBER", "STATE", "TITLE"], rows=rows) %}{{ t.header_row() }}
{% for row in rows %}{{ t.row(row) }}
{% endfor %}"#,
    );
    let lines: Vec<&str> = rendered.lines().map(|line| line.trim_end()).collect();
    assert_eq!(
        lines,
        vec![
            "NUMBER  STATE   TITLE",
            "#12     open    Add pagination",
            "#7      merged  Fix retry",
        ]
    );
}

#[test]
fn function_tabular_rows_measures_null_repr_where_a_row_stops_short() {
    let mut env = setup_env();
    env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": {"min": 0}}, {"width": {"min": 0}, "null_repr": "unknown"}, {"width": "fill"}], separator="  ", width=60, rows=rows) %}{{ fmt.widths }}|{{ fmt.row(rows[0]) }}"#,
        )
        .unwrap();
    let rendered = env
        .get_template("test")
        .unwrap()
        .render(context!(rows => vec![vec!["#12"], vec!["#7"]]))
        .unwrap();
    let (widths, row) = rendered.split_once('|').unwrap();
    assert_eq!(standout_bbparser::strip_tags(widths), "[3, 7, 46]");
    assert!(
        row.contains("unknown"),
        "the omitted cell renders `null_repr` in full: {row:?}"
    );
}

#[test]
fn function_table_measures_null_repr_where_the_header_stops_short() {
    let mut env = setup_env();
    env.add_template(
            "test",
            r#"{% set t = table([{"width": {"min": 0}}, {"width": {"min": 0}, "null_repr": "unknown"}, {"width": "fill"}], separator="  ", width=60, header=["NUMBER"], rows=rows) %}{{ t.header_row() }}"#,
        )
        .unwrap();
    let header = env
        .get_template("test")
        .unwrap()
        .render(context!(rows => vec![vec!["#12"], vec!["#7"]]))
        .unwrap();
    assert!(
        header.contains("unknown"),
        "the header column the caller left out renders `null_repr` in full: {header:?}"
    );
}

#[test]
fn function_tabular_rows_rejects_a_row_that_is_not_an_array() {
    let mut env = setup_env();
    env.add_template(
            "test",
            r#"{% set fmt = tabular([{"width": {"min": 0}}], rows=[{"number": "n12"}]) %}{{ fmt.widths }}"#,
        )
        .unwrap();
    let error = env
        .get_template("test")
        .unwrap()
        .render(context!())
        .unwrap_err();
    assert!(error.to_string().contains("row 0 is map"), "{error}");
}

#[test]
fn function_table_header_rejects_a_string() {
    let mut env = setup_env();
    env.add_template(
        "test",
        r#"{% set t = table([{"width": 6}], header="NUMBER") %}{{ t.header_row() }}"#,
    )
    .unwrap();
    let error = env
        .get_template("test")
        .unwrap()
        .render(context!())
        .unwrap_err();
    assert!(
        error.to_string().contains("header must be an array"),
        "{error}"
    );
}

#[test]
fn function_tabular_rows_rejects_a_scalar() {
    let mut env = setup_env();
    env.add_template(
        "test",
        r#"{% set fmt = tabular([{"width": {"min": 0}}], rows=12) %}{{ fmt.widths }}"#,
    )
    .unwrap();
    let error = env
        .get_template("test")
        .unwrap()
        .render(context!())
        .unwrap_err();
    assert!(
        error.to_string().contains("rows must be an array"),
        "{error}"
    );
}
