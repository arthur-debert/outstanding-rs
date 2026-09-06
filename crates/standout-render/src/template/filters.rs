use minijinja::Environment;

pub fn register_filters(env: &mut Environment<'static>) {
    super::engine::register_filters(env);
}

pub fn register_filters_with_policy(env: &mut Environment<'static>, policy: crate::AmbiguousWidth) {
    super::engine::register_filters_with_policy(env, policy);
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
    fn interpolation_claims_no_data_tags_and_verbatim_is_absent() {
        let mut env = crate::template::new_environment();
        register_filters(&mut env);
        let body = "[severity_map]\nnote = \"low\"\n";

        assert!(env
            .render_str("{{ body | verbatim }}", minijinja::context! { body })
            .is_err());
        let plain = env
            .render_str("{{ body }}", minijinja::context! { body })
            .unwrap();

        let _window = crate::diagnostics::begin_capture();
        assert_eq!(resolve(&plain), body);
        assert!(
            crate::diagnostics::unresolved_in_current_window().is_empty(),
            "escaped generated text claims no style tag"
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
            err_msg.contains("style_as") || err_msg.contains("[stylename]"),
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
