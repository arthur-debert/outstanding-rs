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
