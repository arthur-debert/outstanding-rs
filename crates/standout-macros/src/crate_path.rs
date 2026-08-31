//! Resolves the crate a macro expansion names.
//!
//! A consumer either depends on a leaf crate (`standout-input`,
//! `standout-dispatch`) or only on `standout`, which re-exports both. The
//! expansion has to name whichever one the consuming crate actually declared,
//! so each path is read from that crate's manifest at expansion time rather
//! than hard-coded.

use crate::ident::safe_ident;
use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;

pub(crate) fn input() -> TokenStream {
    resolve("standout-input", quote! { input })
}

pub(crate) fn dispatch() -> TokenStream {
    resolve("standout-dispatch", quote! { dispatch })
}

fn resolve(leaf: &str, re_export: TokenStream) -> TokenStream {
    match crate_name(leaf) {
        Ok(FoundCrate::Itself) => quote! { crate },
        Ok(FoundCrate::Name(name)) => {
            let ident = ident(&name);
            quote! { ::#ident }
        }
        Err(_) => match crate_name("standout") {
            Ok(FoundCrate::Itself) => quote! { crate::#re_export },
            Ok(FoundCrate::Name(name)) => {
                let ident = ident(&name);
                quote! { ::#ident::#re_export }
            }
            Err(_) => {
                let ident = ident(leaf);
                quote! { ::#ident }
            }
        },
    }
}

fn ident(crate_name: &str) -> proc_macro2::Ident {
    safe_ident(&crate_name.replace('-', "_"), Span::call_site())
}
