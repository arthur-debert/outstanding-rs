use super::*;
use crate::tabular::{display_width, Width};

#[test]
fn format_row_lines_single_line() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(10)))
        .column(Column::new(Width::Fixed(8)))
        .separator("  ")
        .build();
    let formatter = TabularFormatter::new(&spec, 80);

    let lines = formatter.format_row_lines(&["Hello", "World"]);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], formatter.format_row(&["Hello", "World"]));
}

#[test]
fn format_row_lines_multi_line() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(8)).wrap())
        .column(Column::new(Width::Fixed(6)))
        .separator("  ")
        .build();
    let formatter = TabularFormatter::new(&spec, 80);

    let lines = formatter.format_row_lines(&["This is long", "Short"]);

    assert!(!lines.is_empty());

    let expected_width = display_width(&lines[0]);
    for line in &lines {
        assert_eq!(display_width(line), expected_width);
    }
}

#[test]
fn format_row_lines_mixed_columns() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(6))) // truncates
        .column(Column::new(Width::Fixed(10)).wrap()) // wraps
        .column(Column::new(Width::Fixed(4))) // truncates
        .separator(" ")
        .build();
    let formatter = TabularFormatter::new(&spec, 80);

    let lines = formatter.format_row_lines(&["aaaaa", "this text wraps here", "bbbb"]);

    assert!(!lines.is_empty());
}

#[test]
fn format_row_all_left_anchor_no_gap() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(5)))
        .column(Column::new(Width::Fixed(5)))
        .separator(" ")
        .build();
    let formatter = TabularFormatter::new(&spec, 50);

    let output = formatter.format_row(&["A", "B"]);
    assert_eq!(output, "A     B    ");
    assert_eq!(display_width(&output), 11);
}

#[test]
fn format_row_with_right_anchor() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(5))) // left-anchored
        .column(Column::new(Width::Fixed(5)).anchor_right()) // right-anchored
        .separator(" ")
        .build();

    let formatter = TabularFormatter::new(&spec, 30);

    let output = formatter.format_row(&["L", "R"]);
    assert_eq!(display_width(&output), 30);
    assert!(output.starts_with("L    "));
    assert!(output.ends_with("R    "));
}

#[test]
fn format_row_with_right_anchor_exact_fit() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(10)))
        .column(Column::new(Width::Fixed(10)).anchor_right())
        .separator("  ")
        .build();

    let formatter = TabularFormatter::new(&spec, 22);

    let output = formatter.format_row(&["Left", "Right"]);
    assert_eq!(display_width(&output), 22);
    assert!(output.contains("  ")); // Original separator preserved
}

#[test]
fn format_row_all_right_anchor_no_gap() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(5)).anchor_right())
        .column(Column::new(Width::Fixed(5)).anchor_right())
        .separator(" ")
        .build();
    let formatter = TabularFormatter::new(&spec, 50);

    let output = formatter.format_row(&["A", "B"]);
    assert_eq!(output, "A     B    ");
}

#[test]
fn format_row_multiple_anchors() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(4))) // L1
        .column(Column::new(Width::Fixed(4))) // L2
        .column(Column::new(Width::Fixed(4)).anchor_right()) // R1
        .column(Column::new(Width::Fixed(4)).anchor_right()) // R2
        .separator(" ")
        .build();

    let formatter = TabularFormatter::new(&spec, 40);

    let output = formatter.format_row(&["A", "B", "C", "D"]);
    assert_eq!(display_width(&output), 40);
    assert!(output.starts_with("A    B   "));
}

#[test]
fn calculate_anchor_gap_no_transition() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(10)))
        .column(Column::new(Width::Fixed(10)))
        .build();
    let formatter = TabularFormatter::new(&spec, 50);

    let (gap, transition) = formatter.calculate_anchor_gap();
    assert_eq!(transition, 2); // No right-anchored columns
    assert_eq!(gap, 0);
}

#[test]
fn calculate_anchor_gap_with_transition() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(10)))
        .column(Column::new(Width::Fixed(10)).anchor_right())
        .separator(" ")
        .build();
    let formatter = TabularFormatter::new(&spec, 50);

    let (gap, transition) = formatter.calculate_anchor_gap();
    assert_eq!(transition, 1); // Second column is right-anchored
    assert!(gap > 0);
}

#[test]
fn format_row_lines_with_anchor() {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Fixed(8)).wrap())
        .column(Column::new(Width::Fixed(6)).anchor_right())
        .separator(" ")
        .build();
    let formatter = TabularFormatter::new(&spec, 40);

    let lines = formatter.format_row_lines(&["This is text", "Right"]);

    for line in &lines {
        assert_eq!(display_width(line), 40);
    }
}
