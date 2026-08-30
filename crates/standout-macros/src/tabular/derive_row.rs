use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result};

use super::attrs::parse_col_attrs;

pub fn tabular_row_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(Error::new(
                    input.span(),
                    "TabularRow can only be derived for structs with named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new(
                input.span(),
                "TabularRow can only be derived for structs",
            ))
        }
    };

    let mut field_conversions: Vec<TokenStream> = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;

        let col_attrs = parse_col_attrs(&field.attrs)?;

        if col_attrs.skip {
            continue;
        }

        field_conversions.push(quote! {
            self.#field_name.to_tabular_cell()
        });
    }

    let expanded = quote! {
        impl ::standout::tabular::TabularRow for #struct_name {
            fn to_row(&self) -> Vec<String> {
                use ::standout::tabular::{TabularFieldDisplay, TabularFieldOption};
                vec![
                    #(#field_conversions),*
                ]
            }
        }
    };

    Ok(expanded)
}
