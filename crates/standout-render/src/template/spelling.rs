//! How standout spells minijinja values as text.
//!
//! minijinja renders booleans and none with Jinja2's Python spellings —
//! `True`, `False`, `None`. Standout renders `true`, `false`, and `none`.
//! Two seams keep that true everywhere: [`new_environment`] installs a
//! formatter that normalizes top-level interpolation, and [`stringify`]
//! replaces `Value::to_string()` wherever standout itself turns a value into
//! text (filters, table cells, borders) — a formatter cannot reach those,
//! since they go through `Display for Value` directly.
//!
//! Known limitation: the `~` concatenation operator formats its operands
//! inside minijinja's evaluator, which exposes no hook, so `{{ "x" ~ flag }}`
//! still yields `xTrue`. Use `{{ "x" }}{{ flag }}` or `{{ "x" ~ flag|string }}`.

use std::borrow::Cow;

use minijinja::value::ValueKind;
use minijinja::{
    escape_formatter, AutoEscape, Environment, Error, ErrorKind, Output, State, UndefinedBehavior,
    Value,
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
    env.add_filter("string", string_filter);
    env.add_filter("join", join_filter);
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
    Ok(match value.kind() {
        ValueKind::String => value,
        _ => Value::from(stringify(&value).into_owned()),
    })
}

fn join_filter(state: &State, value: Value, joiner: Option<Value>) -> Result<Value, Error> {
    let items = value
        .try_iter()
        .map_err(|err| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("cannot join value of type {}", value.kind()),
            )
            .with_source(err)
        })?
        .collect::<Vec<_>>();
    let separator = joiner.as_ref().map(stringify).unwrap_or_default();

    let joiner_is_safe = joiner.as_ref().is_some_and(Value::is_safe);
    let escaping = !matches!(state.auto_escape(), AutoEscape::None);
    if escaping && (joiner_is_safe || items.iter().any(Value::is_safe)) {
        let mut output = String::new();
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                output.push_str(&separator);
            }
            match item.as_str().filter(|_| item.is_safe()) {
                Some(safe) => output.push_str(safe),
                None => output.push_str(&state.format(item.clone())?),
            }
        }
        return Ok(Value::from_safe_string(output));
    }

    let mut output = String::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.push_str(&separator);
        }
        output.push_str(&stringify(item));
    }
    Ok(Value::from(output))
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

fn spelling_formatter(out: &mut Output, state: &State, value: &Value) -> Result<(), Error> {
    match value.kind() {
        ValueKind::Bool
        | ValueKind::None
        | ValueKind::Seq
        | ValueKind::Map
        | ValueKind::Iterable => {
            escape_formatter(out, state, &Value::from(stringify(value).into_owned()))
        }
        _ => escape_formatter(out, state, value),
    }
}

// minijinja renders container elements with Debug, which quotes strings, so
// this in-container form of `stringify` does too.
fn repr(value: &Value) -> Cow<'_, str> {
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
