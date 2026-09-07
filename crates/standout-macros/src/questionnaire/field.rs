use super::attrs::{doc_prompt, ActiveWhenAttr, QuestionAttr};
use super::field_type::{option_inner, FieldKind, ScalarKind};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{spanned::Spanned, Error, Field, Path, Result, Type};

pub(super) struct FieldInfo {
    ident: syn::Ident,
    pub(super) id: String,
    pub(super) id_span: Span,
    prompt: String,
    default: Option<String>,
    kind: FieldKind,
    optional: bool,
    min: Option<usize>,
    max: Option<usize>,
    active_when: Option<ActiveWhenAttr>,
    default_with: Option<Path>,
    validate: Option<Path>,
    revision: Option<String>,
}

impl FieldInfo {
    pub(super) fn new(field: &Field) -> Result<Self> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
        let attrs = QuestionAttr::from_attrs(&field.attrs)?;
        let (base_ty, optional) = option_inner(&field.ty).unwrap_or((&field.ty, false));
        let mut kind = FieldKind::from_type(base_ty, &attrs, optional)?;

        if let Some(prose_span) = attrs.prose {
            if !matches!(
                kind,
                FieldKind::Scalar {
                    scalar: ScalarKind::String
                }
            ) {
                return Err(Error::new(
                    prose_span,
                    "prose is only supported on String fields",
                ));
            }
            kind = FieldKind::Scalar {
                scalar: ScalarKind::Text,
            };
        }

        if attrs.default.is_some() && attrs.default_with.is_some() {
            let span = attrs
                .default_with
                .as_ref()
                .map(|(_, span)| *span)
                .expect("checked existing default_with");
            return Err(Error::new(
                span,
                "default and default_with cannot be declared on the same field",
            ));
        }

        if let Some((default, span)) = attrs.default.as_ref() {
            match kind {
                FieldKind::Scalar {
                    scalar: ScalarKind::Bool,
                } if parse_bool(default).is_none() => {
                    return Err(Error::new(
                        *span,
                        "bool defaults must be one of true, false, yes, no, y, or n",
                    ));
                }
                FieldKind::Scalar { .. } | FieldKind::Choice { .. } => {}
                _ => {
                    return Err(Error::new(
                        *span,
                        "default is only supported on scalar fields and choice fields",
                    ));
                }
            }
        }

        if let Some((_, span)) = attrs.default_with.as_ref() {
            if !matches!(kind, FieldKind::Scalar { .. } | FieldKind::Choice { .. }) {
                return Err(Error::new(
                    *span,
                    "default_with is only supported on scalar fields and choice fields",
                ));
            }
        }

        if let Some((_, span)) = attrs.validate.as_ref() {
            if !matches!(kind, FieldKind::Scalar { .. } | FieldKind::Choice { .. }) {
                return Err(Error::new(
                    *span,
                    "validate is only supported on scalar fields and choice fields",
                ));
            }
        }

        if (attrs.default_with.is_some() || attrs.validate.is_some()) && attrs.revision.is_none() {
            let span = attrs
                .default_with
                .as_ref()
                .map(|(_, span)| *span)
                .or_else(|| attrs.validate.as_ref().map(|(_, span)| *span))
                .expect("checked existing hook");
            return Err(Error::new(
                span,
                "default_with and validate require revision = \"...\"",
            ));
        }

        if attrs.revision.is_some() && attrs.default_with.is_none() && attrs.validate.is_none() {
            let span = attrs
                .revision
                .as_ref()
                .map(|(_, span)| *span)
                .expect("checked existing revision");
            return Err(Error::new(
                span,
                "revision is only supported with default_with or validate",
            ));
        }

        if optional && !matches!(kind, FieldKind::Scalar { .. } | FieldKind::Choice { .. }) {
            return Err(Error::new(
                field.ty.span(),
                "Option<T> is only supported for scalar questionnaire fields and choice fields",
            ));
        }

        if let Some(active_when) = attrs.active_when.as_ref() {
            match (&kind, optional) {
                (FieldKind::Scalar { .. } | FieldKind::Choice { .. }, true) => {}
                (FieldKind::Scalar { .. } | FieldKind::Choice { .. }, false) => {
                    return Err(Error::new(
                        active_when.span,
                        "active_when is only supported on Option<T> fields because inactive answers are omitted during decode",
                    ));
                }
                (FieldKind::Nested { .. } | FieldKind::RepeatedNested { .. }, _) => {
                    return Err(Error::new(
                        active_when.span,
                        "active_when is only supported on scalar Option<T> fields and #[question(choice)] Option<T> fields",
                    ));
                }
            }
        }

        if (attrs.min.is_some() || attrs.max.is_some())
            && !matches!(kind, FieldKind::RepeatedNested { .. })
        {
            let span = attrs
                .min
                .as_ref()
                .map(|(_, span)| *span)
                .or_else(|| attrs.max.as_ref().map(|(_, span)| *span))
                .expect("checked an existing min or max");
            return Err(Error::new(
                span,
                "min and max are only supported on repeatable Vec fields",
            ));
        }

        if let Some((min, span)) = attrs.min.as_ref() {
            if *min == 0 {
                return Err(Error::new(
                    *span,
                    "min must be at least 1 for repeatable questionnaire groups",
                ));
            }
        }

        if let Some((max, span)) = attrs.max.as_ref() {
            if *max == 0 {
                return Err(Error::new(
                    *span,
                    "max must be at least 1 for repeatable questionnaire groups",
                ));
            }
        }

        if let (Some((min, _)), Some((max, span))) = (attrs.min.as_ref(), attrs.max.as_ref()) {
            if max < min {
                return Err(Error::new(
                    *span,
                    "max must be greater than or equal to min for repeatable questionnaire groups",
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

        Ok(Self {
            ident,
            id,
            id_span,
            prompt,
            default: attrs.default.map(|(default, _)| default),
            kind,
            optional,
            min: attrs.min.map(|(min, _)| min),
            max: attrs.max.map(|(max, _)| max),
            active_when: attrs.active_when,
            default_with: attrs.default_with.map(|(path, _)| path),
            validate: attrs.validate.map(|(path, _)| path),
            revision: attrs.revision.map(|(revision, _)| revision),
        })
    }

    pub(super) fn builder_tokens(&self, field_ids: &[(String, String)]) -> Result<TokenStream> {
        let id = prefixed_id_tokens(&self.id);
        let prompt = &self.prompt;
        let active_when = self
            .active_when
            .as_ref()
            .map(|active_when| -> Result<TokenStream> {
                let controller_id = field_ids
                    .iter()
                    .find_map(|(name, id)| (*name == active_when.field).then_some(id))
                    .ok_or_else(|| {
                        Error::new(
                            active_when.span,
                            format!(
                                "active_when field '{}' does not name a field of this struct; a controller must be declared in the same derived struct",
                                active_when.field
                            ),
                        )
                    })?;
                let controller = prefixed_id_tokens(controller_id);
                let expected = &active_when.expected;
                Ok(quote! { .active_when(#controller, #expected) })
            })
            .transpose()?;
        let dynamic_default = self.default_with.as_ref().map(|path| {
            let revision = self.revision.as_deref().unwrap_or_default();
            quote! {
                .with_dynamic_default(
                    __standout_input::questionnaire::DynamicDefault::new(#revision, #path)
                )
            }
        });
        let validator = self.validate.as_ref().map(|path| {
            let revision = self.revision.as_deref().unwrap_or_default();
            quote! {
                .with_validator(
                    __standout_input::questionnaire::FieldValidator::new(#revision, #path)
                )
            }
        });
        let tokens = match &self.kind {
            FieldKind::Scalar { scalar } => {
                let kind = scalar.tokens();
                let optional = self.optional.then(|| quote! { .optional() });
                let default = self
                    .default
                    .as_ref()
                    .map(|default| quote! { .with_default(#default) });

                quote! {
                    {
                        let __id = #id;
                        __standout_input::questionnaire::ScalarField::new(
                            __id,
                            #prompt,
                            #kind,
                        )
                        #optional
                        #default
                        #dynamic_default
                        #active_when
                        #validator
                        .into()
                    }
                }
            }
            FieldKind::Choice { ty } => {
                let optional = self.optional.then(|| quote! { .optional() });
                let default = self
                    .default
                    .as_ref()
                    .map(|default| quote! { .with_default(#default) });

                quote! {
                    {
                        let __id = #id;
                        __standout_input::questionnaire::ScalarField::new(
                            __id,
                            #prompt,
                            __standout_input::questionnaire::ScalarKind::String,
                        )
                        #optional
                        .one_of(
                            <#ty as __standout_input::questionnaire::QuestionnaireChoices>::choices()
                                .iter()
                                .copied()
                        )
                        #default
                        #dynamic_default
                        #active_when
                        #validator
                        .into()
                    }
                }
            }
            FieldKind::Nested { ty } => quote! {
                {
                    let __group_id = #id;
                    __standout_input::questionnaire::Group::new(
                        __group_id.clone(),
                        #prompt,
                        <#ty as __standout_input::questionnaire::QuestionnaireInput>::questionnaire_items(
                            &__group_id,
                        ),
                    )
                    .into()
                }
            },
            FieldKind::RepeatedNested { ty } => {
                let repeat = self.repeat_tokens();
                quote! {
                    {
                        let __group_id = #id;
                        __standout_input::questionnaire::Group::new(
                            __group_id.clone(),
                            #prompt,
                            <#ty as __standout_input::questionnaire::QuestionnaireInput>::questionnaire_items(
                                &__group_id,
                            ),
                        )
                        #repeat
                        .into()
                    }
                }
            }
        };
        Ok(tokens)
    }

    pub(super) fn fill_tokens(&self) -> TokenStream {
        let ident = &self.ident;
        let id = prefixed_id_tokens(&self.id);
        let missing = format!("decoded answers are missing required field '{}'", self.id);
        let value = match &self.kind {
            FieldKind::Scalar { scalar } => {
                scalar_value_tokens(*scalar, self.optional, id, missing)
            }
            FieldKind::Choice { ty } => {
                choice_value_tokens(ty, self.optional, id, missing, &self.id)
            }
            FieldKind::Nested { ty } => quote! {
                {
                    let __id = #id;
                    <#ty as __standout_input::questionnaire::QuestionnaireInput>::from_decoded_answers_at(
                        answers,
                        &__id,
                    )
                }
            },
            FieldKind::RepeatedNested { ty } => quote! {
                {
                    let __group_id = #id;
                    let __count = answers.occurrence_count(&__group_id);
                    (0..__count)
                        .map(|__index| {
                            let __occurrence = ::std::format!("{}[{}]", __group_id, __index);
                            <#ty as __standout_input::questionnaire::QuestionnaireInput>::from_decoded_answers_at(
                                answers,
                                &__occurrence,
                            )
                        })
                        .collect()
                }
            },
        };
        quote! { #ident: #value }
    }

    fn repeat_tokens(&self) -> TokenStream {
        let min = self.min.unwrap_or(1);
        let max = self.max.map(|max| quote! { .max_occurrences(#max) });
        quote! {
            .repeatable(#min)
            #max
        }
    }
}

fn scalar_value_tokens(
    kind: ScalarKind,
    optional: bool,
    id: TokenStream,
    missing: String,
) -> TokenStream {
    match (kind, optional) {
        (ScalarKind::Bool, false) => quote! {
            {
                let __id = #id;
                answers.get_bool(&__id).unwrap_or_else(|| unreachable!(#missing))
            }
        },
        (ScalarKind::Bool, true) => quote! {
            {
                let __id = #id;
                answers.get_bool(&__id)
            }
        },
        (ScalarKind::Path, false) => quote! {
            {
                let __id = #id;
                ::std::path::PathBuf::from(
                    answers.get_text(&__id).unwrap_or_else(|| unreachable!(#missing))
                )
            }
        },
        (ScalarKind::Path, true) => quote! {
            {
                let __id = #id;
                answers.get_text(&__id).map(::std::path::PathBuf::from)
            }
        },
        (ScalarKind::String | ScalarKind::Text, false) => quote! {
            {
                let __id = #id;
                answers
                    .get_text(&__id)
                    .unwrap_or_else(|| unreachable!(#missing))
                    .to_string()
            }
        },
        (ScalarKind::String | ScalarKind::Text, true) => quote! {
            {
                let __id = #id;
                answers.get_text(&__id).map(|value| value.to_string())
            }
        },
    }
}

fn choice_value_tokens(
    ty: &Type,
    optional: bool,
    id: TokenStream,
    missing: String,
    field_id: &str,
) -> TokenStream {
    let parse_failure = format!(
        "decoded answers carried an undeclared choice for field '{}'",
        field_id
    );
    if optional {
        quote! {
            {
                let __id = #id;
                answers.get_text(&__id).map(|value| {
                    value
                        .parse::<#ty>()
                        .unwrap_or_else(|_| unreachable!(#parse_failure))
                })
            }
        }
    } else {
        quote! {
            {
                let __id = #id;
                answers
                    .get_text(&__id)
                    .unwrap_or_else(|| unreachable!(#missing))
                    .parse::<#ty>()
                    .unwrap_or_else(|_| unreachable!(#parse_failure))
            }
        }
    }
}

fn prefixed_id_tokens(id: &str) -> TokenStream {
    quote! {
        if __prefix.is_empty() {
            #id.to_string()
        } else {
            ::std::format!("{}.{}", __prefix, #id)
        }
    }
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" => Some(true),
        "false" | "no" | "n" => Some(false),
        _ => None,
    }
}
