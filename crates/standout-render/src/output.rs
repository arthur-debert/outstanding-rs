//! [`Representation`] is what a run puts on stdout: the human template, its
//! style-tag diagnostic view, or one of the structured encodings, which skip
//! template rendering entirely. `Ndjson` is the one representation whose
//! stdout is a sequence of one-line JSON entries rather than a single
//! document. [`StyleMode`] is the separate decision of whether rendered human
//! text carries escape sequences; [`crate::request`] resolves it from the
//! representation, the [`crate::ColorPolicy`] and stdout capability on
//! [`crate::TargetProperties`], and this module never probes the process
//! itself while applying styles.

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

/// Creates the redirect target with the same parent check as `write_output`.
pub fn open_output_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    validate_path(path)?;
    std::fs::File::create(path)
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

/// What the run produces on stdout. The human template has no `--output`
/// spelling; `term-debug` is the diagnostic view of its style tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Representation {
    #[default]
    Human,
    TermDebug,
    Json,
    Yaml,
    Csv,
    Ndjson,
}

impl Representation {
    pub fn is_human(&self) -> bool {
        matches!(self, Representation::Human | Representation::TermDebug)
    }

    pub fn is_debug(&self) -> bool {
        matches!(self, Representation::TermDebug)
    }

    pub fn is_structured(&self) -> bool {
        matches!(
            self,
            Representation::Json
                | Representation::Yaml
                | Representation::Csv
                | Representation::Ndjson
        )
    }

    /// True only for `Ndjson`, the one representation whose stdout is a stream of entries.
    pub fn is_stream(&self) -> bool {
        matches!(self, Representation::Ndjson)
    }
}

/// Whether rendered human text carries escape sequences. Resolved per run from
/// the representation, the color policy and the destination; a structured
/// encoding never reaches a style decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StyleMode {
    Ansi,
    #[default]
    Plain,
    Debug,
}

impl StyleMode {
    pub fn should_use_color(&self) -> bool {
        matches!(self, StyleMode::Ansi)
    }

    pub fn is_debug(&self) -> bool {
        matches!(self, StyleMode::Debug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_representation_is_the_human_template() {
        assert_eq!(Representation::default(), Representation::Human);
        assert!(Representation::Human.is_human());
        assert!(!Representation::Human.is_structured());
        assert!(!Representation::Human.is_debug());
    }

    #[test]
    fn term_debug_is_a_human_representation_with_a_debug_style() {
        assert!(Representation::TermDebug.is_human());
        assert!(Representation::TermDebug.is_debug());
        assert!(!Representation::TermDebug.is_structured());
        assert!(StyleMode::Debug.is_debug());
        assert!(!StyleMode::Debug.should_use_color());
    }

    #[test]
    fn ndjson_is_the_one_structured_stream_representation() {
        assert!(Representation::Ndjson.is_structured());
        assert!(Representation::Ndjson.is_stream());
        for representation in [
            Representation::Human,
            Representation::TermDebug,
            Representation::Json,
            Representation::Yaml,
            Representation::Csv,
        ] {
            assert!(!representation.is_stream(), "{representation:?}");
        }
    }

    #[test]
    fn only_the_ansi_style_mode_colors() {
        assert!(StyleMode::Ansi.should_use_color());
        assert!(!StyleMode::Plain.should_use_color());
        assert_eq!(StyleMode::default(), StyleMode::Plain);
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
