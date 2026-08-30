use std::fmt;

#[derive(Debug)]
pub enum RenderError {
    TemplateError(String),
    TemplateNotFound(String),
    SerializationError(String),
    StyleError(String),
    IoError(std::io::Error),
    OperationError(String),
    ContextError(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::TemplateError(msg) => write!(f, "template error: {}", msg),
            RenderError::TemplateNotFound(name) => write!(f, "template not found: {}", name),
            RenderError::SerializationError(msg) => write!(f, "serialization error: {}", msg),
            RenderError::StyleError(msg) => write!(f, "style error: {}", msg),
            RenderError::IoError(err) => write!(f, "I/O error: {}", err),
            RenderError::OperationError(msg) => write!(f, "{}", msg),
            RenderError::ContextError(msg) => write!(f, "context error: {}", msg),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenderError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(err: std::io::Error) -> Self {
        RenderError::IoError(err)
    }
}

impl From<serde_json::Error> for RenderError {
    fn from(err: serde_json::Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<serde_yaml::Error> for RenderError {
    fn from(err: serde_yaml::Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<quick_xml::DeError> for RenderError {
    fn from(err: quick_xml::DeError) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<csv::Error> for RenderError {
    fn from(err: csv::Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<csv::IntoInnerError<csv::Writer<Vec<u8>>>> for RenderError {
    fn from(err: csv::IntoInnerError<csv::Writer<Vec<u8>>>) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for RenderError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        RenderError::SerializationError(err.to_string())
    }
}

impl From<minijinja::Error> for RenderError {
    fn from(err: minijinja::Error) -> Self {
        use minijinja::ErrorKind;

        match err.kind() {
            ErrorKind::TemplateNotFound => RenderError::TemplateNotFound(err.to_string()),
            ErrorKind::SyntaxError
            | ErrorKind::BadEscape
            | ErrorKind::UndefinedError
            | ErrorKind::UnknownTest
            | ErrorKind::UnknownFunction
            | ErrorKind::UnknownFilter
            | ErrorKind::UnknownMethod => RenderError::TemplateError(err.to_string()),
            ErrorKind::BadSerialization => RenderError::SerializationError(err.to_string()),
            _ => RenderError::OperationError(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn test_display_template_not_found() {
        let err = RenderError::TemplateNotFound("foo".to_string());
        assert_eq!(err.to_string(), "template not found: foo");
    }

    #[test]
    fn test_display_template_error() {
        let err = RenderError::TemplateError("bad tag".to_string());
        assert_eq!(err.to_string(), "template error: bad tag");
    }

    #[test]
    fn test_display_serialization_error() {
        let err = RenderError::SerializationError("oops".to_string());
        assert_eq!(err.to_string(), "serialization error: oops");
    }

    #[test]
    fn test_display_style_error() {
        let err = RenderError::StyleError("alias cycle".to_string());
        assert_eq!(err.to_string(), "style error: alias cycle");
    }

    #[test]
    fn test_display_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let err = RenderError::IoError(io_err);
        let s = err.to_string();
        assert!(s.starts_with("I/O error: "), "got: {}", s);
        assert!(s.contains("nope"));
    }

    #[test]
    fn test_display_operation_error_has_no_prefix() {
        // Unlike the other variants, OperationError has no "<kind>: " prefix.
        let err = RenderError::OperationError("something operational".to_string());
        assert_eq!(err.to_string(), "something operational");
    }

    #[test]
    fn test_display_context_error() {
        let err = RenderError::ContextError("missing field".to_string());
        assert_eq!(err.to_string(), "context error: missing field");
    }

    #[test]
    fn test_source_returns_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = RenderError::IoError(io_err);
        let src = err.source();
        assert!(src.is_some(), "IoError should expose its source");
        assert!(src.unwrap().downcast_ref::<std::io::Error>().is_some());
    }

    #[test]
    fn test_source_is_none_for_string_variants() {
        for err in [
            RenderError::TemplateError("x".into()),
            RenderError::TemplateNotFound("x".into()),
            RenderError::SerializationError("x".into()),
            RenderError::StyleError("x".into()),
            RenderError::OperationError("x".into()),
            RenderError::ContextError("x".into()),
        ] {
            assert!(
                err.source().is_none(),
                "variant unexpectedly had a source: {:?}",
                err
            );
        }
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let render_err: RenderError = io_err.into();
        assert!(matches!(render_err, RenderError::IoError(_)));
    }

    #[test]
    fn test_from_serde_json_error() {
        let parse_err = serde_json::from_str::<serde_json::Value>("{not json").unwrap_err();
        let render_err: RenderError = parse_err.into();
        match render_err {
            RenderError::SerializationError(msg) => assert!(!msg.is_empty()),
            other => panic!("expected SerializationError, got {:?}", other),
        }
    }

    #[test]
    fn test_from_serde_yaml_error() {
        let parse_err = serde_yaml::from_str::<serde_yaml::Value>("a:\n\tb: 1").unwrap_err();
        let render_err: RenderError = parse_err.into();
        assert!(matches!(render_err, RenderError::SerializationError(_)));
    }

    #[test]
    fn test_from_quick_xml_de_error() {
        let parse_err = quick_xml::de::from_str::<serde_json::Value>("<unclosed").unwrap_err();
        let render_err: RenderError = parse_err.into();
        assert!(matches!(render_err, RenderError::SerializationError(_)));
    }

    #[test]
    fn test_from_csv_error() {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(false)
            .from_reader("a,b\n1,2,3\n".as_bytes());
        let parse_err = rdr
            .records()
            .find_map(|r| r.err())
            .expect("expected a csv::Error from mismatched row length");
        let render_err: RenderError = parse_err.into();
        assert!(matches!(render_err, RenderError::SerializationError(_)));
    }

    #[test]
    fn test_from_from_utf8_error() {
        let utf8_err = String::from_utf8(vec![0x80]).unwrap_err();
        let render_err: RenderError = utf8_err.into();
        assert!(matches!(render_err, RenderError::SerializationError(_)));
    }

    fn classify(kind: minijinja::ErrorKind) -> RenderError {
        let mj_err = minijinja::Error::new(kind, "x");
        mj_err.into()
    }

    #[test]
    fn test_from_minijinja_template_not_found() {
        assert!(matches!(
            classify(minijinja::ErrorKind::TemplateNotFound),
            RenderError::TemplateNotFound(_)
        ));
    }

    #[test]
    fn test_from_minijinja_template_kinds_map_to_template_error() {
        for kind in [
            minijinja::ErrorKind::SyntaxError,
            minijinja::ErrorKind::BadEscape,
            minijinja::ErrorKind::UndefinedError,
            minijinja::ErrorKind::UnknownTest,
            minijinja::ErrorKind::UnknownFunction,
            minijinja::ErrorKind::UnknownFilter,
            minijinja::ErrorKind::UnknownMethod,
        ] {
            assert!(
                matches!(classify(kind), RenderError::TemplateError(_)),
                "kind {:?} should map to TemplateError",
                kind,
            );
        }
    }

    #[test]
    fn test_from_minijinja_bad_serialization() {
        assert!(matches!(
            classify(minijinja::ErrorKind::BadSerialization),
            RenderError::SerializationError(_)
        ));
    }

    #[test]
    fn test_from_minijinja_default_arm_is_operation_error() {
        assert!(matches!(
            classify(minijinja::ErrorKind::InvalidOperation),
            RenderError::OperationError(_)
        ));
    }

    #[test]
    fn test_from_minijinja_preserves_message() {
        let mj_err =
            minijinja::Error::new(minijinja::ErrorKind::SyntaxError, "specific marker xyzzy");
        let render_err: RenderError = mj_err.into();
        assert!(
            render_err.to_string().contains("xyzzy"),
            "message should be preserved through conversion: got {}",
            render_err,
        );
    }
}
