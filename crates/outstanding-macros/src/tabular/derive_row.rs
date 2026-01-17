//! Implementation of the `#[derive(TabularRow)]` macro.
//!
//! This macro generates optimized row extraction without JSON serialization.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result};

use super::attrs::parse_col_attrs;

/// Main implementation of the TabularRow derive macro.
pub fn tabular_row_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    // Ensure we have a struct with named fields
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

    // Collect field accessors for non-skipped fields
    let mut field_conversions: Vec<TokenStream> = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;

        // Parse field attributes
        let col_attrs = parse_col_attrs(&field.attrs)?;

        // Skip if marked with #[col(skip)]
        if col_attrs.skip {
            continue;
        }

        // Generate the field conversion
        // We use ToString trait which is implemented for all Display types
        field_conversions.push(quote! {
            self.#field_name.to_tabular_cell()
        });
    }

    // Generate the impl block
    let expanded = quote! {
        impl ::outstanding::tabular::TabularRow for #struct_name {
            fn to_row(&self) -> Vec<String> {
                use ::outstanding::tabular::{TabularFieldDisplay, TabularFieldOption}; // Import helper traits
                vec![
                    #(#field_conversions),*
                ]
            }
        }
    };

    Ok(expanded)
}
