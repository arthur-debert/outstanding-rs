//! Derive macro for declarative command dispatch.
//!
//! This module provides [`Dispatch`] derive macro that generates dispatch configuration
//! from clap `Subcommand` enums, eliminating boilerplate command-to-handler mappings.
//!
//! For working examples, see `standout/tests/dispatch_derive.rs`.
//!
//! # Motivation
//!
//! Without this macro, you must explicitly map every command to its handler.
//! With `#[derive(Dispatch)]`, the mapping becomes implicit via naming conventions:
//!
//! - `Add` variant → `handlers::add` function
//! - `ListAll` variant → `handlers::list_all` function
//!
//! # Convention-Based Defaults
//!
//! - Handler: `{handlers_module}::{variant_snake_case}`
//! - Template: `{variant_snake_case}.j2`
//!
//! # Container Attributes
//!
//! Applied to the enum with `#[dispatch(...)]`:
//!
//! | Attribute | Required | Description |
//! |-----------|----------|-------------|
//! | `handlers = path` | Yes | Module containing handler functions |
//!
//! # Variant Attributes
//!
//! Applied to enum variants with `#[dispatch(...)]`:
//!
//! | Attribute | Description | Default |
//! |-----------|-------------|---------|
//! | `handler = path` | Handler function path | `{handlers}::{snake_case}` |
//! | `template = "path"` | Template file path | `{snake_case}.j2` |
//! | `pre_dispatch = fn` | Pre-dispatch hook | None |
//! | `post_dispatch = fn` | Post-dispatch hook | None |
//! | `post_output = fn` | Post-output hook | None |
//! | `nested` | Treat variant as nested subcommand | false |
//! | `skip` | Skip this variant | false |
//! | `default` | Use as default command when no subcommand specified | false |
//! | `list_view` | Enable ListView integration | false |
//! | `item_type` | Type name for tabular spec injection | None |
//! | `simple` | Handler only takes `&ArgMatches` (no context) | false |
//!
//! # Generated Code
//!
//! The macro generates a `dispatch_config()` method returning a closure for
//! use with `App::builder().commands()`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Data, DeriveInput, Error, Expr, Fields, Meta, Path, Result, Token,
};

/// Container-level attributes: `#[dispatch(handlers = path)]`
#[derive(Default)]
struct ContainerAttrs {
    handlers: Option<Path>,
}

/// Variant-level attributes: `#[dispatch(handler = path, template = "...", ...)]`
#[derive(Default)]
struct VariantAttrs {
    handler: Option<Path>,
    template: Option<String>,
    pre_dispatch: Option<Path>,
    post_dispatch: Option<Path>,
    post_output: Option<Path>,
    nested: bool,
    skip: bool,
    default: bool,
    list_view: bool,
    item_type: Option<String>,
    /// Handler only takes `&ArgMatches` (no `&CommandContext`)
    simple: bool,
}

/// Information extracted from a single enum variant
struct VariantInfo {
    snake_name: String,
    attrs: VariantAttrs,
    is_nested: bool,
    nested_type: Option<Path>,
}

impl Parse for ContainerAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = ContainerAttrs::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                Meta::NameValue(nv) if nv.path.is_ident("handlers") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.handlers = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown attribute, expected `handlers = path`",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

impl Parse for VariantAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = VariantAttrs::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                Meta::NameValue(nv) if nv.path.is_ident("handler") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.handler = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("template") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.template = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("pre_dispatch") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.pre_dispatch = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("post_dispatch") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.post_dispatch = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("post_output") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.post_output = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::Path(p) if p.is_ident("nested") => {
                    attrs.nested = true;
                }
                Meta::Path(p) if p.is_ident("skip") => {
                    attrs.skip = true;
                }
                Meta::Path(p) if p.is_ident("default") => {
                    attrs.default = true;
                }
                Meta::Path(p) if p.is_ident("list_view") => {
                    attrs.list_view = true;
                }
                Meta::NameValue(nv) if nv.path.is_ident("item_type") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.item_type = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::Path(p) if p.is_ident("simple") => {
                    attrs.simple = true;
                }
                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown attribute, expected one of: handler, template, pre_dispatch, post_dispatch, post_output, nested, skip, default, simple",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

/// Converts PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Extract container-level `#[dispatch(...)]` attributes
fn parse_container_attrs(input: &DeriveInput) -> Result<ContainerAttrs> {
    for attr in &input.attrs {
        if attr.path().is_ident("dispatch") {
            return attr.parse_args::<ContainerAttrs>();
        }
    }

    Err(Error::new(
        input.span(),
        "missing `#[dispatch(handlers = path)]` attribute",
    ))
}

/// Extract variant-level `#[dispatch(...)]` attributes
fn parse_variant_attrs(attrs: &[syn::Attribute]) -> Result<VariantAttrs> {
    for attr in attrs {
        if attr.path().is_ident("dispatch") {
            return attr.parse_args::<VariantAttrs>();
        }
    }
    Ok(VariantAttrs::default())
}

/// Check if a variant is a nested subcommand (tuple with single type argument)
fn is_nested_subcommand(fields: &Fields) -> Option<Path> {
    if let Fields::Unnamed(unnamed) = fields {
        if unnamed.unnamed.len() == 1 {
            let field = unnamed.unnamed.first().unwrap();
            if let syn::Type::Path(type_path) = &field.ty {
                // Assume it's a nested subcommand if it's a path type
                // This heuristic works because Args types typically don't have
                // a Dispatch derive, so the generated code will fail at compile
                // time if misused
                return Some(type_path.path.clone());
            }
        }
    }
    None
}

/// Main implementation of the Dispatch derive macro
pub fn dispatch_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let container_attrs = parse_container_attrs(&input)?;
    let handlers_path = container_attrs.handlers.ok_or_else(|| {
        Error::new(
            input.span(),
            "missing `handlers` in `#[dispatch(handlers = path)]`",
        )
    })?;

    let enum_name = &input.ident;

    let data = match &input.data {
        Data::Enum(data) => data,
        _ => {
            return Err(Error::new(
                input.span(),
                "Dispatch can only be derived for enums",
            ))
        }
    };

    // Collect variant info
    let mut variants: Vec<VariantInfo> = Vec::new();

    for variant in &data.variants {
        let attrs = parse_variant_attrs(&variant.attrs)?;

        if attrs.skip {
            continue;
        }

        let snake_name = to_snake_case(&variant.ident.to_string());
        let nested_type_candidate = is_nested_subcommand(&variant.fields);

        // Determine is_nested:
        // 1. If explicit #[dispatch(nested)], it MUST be nested (and must have a valid nested type).
        // 2. If NO explicit nested, it is a leaf command (default), even if it looks like a nested one.
        //    This fixes the bug where Command(String) was treated as nested.
        let is_nested = attrs.nested;

        if is_nested && nested_type_candidate.is_none() {
            return Err(Error::new(
                variant.span(),
                "#[dispatch(nested)] requires a tuple variant with a single field (the nested subcommand enum)",
            ));
        }

        variants.push(VariantInfo {
            snake_name,
            attrs,
            is_nested,
            nested_type: nested_type_candidate,
        });
    }

    // Find the default command (if any)
    let default_command: Option<&str> = {
        let defaults: Vec<_> = variants.iter().filter(|v| v.attrs.default).collect();

        if defaults.len() > 1 {
            // This will be caught at runtime by GroupBuilder::default_command panic,
            // but we can provide a better error at compile time
            let names: Vec<_> = defaults.iter().map(|v| v.snake_name.as_str()).collect();
            return Err(Error::new(
                input.span(),
                format!(
                    "Only one command can be marked as default. Found multiple: {}",
                    names.join(", ")
                ),
            ));
        }

        defaults.first().map(|v| v.snake_name.as_str())
    };

    // Generate the command registration calls
    let command_registrations: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let cmd_name = &v.snake_name;

            if v.is_nested {
                // Nested subcommand - delegate to its dispatch_config
                let nested_type = v.nested_type.as_ref().unwrap();
                quote! {
                    let __builder = __builder.group(#cmd_name, #nested_type::dispatch_config());
                }
            } else {
                // Leaf command
                let handler_path = v.attrs.handler.clone().unwrap_or_else(|| {
                    let handler_ident = format_ident!("{}", v.snake_name);
                    let mut path = handlers_path.clone();
                    path.segments.push(syn::PathSegment {
                        ident: handler_ident,
                        arguments: syn::PathArguments::None,
                    });
                    path
                });

                // If list_view is enabled, default template if not set
                let mut v_template = v.attrs.template.clone();
                if v.attrs.list_view && v_template.is_none() {
                    v_template = Some("standout/list-view".to_string());
                }

                let has_config = v_template.is_some()
                    || v.attrs.pre_dispatch.is_some()
                    || v.attrs.post_dispatch.is_some()
                    || v.attrs.post_output.is_some()
                    || (v.attrs.list_view && v.attrs.item_type.is_some());

                // Determine the handler expression (original or wrapped)
                // Simple handlers only take &ArgMatches, so we wrap them in a closure
                // that ignores the context parameter
                let handler_expr = if v.attrs.list_view {
                     if let Some(item_type_str) = &v.attrs.item_type {
                        let item_type_path: syn::Path = syn::parse_str(item_type_str)
                            .expect("Failed to parse item_type as path");
                        // Generate wrapper to inject tabular spec
                        // Handle both simple and regular handlers
                        if v.attrs.simple {
                            quote! {
                                |matches, _ctx| {
                                    let result = #handler_path(matches);
                                    result.map(|output| {
                                        match output {
                                            ::standout::cli::handler::Output::Render(mut lv) => {
                                                 lv.tabular_spec = Some(<#item_type_path as ::standout::tabular::Tabular>::tabular_spec());
                                                 ::standout::cli::handler::Output::Render(lv)
                                            }
                                            o => o
                                        }
                                    })
                                }
                            }
                        } else {
                            quote! {
                                |matches, ctx| {
                                    let result = #handler_path(matches, ctx);
                                    result.map(|output| {
                                        match output {
                                            ::standout::cli::handler::Output::Render(mut lv) => {
                                                 lv.tabular_spec = Some(<#item_type_path as ::standout::tabular::Tabular>::tabular_spec());
                                                 ::standout::cli::handler::Output::Render(lv)
                                            }
                                            o => o
                                        }
                                    })
                                }
                            }
                        }
                     } else if v.attrs.simple {
                        // Simple handler without list_view
                        quote! { |matches, _ctx| #handler_path(matches) }
                     } else {
                        quote! { #handler_path }
                     }
                } else if v.attrs.simple {
                    // Simple handler (only takes &ArgMatches, no context)
                    quote! { |matches, _ctx| #handler_path(matches) }
                } else {
                    quote! { #handler_path }
                };

                if has_config {
                    // Use command_with for custom configuration
                    let template_call = v_template.as_ref().map(|t| {
                        quote! { __cfg = __cfg.template(#t); }
                    });
                    let pre_dispatch_call = v.attrs.pre_dispatch.as_ref().map(|p| {
                        quote! { __cfg = __cfg.pre_dispatch(#p); }
                    });
                    let post_dispatch_call = v.attrs.post_dispatch.as_ref().map(|p| {
                        quote! { __cfg = __cfg.post_dispatch(#p); }
                    });
                    let post_output_call = v.attrs.post_output.as_ref().map(|p| {
                        quote! { __cfg = __cfg.post_output(#p); }
                    });

                    quote! {
                        let __builder = __builder.command_with(#cmd_name, #handler_expr, |mut __cfg| {
                            #template_call
                            #pre_dispatch_call
                            #post_dispatch_call
                            #post_output_call
                            __cfg
                        });
                    }
                } else {
                    // Simple command registration
                    quote! {
                        let __builder = __builder.command(#cmd_name, #handler_expr);
                    }
                }
            }
        })
        .collect();

    // Generate default command registration if one was marked
    let default_command_registration = default_command.map(|name| {
        quote! {
            let __builder = __builder.default_command(#name);
        }
    });

    let expanded = quote! {
        impl #enum_name {
            /// Returns a dispatch configuration closure for use with `App::builder().commands()`.
            ///
            /// Generated by `#[derive(Dispatch)]`.
            pub fn dispatch_config() -> impl FnOnce(::standout::cli::GroupBuilder) -> ::standout::cli::GroupBuilder {
                |__builder: ::standout::cli::GroupBuilder| {
                    #(#command_registrations)*
                    #default_command_registration
                    __builder
                }
            }
        }
    };

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Add"), "add");
        assert_eq!(to_snake_case("ListAll"), "list_all");
        assert_eq!(to_snake_case("HTTPServer"), "h_t_t_p_server");
        assert_eq!(to_snake_case("getHTTPResponse"), "get_h_t_t_p_response");
    }

    #[test]
    fn test_to_snake_case_simple() {
        assert_eq!(to_snake_case("Complete"), "complete");
        assert_eq!(to_snake_case("Delete"), "delete");
    }
}
