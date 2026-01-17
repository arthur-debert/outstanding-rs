//! Implementation of the `#[derive(Tabular)]` macro.
//!
//! This macro generates a `TabularSpec` from struct field annotations.

// Allow dead_code during incremental development - this will be used when the macro is registered
#![allow(dead_code)]

use proc_macro2::TokenStream;
use syn::{DeriveInput, Result};

/// Main implementation of the Tabular derive macro.
///
/// This is a stub that will be fully implemented in Phase 2.
pub fn tabular_derive_impl(_input: DeriveInput) -> Result<TokenStream> {
    // Phase 2 will implement this
    Ok(TokenStream::new())
}
