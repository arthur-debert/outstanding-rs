//! MiniJinja filter registration.

use minijinja::{Environment, Error, ErrorKind, Value};

/// Registers all built-in filters on a minijinja environment.
///
/// Styling is now handled by BBParser tags (e.g., `[title]text[/title]`) in a
/// second pass after MiniJinja rendering. This function registers utility filters
/// like `nl` and table formatting filters.
///
/// # Arguments
///
/// * `env` - The MiniJinja environment to register filters on
pub(crate) fn register_filters(env: &mut Environment<'static>) {
    // Filter to append a newline to the value, enabling explicit line break control.
    // Usage: {{ content | nl }} outputs content followed by \n
    //        {{ "" | nl }} outputs just \n (a blank line)
    env.add_filter("nl", |value: Value| -> String { format!("{}\n", value) });

    // Deprecated style filter - provide helpful migration message
    // The old style() filter was replaced with BBCode-style tags in Standout 1.0
    env.add_filter(
        "style",
        |_value: Value, _name: String| -> Result<String, Error> {
            Err(Error::new(
                ErrorKind::InvalidOperation,
                "The `style()` filter was removed in Standout 1.0. \
                 Use BBCode-style tags instead: `[name]text[/name]` \
                 Example: `{{ title | style('header') }}` → `[header]{{ title }}[/header]`",
            ))
        },
    );

    // Register tabular formatting filters (col, pad_left, pad_right, truncate_at, etc.)
    crate::tabular::filters::register_tabular_filters(env);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deprecated_style_filter_gives_helpful_error() {
        let mut env = Environment::new();
        register_filters(&mut env);

        env.add_template("test", "{{ value | style('header') }}")
            .unwrap();

        let result = env
            .get_template("test")
            .unwrap()
            .render(minijinja::context! {
                value => "hello"
            });

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();

        // Verify the error message is helpful
        assert!(
            err_msg.contains("style()"),
            "Error should mention the filter name"
        );
        assert!(
            err_msg.contains("BBCode") || err_msg.contains("[name]"),
            "Error should mention the replacement syntax"
        );
        assert!(
            err_msg.contains("1.0") || err_msg.contains("removed"),
            "Error should indicate this was a breaking change"
        );
    }
}
