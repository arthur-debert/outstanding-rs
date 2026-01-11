//! MiniJinja filter registration.

use minijinja::{Environment, Value};

use crate::output::OutputMode;
use crate::style::Styles;

/// Registers all built-in filters on a minijinja environment.
///
/// # Arguments
///
/// * `env` - The MiniJinja environment to register filters on
/// * `styles` - Pre-resolved styles for the current color mode
/// * `mode` - Output mode (determines whether to apply ANSI styling)
pub(crate) fn register_filters(env: &mut Environment<'static>, styles: Styles, mode: OutputMode) {
    let is_debug = mode.is_debug();
    let use_color = mode.should_use_color();

    env.add_filter("style", move |value: Value, name: String| -> String {
        let text = value.to_string();
        if is_debug {
            styles.apply_debug(&name, &text)
        } else {
            styles.apply_with_mode(&name, &text, use_color)
        }
    });

    // Filter to append a newline to the value, enabling explicit line break control.
    // Usage: {{ content | nl }} outputs content followed by \n
    //        {{ "" | nl }} outputs just \n (a blank line)
    env.add_filter("nl", |value: Value| -> String { format!("{}\n", value) });

    // Register table formatting filters (col, pad_left, pad_right, truncate_at, etc.)
    crate::table::filters::register_table_filters(env);
}
