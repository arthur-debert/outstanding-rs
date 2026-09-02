//! [`OutputMode`] controls how rendering behaves, from terminal colors to
//! structured serialization (JSON/YAML/XML/CSV/NDJSON, which skip template
//! rendering entirely). `Ndjson` is the one stream mode: stdout is a sequence
//! of one-line JSON entries rather than a single document. `Auto` is resolved
//! from the request ([`crate::ColorPolicy::Auto`] plus stdout capability on
//! [`crate::TargetProperties`]) by callers at the edge — this module never
//! probes the process itself while applying styles.

use std::io::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDestination {
    Stdout,
    File(std::path::PathBuf),
}

fn validate_path(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Parent directory does not exist: {}", parent.display()),
            ));
        }
    }
    Ok(())
}

pub fn write_output(content: &str, dest: &OutputDestination) -> std::io::Result<()> {
    match dest {
        OutputDestination::Stdout => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            writeln!(handle, "{}", content)
        }
        OutputDestination::File(path) => {
            validate_path(path)?;
            std::fs::write(path, content)
        }
    }
}

pub fn write_binary_output(content: &[u8], dest: &OutputDestination) -> std::io::Result<()> {
    match dest {
        OutputDestination::Stdout => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content)
        }
        OutputDestination::File(path) => {
            validate_path(path)?;
            std::fs::write(path, content)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    #[default]
    Auto,
    Term,
    Text,
    TermDebug,
    Json,
    Yaml,
    Xml,
    Csv,
    Ndjson,
}

impl OutputMode {
    pub fn should_use_color(&self) -> bool {
        matches!(self, OutputMode::Term)
    }

    pub fn is_debug(&self) -> bool {
        matches!(self, OutputMode::TermDebug)
    }

    pub fn is_structured(&self) -> bool {
        matches!(
            self,
            OutputMode::Json
                | OutputMode::Yaml
                | OutputMode::Xml
                | OutputMode::Csv
                | OutputMode::Ndjson
        )
    }

    /// Whether stdout is a stream of one-line entries rather than one
    /// document: true only for `Ndjson`.
    pub fn is_stream(&self) -> bool {
        matches!(self, OutputMode::Ndjson)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_term_should_use_color() {
        assert!(OutputMode::Term.should_use_color());
    }

    #[test]
    fn test_output_mode_text_should_not_use_color() {
        assert!(!OutputMode::Text.should_use_color());
    }

    #[test]
    fn test_output_mode_default_is_auto() {
        assert_eq!(OutputMode::default(), OutputMode::Auto);
    }

    #[test]
    fn test_output_mode_term_debug_is_debug() {
        assert!(OutputMode::TermDebug.is_debug());
        assert!(!OutputMode::Auto.is_debug());
        assert!(!OutputMode::Term.is_debug());
        assert!(!OutputMode::Text.is_debug());
        assert!(!OutputMode::Json.is_debug());
    }

    #[test]
    fn test_output_mode_term_debug_should_not_use_color() {
        assert!(!OutputMode::TermDebug.should_use_color());
    }

    #[test]
    fn test_output_mode_json_should_not_use_color() {
        assert!(!OutputMode::Json.should_use_color());
    }

    #[test]
    fn test_output_mode_json_is_structured() {
        assert!(OutputMode::Json.is_structured());
    }

    #[test]
    fn ndjson_is_the_one_structured_stream_mode() {
        assert!(OutputMode::Ndjson.is_structured());
        assert!(OutputMode::Ndjson.is_stream());
        assert!(!OutputMode::Ndjson.should_use_color());
        assert!(!OutputMode::Ndjson.is_debug());
        for mode in [
            OutputMode::Auto,
            OutputMode::Term,
            OutputMode::Text,
            OutputMode::TermDebug,
            OutputMode::Json,
            OutputMode::Yaml,
            OutputMode::Xml,
            OutputMode::Csv,
        ] {
            assert!(!mode.is_stream(), "{mode:?}");
        }
    }

    #[test]
    fn test_output_mode_non_json_not_structured() {
        assert!(!OutputMode::Auto.is_structured());
        assert!(!OutputMode::Term.is_structured());
        assert!(!OutputMode::Text.is_structured());
        assert!(!OutputMode::TermDebug.is_structured());
    }

    #[test]
    fn test_output_mode_json_not_debug() {
        assert!(!OutputMode::Json.is_debug());
    }

    #[test]
    fn test_write_output_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let dest = OutputDestination::File(file_path.clone());

        write_output("hello", &dest).unwrap();

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_write_output_file_overwrite() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        std::fs::write(&file_path, "initial").unwrap();

        let dest = OutputDestination::File(file_path.clone());
        write_output("new", &dest).unwrap();

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "new");
    }

    #[test]
    fn test_write_output_binary_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.bin");
        let dest = OutputDestination::File(file_path.clone());

        write_binary_output(&[1, 2, 3], &dest).unwrap();

        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content, vec![1, 2, 3]);
    }

    #[test]
    fn test_write_output_invalid_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("missing").join("output.txt");
        let dest = OutputDestination::File(file_path);

        let result = write_output("hello", &dest);
        assert!(result.is_err());
    }
}
