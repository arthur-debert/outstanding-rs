//! Proc macros for Standout: compile-time resource embedding, dispatch
//! configuration, tabular/seeker/questionnaire derives, and the handler
//! attribute macro.

mod contract;
mod crate_path;
mod dispatch;
mod embed;
mod handler;
mod ident;
mod questionnaire;
mod seeker;
mod tabular;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, LitStr};

#[proc_macro]
pub fn embed_templates(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    embed::embed_templates_impl(path_lit).into()
}

#[proc_macro]
pub fn embed_styles(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    embed::embed_styles_impl(path_lit).into()
}

/// Registers an enum's variants as commands on a `GroupBuilder`; the
/// `#[dispatch(...)]` forms are documented in `docs/topics/dispatch-attributes.md`.
#[proc_macro_derive(Dispatch, attributes(dispatch))]
pub fn dispatch_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    dispatch::dispatch_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// `#[contract(schema_version = N)]` sets `ContractSurface::SCHEMA_VERSION`,
/// the number `Envelope<T>` stamps beside the data in structured output.
#[proc_macro_derive(ContractSurface, attributes(contract))]
pub fn contract_surface_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    contract::contract_surface_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_derive(Tabular, attributes(col, tabular))]
pub fn tabular_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    tabular::tabular_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_derive(TabularRow, attributes(col))]
pub fn tabular_row_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    tabular::tabular_row_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_derive(Seekable, attributes(seek))]
pub fn seekable_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    seeker::seekable_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_derive(Questionnaire, attributes(question))]
pub fn questionnaire_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    questionnaire::questionnaire_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_derive(QuestionnaireChoices, attributes(question))]
pub fn questionnaire_choices_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    questionnaire::questionnaire_choices_derive_impl(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Generates a `<name>__handler` wrapper that reads the function's parameters
/// out of `ArgMatches`; the parameter-to-argument rule is documented in
/// `docs/topics/dispatch-attributes.md`.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    handler::handler_impl(attr, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
