//! `#[handler]` proc macro for pure function handlers.
//!
//! This macro transforms pure Rust functions into Standout-compatible handlers,
//! extracting CLI arguments automatically and generating wrapper functions.
//!
//! # Example
//!
//! ```rust,ignore
//! use standout_macros::handler;
//!
//! #[handler]
//! fn list(#[flag] all: bool, #[arg] limit: Option<usize>) -> Result<Vec<Item>, Error> {
//!     storage::list(all, limit)
//! }
//!
//! // Generates:
//! // pub fn list__handler(m: &ArgMatches) -> Result<Vec<Item>, Error> {
//! //     let all = m.get_flag("all");
//! //     let limit = m.get_one::<usize>("limit").copied();
//! //     list(all, limit)
//! // }
//! ```
//!
//! # Parameter Annotations
//!
//! | Annotation | Type | Extraction |
//! |------------|------|------------|
//! | `#[flag]` | `bool` | `m.get_flag("name")` |
//! | `#[flag(name = "x")]` | `bool` | `m.get_flag("x")` |
//! | `#[arg]` | `T` | `m.get_one::<T>("name").unwrap().clone()` |
//! | `#[arg]` | `Option<T>` | `m.get_one::<T>("name").cloned()` |
//! | `#[arg]` | `Vec<T>` | `m.get_many::<T>("name")...` |
//! | `#[arg(name = "x")]` | `T` | `m.get_one::<T>("x")...` |
//! | `#[ctx]` | `&CommandContext` | Pass through from wrapper |
//! | `#[matches]` | `&ArgMatches` | Pass through directly |
//!
//! # Return Type Handling
//!
//! | Return Type | Generated Wrapper Returns |
//! |-------------|---------------------------|
//! | `Result<T, E>` | `Result<T, E>` (dispatch auto-wraps via IntoHandlerResult) |
//! | `Result<(), E>` | `HandlerResult<()>` with explicit `Output::Silent` |

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Error, Expr, FnArg, ItemFn, Meta, Pat, PatType, Result, Token, Type,
};

/// Information about a parameter annotation
#[derive(Debug, Clone)]
enum ParamKind {
    /// `#[flag]` or `#[flag(name = "x")]`
    Flag { cli_name: Option<String> },
    /// `#[arg]` or `#[arg(name = "x")]`
    Arg { cli_name: Option<String> },
    /// `#[ctx]` - CommandContext reference
    Ctx,
    /// `#[matches]` - ArgMatches reference
    Matches,
    /// No annotation (not supported, will error)
    None,
}

/// Parsed parameter information
struct ParamInfo {
    /// The parameter name in Rust code
    rust_name: String,
    /// The CLI argument name (may differ from rust_name)
    cli_name: String,
    /// The parameter type
    ty: Type,
    /// What kind of parameter this is
    kind: ParamKind,
}

/// Attribute arguments for #[flag(name = "x")] or #[arg(name = "x")]
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

/// Parse parameter annotations from a PatType
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

/// Extract the parameter name from a Pat
fn extract_param_name(pat: &Pat) -> Result<String> {
    match pat {
        Pat::Ident(ident) => Ok(ident.ident.to_string()),
        _ => Err(Error::new(
            pat.span(),
            "expected identifier pattern for parameter",
        )),
    }
}

/// Check if a type is Option<T>
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Check if a type is Vec<T>
fn is_vec_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Vec";
        }
    }
    false
}

/// Extract the inner type from Option<T> or Vec<T>
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

/// Check if a type is a reference (&T)
fn is_reference_type(ty: &Type) -> bool {
    matches!(ty, Type::Reference(_))
}

/// Check if the return type is Result<(), E> (unit result)
fn is_unit_result(fn_item: &ItemFn) -> bool {
    if let syn::ReturnType::Type(_, ty) = &fn_item.sig.output {
        if let Type::Path(type_path) = ty.as_ref() {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        // Check if the Ok type is ()
                        if let Some(syn::GenericArgument::Type(Type::Tuple(tuple))) =
                            args.args.first()
                        {
                            return tuple.elems.is_empty();
                        }
                    }
                }
            }
        }
    }
    false
}

/// Generate extraction code for a parameter
fn generate_extraction(param: &ParamInfo) -> TokenStream {
    let rust_name = format_ident!("{}", param.rust_name);
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
                // Option<T> -> get_one::<T>().cloned()
                let inner = extract_inner_type(ty).unwrap_or(ty);
                quote! {
                    let #rust_name: #ty = __matches.get_one::<#inner>(#cli_name).cloned();
                }
            } else if is_vec_type(ty) {
                // Vec<T> -> get_many::<T>().map(|v| v.cloned().collect()).unwrap_or_default()
                let inner = extract_inner_type(ty).unwrap_or(ty);
                quote! {
                    let #rust_name: #ty = __matches
                        .get_many::<#inner>(#cli_name)
                        .map(|v| v.cloned().collect())
                        .unwrap_or_default();
                }
            } else {
                // Required T -> get_one::<T>().unwrap().clone()
                quote! {
                    let #rust_name: #ty = __matches.get_one::<#ty>(#cli_name)
                        .expect(concat!("Missing required argument '", #cli_name, "' - ensure clap definition matches handler"))
                        .clone();
                }
            }
        }
        ParamKind::Ctx | ParamKind::Matches | ParamKind::None => {
            // These don't need extraction - they're passed through
            quote! {}
        }
    }
}

/// Generate the call argument for a parameter
fn generate_call_arg(param: &ParamInfo) -> TokenStream {
    let rust_name = format_ident!("{}", param.rust_name);

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
        ParamKind::None => {
            // This shouldn't happen if we validate properly
            quote! { #rust_name }
        }
    }
}

/// Main implementation of the #[handler] macro
pub fn handler_impl(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse the function
    let fn_item: ItemFn = syn::parse2(item)?;

    // Parse any attributes on #[handler(...)]
    // Currently unused but could support options in the future
    let _attr_args: TokenStream = attr;

    let fn_name = &fn_item.sig.ident;
    let wrapper_name = format_ident!("{}__handler", fn_name);
    let fn_vis = &fn_item.vis;

    // Analyze parameters
    let mut params: Vec<ParamInfo> = Vec::new();
    let mut has_ctx = false;
    let mut _has_matches = false;

    for fn_arg in &fn_item.sig.inputs {
        match fn_arg {
            FnArg::Typed(pat_type) => {
                let kind = parse_param_kind(pat_type)?;
                let rust_name = extract_param_name(&pat_type.pat)?;

                // Determine CLI name
                let cli_name = match &kind {
                    ParamKind::Flag { cli_name } | ParamKind::Arg { cli_name } => cli_name
                        .clone()
                        .unwrap_or_else(|| rust_name.replace('_', "-")),
                    _ => rust_name.clone(),
                };

                // Track ctx and matches usage
                if matches!(kind, ParamKind::Ctx) {
                    has_ctx = true;
                }
                if matches!(kind, ParamKind::Matches) {
                    _has_matches = true;
                }

                // Validate parameter annotations
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

    // Generate extraction code
    let extractions: Vec<TokenStream> = params.iter().map(generate_extraction).collect();

    // Generate call arguments
    let call_args: Vec<TokenStream> = params.iter().map(generate_call_arg).collect();

    // Determine wrapper signature
    let _wrapper_params = if has_ctx {
        quote! { __matches: &::clap::ArgMatches, __ctx: &::standout_dispatch::CommandContext }
    } else {
        // Even if has_matches, we still use the simple signature
        quote! { __matches: &::clap::ArgMatches }
    };

    // Get return type
    let return_type = &fn_item.sig.output;

    // Handle unit result specially - wrap in Output::Silent
    let call_and_return = if is_unit_result(&fn_item) {
        quote! {
            #fn_name(#(#call_args),*)?;
            Ok(::standout_dispatch::Output::Silent)
        }
    } else {
        quote! {
            #fn_name(#(#call_args),*)
        }
    };

    // For unit results, we need to change the return type to HandlerResult<()>
    let wrapper_return_type = if is_unit_result(&fn_item) {
        quote! { -> ::standout_dispatch::HandlerResult<()> }
    } else {
        quote! { #return_type }
    };

    // Strip attributes from the original function's parameters
    let mut clean_fn = fn_item.clone();
    for fn_arg in &mut clean_fn.sig.inputs {
        if let FnArg::Typed(pat_type) = fn_arg {
            // Remove our custom attributes
            pat_type.attrs.retain(|attr| {
                !attr.path().is_ident("flag")
                    && !attr.path().is_ident("arg")
                    && !attr.path().is_ident("ctx")
                    && !attr.path().is_ident("matches")
            });
        }
    }

    // Generate the output
    Ok(quote! {
        // Original function (with annotations stripped)
        #clean_fn

        // Generated wrapper
        #fn_vis fn #wrapper_name(__matches: &::clap::ArgMatches, __ctx: &::standout_dispatch::CommandContext) #wrapper_return_type {
            #(#extractions)*
            #call_and_return
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
    fn test_is_vec_type() {
        let ty: Type = syn::parse_quote!(Vec<String>);
        assert!(is_vec_type(&ty));

        let ty: Type = syn::parse_quote!(String);
        assert!(!is_vec_type(&ty));
    }
}
