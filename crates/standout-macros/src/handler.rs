use crate::ident::safe_ident;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    ext::IdentExt,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Error, Expr, FnArg, ItemFn, Meta, Pat, PatType, Result, Token, Type,
};

#[derive(Clone)]
enum ParamKind {
    Flag {
        cli_name: Option<String>,
    },
    Arg {
        cli_name: Option<String>,
    },
    Ctx,
    Matches,
    /// `&mut Results<E>`: the command's typed results channel, carrying `E`.
    Results(Type),
    None,
}

struct ParamInfo {
    rust_name: String,
    cli_name: String,
    ty: Type,
    kind: ParamKind,
}

struct AttrArgs {
    name: Option<String>,
}

impl Parse for AttrArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = AttrArgs { name: None };

        if input.is_empty() {
            return Ok(args);
        }

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            if let Meta::NameValue(nv) = meta {
                if nv.path.is_ident("name") {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            args.name = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                } else {
                    return Err(Error::new(
                        nv.path.span(),
                        "unknown attribute, expected `name`",
                    ));
                }
            }
        }

        Ok(args)
    }
}

fn parse_param_kind(pat_type: &PatType) -> Result<ParamKind> {
    for attr in &pat_type.attrs {
        if attr.path().is_ident("flag") {
            let args: AttrArgs = if attr.meta.require_path_only().is_ok() {
                AttrArgs { name: None }
            } else {
                attr.parse_args()?
            };
            return Ok(ParamKind::Flag {
                cli_name: args.name,
            });
        }
        if attr.path().is_ident("arg") {
            let args: AttrArgs = if attr.meta.require_path_only().is_ok() {
                AttrArgs { name: None }
            } else {
                attr.parse_args()?
            };
            return Ok(ParamKind::Arg {
                cli_name: args.name,
            });
        }
        if attr.path().is_ident("ctx") {
            return Ok(ParamKind::Ctx);
        }
        if attr.path().is_ident("matches") {
            return Ok(ParamKind::Matches);
        }
    }

    Ok(ParamKind::None)
}

/// The name without its `r#`, the way clap's derive unraws a field name.
fn extract_param_name(pat: &Pat) -> Result<String> {
    match pat {
        Pat::Ident(ident) => Ok(ident.ident.unraw().to_string()),
        _ => Err(Error::new(
            pat.span(),
            "expected identifier pattern for parameter",
        )),
    }
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

fn is_vec_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Vec";
        }
    }
    false
}

fn extract_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner);
                }
            }
        }
    }
    None
}

fn is_reference_type(ty: &Type) -> bool {
    matches!(ty, Type::Reference(_))
}

/// The event type of a `&mut Results<E>` parameter, which is what makes a
/// handler incremental; `None` for every other parameter type.
fn results_event_type(ty: &Type) -> Option<Type> {
    let Type::Reference(reference) = ty else {
        return None;
    };
    reference.mutability?;
    let Type::Path(path) = reference.elem.as_ref() else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Results" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        syn::GenericArgument::Type(event) => Some(event.clone()),
        _ => None,
    }
}

fn is_unit_result(fn_item: &ItemFn) -> bool {
    matches!(extract_result_ok_type(fn_item), Some(Type::Tuple(t)) if t.elems.is_empty())
}

fn extract_result_ok_type(fn_item: &ItemFn) -> Option<Type> {
    if let syn::ReturnType::Type(_, ty) = &fn_item.sig.output {
        if let Type::Path(type_path) = ty.as_ref() {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(ok_type)) = args.args.first() {
                            return Some(ok_type.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

fn generate_expected_arg(param: &ParamInfo, dispatch: &TokenStream) -> Option<TokenStream> {
    let cli_name = &param.cli_name;
    let rust_name = &param.rust_name;

    match &param.kind {
        ParamKind::Flag { .. } => Some(quote! {
            #dispatch::verify::ExpectedArg::flag(#cli_name, #rust_name)
        }),
        ParamKind::Arg { .. } => {
            let ty = &param.ty;
            if is_option_type(ty) {
                Some(quote! {
                    #dispatch::verify::ExpectedArg::optional_arg(#cli_name, #rust_name)
                })
            } else if is_vec_type(ty) {
                Some(quote! {
                    #dispatch::verify::ExpectedArg::vec_arg(#cli_name, #rust_name)
                })
            } else {
                Some(quote! {
                    #dispatch::verify::ExpectedArg::required_arg(#cli_name, #rust_name)
                })
            }
        }
        ParamKind::Ctx | ParamKind::Matches | ParamKind::Results(_) | ParamKind::None => None,
    }
}

fn generate_extraction(param: &ParamInfo) -> TokenStream {
    let rust_name = safe_ident(&param.rust_name, Span::call_site());
    let cli_name = &param.cli_name;
    let ty = &param.ty;

    match &param.kind {
        ParamKind::Flag { .. } => {
            quote! {
                let #rust_name: bool = __matches.get_flag(#cli_name);
            }
        }
        ParamKind::Arg { .. } => {
            if is_option_type(ty) {
                let inner = extract_inner_type(ty).unwrap_or(ty);
                quote! {
                    let #rust_name: #ty = __matches.get_one::<#inner>(#cli_name).cloned();
                }
            } else if is_vec_type(ty) {
                let inner = extract_inner_type(ty).unwrap_or(ty);
                quote! {
                    let #rust_name: #ty = __matches
                        .get_many::<#inner>(#cli_name)
                        .map(|v| v.cloned().collect())
                        .unwrap_or_default();
                }
            } else {
                quote! {
                    let #rust_name: #ty = __matches.get_one::<#ty>(#cli_name)
                        .expect(concat!("Missing required argument '", #cli_name, "' - ensure clap definition matches handler"))
                        .clone();
                }
            }
        }
        ParamKind::Ctx | ParamKind::Matches | ParamKind::Results(_) | ParamKind::None => {
            quote! {}
        }
    }
}

fn generate_call_arg(param: &ParamInfo) -> TokenStream {
    let rust_name = safe_ident(&param.rust_name, Span::call_site());

    match &param.kind {
        ParamKind::Flag { .. } | ParamKind::Arg { .. } => {
            quote! { #rust_name }
        }
        ParamKind::Ctx => {
            quote! { __ctx }
        }
        ParamKind::Matches => {
            quote! { __matches }
        }
        ParamKind::Results(_) => {
            quote! { __results }
        }
        ParamKind::None => {
            quote! { #rust_name }
        }
    }
}

/// The `Results` parameter alone decides which wrapper a handler returns, so a
/// batch function returning an application's own `Summary<T>` keeps that as its
/// output type. An emitting function still unwraps `Output<T>`, the batch
/// wrapper written by mistake, so it reads as the `Summary`-not-`Output`
/// diagnostic rather than as `Output<T>` failing to be `Serialize`.
fn extract_output_type(ty: &Type, emits: bool) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Output" || (emits && segment.ident == "Summary") {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return Some(inner);
                    }
                }
            }
        }
    }
    None
}

pub fn handler_impl(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let fn_item: ItemFn = syn::parse2(item)?;

    let _attr_args: TokenStream = attr;

    let dispatch = crate::crate_path::dispatch();

    let fn_name = &fn_item.sig.ident;
    let wrapper_name = format_ident!("{}__handler", fn_name.unraw());
    let fn_vis = &fn_item.vis;

    let mut params: Vec<ParamInfo> = Vec::new();
    let mut has_ctx = false;
    let mut _has_matches = false;
    let mut event_type: Option<Type> = None;

    for fn_arg in &fn_item.sig.inputs {
        match fn_arg {
            FnArg::Typed(pat_type) => {
                let kind = match results_event_type(&pat_type.ty) {
                    Some(event) => ParamKind::Results(event),
                    None => parse_param_kind(pat_type)?,
                };
                let rust_name = extract_param_name(&pat_type.pat)?;

                let cli_name = match &kind {
                    ParamKind::Flag { cli_name } | ParamKind::Arg { cli_name } => cli_name
                        .clone()
                        .unwrap_or_else(|| rust_name.replace('_', "-")),
                    _ => rust_name.clone(),
                };

                if matches!(kind, ParamKind::Ctx) {
                    has_ctx = true;
                }
                if matches!(kind, ParamKind::Matches) {
                    _has_matches = true;
                }
                if let ParamKind::Results(event) = &kind {
                    if event_type.is_some() {
                        return Err(Error::new(
                            pat_type.span(),
                            "#[handler] functions take at most one Results parameter",
                        ));
                    }
                    event_type = Some(event.clone());
                }

                if matches!(kind, ParamKind::None) && !is_reference_type(&pat_type.ty) {
                    return Err(Error::new(
                        pat_type.span(),
                        "parameter must have #[flag], #[arg], #[ctx], or #[matches] annotation",
                    ));
                }

                params.push(ParamInfo {
                    rust_name,
                    cli_name,
                    ty: (*pat_type.ty).clone(),
                    kind,
                });
            }
            FnArg::Receiver(_) => {
                return Err(Error::new(
                    fn_arg.span(),
                    "#[handler] functions cannot have self parameter",
                ));
            }
        }
    }

    let extractions: Vec<TokenStream> = params.iter().map(generate_extraction).collect();

    let call_args: Vec<TokenStream> = params.iter().map(generate_call_arg).collect();

    let expected_args: Vec<TokenStream> = params
        .iter()
        .filter_map(|param| generate_expected_arg(param, &dispatch))
        .collect();
    let expected_args_name = format_ident!("{}__expected_args", fn_name.unraw());

    let _wrapper_params = if has_ctx {
        quote! { __matches: &::clap::ArgMatches, __ctx: &#dispatch::CommandContext }
    } else {
        quote! { __matches: &::clap::ArgMatches }
    };
    let (event_param, event_argument, handler_event, handler_results) = match &event_type {
        Some(event) => (
            quote! { , __results: &mut #dispatch::Results<#event> },
            quote! { , results },
            quote! { #event },
            quote! { results },
        ),
        None => (
            quote! {},
            quote! {},
            quote! { #dispatch::NoEvents },
            quote! { _results },
        ),
    };

    let return_type = &fn_item.sig.output;

    let emits = event_type.is_some();

    let call_and_return = if is_unit_result(&fn_item) {
        let empty = if emits {
            quote! { #dispatch::Summary::Silent }
        } else {
            quote! { #dispatch::Output::Silent }
        };
        quote! {
            #fn_name(#(#call_args),*)?;
            Ok(#empty)
        }
    } else {
        quote! {
            #fn_name(#(#call_args),*)
        }
    };

    let wrapper_return_type = if is_unit_result(&fn_item) {
        if emits {
            quote! { -> #dispatch::SummaryResult<()> }
        } else {
            quote! { -> #dispatch::HandlerResult<()> }
        }
    } else {
        quote! { #return_type }
    };

    let mut clean_fn = fn_item.clone();
    for fn_arg in &mut clean_fn.sig.inputs {
        if let FnArg::Typed(pat_type) = fn_arg {
            pat_type.attrs.retain(|attr| {
                !attr.path().is_ident("flag")
                    && !attr.path().is_ident("arg")
                    && !attr.path().is_ident("ctx")
                    && !attr.path().is_ident("matches")
            });
        }
    }

    let handler_struct_name = format_ident!("{}_Handler", fn_name.unraw());
    let ok_type = extract_result_ok_type(&fn_item).ok_or_else(|| {
        Error::new(
            fn_item.sig.output.span(),
            "handler must return Result<T, E>",
        )
    })?;

    let output_type = if is_unit_result(&fn_item) {
        quote! { () }
    } else if let Some(inner) = extract_output_type(&ok_type, emits) {
        quote! { #inner }
    } else {
        quote! { #ok_type }
    };

    let (handler_outcome, handle_result, into_outcome) = if emits {
        (
            quote! { #dispatch::Summary<#output_type> },
            quote! { #dispatch::SummaryResult<Self::Output> },
            quote! {
                #dispatch::IntoSummaryResult::into_summary_result(#wrapper_name(matches, ctx #event_argument))
            },
        )
    } else {
        (
            quote! { #dispatch::Output<#output_type> },
            quote! { #dispatch::HandlerResult<Self::Output> },
            quote! {
                #dispatch::IntoHandlerResult::into_handler_result(#wrapper_name(matches, ctx #event_argument))
            },
        )
    };

    Ok(quote! {
        #clean_fn

        #[allow(non_snake_case)]
        #fn_vis fn #wrapper_name(__matches: &::clap::ArgMatches, __ctx: &#dispatch::CommandContext #event_param) #wrapper_return_type {
            #(#extractions)*
            #call_and_return
        }

        #[allow(non_snake_case)]
        #fn_vis fn #expected_args_name() -> ::std::vec::Vec<#dispatch::verify::ExpectedArg> {
            vec![#(#expected_args),*]
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy)]
        #fn_vis struct #handler_struct_name;

        impl #dispatch::Handler for #handler_struct_name {
            type Event = #handler_event;
            type Output = #output_type;
            type Outcome = #handler_outcome;

            fn handle(
                &mut self,
                matches: &::clap::ArgMatches,
                ctx: &#dispatch::CommandContext,
                #handler_results: &mut #dispatch::Results<Self::Event>,
            ) -> #handle_result
            {
                #into_outcome
            }

            fn expected_args(&self) -> ::std::vec::Vec<#dispatch::verify::ExpectedArg> {
                #expected_args_name()
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_option_type() {
        let ty: Type = syn::parse_quote!(Option<String>);
        assert!(is_option_type(&ty));

        let ty: Type = syn::parse_quote!(String);
        assert!(!is_option_type(&ty));
    }

    #[test]
    fn a_batch_return_type_named_summary_stays_the_output_type() {
        let summary: Type = syn::parse_quote!(domain::Summary<Row>);
        let output: Type = syn::parse_quote!(Output<Row>);

        assert!(extract_output_type(&summary, false).is_none());
        assert!(extract_output_type(&output, false).is_some());
        assert!(extract_output_type(&summary, true).is_some());
        assert!(extract_output_type(&output, true).is_some());
    }

    #[test]
    fn test_is_vec_type() {
        let ty: Type = syn::parse_quote!(Vec<String>);
        assert!(is_vec_type(&ty));

        let ty: Type = syn::parse_quote!(String);
        assert!(!is_vec_type(&ty));
    }
}
