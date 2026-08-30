//! MiniJinja filter registration.

use minijinja::{Environment, Error, ErrorKind, Value};

pub fn register_filters(env: &mut Environment<'static>) {
    register_filters_with_policy(env, crate::AmbiguousWidth::Narrow);
}

pub fn register_filters_with_policy(env: &mut Environment<'static>, policy: crate::AmbiguousWidth) {
    crate::template::spelling::install(env);

    env.add_filter("nl", |value: Value| -> String {
        format!("{}\n", crate::template::spelling::stringify(&value))
    });

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

    crate::tabular::filters::register_tabular_filters_with_policy(env, policy);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deprecated_style_filter_gives_helpful_error() {
        let mut env = crate::template::new_environment();
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

    #[test]
    fn policy_aware_registration_reaches_width_filters() {
        let mut env = crate::template::new_environment();
        register_filters_with_policy(&mut env, crate::AmbiguousWidth::Wide);
        assert_eq!(
            env.render_str("{{ '↦≈Δ' | display_width }}", ()).unwrap(),
            "5"
        );
    }
}
