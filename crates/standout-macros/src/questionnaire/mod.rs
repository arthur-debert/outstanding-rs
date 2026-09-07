//! Implementation of the questionnaire derive macros.
//!
//! `Questionnaire` lowers scalar fields, marked enum-choice fields, nested
//! questionnaire structs, and repeatable groups to `standout-input`'s public
//! questionnaire builder, then emits direct typed filling from decoded answers.
//! `QuestionnaireChoices` lowers a unit-variant enum to the choice vocabulary
//! consumed by `#[question(choice)]` fields.
//!
//! Both derives name the runtime through `__standout_input`, an alias bound
//! inside the const block that wraps every expansion, so a consumer needs
//! either `standout-input` or `standout`, not both.

mod attrs;
mod choices;
mod field;
mod field_type;

use attrs::QuestionAttr;
pub use choices::questionnaire_choices_derive_impl;
use field::FieldInfo;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::HashSet;
use syn::{spanned::Spanned, Data, DeriveInput, Error, Fields, Result};

fn scoped(expanded: TokenStream) -> TokenStream {
    let input_crate = crate::crate_path::input();
    quote! {
        const _: () = {
            use #input_crate as __standout_input;

            #expanded
        };
    }
}

pub fn questionnaire_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let container = QuestionAttr::from_attrs(&input.attrs)?;
    let questionnaire_id = container
        .id
        .as_ref()
        .ok_or_else(|| Error::new(input.ident.span(), "missing #[question(id = \"...\")]"))?
        .0
        .clone();
    if container.default.is_some()
        || container.prose.is_some()
        || container.choice.is_some()
        || container.min.is_some()
        || container.max.is_some()
        || container.active_when.is_some()
        || container.default_with.is_some()
        || container.validate.is_some()
        || container.revision.is_some()
    {
        return Err(Error::new(
            input.span(),
            "container #[question(...)] only supports id",
        ));
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(Error::new(
                    input.span(),
                    "Questionnaire can only be derived for structs with named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new(
                input.span(),
                "Questionnaire can only be derived for structs",
            ))
        }
    };

    let field_ids = fields
        .iter()
        .map(|field| {
            let ident = field
                .ident
                .clone()
                .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
            let attrs = QuestionAttr::from_attrs(&field.attrs)?;
            let id = attrs
                .id
                .as_ref()
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| ident.to_string());
            Ok((ident.to_string(), id))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut seen_ids = HashSet::new();
    let mut builder_fields = Vec::new();
    let mut fill_fields = Vec::new();

    for field in fields {
        let info = FieldInfo::new(field)?;
        if !seen_ids.insert(info.id.clone()) {
            return Err(Error::new(
                info.id_span,
                format!("duplicate question id '{}'", info.id),
            ));
        }
        builder_fields.push(info.builder_tokens(&field_ids)?);
        fill_fields.push(info.fill_tokens());
    }

    let expanded = quote! {
        impl #impl_generics __standout_input::questionnaire::QuestionnaireInput for #struct_name #ty_generics #where_clause {
            fn questionnaire() -> ::core::result::Result<
                __standout_input::questionnaire::Questionnaire,
                __standout_input::questionnaire::QuestionnaireError,
            > {
                __standout_input::questionnaire::Questionnaire::new(
                    #questionnaire_id,
                    <Self as __standout_input::questionnaire::QuestionnaireInput>::questionnaire_items(""),
                )
            }

            fn from_decoded_answers(
                answers: &__standout_input::questionnaire::Answers,
            ) -> Self {
                <Self as __standout_input::questionnaire::QuestionnaireInput>::from_decoded_answers_at(
                    answers,
                    "",
                )
            }

            fn questionnaire_items(__prefix: &str) -> ::std::vec::Vec<__standout_input::questionnaire::Item> {
                vec![
                    #(#builder_fields),*
                ]
            }

            fn from_decoded_answers_at(
                answers: &__standout_input::questionnaire::Answers,
                __prefix: &str,
            ) -> Self {
                Self {
                    #(#fill_fields),*
                }
            }
        }
    };

    Ok(scoped(expanded))
}
