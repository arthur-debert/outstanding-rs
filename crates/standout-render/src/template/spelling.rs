//! How standout spells minijinja values as text.
//!
//! minijinja renders booleans and none with Jinja2's Python spellings — `True`,
//! `False`, `None`. Standout renders `true`, `false`, and `none`. Two seams
//! keep that true everywhere: [`new_environment`] installs a formatter that
//! normalizes top-level interpolation, and [`stringify()`] replaces
//! `Value::to_string()` wherever standout itself turns a value into text
//! (filters, table cells, borders) — a formatter cannot reach those, since they
//! go through `Display for Value` directly.
//!
//! Known limitation: the `~` concatenation operator formats its operands inside
//! minijinja's evaluator, which exposes no hook, so `{{ "x" ~ flag }}` still
//! yields `xTrue`. Use `{{ "x" }}{{ flag }}` or `{{ "x" ~ flag|string }}`.

use std::borrow::Cow;

use minijinja::value::{Kwargs, Rest, ValueKind};
use minijinja::{
    AutoEscape, Environment, Error, ErrorKind, Output, State, UndefinedBehavior, Value,
};

// The only sanctioned constructor for a rendering environment inside
// standout; `tests/environment_construction.rs` fails if any crate's `src/`
// calls `minijinja::Environment::new()` directly.
pub fn new_environment() -> Environment<'static> {
    let mut env = Environment::new();
    install(&mut env);
    env
}

pub(crate) fn install(env: &mut Environment<'static>) {
    env.set_formatter(spelling_formatter);
    env.set_auto_escape_callback(|_| AutoEscape::Custom("standout"));
    env.add_filter("__standout_capture", super::presentation::capture);
    env.add_function(
        "__standout_call",
        |state: &State, value: Value, args: minijinja::value::Rest<Value>| {
            value.call(state, &args).map(super::presentation::capture)
        },
    );
    env.add_filter(
        "__standout_plain_if_formatted",
        super::presentation::plain_if_formatted,
    );
    env.add_filter(
        "__standout_plain_for_comparison",
        super::presentation::plain_for_comparison,
    );
    env.add_filter("attr", |value: Value, key: Value| {
        minijinja::filters::attr(
            &super::presentation::plain_if_formatted(value),
            &super::presentation::plain_if_formatted(key),
        )
    });
    install_comparison_tests(env);
    env.add_filter("map", map_filter);
    env.add_filter("sort", |state: &State, value: Value, kwargs: Kwargs| {
        minijinja::filters::sort(state, value, project_attribute(kwargs)?)
    });
    env.add_filter("unique", |state: &State, value: Value, kwargs: Kwargs| {
        minijinja::filters::unique(state, value, project_attribute(kwargs)?)
    });
    env.add_filter(
        "groupby",
        |value: Value, attribute: Option<Value>, kwargs: Kwargs| {
            let attribute = attribute.map(super::presentation::plain_if_formatted);
            let attribute = attribute
                .as_ref()
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        Error::new(ErrorKind::InvalidOperation, "value is not a string")
                    })
                })
                .transpose()?;
            minijinja::filters::groupby(value, attribute, project_attribute(kwargs)?)
        },
    );
    for name in ["safe", "escape", "e"] {
        env.add_filter(name, |value: Value| value);
    }
    env.add_filter("replace", |value: Value, from: String, to: String| {
        stringify(&value).replace(&from, &to)
    });
    env.add_filter("tojson", |value: Value| -> Result<String, Error> {
        serde_json::to_string(&value)
            .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))
    });
    env.add_filter("string", string_filter);
    env.add_filter("join", join_filter);
}

fn install_comparison_tests(env: &mut Environment<'static>) {
    use super::presentation::plain_for_comparison;
    use minijinja::tests;
    for (names, test) in [
        (
            &["eq", "equalto", "=="][..],
            tests::is_eq as fn(&Value, &Value) -> bool,
        ),
        (&["ne", "!="][..], tests::is_ne),
        (&["lt", "lessthan", "<"][..], tests::is_lt),
        (&["le", "<="][..], tests::is_le),
        (&["gt", "greaterthan", ">"][..], tests::is_gt),
        (&["ge", ">="][..], tests::is_ge),
    ] {
        for name in names {
            env.add_test(*name, move |value: Value, other: Value| {
                test(&plain_for_comparison(value), &plain_for_comparison(other))
            });
        }
    }
    env.add_test("in", |state: &State, value: Value, other: Value| {
        tests::is_in(
            state,
            &plain_for_comparison(value),
            &plain_for_comparison(other),
        )
    });
}

fn project_attribute(kwargs: Kwargs) -> Result<Kwargs, Error> {
    kwargs
        .args()
        .map(|name| {
            let value = kwargs.get::<Value>(name)?;
            Ok((
                name.to_owned(),
                if name == "attribute" {
                    super::presentation::plain_if_formatted(value)
                } else {
                    value
                },
            ))
        })
        .collect()
}

fn map_filter(state: &State, value: Value, args: Rest<Value>) -> Result<Vec<Value>, Error> {
    let (args, kwargs): (&[Value], Kwargs) = minijinja::value::from_args(&args)?;
    let mut args = args.to_vec();
    if let Some(name) = args.first_mut() {
        *name = super::presentation::plain_if_formatted(name.clone());
    }
    args.push(project_attribute(kwargs)?.into());
    minijinja::filters::map(state, value, Rest(args))
}

fn string_filter(state: &State, value: Value) -> Result<Value, Error> {
    if value.is_undefined()
        && matches!(
            state.undefined_behavior(),
            UndefinedBehavior::Strict | UndefinedBehavior::SemiStrict
        )
    {
        return Err(Error::from(ErrorKind::UndefinedError));
    }
    Ok(Value::from(stringify(&value).into_owned()))
}

fn join_filter(_state: &State, value: Value, joiner: Option<Value>) -> Result<Value, Error> {
    let separator = joiner
        .as_ref()
        .map(super::presentation::markup)
        .unwrap_or_default();
    let mut output = String::new();
    for (index, item) in value.try_iter()?.enumerate() {
        if index > 0 {
            output.push_str(&separator);
        }
        output.push_str(&super::presentation::markup(&item));
    }
    Ok(super::presentation::fragment(output))
}

pub fn stringify(value: &Value) -> Cow<'_, str> {
    match value.kind() {
        ValueKind::Bool => Cow::Borrowed(bool_str(value)),
        ValueKind::None => Cow::Borrowed(NONE),
        ValueKind::String => match value.as_str() {
            Some(text) => Cow::Borrowed(text),
            None => Cow::Owned(value.to_string()),
        },
        ValueKind::Seq | ValueKind::Map | ValueKind::Iterable => match container(value) {
            Some(text) => Cow::Owned(text),
            None => Cow::Owned(value.to_string()),
        },
        _ => Cow::Owned(value.to_string()),
    }
}

const NONE: &str = "none";

fn bool_str(value: &Value) -> &'static str {
    if value.is_true() {
        "true"
    } else {
        "false"
    }
}

fn spelling_formatter(out: &mut Output, _state: &State, value: &Value) -> Result<(), Error> {
    out.write_str(&super::presentation::markup(value))?;
    Ok(())
}

// minijinja renders container elements with Debug, which quotes strings, so
// this in-container form of `stringify` does too.
fn repr(value: &Value) -> Cow<'_, str> {
    let projected = super::presentation::plain_if_formatted(value.clone());
    if projected.kind() == ValueKind::String && value.kind() != ValueKind::String {
        return Cow::Owned(format!("{:?}", projected.as_str().unwrap()));
    }
    match value.kind() {
        ValueKind::Bool => Cow::Borrowed(bool_str(value)),
        ValueKind::None => Cow::Borrowed(NONE),
        ValueKind::Seq | ValueKind::Map | ValueKind::Iterable => match container(value) {
            Some(text) => Cow::Owned(text),
            None => Cow::Owned(format!("{value:?}")),
        },
        _ => Cow::Owned(format!("{value:?}")),
    }
}

fn container(value: &Value) -> Option<String> {
    let mut out = String::new();
    if value.kind() == ValueKind::Map {
        out.push('{');
        for (index, key) in value.try_iter().ok()?.enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&repr(&key));
            out.push_str(": ");
            out.push_str(&repr(&value.get_item(&key).unwrap_or_default()));
        }
        out.push('}');
    } else {
        value.len()?;
        out.push('[');
        for (index, item) in value.try_iter().ok()?.enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&repr(&item));
        }
        out.push(']');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn scalars_use_standout_spelling() {
        assert_eq!(stringify(&Value::from(true)), "true");
        assert_eq!(stringify(&Value::from(false)), "false");
        assert_eq!(stringify(&Value::from(())), "none");
    }

    #[test]
    fn other_scalars_keep_minijinja_formatting() {
        for value in [Value::from(42), Value::from(1.5), Value::from("True")] {
            assert_eq!(stringify(&value), value.to_string());
        }
        assert_eq!(stringify(&Value::UNDEFINED), "");
    }

    #[test]
    fn containers_normalize_their_elements() {
        let seq = Value::from(vec![Value::from(true), Value::from(false), Value::from(())]);
        assert_eq!(stringify(&seq), "[true, false, none]");

        let mut map = BTreeMap::new();
        map.insert("on", Value::from(true));
        map.insert("off", Value::from(false));
        assert_eq!(
            stringify(&Value::from(map)),
            r#"{"off": false, "on": true}"#
        );
    }

    #[test]
    fn containers_keep_minijinja_shape_for_everything_else() {
        let seq = Value::from(vec![Value::from("a"), Value::from(1)]);
        assert_eq!(stringify(&seq), seq.to_string());
    }

    #[test]
    fn nesting_normalizes_at_every_depth() {
        let inner = Value::from(vec![Value::from(true)]);
        let mut map = BTreeMap::new();
        map.insert("flags", inner);
        let outer = Value::from(vec![Value::from(map)]);
        assert_eq!(stringify(&outer), r#"[{"flags": [true]}]"#);
    }
}
