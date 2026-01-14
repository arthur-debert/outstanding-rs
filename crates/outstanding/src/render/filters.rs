//! MiniJinja filter registration.

use minijinja::{Environment, Value};

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

    // Register table formatting filters (col, pad_left, pad_right, truncate_at, etc.)
    crate::table::filters::register_table_filters(env);
}
