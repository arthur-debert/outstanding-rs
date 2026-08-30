//! [`SimpleEngine`]: format-string style `{variable}` substitution, with
//! `{a.b.0}` dotted/indexed path access and `{{`/`}}` as escaped braces.
//! Deliberately has no loops, conditionals, filters, includes, or macros —
//! use [`MiniJinjaEngine`](super::MiniJinjaEngine) when those are needed.

use std::collections::HashMap;

use crate::error::RenderError;

use super::TemplateEngine;

pub struct SimpleEngine {
    templates: HashMap<String, String>,
}

impl SimpleEngine {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    fn resolve_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
        let mut current = value;

        for part in path.split('.') {
            current = match current {
                serde_json::Value::Object(map) => map.get(part)?,
                serde_json::Value::Array(arr) => {
                    let index: usize = part.parse().ok()?;
                    arr.get(index)?
                }
                _ => return None,
            };
        }

        Some(current)
    }

    fn format_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => String::new(),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => value.to_string(),
        }
    }

    fn render_impl(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: Option<&HashMap<String, serde_json::Value>>,
    ) -> Result<String, RenderError> {
        let mut result = String::with_capacity(template.len());
        let mut chars = template.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    result.push('{');
                } else {
                    let mut var_name = String::new();
                    let mut found_close = false;

                    for inner_ch in chars.by_ref() {
                        if inner_ch == '}' {
                            found_close = true;
                            break;
                        }
                        var_name.push(inner_ch);
                    }

                    if !found_close {
                        return Err(RenderError::TemplateError(format!(
                            "Unclosed variable substitution: {{{}",
                            var_name
                        )));
                    }

                    let var_name = var_name.trim();

                    if var_name.is_empty() {
                        return Err(RenderError::TemplateError(
                            "Empty variable name in template".to_string(),
                        ));
                    }

                    let value = if let Some(ctx) = context {
                        if !var_name.contains('.') {
                            if let Some(ctx_val) = ctx.get(var_name) {
                                Some(ctx_val)
                            } else {
                                Self::resolve_path(data, var_name)
                            }
                        } else {
                            let first_segment = var_name.split('.').next().unwrap_or(var_name);
                            if let Some(ctx_val) = ctx.get(first_segment) {
                                let rest = &var_name[first_segment.len()..];
                                if rest.is_empty() {
                                    Some(ctx_val)
                                } else {
                                    Self::resolve_path(ctx_val, &rest[1..])
                                }
                            } else {
                                Self::resolve_path(data, var_name)
                            }
                        }
                    } else {
                        Self::resolve_path(data, var_name)
                    };

                    match value {
                        Some(v) => result.push_str(&Self::format_value(v)),
                        None => {
                            result.push_str(&format!("{{{}}}", var_name));
                        }
                    }
                }
            } else if ch == '}' {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    result.push('}');
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }
}

impl Default for SimpleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine for SimpleEngine {
    fn render_template(
        &self,
        template: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError> {
        self.render_impl(template, data, None)
    }

    fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
        self.templates.insert(name.to_string(), source.to_string());
        Ok(())
    }

    fn render_named(&self, name: &str, data: &serde_json::Value) -> Result<String, RenderError> {
        let template = self
            .templates
            .get(name)
            .ok_or_else(|| RenderError::TemplateNotFound(name.to_string()))?;
        self.render_impl(template, data, None)
    }

    fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    fn render_with_context(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
    ) -> Result<String, RenderError> {
        self.render_impl(template, data, Some(&context))
    }

    fn supports_includes(&self) -> bool {
        false
    }

    fn supports_filters(&self) -> bool {
        false
    }

    fn supports_control_flow(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_substitution() {
        let engine = SimpleEngine::new();
        let data = json!({"name": "World"});

        let output = engine.render_template("Hello, {name}!", &data).unwrap();
        assert_eq!(output, "Hello, World!");
    }

    #[test]
    fn test_multiple_variables() {
        let engine = SimpleEngine::new();
        let data = json!({"first": "John", "last": "Doe"});

        let output = engine.render_template("{first} {last}", &data).unwrap();
        assert_eq!(output, "John Doe");
    }

    #[test]
    fn test_nested_access() {
        let engine = SimpleEngine::new();
        let data = json!({
            "user": {
                "name": "Alice",
                "profile": {
                    "email": "alice@example.com"
                }
            }
        });

        let output = engine
            .render_template("Name: {user.name}, Email: {user.profile.email}", &data)
            .unwrap();
        assert_eq!(output, "Name: Alice, Email: alice@example.com");
    }

    #[test]
    fn test_array_index() {
        let engine = SimpleEngine::new();
        let data = json!({
            "items": ["first", "second", "third"]
        });

        let output = engine
            .render_template("First: {items.0}, Third: {items.2}", &data)
            .unwrap();
        assert_eq!(output, "First: first, Third: third");
    }

    #[test]
    fn test_array_object_access() {
        let engine = SimpleEngine::new();
        let data = json!({
            "users": [
                {"name": "Alice"},
                {"name": "Bob"}
            ]
        });

        let output = engine
            .render_template("{users.0.name} and {users.1.name}", &data)
            .unwrap();
        assert_eq!(output, "Alice and Bob");
    }

    #[test]
    fn test_number_values() {
        let engine = SimpleEngine::new();
        let data = json!({"count": 42, "price": 19.99});

        let output = engine
            .render_template("Count: {count}, Price: {price}", &data)
            .unwrap();
        assert_eq!(output, "Count: 42, Price: 19.99");
    }

    #[test]
    fn test_boolean_values() {
        let engine = SimpleEngine::new();
        let data = json!({"active": true, "deleted": false});

        let output = engine
            .render_template("Active: {active}, Deleted: {deleted}", &data)
            .unwrap();
        assert_eq!(output, "Active: true, Deleted: false");
    }

    #[test]
    fn test_null_value() {
        let engine = SimpleEngine::new();
        let data = json!({"value": null});

        let output = engine.render_template("Value: {value}", &data).unwrap();
        assert_eq!(output, "Value: ");
    }

    #[test]
    fn test_escaped_braces() {
        let engine = SimpleEngine::new();
        let data = json!({"name": "test"});

        let output = engine
            .render_template("Use {{name}} for {name}", &data)
            .unwrap();
        assert_eq!(output, "Use {name} for test");
    }

    #[test]
    fn test_escaped_closing_brace() {
        let engine = SimpleEngine::new();
        let data = json!({});

        let output = engine
            .render_template("JSON: {{\"key\": \"value\"}}", &data)
            .unwrap();
        assert_eq!(output, "JSON: {\"key\": \"value\"}");
    }

    #[test]
    fn test_missing_variable() {
        let engine = SimpleEngine::new();
        let data = json!({"name": "test"});

        let output = engine.render_template("Hello {missing}!", &data).unwrap();
        assert_eq!(output, "Hello {missing}!");
    }

    #[test]
    fn test_unclosed_variable() {
        let engine = SimpleEngine::new();
        let data = json!({});

        let result = engine.render_template("Hello {name", &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unclosed"));
    }

    #[test]
    fn test_empty_variable_name() {
        let engine = SimpleEngine::new();
        let data = json!({});

        let result = engine.render_template("Hello {}!", &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty variable"));
    }

    #[test]
    fn test_whitespace_in_variable() {
        let engine = SimpleEngine::new();
        let data = json!({"name": "World"});

        let output = engine.render_template("Hello { name }!", &data).unwrap();
        assert_eq!(output, "Hello World!");
    }

    #[test]
    fn test_named_template() {
        let mut engine = SimpleEngine::new();
        engine.add_template("greeting", "Hello, {name}!").unwrap();

        let data = json!({"name": "World"});
        let output = engine.render_named("greeting", &data).unwrap();
        assert_eq!(output, "Hello, World!");
    }

    #[test]
    fn test_named_template_not_found() {
        let engine = SimpleEngine::new();
        let data = json!({});

        let result = engine.render_named("missing", &data);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RenderError::TemplateNotFound(_)
        ));
    }

    #[test]
    fn test_has_template() {
        let mut engine = SimpleEngine::new();
        assert!(!engine.has_template("test"));

        engine.add_template("test", "content").unwrap();
        assert!(engine.has_template("test"));
    }

    #[test]
    fn test_with_context() {
        let engine = SimpleEngine::new();
        let data = json!({"name": "Alice"});
        let mut context = HashMap::new();
        context.insert("version".to_string(), json!("1.0.0"));

        let output = engine
            .render_with_context("{name} v{version}", &data, context)
            .unwrap();
        assert_eq!(output, "Alice v1.0.0");
    }

    #[test]
    fn test_context_data_precedence() {
        let engine = SimpleEngine::new();
        let data = json!({"value": "from_data"});
        let mut context = HashMap::new();
        context.insert("value".to_string(), json!("from_context"));

        let output = engine
            .render_with_context("{value}", &data, context)
            .unwrap();
        assert_eq!(output, "from_context");
    }

    #[test]
    fn test_supports_flags() {
        let engine = SimpleEngine::new();
        assert!(!engine.supports_includes());
        assert!(!engine.supports_filters());
        assert!(!engine.supports_control_flow());
    }

    #[test]
    fn test_no_template_logic() {
        let engine = SimpleEngine::new();
        let data = json!({"items": [1, 2, 3]});

        let output = engine
            .render_template("{% for i in items %}{{i}}{% endfor %}", &data)
            .unwrap();
        assert_eq!(output, "{% for i in items %}{i}{% endfor %}");
    }

    #[test]
    fn test_plain_text() {
        let engine = SimpleEngine::new();
        let data = json!({});

        let output = engine
            .render_template("Just plain text, no variables", &data)
            .unwrap();
        assert_eq!(output, "Just plain text, no variables");
    }

    #[test]
    fn test_complex_json_value() {
        let engine = SimpleEngine::new();
        let data = json!({
            "obj": {"a": 1, "b": 2},
            "arr": [1, 2, 3]
        });

        let output = engine.render_template("Obj: {obj}", &data).unwrap();
        assert!(output.contains("\"a\":1") || output.contains("\"a\": 1"));
    }
}
