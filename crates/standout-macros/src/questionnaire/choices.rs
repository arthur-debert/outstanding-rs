use super::attrs::parse_choice_rename;
use super::scoped;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::collections::HashSet;
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result, Variant};

pub fn questionnaire_choices_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let enum_name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "QuestionnaireChoices can only be derived for enums without generics",
        ));
    }

    let variants = match &input.data {
        Data::Enum(data) => &data.variants,
        _ => {
            return Err(Error::new(
                input.span(),
                "QuestionnaireChoices can only be derived for enums",
            ))
        }
    };

    let mut seen_choices = HashSet::new();
    let mut choice_literals = Vec::new();
    let mut parse_arms = Vec::new();
    let mut display_arms = Vec::new();

    for variant in variants {
        let info = ChoiceVariant::new(variant)?;
        if !seen_choices.insert(info.choice.clone()) {
            return Err(Error::new(
                info.choice_span,
                format!("duplicate questionnaire choice '{}'", info.choice),
            ));
        }
        let ident = &info.ident;
        let choice = &info.choice;
        choice_literals.push(quote! { #choice });
        parse_arms.push(quote! { #choice => ::core::result::Result::Ok(Self::#ident) });
        display_arms.push(quote! { Self::#ident => #choice });
    }

    let expanded = quote! {
        impl __standout_input::questionnaire::QuestionnaireChoices for #enum_name {
            fn choices() -> &'static [&'static str] {
                &[#(#choice_literals),*]
            }
        }

        impl ::core::str::FromStr for #enum_name {
            type Err = __standout_input::questionnaire::QuestionnaireChoiceParseError;

            fn from_str(value: &str) -> ::core::result::Result<Self, Self::Err> {
                match value.trim() {
                    #(#parse_arms,)*
                    _ => ::core::result::Result::Err(
                        __standout_input::questionnaire::QuestionnaireChoiceParseError::new(
                            <Self as __standout_input::questionnaire::QuestionnaireChoices>::choices()
                        )
                    ),
                }
            }
        }

        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(match self {
                    #(#display_arms,)*
                })
            }
        }
    };

    Ok(scoped(expanded))
}

struct ChoiceVariant {
    ident: syn::Ident,
    choice: String,
    choice_span: Span,
}

impl ChoiceVariant {
    fn new(variant: &Variant) -> Result<Self> {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(Error::new(
                variant.fields.span(),
                "QuestionnaireChoices variants must be unit variants",
            ));
        }
        let Some((choice, choice_span)) = parse_choice_rename(&variant.attrs)? else {
            return Err(Error::new(
                variant.ident.span(),
                "missing #[question(rename = \"...\")]: every QuestionnaireChoices variant declares its user-facing choice string explicitly",
            ));
        };
        Ok(Self {
            ident: variant.ident.clone(),
            choice,
            choice_span,
        })
    }
}
