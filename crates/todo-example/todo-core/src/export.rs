use crate::{Todo, TodoFilter};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvExport {
    pub csv: Vec<u8>,
    pub suggested_filename: String,
    pub exported: usize,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportWarning {
    CompletedOmitted { count: usize },
    TitleFlattened { id: u32 },
}

pub(crate) fn export_csv(
    todos: &[Todo],
    filter: TodoFilter,
    omitted_completed: usize,
) -> CsvExport {
    let mut csv = String::from("id,title,done\n");
    let mut warnings = Vec::new();

    for todo in todos {
        let flattened = todo.title.replace(['\n', '\r'], " ");
        if flattened != todo.title {
            warnings.push(ExportWarning::TitleFlattened { id: todo.id });
        }
        csv.push_str(&format!(
            "{},{},{}\n",
            todo.id,
            escape_field(&flattened),
            todo.done
        ));
    }

    if filter == TodoFilter::Pending && omitted_completed > 0 {
        warnings.push(ExportWarning::CompletedOmitted {
            count: omitted_completed,
        });
    }

    CsvExport {
        csv: csv.into_bytes(),
        suggested_filename: "todos.csv".to_string(),
        exported: todos.len(),
        warnings,
    }
}

fn escape_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: u32, title: &str, done: bool) -> Todo {
        Todo {
            id,
            title: title.to_string(),
            done,
        }
    }

    #[test]
    fn empty_export_is_a_header_row_with_no_warnings() {
        let export = export_csv(&[], TodoFilter::All, 0);
        assert_eq!(export.csv, b"id,title,done\n");
        assert_eq!(export.exported, 0);
        assert!(export.warnings.is_empty());
        assert_eq!(export.suggested_filename, "todos.csv");
    }

    #[test]
    fn rows_carry_id_title_and_state() {
        let export = export_csv(
            &[todo(1, "buy milk", false), todo(2, "ship it", true)],
            TodoFilter::All,
            0,
        );
        assert_eq!(
            String::from_utf8(export.csv).unwrap(),
            "id,title,done\n1,buy milk,false\n2,ship it,true\n"
        );
        assert_eq!(export.exported, 2);
    }

    #[test]
    fn separators_and_quotes_are_escaped() {
        let export = export_csv(
            &[todo(1, "milk, eggs", false), todo(2, "say \"hi\"", false)],
            TodoFilter::All,
            0,
        );
        assert_eq!(
            String::from_utf8(export.csv).unwrap(),
            "id,title,done\n1,\"milk, eggs\",false\n2,\"say \"\"hi\"\"\",false\n"
        );
        assert!(export.warnings.is_empty(), "escaping loses nothing");
    }

    #[test]
    fn a_flattened_title_is_a_warning_not_a_silent_rewrite() {
        let export = export_csv(&[todo(7, "two\nlines", false)], TodoFilter::All, 0);
        assert_eq!(
            String::from_utf8(export.csv).unwrap(),
            "id,title,done\n7,two lines,false\n"
        );
        assert_eq!(
            export.warnings,
            vec![ExportWarning::TitleFlattened { id: 7 }]
        );
    }

    #[test]
    fn omitted_completed_todos_are_reported_only_for_the_pending_filter() {
        let pending = export_csv(&[todo(1, "buy milk", false)], TodoFilter::Pending, 2);
        assert_eq!(
            pending.warnings,
            vec![ExportWarning::CompletedOmitted { count: 2 }]
        );

        let all = export_csv(&[todo(1, "buy milk", false)], TodoFilter::All, 2);
        assert!(all.warnings.is_empty(), "nothing is omitted from --all");
    }

    #[test]
    fn warnings_serialize_as_typed_facts() {
        let value = serde_json::to_value(ExportWarning::CompletedOmitted { count: 2 }).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"kind": "completed_omitted", "count": 2})
        );
    }
}
