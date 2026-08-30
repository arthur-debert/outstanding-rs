use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result};

use super::attrs::{parse_seek_attrs, SeekType};

struct FieldInfo {
    query_name: String,
    seek_type: SeekType,
    field_ident: syn::Ident,
}

pub fn seekable_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(Error::new(
                    input.span(),
                    "Seekable can only be derived for structs with named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new(
                input.span(),
                "Seekable can only be derived for structs",
            ))
        }
    };

    let mut field_infos: Vec<FieldInfo> = Vec::new();

    for field in fields.iter() {
        let field_name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;

        let seek_attrs = parse_seek_attrs(&field.attrs)?;

        if seek_attrs.skip {
            continue;
        }

        let seek_type = match seek_attrs.seek_type {
            Some(t) => t,
            None => continue,
        };

        let query_name = seek_attrs.rename.unwrap_or_else(|| field_name.to_string());

        field_infos.push(FieldInfo {
            query_name,
            seek_type,
            field_ident: field_name.clone(),
        });
    }

    let field_constants: Vec<TokenStream> = field_infos
        .iter()
        .map(|info| {
            let const_name = format_ident!("{}", to_screaming_snake_case(&info.query_name));
            let query_name = &info.query_name;
            quote! {
                pub const #const_name: &'static str = #query_name;
            }
        })
        .collect();

    let field_matches: Vec<TokenStream> = field_infos
        .iter()
        .map(|info| {
            let query_name = &info.query_name;
            let field_ident = &info.field_ident;
            let value_expr = match info.seek_type {
                SeekType::String => {
                    quote! { ::standout_seeker::Value::String(&self.#field_ident) }
                }
                SeekType::Number => {
                    quote! { ::standout_seeker::Value::Number(::standout_seeker::Number::from(self.#field_ident)) }
                }
                SeekType::Timestamp => {
                    quote! {
                        ::standout_seeker::Value::Timestamp(
                            ::standout_seeker::SeekerTimestamp::seeker_timestamp(&self.#field_ident)
                        )
                    }
                }
                SeekType::Enum => {
                    quote! {
                        ::standout_seeker::Value::Enum(
                            ::standout_seeker::SeekerEnum::seeker_discriminant(&self.#field_ident)
                        )
                    }
                }
                SeekType::Bool => {
                    quote! { ::standout_seeker::Value::Bool(self.#field_ident) }
                }
            };
            quote! {
                #query_name => #value_expr,
            }
        })
        .collect();

    let schema_field_type_matches: Vec<TokenStream> = field_infos
        .iter()
        .map(|info| {
            let query_name = &info.query_name;
            let seek_type_token = match info.seek_type {
                SeekType::String => quote! { ::standout_seeker::SeekType::String },
                SeekType::Number => quote! { ::standout_seeker::SeekType::Number },
                SeekType::Timestamp => quote! { ::standout_seeker::SeekType::Timestamp },
                SeekType::Enum => quote! { ::standout_seeker::SeekType::Enum },
                SeekType::Bool => quote! { ::standout_seeker::SeekType::Bool },
            };
            quote! {
                #query_name => ::core::option::Option::Some(#seek_type_token),
            }
        })
        .collect();

    let field_name_literals: Vec<&str> = field_infos
        .iter()
        .map(|info| info.query_name.as_str())
        .collect();

    let expanded = quote! {
        impl #struct_name {
            #(#field_constants)*
        }

        impl ::standout_seeker::Seekable for #struct_name {
            fn seeker_field_value(&self, field: &str) -> ::standout_seeker::Value<'_> {
                match field {
                    #(#field_matches)*
                    _ => ::standout_seeker::Value::None,
                }
            }
        }

        impl ::standout_seeker::SeekerSchema for #struct_name {
            fn field_type(field: &str) -> ::core::option::Option<::standout_seeker::SeekType> {
                match field {
                    #(#schema_field_type_matches)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_names() -> &'static [&'static str] {
                &[#(#field_name_literals),*]
            }
        }
    };

    Ok(expanded)
}

fn to_screaming_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    let mut prev_was_lower = false;

    for c in s.chars() {
        if c.is_uppercase() {
            if prev_was_lower {
                result.push('_');
            }
            result.push(c);
            prev_was_lower = false;
        } else if c == '_' || c == '-' {
            result.push('_');
            prev_was_lower = false;
        } else {
            result.push(c.to_ascii_uppercase());
            prev_was_lower = true;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screaming_snake_case() {
        assert_eq!(to_screaming_snake_case("name"), "NAME");
        assert_eq!(to_screaming_snake_case("created_at"), "CREATED_AT");
        assert_eq!(to_screaming_snake_case("createdAt"), "CREATED_AT");
        assert_eq!(to_screaming_snake_case("my-field"), "MY_FIELD");
        assert_eq!(to_screaming_snake_case("XMLParser"), "XMLPARSER");
    }
}
