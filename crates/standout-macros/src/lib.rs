//! Proc macros for Standout: compile-time resource embedding, dispatch
//! configuration, tabular/seeker/questionnaire derives, and handler/command
//! attribute macros.

mod command;
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

/// Registers an enum's variants as commands on a `GroupBuilder`.
///
/// A variant registers under its kebab-cased name — `ListUnits` becomes
/// `list-units`, the name clap's own derive gives the subcommand — and
/// `#[dispatch(name = "...")]` renames it. The handler defaults to
/// `<handlers>::<variant_in_snake_case>`, a plain
/// `fn(&ArgMatches, &CommandContext) -> HandlerResult<T>`; `#[dispatch(pure)]`
/// registers the wrapper `#[handler]` generates for that function instead,
/// which is how a `#[handler]`-annotated function is registered under any
/// `#[dispatch(...)]` form, questionnaire commands included.
///
/// A renamed command is one command: dispatch splits registration paths on
/// `.`, so a `name` carrying one is rejected, and nesting is
/// `#[dispatch(nested)]`. A variant may carry several `#[dispatch(...)]`
/// attributes; they all speak for that variant, so their values merge and the
/// pairs that cannot both hold are rejected across them.
#[proc_macro_derive(Dispatch, attributes(dispatch))]
pub fn dispatch_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    dispatch::dispatch_derive_impl(input)
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

/// Adapts a function into a dispatch handler, generating a
/// `<name>__handler` wrapper that reads the function's parameters out of
/// `ArgMatches`.
///
/// One rule maps a parameter to the clap argument it reads: underscores in the
/// parameter name become hyphens, so `no_legend` reads the argument whose id is
/// `no-legend`. Clap's derive ids an argument by the *field* name, so a
/// clap-derive `no_legend` field is declared `#[arg(id = "no-legend")]`, or the
/// handler parameter names the id itself with `#[flag(name = "no_legend")]` /
/// `#[arg(name = "...")]`. `app.verify_command(&cmd)` reports a mismatch before
/// the argument is read. A raw identifier drops its `r#` first, as clap's
/// derive does for a field: `r#type` reads the argument id `type`.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    handler::handler_impl(attr, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    command::command_impl(attr, item)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
