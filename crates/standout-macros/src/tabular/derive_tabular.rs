//! Implementation of the `#[derive(Tabular)]` macro.
//!
//! This macro generates a `TabularSpec` from struct field annotations.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result};

use super::attrs::{
    generate_align_tokens, generate_anchor_tokens, generate_overflow_tokens, generate_width_tokens,
    parse_col_attrs, parse_tabular_attrs,
};

/// Main implementation of the Tabular derive macro.
pub fn tabular_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    // Parse container attributes
    let container_attrs = parse_tabular_attrs(&input.attrs)?;

    // Ensure we have a struct with named fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(Error::new(
                    input.span(),
                    "Tabular can only be derived for structs with named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new(
                input.span(),
                "Tabular can only be derived for structs",
            ))
        }
    };

    // Generate column definitions for each field
    let mut column_tokens: Vec<TokenStream> = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
        let field_name_str = field_name.to_string();

        // Parse field attributes
        let col_attrs = parse_col_attrs(&field.attrs)?;

        // Skip if marked with #[col(skip)]
        if col_attrs.skip {
            continue;
        }

        // Generate width tokens
        let width_tokens = generate_width_tokens(&col_attrs);

        // Generate align tokens
        let align_tokens = generate_align_tokens(&col_attrs.align)?;

        // Generate anchor tokens
        let anchor_tokens = generate_anchor_tokens(&col_attrs.anchor)?;

        // Generate overflow tokens
        let overflow_tokens = generate_overflow_tokens(&col_attrs)?;

        // Generate style tokens
        let style_tokens = match &col_attrs.style {
            Some(s) => quote! { Some(#s.to_string()) },
            None => quote! { None },
        };

        // Generate style_from_value
        let style_from_value = col_attrs.style_from_value;

        // Generate header tokens (use header if specified, otherwise field name)
        let header_tokens = match &col_attrs.header {
            Some(h) => quote! { Some(#h.to_string()) },
            None => quote! { Some(#field_name_str.to_string()) },
        };

        // Generate null_repr tokens
        let null_repr_tokens = match &col_attrs.null_repr {
            Some(n) => quote! { #n.to_string() },
            None => quote! { "-".to_string() },
        };

        // Generate key tokens (use key if specified, otherwise field name)
        let key_tokens = match &col_attrs.key {
            Some(k) => quote! { Some(#k.to_string()) },
            None => quote! { Some(#field_name_str.to_string()) },
        };

        // Generate the Column construction
        column_tokens.push(quote! {
            ::standout::tabular::Column {
                name: Some(#field_name_str.to_string()),
                width: #width_tokens,
                align: #align_tokens,
                anchor: #anchor_tokens,
                overflow: #overflow_tokens,
                null_repr: #null_repr_tokens,
                style: #style_tokens,
                style_from_value: #style_from_value,
                key: #key_tokens,
                header: #header_tokens,
            }
        });
    }

    // Generate decorations
    let separator = container_attrs.separator.as_deref().unwrap_or("  ");
    let prefix = container_attrs.prefix.as_deref().unwrap_or("");
    let suffix = container_attrs.suffix.as_deref().unwrap_or("");

    // Generate the impl block
    let expanded = quote! {
        impl ::standout::tabular::Tabular for #struct_name {
            fn tabular_spec() -> ::standout::tabular::TabularSpec {
                ::standout::tabular::TabularSpec {
                    columns: vec![
                        #(#column_tokens),*
                    ],
                    decorations: ::standout::tabular::Decorations {
                        column_sep: #separator.to_string(),
                        row_prefix: #prefix.to_string(),
                        row_suffix: #suffix.to_string(),
                    },
                }
            }
        }
    };

    Ok(expanded)
}
