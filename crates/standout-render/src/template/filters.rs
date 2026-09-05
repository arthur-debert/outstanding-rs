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

    env.add_filter("verbatim", |value: Value| -> String {
        crate::util::escape_style_tags(&crate::template::spelling::stringify(&value)).into_owned()
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
    use std::collections::HashMap;

    use standout_bbparser::{TagTransform, UnknownTagBehavior};

    use super::*;

    fn resolve(text: &str) -> String {
        crate::diagnostics::resolve_tags(
            text,
            HashMap::new(),
            TagTransform::Remove,
            UnknownTagBehavior::Strip,
        )
    }

    #[test]
    fn verbatim_renders_generated_markup_unchanged_and_claims_no_tag() {
        let mut env = crate::template::new_environment();
        register_filters(&mut env);
        let body = "[severity_map]\nnote = \"low\"\n";

        let escaped = env
            .render_str("{{ body | verbatim }}", minijinja::context! { body })
            .unwrap();
        let plain = env
            .render_str("{{ body }}", minijinja::context! { body })
            .unwrap();

        let _window = crate::diagnostics::begin_capture();
        assert_eq!(resolve(&escaped), body);
        assert!(
            crate::diagnostics::unresolved_in_current_window().is_empty(),
            "escaped generated text claims no style tag"
        );

        resolve(&plain);
        assert_eq!(
            crate::diagnostics::unresolved_in_current_window(),
            ["severity_map"],
            "the same text unescaped is read as a style tag"
        );
    }

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
