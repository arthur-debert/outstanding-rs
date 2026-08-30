use crate::Op;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeekType {
    String,
    Number,
    Timestamp,
    Enum,
    Bool,
}
impl SeekType {
    pub fn default_operator(self) -> Op {
        match self {
            SeekType::String => Op::Eq,
            SeekType::Number => Op::Eq,
            SeekType::Timestamp => Op::Eq,
            SeekType::Enum => Op::Eq,
            SeekType::Bool => Op::Is,
        }
    }
    pub fn is_valid_operator(self, op: Op) -> bool {
        match self {
            SeekType::String => op.is_string_op(),
            SeekType::Number => op.is_number_op(),
            SeekType::Timestamp => op.is_timestamp_op(),
            SeekType::Enum => op.is_enum_op(),
            SeekType::Bool => op.is_bool_op(),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            SeekType::String => "string",
            SeekType::Number => "number",
            SeekType::Timestamp => "timestamp",
            SeekType::Enum => "enum",
            SeekType::Bool => "boolean",
        }
    }
}
impl std::fmt::Display for SeekType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
pub trait SeekerSchema {
    fn field_type(field: &str) -> Option<SeekType>;
    fn field_names() -> &'static [&'static str];
    fn resolve_enum_variant(_field: &str, _variant: &str) -> Option<u32> {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seek_type_default_operators() {
        assert_eq!(SeekType::String.default_operator(), Op::Eq);
        assert_eq!(SeekType::Number.default_operator(), Op::Eq);
        assert_eq!(SeekType::Timestamp.default_operator(), Op::Eq);
        assert_eq!(SeekType::Enum.default_operator(), Op::Eq);
        assert_eq!(SeekType::Bool.default_operator(), Op::Is);
    }
    #[test]
    fn seek_type_valid_operators() {
        assert!(SeekType::String.is_valid_operator(Op::Eq));
        assert!(SeekType::String.is_valid_operator(Op::Contains));
        assert!(SeekType::String.is_valid_operator(Op::Regex));
        assert!(!SeekType::String.is_valid_operator(Op::Gt));
        assert!(!SeekType::String.is_valid_operator(Op::Before));
        assert!(SeekType::Number.is_valid_operator(Op::Eq));
        assert!(SeekType::Number.is_valid_operator(Op::Gt));
        assert!(SeekType::Number.is_valid_operator(Op::Lte));
        assert!(!SeekType::Number.is_valid_operator(Op::Contains));
        assert!(!SeekType::Number.is_valid_operator(Op::Before));
        assert!(SeekType::Timestamp.is_valid_operator(Op::Eq));
        assert!(SeekType::Timestamp.is_valid_operator(Op::Before));
        assert!(SeekType::Timestamp.is_valid_operator(Op::After));
        assert!(SeekType::Timestamp.is_valid_operator(Op::Lt));
        assert!(!SeekType::Timestamp.is_valid_operator(Op::Contains));
        assert!(SeekType::Enum.is_valid_operator(Op::Eq));
        assert!(SeekType::Enum.is_valid_operator(Op::In));
        assert!(!SeekType::Enum.is_valid_operator(Op::Gt));
        assert!(!SeekType::Enum.is_valid_operator(Op::Contains));
        assert!(SeekType::Bool.is_valid_operator(Op::Eq));
        assert!(SeekType::Bool.is_valid_operator(Op::Is));
        assert!(!SeekType::Bool.is_valid_operator(Op::Gt));
        assert!(!SeekType::Bool.is_valid_operator(Op::Contains));
    }
    #[test]
    fn seek_type_display() {
        assert_eq!(SeekType::String.to_string(), "string");
        assert_eq!(SeekType::Number.to_string(), "number");
        assert_eq!(SeekType::Timestamp.to_string(), "timestamp");
        assert_eq!(SeekType::Enum.to_string(), "enum");
        assert_eq!(SeekType::Bool.to_string(), "boolean");
    }
    struct TestSchema;
    impl SeekerSchema for TestSchema {
        fn field_type(field: &str) -> Option<SeekType> {
            match field {
                "name" => Some(SeekType::String),
                "count" => Some(SeekType::Number),
                "status" => Some(SeekType::Enum),
                _ => None,
            }
        }
        fn field_names() -> &'static [&'static str] {
            &["name", "count", "status"]
        }
        fn resolve_enum_variant(field: &str, variant: &str) -> Option<u32> {
            if field == "status" {
                match variant {
                    "pending" => Some(0),
                    "active" => Some(1),
                    "done" => Some(2),
                    _ => None,
                }
            } else {
                None
            }
        }
    }
    #[test]
    fn seeker_schema_field_type() {
        assert_eq!(TestSchema::field_type("name"), Some(SeekType::String));
        assert_eq!(TestSchema::field_type("count"), Some(SeekType::Number));
        assert_eq!(TestSchema::field_type("status"), Some(SeekType::Enum));
        assert_eq!(TestSchema::field_type("unknown"), None);
    }
    #[test]
    fn seeker_schema_field_names() {
        assert_eq!(TestSchema::field_names(), &["name", "count", "status"]);
    }
    #[test]
    fn seeker_schema_enum_variant_resolution() {
        assert_eq!(
            TestSchema::resolve_enum_variant("status", "pending"),
            Some(0)
        );
        assert_eq!(
            TestSchema::resolve_enum_variant("status", "active"),
            Some(1)
        );
        assert_eq!(TestSchema::resolve_enum_variant("status", "done"), Some(2));
        assert_eq!(TestSchema::resolve_enum_variant("status", "unknown"), None);
        assert_eq!(TestSchema::resolve_enum_variant("name", "pending"), None);
    }
}
