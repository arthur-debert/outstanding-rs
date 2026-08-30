use heck::{ToKebabCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    ext::IdentExt,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Data, DeriveInput, Error, Expr, Fields, Ident, Meta, Path, Result, Token,
};

#[derive(Default)]
struct ContainerAttrs {
    handlers: Option<Path>,
}

#[derive(Default)]
struct VariantAttrs {
    name: Option<String>,
    handler: Option<Path>,
    template: Option<String>,
    template_name: Option<String>,
    silent: bool,
    binary: bool,
    structured_only: bool,
    pre_dispatch: Option<Path>,
    post_dispatch: Option<Path>,
    post_output: Option<Path>,
    questionnaire: Option<Path>,
    nested: bool,
    skip: bool,
    default: bool,
    list_view: bool,
    item_type: Option<String>,
    pipe_to: Option<String>,
    pipe_through: Option<String>,
    pipe_to_clipboard: bool,
    simple: bool,
    pure: bool,
}

struct VariantInfo {
    snake_name: String,
    command_name: String,
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
                Meta::NameValue(nv) if nv.path.is_ident("name") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            let value = lit_str.value();
                            if value.is_empty() {
                                return Err(Error::new(nv.value.span(), "`name` cannot be empty"));
                            }
                            attrs.name = Some(value);
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
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
                Meta::NameValue(nv) if nv.path.is_ident("template_name") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.template_name = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::Path(p) if p.is_ident("silent") => {
                    attrs.silent = true;
                }
                Meta::Path(p) if p.is_ident("binary") => {
                    attrs.binary = true;
                }
                Meta::Path(p) if p.is_ident("structured_only") => {
                    attrs.structured_only = true;
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
                Meta::NameValue(nv) if nv.path.is_ident("questionnaire") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.questionnaire = Some(expr_path.path.clone());
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
                Meta::NameValue(nv) if nv.path.is_ident("pipe_to") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.pipe_to = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("pipe_through") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.pipe_through = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::Path(p) if p.is_ident("pipe_to_clipboard") => {
                    attrs.pipe_to_clipboard = true;
                }
                Meta::Path(p) if p.is_ident("simple") => {
                    attrs.simple = true;
                }
                Meta::Path(p) if p.is_ident("pure") => {
                    attrs.pure = true;
                }
                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown attribute, expected one of: name, handler, template, template_name, silent, binary, structured_only, pre_dispatch, post_dispatch, post_output, questionnaire, nested, skip, default, list_view, item_type, pipe_to, pipe_through, pipe_to_clipboard, simple, pure",
                    ));
                }
            }
        }

        if attrs.pure && attrs.handler.is_some() {
            return Err(Error::new(
                input.span(),
                "`pure` and `handler` cannot be used together: `pure` registers the wrapper `#[handler]` generates for the variant's own function, while `handler = path` names a function to register. Drop `pure` and point `handler` at the wrapper (`handler = handlers::name__handler`) to register another `#[handler]` function",
            ));
        }
        if attrs.pure && attrs.simple {
            return Err(Error::new(
                input.span(),
                "`pure` and `simple` cannot be used together: `simple` registers a function taking only `&ArgMatches`, while the wrapper `#[handler]` generates always takes `(&ArgMatches, &CommandContext)`. Drop `simple`; a `#[handler]` function declares what it needs through its parameter attributes",
            ));
        }
        if attrs.template.is_some() && attrs.template_name.is_some() {
            return Err(Error::new(
                input.span(),
                "`template` and `template_name` cannot be used together",
            ));
        }
        let absence_count = usize::from(attrs.silent)
            + usize::from(attrs.binary)
            + usize::from(attrs.structured_only);
        if absence_count > 1 {
            return Err(Error::new(
                input.span(),
                "`silent`, `binary`, and `structured_only` cannot be used together",
            ));
        }
        if absence_count == 1 && (attrs.template.is_some() || attrs.template_name.is_some()) {
            return Err(Error::new(
                input.span(),
                "`silent`, `binary`, and `structured_only` cannot be combined with `template` or `template_name`",
            ));
        }

        Ok(attrs)
    }
}

/// Clap's own conversions, so a variant registers under the name clap's derive
/// gives its subcommand: `heck` on the unraw'd identifier, exactly as
/// `clap_derive` does. Approximating the split diverges on digit/acronym runs
/// (`X2FA` is `x2fa` to clap, `x2-fa` to a hand-rolled splitter) and on raw
/// identifiers.
fn to_snake_case(ident: &Ident) -> String {
    ident.unraw().to_string().to_snake_case()
}

fn to_kebab_case(ident: &Ident) -> String {
    ident.unraw().to_string().to_kebab_case()
}

/// `move` is a valid handler-function name behind `r#`, so a derived name that
/// is a keyword becomes a raw identifier rather than a syntax error.
fn path_ident(name: &str, span: proc_macro2::Span) -> Ident {
    match syn::parse_str::<Ident>(name) {
        Ok(mut ident) => {
            ident.set_span(span);
            ident
        }
        Err(_) => Ident::new_raw(name, span),
    }
}

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

fn parse_variant_attrs(attrs: &[syn::Attribute]) -> Result<VariantAttrs> {
    for attr in attrs {
        if attr.path().is_ident("dispatch") {
            return attr.parse_args::<VariantAttrs>();
        }
    }
    Ok(VariantAttrs::default())
}

fn is_nested_subcommand(fields: &Fields) -> Option<Path> {
    if let Fields::Unnamed(unnamed) = fields {
        if unnamed.unnamed.len() == 1 {
            let field = unnamed.unnamed.first().unwrap();
            if let syn::Type::Path(type_path) = &field.ty {
                return Some(type_path.path.clone());
            }
        }
    }
    None
}

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

    let mut variants: Vec<VariantInfo> = Vec::new();
    let mut claimed_names: Vec<(String, String)> = Vec::new();

    for variant in &data.variants {
        let attrs = parse_variant_attrs(&variant.attrs)?;

        if attrs.skip {
            continue;
        }

        let snake_name = to_snake_case(&variant.ident);
        let command_name = attrs
            .name
            .clone()
            .unwrap_or_else(|| to_kebab_case(&variant.ident));

        if let Some((_, first_variant)) = claimed_names
            .iter()
            .find(|(claimed, _)| *claimed == command_name)
        {
            return Err(Error::new(
                variant.span(),
                format!(
                    "`{}` and `{}` both register the command `{command_name}`, so only \
                     the last one would run. Rename one variant, or give it a \
                     `#[dispatch(name = \"...\")]` of its own.",
                    first_variant, variant.ident,
                ),
            ));
        }
        claimed_names.push((command_name.clone(), variant.ident.to_string()));

        let nested_type_candidate = is_nested_subcommand(&variant.fields);

        let is_nested = attrs.nested;

        if is_nested && nested_type_candidate.is_none() {
            return Err(Error::new(
                variant.span(),
                "#[dispatch(nested)] requires a tuple variant with a single field (the nested subcommand enum)",
            ));
        }

        variants.push(VariantInfo {
            snake_name,
            command_name,
            attrs,
            is_nested,
            nested_type: nested_type_candidate,
        });
    }

    let default_command: Option<&str> = {
        let defaults: Vec<_> = variants.iter().filter(|v| v.attrs.default).collect();

        if defaults.len() > 1 {
            let names: Vec<_> = defaults.iter().map(|v| v.command_name.as_str()).collect();
            return Err(Error::new(
                input.span(),
                format!(
                    "Only one command can be marked as default. Found multiple: {}",
                    names.join(", ")
                ),
            ));
        }

        defaults.first().map(|v| v.command_name.as_str())
    };

    let command_registrations: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let cmd_name = &v.command_name;

            if v.is_nested {
                let nested_type = v.nested_type.as_ref().unwrap();
                quote! {
                    let __builder = __builder.group(#cmd_name, #nested_type::dispatch_config());
                }
            } else {
                let handler_path = v.attrs.handler.clone().unwrap_or_else(|| {
                    let mut handler_name = v.snake_name.clone();
                    if v.attrs.pure {
                        handler_name = format!("{}__handler", handler_name);
                    }
                    let handler_ident = path_ident(&handler_name, proc_macro2::Span::call_site());
                    let mut path = handlers_path.clone();
                    path.segments.push(syn::PathSegment {
                        ident: handler_ident,
                        arguments: syn::PathArguments::None,
                    });
                    path
                });

                let v_template = v.attrs.template.clone();
                let mut v_template_name = v.attrs.template_name.clone();
                let uses_framework_list_view =
                    v.attrs.list_view && v_template.is_none() && v_template_name.is_none();
                if uses_framework_list_view {
                    v_template_name = Some("standout/list-view".to_string());
                }

                let has_config = v_template.is_some()
                    || v_template_name.is_some()
                    || v.attrs.silent
                    || v.attrs.binary
                    || v.attrs.structured_only
                    || v.attrs.pre_dispatch.is_some()
                    || v.attrs.post_dispatch.is_some()
                    || v.attrs.post_output.is_some()
                    || v.attrs.questionnaire.is_some()
                    || (v.attrs.list_view && v.attrs.item_type.is_some())
                    || v.attrs.pipe_to.is_some()
                    || v.attrs.pipe_through.is_some()
                    || v.attrs.pipe_to_clipboard;

                let handler_expr = if v.attrs.list_view {
                     if let Some(item_type_str) = &v.attrs.item_type {
                        let item_type_path: syn::Path = syn::parse_str(item_type_str)
                            .expect("Failed to parse item_type as path");
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
                        quote! { |matches, _ctx| #handler_path(matches) }
                     } else {
                        quote! { #handler_path }
                     }
                } else if v.attrs.simple {
                    quote! { |matches, _ctx| #handler_path(matches) }
                } else {
                    quote! { #handler_path }
                };

                if has_config {
                    let template_call = v_template
                        .as_ref()
                        .map(|template| quote! { __cfg = __cfg.template(#template); });
                    let template_name_call = v_template_name.as_ref().map(
                        |template_name| quote! { __cfg = __cfg.template_name(#template_name); },
                    );
                    let absence_call = if v.attrs.silent {
                        Some(quote! { __cfg = __cfg.silent(); })
                    } else if v.attrs.binary {
                        Some(quote! { __cfg = __cfg.binary(); })
                    } else if v.attrs.structured_only {
                        Some(quote! { __cfg = __cfg.structured_only(); })
                    } else {
                        None
                    };
                    let pre_dispatch_call = v.attrs.pre_dispatch.as_ref().map(|p| {
                        quote! { __cfg = __cfg.pre_dispatch(#p); }
                    });
                    let post_dispatch_call = v.attrs.post_dispatch.as_ref().map(|p| {
                        quote! { __cfg = __cfg.post_dispatch(#p); }
                    });
                    let post_output_call = v.attrs.post_output.as_ref().map(|p| {
                        quote! { __cfg = __cfg.post_output(#p); }
                    });
                    let questionnaire_call = v.attrs.questionnaire.as_ref().map(|p| {
                        quote! { __cfg = __cfg.questionnaire::<#p>(); }
                    });
                    let pipe_to_call = v.attrs.pipe_to.as_ref().map(|p| {
                        quote! { __cfg = __cfg.pipe_to(#p); }
                    });
                    let pipe_through_call = v.attrs.pipe_through.as_ref().map(|p| {
                        quote! { __cfg = __cfg.pipe_through(#p); }
                    });
                     let pipe_clipboard_call = if v.attrs.pipe_to_clipboard {
                        Some(quote! { __cfg = __cfg.pipe_to_clipboard(); })
                    } else {
                        None
                    };

                    quote! {
                        let __builder = __builder.command_with(#cmd_name, #handler_expr, |mut __cfg| {
                            #template_call
                            #template_name_call
                            #absence_call
                            #questionnaire_call
                            #pre_dispatch_call
                            #post_dispatch_call
                            #post_output_call
                            #pipe_to_call
                            #pipe_through_call
                            #pipe_clipboard_call
                            __cfg
                        });
                    }
                } else {
                    quote! {
                        let __builder = __builder.command(#cmd_name, #handler_expr);
                    }
                }
            }
        })
        .collect();

    let default_command_registration = default_command.map(|name| {
        quote! {
            let __builder = __builder.default_command(#name);
        }
    });

    let expanded = quote! {
        impl #enum_name {
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

    fn ident(s: &str) -> Ident {
        syn::parse_str::<Ident>(s).unwrap()
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case(&ident("Add")), "add");
        assert_eq!(to_snake_case(&ident("ListAll")), "list_all");
        assert_eq!(to_snake_case(&ident("HTTPServer")), "http_server");
        assert_eq!(
            to_snake_case(&ident("getHTTPResponse")),
            "get_http_response"
        );
    }

    #[test]
    fn test_to_snake_case_simple() {
        assert_eq!(to_snake_case(&ident("Complete")), "complete");
        assert_eq!(to_snake_case(&ident("Delete")), "delete");
    }

    /// The right-hand sides are what clap's own derive registers; the
    /// `crates/standout/tests/dispatch_derive.rs` parity test reads them back
    /// off a `Command` built by `#[derive(Subcommand)]`.
    #[test]
    fn test_to_kebab_case_matches_clap_derive() {
        assert_eq!(to_kebab_case(&ident("Add")), "add");
        assert_eq!(to_kebab_case(&ident("ListUnits")), "list-units");
        assert_eq!(to_kebab_case(&ident("HTTPServer")), "http-server");
        assert_eq!(
            to_kebab_case(&ident("getHTTPResponse")),
            "get-http-response"
        );
    }

    #[test]
    fn digit_runs_keep_clap_word_boundaries() {
        assert_eq!(to_kebab_case(&ident("X2FA")), "x2fa");
        assert_eq!(to_kebab_case(&ident("A1B2")), "a1b2");
        assert_eq!(to_kebab_case(&ident("Sha256Sum")), "sha256-sum");
        assert_eq!(to_kebab_case(&ident("Utf8Check")), "utf8-check");
        assert_eq!(to_snake_case(&ident("X2FA")), "x2fa");
        assert_eq!(to_snake_case(&ident("Sha256Sum")), "sha256_sum");
    }

    #[test]
    fn raw_identifiers_drop_the_prefix_as_clap_does() {
        let span = proc_macro2::Span::call_site();
        assert_eq!(to_kebab_case(&Ident::new_raw("Move", span)), "move");
        assert_eq!(to_snake_case(&Ident::new_raw("Move", span)), "move");
        assert_eq!(to_kebab_case(&Ident::new_raw("TypeOf", span)), "type-of");
    }

    #[test]
    fn a_keyword_handler_name_stays_a_usable_path_segment() {
        let span = proc_macro2::Span::call_site();
        assert_eq!(path_ident("move", span).to_string(), "r#move");
        assert_eq!(path_ident("list_units", span).to_string(), "list_units");
    }
}
