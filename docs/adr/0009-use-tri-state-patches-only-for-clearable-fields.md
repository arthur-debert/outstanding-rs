# Use tri-state patches only for clearable fields

Generated patch types use `Option<T>` for non-clearable fields, representing unchanged versus set without permitting an invalid clear. Clearable fields use an explicit tri-state value for unchanged, set, or clear rather than `Option<Option<T>>`; applications may override the patch associated type when a Resource needs a different domain-specific model.
