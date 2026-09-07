use super::attrs::QuestionAttr;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Error, GenericArgument, PathArguments, Result, Type};

pub(super) enum FieldKind {
    Scalar { scalar: ScalarKind },
    Choice { ty: Type },
    Nested { ty: Type },
    RepeatedNested { ty: Type },
}

impl FieldKind {
    pub(super) fn from_type(ty: &Type, attrs: &QuestionAttr, optional: bool) -> Result<Self> {
        if optional {
            if let Some(scalar) = ScalarKind::from_type(ty) {
                return Ok(Self::Scalar { scalar });
            }
            if attrs.choice.is_some() {
                return choice_kind(ty);
            }
            return Err(Error::new(
                ty.span(),
                "Option<T> is only supported for String, PathBuf, bool, and #[question(choice)] enum fields",
            ));
        }

        if let Some(inner) = vec_inner(ty) {
            if let Some(span) = attrs.choice {
                return Err(Error::new(
                    span,
                    "choice is only supported on non-Vec enum fields",
                ));
            }
            if ScalarKind::from_type(inner).is_some() {
                return Err(Error::new(
                    inner.span(),
                    "unsupported Vec element type; Vec<T> is only supported when T is a nested Questionnaire type (collect a list as a String field and split it in application code)",
                ));
            }
            reject_known_non_questionnaire_type(inner, "unsupported Vec element type")?;
            if attrs.default.is_some() {
                let span = attrs
                    .default
                    .as_ref()
                    .map(|(_, span)| *span)
                    .unwrap_or(ty.span());
                return Err(Error::new(
                    span,
                    "default is only supported on scalar fields and choice fields",
                ));
            }
            Ok(Self::RepeatedNested { ty: inner.clone() })
        } else if let Some(scalar) = ScalarKind::from_type(ty) {
            if let Some(span) = attrs.choice {
                return Err(Error::new(
                    span,
                    "choice is only supported on enum choice fields",
                ));
            }
            Ok(Self::Scalar { scalar })
        } else if attrs.choice.is_some() {
            choice_kind(ty)
        } else {
            reject_known_non_questionnaire_type(
                ty,
                "unsupported questionnaire field type; expected String, PathBuf, bool, Option<T>, a nested Questionnaire type, a Vec of a nested Questionnaire type, or #[question(choice)] enum field",
            )?;
            Ok(Self::Nested { ty: ty.clone() })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScalarKind {
    String,
    Text,
    Bool,
    Path,
}

impl ScalarKind {
    pub(super) fn from_type(ty: &Type) -> Option<Self> {
        let Type::Path(type_path) = ty else {
            return None;
        };
        let ident = type_path.path.segments.last()?.ident.to_string();
        match ident.as_str() {
            "String" => Some(Self::String),
            "PathBuf" => Some(Self::Path),
            "bool" => Some(Self::Bool),
            _ => None,
        }
    }

    pub(super) fn tokens(self) -> TokenStream {
        match self {
            Self::String => quote! { __standout_input::questionnaire::ScalarKind::String },
            Self::Text => quote! { __standout_input::questionnaire::ScalarKind::Text },
            Self::Bool => quote! { __standout_input::questionnaire::ScalarKind::Bool },
            Self::Path => quote! { __standout_input::questionnaire::ScalarKind::Path },
        }
    }
}

fn choice_kind(ty: &Type) -> Result<FieldKind> {
    if is_known_unsupported_primitive(ty) || !can_be_plain_named_type(ty) {
        return Err(Error::new(
            ty.span(),
            "choice is only supported on QuestionnaireChoices enum fields",
        ));
    }
    Ok(FieldKind::Choice { ty: ty.clone() })
}

fn is_known_unsupported_primitive(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };
    matches!(
        segment.ident.to_string().as_str(),
        "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

fn can_be_plain_named_type(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path.qself.is_none()
        && type_path
            .path
            .segments
            .iter()
            .all(|segment| matches!(segment.arguments, PathArguments::None))
}

pub(super) fn option_inner(ty: &Type) -> Option<(&Type, bool)> {
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

fn vec_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Vec" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

fn reject_known_non_questionnaire_type(ty: &Type, message: &str) -> Result<()> {
    let Type::Path(type_path) = ty else {
        return Err(Error::new(ty.span(), message));
    };
    let Some(segment) = type_path.path.segments.last() else {
        return Err(Error::new(ty.span(), message));
    };
    let ident = segment.ident.to_string();
    if matches!(
        ident.as_str(),
        "bool"
            | "char"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "Option"
            | "Vec"
            | "String"
            | "PathBuf"
    ) {
        return Err(Error::new(ty.span(), message));
    }
    Ok(())
}
