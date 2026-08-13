//! Implementation of the `#[derive(Questionnaire)]` macro.
//!
//! The derive lowers a flat scalar struct to `standout-input`'s public
//! questionnaire builder and emits direct typed filling from decoded answers.

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Data, DeriveInput, Error, Expr, ExprLit, Field, Fields, GenericArgument, Lit, Meta,
    PathArguments, Result, Token, Type,
};

/// Parsed `#[question(...)]` attributes.
#[derive(Debug, Default)]
struct QuestionAttr {
    id: Option<(String, Span)>,
    default: Option<(String, Span)>,
    prose: Option<Span>,
}

impl Parse for QuestionAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attr = QuestionAttr::default();
        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match meta {
                Meta::NameValue(nv) if nv.path.is_ident("id") => {
                    let (value, span) = string_lit(&nv.value, "id must be a string literal")?;
                    if attr.id.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question id attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("default") => {
                    let (value, span) = string_lit(&nv.value, "default must be a string literal")?;
                    if attr.default.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question default attribute",
                        ));
                    }
                }
                Meta::Path(path) if path.is_ident("prose") => {
                    if attr.prose.replace(path.span()).is_some() {
                        return Err(Error::new(
                            path.span(),
                            "duplicate question prose attribute",
                        ));
                    }
                }
                other => {
                    return Err(Error::new(
                        other.span(),
                        "unknown question attribute: expected id, default, or prose",
                    ));
                }
            }
        }

        Ok(attr)
    }
}

/// Main implementation of the Questionnaire derive macro.
pub fn questionnaire_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let container = parse_question_attrs(&input.attrs)?;
    let questionnaire_id = container
        .id
        .as_ref()
        .ok_or_else(|| Error::new(input.ident.span(), "missing #[question(id = \"...\")]"))?
        .0
        .clone();
    if container.default.is_some() || container.prose.is_some() {
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
        builder_fields.push(info.builder_tokens());
        fill_fields.push(info.fill_tokens());
    }

    let expanded = quote! {
        impl #impl_generics ::standout_input::questionnaire::QuestionnaireInput for #struct_name #ty_generics #where_clause {
            fn questionnaire() -> ::core::result::Result<
                ::standout_input::questionnaire::Questionnaire,
                ::standout_input::questionnaire::QuestionnaireError,
            > {
                ::standout_input::questionnaire::Questionnaire::new(
                    #questionnaire_id,
                    vec![
                        #(#builder_fields),*
                    ],
                )
            }

            fn from_decoded_answers(
                answers: &::standout_input::questionnaire::Answers,
            ) -> Self {
                Self {
                    #(#fill_fields),*
                }
            }
        }
    };

    Ok(expanded)
}

#[derive(Debug)]
struct FieldInfo {
    ident: syn::Ident,
    id: String,
    id_span: Span,
    prompt: String,
    default: Option<String>,
    kind: ScalarKind,
    optional: bool,
}

impl FieldInfo {
    fn new(field: &Field) -> Result<Self> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
        let attrs = parse_question_attrs(&field.attrs)?;
        let (base_ty, optional) = option_inner(&field.ty).unwrap_or((&field.ty, false));
        let kind = ScalarKind::from_type(base_ty)?;

        if let Some(prose_span) = attrs.prose {
            if kind != ScalarKind::String {
                return Err(Error::new(
                    prose_span,
                    "prose is only supported on String fields",
                ));
            }
        }

        if let Some((default, span)) = attrs.default.as_ref() {
            if kind == ScalarKind::Bool && parse_bool(default).is_none() {
                return Err(Error::new(
                    *span,
                    "bool defaults must be one of true, false, yes, no, y, or n",
                ));
            }
        }

        let id = attrs
            .id
            .as_ref()
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| ident.to_string());
        let id_span = attrs
            .id
            .as_ref()
            .map(|(_, span)| *span)
            .unwrap_or(ident.span());
        let prompt = doc_prompt(&field.attrs);
        let kind = if attrs.prose.is_some() {
            ScalarKind::Text
        } else {
            kind
        };

        Ok(Self {
            ident,
            id,
            id_span,
            prompt,
            default: attrs.default.map(|(default, _)| default),
            kind,
            optional,
        })
    }

    fn builder_tokens(&self) -> TokenStream {
        let id = &self.id;
        let prompt = &self.prompt;
        let kind = self.kind.tokens();
        let optional = self.optional.then(|| quote! { .optional() });
        let default = self
            .default
            .as_ref()
            .map(|default| quote! { .with_default(#default) });

        quote! {
            ::standout_input::questionnaire::ScalarField::new(
                #id,
                #prompt,
                #kind,
            )
            #optional
            #default
        }
    }

    fn fill_tokens(&self) -> TokenStream {
        let ident = &self.ident;
        let id = &self.id;
        let missing = format!("decoded answers are missing required field '{}'", self.id);
        let value = match (self.kind, self.optional) {
            (ScalarKind::Bool, false) => quote! {
                answers.get_bool(#id).unwrap_or_else(|| unreachable!(#missing))
            },
            (ScalarKind::Bool, true) => quote! {
                answers.get_bool(#id)
            },
            (ScalarKind::Path, false) => quote! {
                ::std::path::PathBuf::from(
                    answers.get_text(#id).unwrap_or_else(|| unreachable!(#missing))
                )
            },
            (ScalarKind::Path, true) => quote! {
                answers.get_text(#id).map(::std::path::PathBuf::from)
            },
            (ScalarKind::String | ScalarKind::Text, false) => quote! {
                answers
                    .get_text(#id)
                    .unwrap_or_else(|| unreachable!(#missing))
                    .to_string()
            },
            (ScalarKind::String | ScalarKind::Text, true) => quote! {
                answers.get_text(#id).map(|value| value.to_string())
            },
        };
        quote! { #ident: #value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    String,
    Text,
    Bool,
    Path,
}

impl ScalarKind {
    fn from_type(ty: &Type) -> Result<Self> {
        let Type::Path(type_path) = ty else {
            return Err(Error::new(
                ty.span(),
                "unsupported questionnaire field type; expected String, PathBuf, bool, or Option<T>",
            ));
        };
        let ident = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| Error::new(ty.span(), "unsupported questionnaire field type"))?
            .ident
            .to_string();
        match ident.as_str() {
            "String" => Ok(Self::String),
            "PathBuf" => Ok(Self::Path),
            "bool" => Ok(Self::Bool),
            _ => Err(Error::new(
                ty.span(),
                "unsupported questionnaire field type; expected String, PathBuf, bool, or Option<T>",
            )),
        }
    }

    fn tokens(self) -> TokenStream {
        match self {
            Self::String => quote! { ::standout_input::questionnaire::ScalarKind::String },
            Self::Text => quote! { ::standout_input::questionnaire::ScalarKind::Text },
            Self::Bool => quote! { ::standout_input::questionnaire::ScalarKind::Bool },
            Self::Path => quote! { ::standout_input::questionnaire::ScalarKind::Path },
        }
    }
}

fn option_inner(ty: &Type) -> Option<(&Type, bool)> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some((inner, true)),
        _ => None,
    }
}

fn parse_question_attrs(attrs: &[Attribute]) -> Result<QuestionAttr> {
    let mut out = QuestionAttr::default();
    for attr in attrs {
        if attr.path().is_ident("question") {
            let parsed = attr.parse_args::<QuestionAttr>()?;
            merge_question_attrs(&mut out, parsed, attr.span())?;
        }
    }
    Ok(out)
}

fn merge_question_attrs(out: &mut QuestionAttr, next: QuestionAttr, span: Span) -> Result<()> {
    if next.id.is_some() && out.id.replace(next.id.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question id attribute"));
    }
    if next.default.is_some() && out.default.replace(next.default.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question default attribute"));
    }
    if next.prose.is_some() && out.prose.replace(next.prose.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question prose attribute"));
    }
    Ok(())
}

fn string_lit(expr: &Expr, message: &str) -> Result<(String, Span)> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(lit), ..
        }) => Ok((lit.value(), lit.span())),
        _ => Err(Error::new(expr.span(), message)),
    }
}

fn doc_prompt(attrs: &[Attribute]) -> String {
    let mut paragraph = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        let Ok((line, _)) = string_lit(&nv.value, "doc attribute must be a string literal") else {
            continue;
        };
        let line = line.trim();
        if line.is_empty() {
            if paragraph.is_empty() {
                continue;
            }
            break;
        }
        paragraph.push(line.to_string());
    }
    paragraph.join(" ")
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" => Some(true),
        "false" | "no" | "n" => Some(false),
        _ => None,
    }
}
