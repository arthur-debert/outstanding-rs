mod context;
mod presentation;
mod reload;

use super::*;
use crate::cli::builder::OUTPUT_MODE_ARG;
use crate::EmbeddedTemplates;

const TEMPLATES: &[(&str, &str)] = &[
    ("info", "{{ name }} v{{ version }}"),
    ("info-2", "{{ title }} by {{ author }} ({{ year }})"),
    ("info-3", "Width: {{ terminal_width }}"),
    ("info-4", "Mode: {{ mode }}"),
    ("test", "{{ value }}"),
    ("list", "{{ app_name }}: list"),
    ("info-5", "{{ app_name }}: info"),
    ("test-2", "Count: {{ count }}, Doubled: {{ doubled_count }}"),
    ("test-3", "Debug: {{ config.debug }}, Max: {{ config.max_items }}"),
    ("list-2", "{% for item in items %}{{ item }}{% if not loop.last %}{{ separator }}{% endif %}{% endfor %}"),
    ("test-4", "{{ data }} + {{ extra }}"),
    ("list-3", "n={{ n }}"),
    ("sibling", "n={{ n }}"),
];

use crate::cli::handler::FnHandler;
use crate::cli::handler::Output as HandlerOutput;
use crate::context::RenderContext;
use crate::{ColorPolicy, Representation};
use clap::Command;

fn color_capable_stderr_target() -> crate::TargetProperties {
    use crate::{AmbiguousWidth, ColorMode, IconMode, TargetProperties};
    TargetProperties {
        width: Some(80),
        stdout_is_terminal: false,
        stderr_is_terminal: true,
        stdout_color_capability: false,
        stderr_color_capability: true,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}
