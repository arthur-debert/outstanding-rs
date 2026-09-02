use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, DeriveInput, Error, Expr, ExprLit, Lit, Meta, Result};

use crate::crate_path;

pub(crate) fn contract_surface_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let version = schema_version(&input)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let dispatch = crate_path::dispatch();
    Ok(quote! {
        impl #impl_generics #dispatch::ContractSurface for #name #ty_generics #where_clause {
            const SCHEMA_VERSION: u32 = #version;
        }
    })
}

fn schema_version(input: &DeriveInput) -> Result<u32> {
    let mut version = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("contract")) {
        let entries = attr.parse_args_with(
            syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
        )?;
        for meta in entries {
            let Meta::NameValue(pair) = &meta else {
                return Err(Error::new(
                    meta.span(),
                    "expected `#[contract(schema_version = N)]`",
                ));
            };
            if !pair.path.is_ident("schema_version") {
                return Err(Error::new(
                    pair.path.span(),
                    "unknown `contract` key; the only key is `schema_version`",
                ));
            }
            if version.is_some() {
                return Err(Error::new(
                    pair.span(),
                    "`schema_version` is declared more than once",
                ));
            }
            let Expr::Lit(ExprLit {
                lit: Lit::Int(literal),
                ..
            }) = &pair.value
            else {
                return Err(Error::new(
                    pair.value.span(),
                    "`schema_version` must be an integer literal",
                ));
            };
            version = Some(literal.base10_parse::<u32>()?);
        }
    }
    version.ok_or_else(|| {
        Error::new(
            input.ident.span(),
            "`#[derive(ContractSurface)]` needs `#[contract(schema_version = N)]`",
        )
    })
}
