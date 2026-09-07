use crate::context::{ContextRegistry, RenderContext};
use crate::error::RenderError;
use crate::output::StyleMode;
use crate::request::TargetProperties;
use crate::theme::{ColorMode, IconMode, Theme};

use super::context::{build_combined_context, build_icon_context};
use super::{apply_style_tags, apply_style_tags_with, RenderResult};

pub fn render_auto_with_engine(
    engine: &dyn crate::template::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<String, RenderError> {
    Ok(render_auto_with_engine_split(
        engine,
        template,
        data,
        theme,
        style,
        context_registry,
        render_context,
    )?
    .formatted)
}

pub fn render_auto_with_engine_split(
    engine: &dyn crate::template::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<RenderResult, RenderError> {
    let detected = TargetProperties::detect();
    render_auto_with_engine_split_kind(
        engine,
        TemplateIdentity::Auto(template),
        data,
        theme,
        style,
        context_registry,
        render_context,
        detected.color_scheme,
        detected.icon_mode,
    )
}

pub fn render_auto_with_engine_split_inline(
    engine: &dyn crate::template::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<RenderResult, RenderError> {
    let detected = TargetProperties::detect();
    render_engine_split_inline(
        engine,
        template,
        data,
        theme,
        style,
        context_registry,
        render_context,
        detected.color_scheme,
        detected.icon_mode,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_engine_split_inline(
    engine: &dyn crate::template::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    color_mode: ColorMode,
    icon_mode: IconMode,
) -> Result<RenderResult, RenderError> {
    render_auto_with_engine_split_kind(
        engine,
        TemplateIdentity::Inline(template),
        data,
        theme,
        style,
        context_registry,
        render_context,
        color_mode,
        icon_mode,
    )
}

pub fn render_auto_with_engine_split_named(
    engine: &dyn crate::template::TemplateEngine,
    name: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<RenderResult, RenderError> {
    let detected = TargetProperties::detect();
    render_engine_split_named(
        engine,
        name,
        data,
        theme,
        style,
        context_registry,
        render_context,
        detected.color_scheme,
        detected.icon_mode,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_engine_split_named(
    engine: &dyn crate::template::TemplateEngine,
    name: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    color_mode: ColorMode,
    icon_mode: IconMode,
) -> Result<RenderResult, RenderError> {
    render_auto_with_engine_split_kind(
        engine,
        TemplateIdentity::Named(name),
        data,
        theme,
        style,
        context_registry,
        render_context,
        color_mode,
        icon_mode,
    )
}

enum TemplateIdentity<'a> {
    Auto(&'a str),
    Inline(&'a str),
    Named(&'a str),
}

#[allow(clippy::too_many_arguments)]
fn render_auto_with_engine_split_kind(
    engine: &dyn crate::template::TemplateEngine,
    template: TemplateIdentity<'_>,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    color_mode: ColorMode,
    icon_mode: IconMode,
) -> Result<RenderResult, RenderError> {
    {
        let styles = theme.resolve_styles(Some(color_mode));

        styles
            .validate()
            .map_err(|e| RenderError::StyleError(e.to_string()))?;

        let icon_context = build_icon_context(theme, icon_mode);
        let context_map =
            build_combined_context(data, context_registry, render_context, icon_context)?;

        let combined_value = standout_types::RenderData::Object(context_map.into_iter().collect());

        let raw_output = match template {
            TemplateIdentity::Auto(template) if engine.has_template(template) => engine
                .render_named_with_render_widths(
                    template,
                    &combined_value,
                    render_context.terminal_width,
                    render_context.ambiguous_width(),
                )?,
            TemplateIdentity::Auto(template) | TemplateIdentity::Inline(template) => engine
                .render_template_with_render_widths(
                    template,
                    &combined_value,
                    render_context.terminal_width,
                    render_context.ambiguous_width(),
                )?,
            TemplateIdentity::Named(name) => engine.render_named_with_render_widths(
                name,
                &combined_value,
                render_context.terminal_width,
                render_context.ambiguous_width(),
            )?,
        };

        // Unresolved-tag warnings are recorded once, on this formatted pass.
        let formatted_output = apply_style_tags_with(
            &raw_output,
            &styles,
            style,
            render_context.warnings.as_ref(),
        );

        let stripped_output = apply_style_tags(&raw_output, &styles, StyleMode::Plain);

        Ok(RenderResult::new(formatted_output, stripped_output))
    }
}
