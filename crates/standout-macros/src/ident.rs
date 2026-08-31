//! Identifiers built from strings the macro did not choose.
//!
//! A derived name comes from a variant, a function parameter or a crate alias,
//! and any of those can be a Rust keyword — `move`, `type` — which
//! `Ident::new` rejects by panicking mid-expansion. Building them here turns a
//! keyword into a raw identifier instead.

use proc_macro2::{Ident, Span};

pub(crate) fn safe_ident(name: &str, span: Span) -> Ident {
    match syn::parse_str::<Ident>(name) {
        Ok(mut ident) => {
            ident.set_span(span);
            ident
        }
        Err(_) => Ident::new_raw(name.strip_prefix("r#").unwrap_or(name), span),
    }
}
