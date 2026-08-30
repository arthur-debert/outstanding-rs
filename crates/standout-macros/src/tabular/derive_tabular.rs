use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result};

use super::attrs::{
    generate_align_tokens, generate_anchor_tokens, generate_overflow_tokens, generate_width_tokens,
    parse_col_attrs, parse_tabular_attrs,
};

pub fn tabular_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    let container_attrs = parse_tabular_attrs(&input.attrs)?;

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

    let mut column_tokens: Vec<TokenStream> = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
        let field_name_str = field_name.to_string();

        let col_attrs = parse_col_attrs(&field.attrs)?;

        if col_attrs.skip {
            continue;
        }

        let width_tokens = generate_width_tokens(&col_attrs);

        let align_tokens = generate_align_tokens(&col_attrs.align)?;

        let anchor_tokens = generate_anchor_tokens(&col_attrs.anchor)?;

        let overflow_tokens = generate_overflow_tokens(&col_attrs)?;

        let style_tokens = match &col_attrs.style {
            Some(s) => quote! { Some(#s.to_string()) },
            None => quote! { None },
        };

        let style_from_value = col_attrs.style_from_value;

        let header_tokens = match &col_attrs.header {
            Some(h) => quote! { Some(#h.to_string()) },
            None => quote! { Some(#field_name_str.to_string()) },
        };

        let null_repr_tokens = match &col_attrs.null_repr {
            Some(n) => quote! { #n.to_string() },
            None => quote! { "-".to_string() },
        };

        let key_tokens = match &col_attrs.key {
            Some(k) => quote! { Some(#k.to_string()) },
            None => quote! { Some(#field_name_str.to_string()) },
        };

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
                sub_columns: None,
            }
        });
    }

    let separator = container_attrs.separator.as_deref().unwrap_or("  ");
    let prefix = container_attrs.prefix.as_deref().unwrap_or("");
    let suffix = container_attrs.suffix.as_deref().unwrap_or("");

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
