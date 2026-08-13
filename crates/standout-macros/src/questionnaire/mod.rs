//! Implementation of the questionnaire derive macros.
//!
//! `Questionnaire` lowers scalar fields, marked enum-choice fields, nested
//! questionnaire structs, and repeatable groups to `standout-input`'s public
//! questionnaire builder, then emits direct typed filling from decoded answers.
//! `QuestionnaireChoices` lowers a unit-variant enum to the choice vocabulary
//! consumed by `#[question(choice)]` fields.

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Data, DeriveInput, Error, Expr, ExprLit, ExprPath, Field, Fields, GenericArgument,
    Lit, Meta, Path, PathArguments, Result, Token, Type, Variant,
};

/// Parsed `#[question(...)]` attributes.
#[derive(Default)]
struct QuestionAttr {
    id: Option<(String, Span)>,
    default: Option<(String, Span)>,
    prose: Option<Span>,
    choice: Option<Span>,
    repeated: Option<Span>,
    min: Option<(usize, Span)>,
    max: Option<(usize, Span)>,
    active_when: Option<ActiveWhenAttr>,
    default_with: Option<(Path, Span)>,
    validate: Option<(Path, Span)>,
    revision: Option<(String, Span)>,
}

#[derive(Default)]
struct ChoiceAttr {
    rename: Option<(String, Span)>,
}

#[derive(Clone)]
struct ActiveWhenAttr {
    field: String,
    expected: String,
    span: Span,
}

impl Parse for ActiveWhenAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;
        let mut field = None;
        let mut expected = None;

        for meta in content {
            match meta {
                Meta::NameValue(nv) if nv.path.is_ident("field") => {
                    let (value, span) =
                        string_lit(&nv.value, "active_when field must be a string literal")?;
                    if field.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate active_when field attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("is") => {
                    let (value, span) =
                        string_lit(&nv.value, "active_when is must be a string literal")?;
                    if expected.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate active_when is attribute",
                        ));
                    }
                }
                other => {
                    return Err(Error::new(
                        other.span(),
                        "unknown active_when attribute: expected field or is",
                    ));
                }
            }
        }

        let (field, _) = field.ok_or_else(|| {
            Error::new(
                input.span(),
                "active_when requires field = \"...\" and is = \"...\"",
            )
        })?;
        let (expected, _) = expected.ok_or_else(|| {
            Error::new(
                input.span(),
                "active_when requires field = \"...\" and is = \"...\"",
            )
        })?;

        Ok(Self {
            field,
            expected,
            span: input.span(),
        })
    }
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
                Meta::Path(path) if path.is_ident("choice") => {
                    if attr.choice.replace(path.span()).is_some() {
                        return Err(Error::new(
                            path.span(),
                            "duplicate question choice attribute",
                        ));
                    }
                }
                Meta::Path(path) if path.is_ident("repeated") => {
                    if attr.repeated.replace(path.span()).is_some() {
                        return Err(Error::new(
                            path.span(),
                            "duplicate question repeated attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("min") => {
                    let (value, span) = usize_lit(&nv.value, "min must be an integer literal")?;
                    if attr.min.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question min attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("max") => {
                    let (value, span) = usize_lit(&nv.value, "max must be an integer literal")?;
                    if attr.max.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question max attribute",
                        ));
                    }
                }
                Meta::List(list) if list.path.is_ident("active_when") => {
                    let mut parsed = list.parse_args::<ActiveWhenAttr>()?;
                    parsed.span = list.path.span();
                    if attr.active_when.replace(parsed).is_some() {
                        return Err(Error::new(
                            list.path.span(),
                            "duplicate question active_when attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("default_with") => {
                    let (path, span) =
                        path_expr(&nv.value, "default_with must be a function path")?;
                    if attr.default_with.replace((path, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question default_with attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("validate") => {
                    let (path, span) = path_expr(&nv.value, "validate must be a function path")?;
                    if attr.validate.replace((path, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question validate attribute",
                        ));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("revision") => {
                    let (value, span) = string_lit(&nv.value, "revision must be a string literal")?;
                    if attr.revision.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question revision attribute",
                        ));
                    }
                }
                other => {
                    return Err(Error::new(
                        other.span(),
                        "unknown question attribute: expected id, default, prose, choice, repeated, min, max, active_when, default_with, validate, or revision",
                    ));
                }
            }
        }

        Ok(attr)
    }
}

impl Parse for ChoiceAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attr = ChoiceAttr::default();
        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match meta {
                Meta::NameValue(nv) if nv.path.is_ident("rename") => {
                    let (value, span) = string_lit(&nv.value, "rename must be a string literal")?;
                    if attr.rename.replace((value, span)).is_some() {
                        return Err(Error::new(
                            nv.path.span(),
                            "duplicate question rename attribute",
                        ));
                    }
                }
                other => {
                    return Err(Error::new(
                        other.span(),
                        "unknown question attribute: expected rename",
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
    if container.default.is_some()
        || container.prose.is_some()
        || container.choice.is_some()
        || container.repeated.is_some()
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

    let mut seen_ids = HashSet::new();
    let mut builder_fields = Vec::new();
    let mut fill_fields = Vec::new();

    let field_ids = fields
        .iter()
        .map(|field| {
            let ident = field
                .ident
                .clone()
                .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
            let attrs = parse_question_attrs(&field.attrs)?;
            let id = attrs
                .id
                .as_ref()
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| ident.to_string());
            Ok((ident.to_string(), id))
        })
        .collect::<Result<Vec<_>>>()?;
    let controller_map_entries = field_ids.iter().map(|(name, id)| {
        quote! {
            (
                #name,
                if __prefix.is_empty() {
                    #id.to_string()
                } else {
                    ::std::format!("{}.{}", __prefix, #id)
                },
            )
        }
    });

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
                    <Self as ::standout_input::questionnaire::QuestionnaireInput>::questionnaire_items(""),
                )
            }

            fn from_decoded_answers(
                answers: &::standout_input::questionnaire::Answers,
            ) -> Self {
                <Self as ::standout_input::questionnaire::QuestionnaireInput>::from_decoded_answers_at(
                    answers,
                    "",
                )
            }

            fn questionnaire_items(__prefix: &str) -> ::std::vec::Vec<::standout_input::questionnaire::Item> {
                <Self as ::standout_input::questionnaire::QuestionnaireInput>::questionnaire_items_with_context(
                    __prefix,
                    &[],
                )
            }

            fn questionnaire_items_with_context(
                __prefix: &str,
                __inherited_controllers: &[(&'static str, ::std::string::String)],
            ) -> ::std::vec::Vec<::standout_input::questionnaire::Item> {
                let mut __controller_ids: ::std::vec::Vec<(&'static str, ::std::string::String)> = vec![
                    #(#controller_map_entries),*
                ];
                __controller_ids.extend_from_slice(__inherited_controllers);
                vec![
                    #(#builder_fields),*
                ]
            }

            fn from_decoded_answers_at(
                answers: &::standout_input::questionnaire::Answers,
                __prefix: &str,
            ) -> Self {
                Self {
                    #(#fill_fields),*
                }
            }
        }
    };

    Ok(expanded)
}

struct FieldInfo {
    ident: syn::Ident,
    id: String,
    id_span: Span,
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
    fn new(field: &Field) -> Result<Self> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
        let attrs = parse_question_attrs(&field.attrs)?;
        let (base_ty, optional) = option_inner(&field.ty).unwrap_or((&field.ty, false));
        let mut kind = FieldKind::from_type(base_ty, &attrs, optional)?;

        if let Some(repeated_span) = attrs.repeated {
            if !matches!(kind, FieldKind::ScalarVec { .. }) {
                return Err(Error::new(
                    repeated_span,
                    "repeated is only supported on scalar Vec fields",
                ));
            }
            kind = kind.into_repeated_scalar_vec();
        }

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
                FieldKind::Scalar { .. }
                | FieldKind::ScalarVec { .. }
                | FieldKind::Choice { .. } => {}
                _ => {
                    return Err(Error::new(
                        *span,
                        "default is only supported on scalar fields, flat scalar Vec fields, and choice fields",
                    ));
                }
            }
        }

        if let Some((_, span)) = attrs.default_with.as_ref() {
            if !matches!(
                kind,
                FieldKind::Scalar { .. } | FieldKind::ScalarVec { .. } | FieldKind::Choice { .. }
            ) {
                return Err(Error::new(
                    *span,
                    "default_with is only supported on scalar fields, flat scalar Vec fields, and choice fields",
                ));
            }
        }

        if let Some((_, span)) = attrs.validate.as_ref() {
            if !matches!(
                kind,
                FieldKind::Scalar { .. } | FieldKind::ScalarVec { .. } | FieldKind::Choice { .. }
            ) {
                return Err(Error::new(
                    *span,
                    "validate is only supported on scalar fields, flat scalar Vec fields, and choice fields",
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
                (FieldKind::ScalarVec { .. } | FieldKind::RepeatedScalarVec { .. }, _) => {
                    return Err(Error::new(
                        active_when.span,
                        "active_when is not supported on Vec<T> fields because inactive answers are omitted and Option<Vec<T>> questionnaire fields are not supported",
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

        if (attrs.min.is_some() || attrs.max.is_some()) && !kind.is_repeatable_group() {
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

    fn builder_tokens(&self) -> TokenStream {
        let id = prefixed_id_tokens(&self.id);
        let prompt = &self.prompt;
        let active_when = self.active_when.as_ref().map(|active_when| {
            let controller = controller_id_tokens(active_when);
            let expected = &active_when.expected;
            quote! { .active_when(#controller, #expected) }
        });
        let dynamic_default = self.default_with.as_ref().map(|path| {
            let revision = self.revision.as_deref().unwrap_or_default();
            quote! {
                .with_dynamic_default(
                    ::standout_input::questionnaire::DynamicDefault::new(#revision, #path)
                )
            }
        });
        let validator = self.validate.as_ref().map(|path| {
            let revision = self.revision.as_deref().unwrap_or_default();
            quote! {
                .with_validator(
                    ::standout_input::questionnaire::FieldValidator::new(#revision, #path)
                )
            }
        });
        match &self.kind {
            FieldKind::Scalar { scalar } | FieldKind::ScalarVec { scalar } => {
                let kind = scalar.tokens();
                let optional = self.optional.then(|| quote! { .optional() });
                let default = self
                    .default
                    .as_ref()
                    .map(|default| quote! { .with_default(#default) });

                quote! {
                    {
                        let __id = #id;
                        ::standout_input::questionnaire::ScalarField::new(
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
                        ::standout_input::questionnaire::ScalarField::new(
                            __id,
                            #prompt,
                            ::standout_input::questionnaire::ScalarKind::String,
                        )
                        #optional
                        .one_of(
                            <#ty as ::standout_input::questionnaire::QuestionnaireChoices>::choices()
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
                    ::standout_input::questionnaire::Group::new(
                        __group_id.clone(),
                        #prompt,
                        <#ty as ::standout_input::questionnaire::QuestionnaireInput>::questionnaire_items_with_context(
                            &__group_id,
                            &__controller_ids,
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
                        ::standout_input::questionnaire::Group::new(
                            __group_id.clone(),
                            #prompt,
                            <#ty as ::standout_input::questionnaire::QuestionnaireInput>::questionnaire_items_with_context(
                                &__group_id,
                                &__controller_ids,
                            ),
                        )
                        #repeat
                        .into()
                    }
                }
            }
            FieldKind::RepeatedScalarVec { scalar } => {
                let kind = scalar.tokens();
                let repeat = self.repeat_tokens();
                quote! {
                    {
                        let __group_id = #id;
                        let __value_id = ::std::format!("{}.value", __group_id);
                        ::standout_input::questionnaire::Group::new(
                            __group_id,
                            #prompt,
                            vec![
                                ::standout_input::questionnaire::ScalarField::new(
                                    __value_id,
                                    #prompt,
                                    #kind,
                                )
                            ],
                        )
                        #repeat
                        .into()
                    }
                }
            }
        }
    }

    fn fill_tokens(&self) -> TokenStream {
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
            FieldKind::ScalarVec { scalar } => scalar_list_tokens(*scalar, id, missing),
            FieldKind::Nested { ty } => quote! {
                {
                    let __id = #id;
                    <#ty as ::standout_input::questionnaire::QuestionnaireInput>::from_decoded_answers_at(
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
                            <#ty as ::standout_input::questionnaire::QuestionnaireInput>::from_decoded_answers_at(
                                answers,
                                &__occurrence,
                            )
                        })
                        .collect()
                }
            },
            FieldKind::RepeatedScalarVec { scalar } => {
                let item = repeated_scalar_value_tokens(*scalar);
                quote! {
                    {
                        let __group_id = #id;
                        let __count = answers.occurrence_count(&__group_id);
                        (0..__count)
                            .map(|__index| {
                                let __value_id = ::std::format!("{}[{}].value", __group_id, __index);
                                #item
                            })
                            .collect()
                    }
                }
            }
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

enum FieldKind {
    Scalar { scalar: ScalarKind },
    Choice { ty: Type },
    ScalarVec { scalar: ScalarKind },
    RepeatedScalarVec { scalar: ScalarKind },
    Nested { ty: Type },
    RepeatedNested { ty: Type },
}

impl FieldKind {
    fn from_type(ty: &Type, attrs: &QuestionAttr, optional: bool) -> Result<Self> {
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
            if let Some(scalar) = ScalarKind::from_type(inner) {
                return match scalar {
                    ScalarKind::String | ScalarKind::Path => Ok(Self::ScalarVec { scalar }),
                    ScalarKind::Bool | ScalarKind::Text => Err(Error::new(
                        inner.span(),
                        "unsupported scalar Vec element type; expected String or PathBuf",
                    )),
                };
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
                    "default is only supported on scalar fields and flat scalar Vec fields",
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
                "unsupported questionnaire field type; expected String, PathBuf, bool, Option<T>, Vec<String>, Vec<PathBuf>, a nested Questionnaire type, or #[question(choice)] enum field",
            )?;
            Ok(Self::Nested { ty: ty.clone() })
        }
    }

    fn into_repeated_scalar_vec(self) -> Self {
        match self {
            Self::ScalarVec { scalar } => Self::RepeatedScalarVec { scalar },
            other => other,
        }
    }

    fn is_repeatable_group(&self) -> bool {
        matches!(
            self,
            Self::RepeatedScalarVec { .. } | Self::RepeatedNested { .. }
        )
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
    fn from_type(ty: &Type) -> Option<Self> {
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

    fn tokens(self) -> TokenStream {
        match self {
            Self::String => quote! { ::standout_input::questionnaire::ScalarKind::String },
            Self::Text => quote! { ::standout_input::questionnaire::ScalarKind::Text },
            Self::Bool => quote! { ::standout_input::questionnaire::ScalarKind::Bool },
            Self::Path => quote! { ::standout_input::questionnaire::ScalarKind::Path },
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

fn scalar_list_tokens(kind: ScalarKind, id: TokenStream, missing: String) -> TokenStream {
    match kind {
        ScalarKind::String => quote! {
            {
                let __id = #id;
                answers
                    .get_text(&__id)
                    .unwrap_or_else(|| unreachable!(#missing))
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect()
            }
        },
        ScalarKind::Path => quote! {
            {
                let __id = #id;
                answers
                    .get_text(&__id)
                    .unwrap_or_else(|| unreachable!(#missing))
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(::std::path::PathBuf::from)
                    .collect()
            }
        },
        ScalarKind::Bool | ScalarKind::Text => unreachable!("unsupported scalar list kind"),
    }
}

fn repeated_scalar_value_tokens(kind: ScalarKind) -> TokenStream {
    match kind {
        ScalarKind::String => quote! {
            answers
                .get_text(&__value_id)
                .unwrap_or_else(|| unreachable!("decoded answers are missing repeated scalar value"))
                .to_string()
        },
        ScalarKind::Path => quote! {
            ::std::path::PathBuf::from(
                answers
                    .get_text(&__value_id)
                    .unwrap_or_else(|| unreachable!("decoded answers are missing repeated scalar value"))
            )
        },
        ScalarKind::Bool | ScalarKind::Text => unreachable!("unsupported repeated scalar kind"),
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

fn controller_id_tokens(active_when: &ActiveWhenAttr) -> TokenStream {
    let controller = &active_when.field;
    quote! {
        __controller_ids
            .iter()
            .find_map(|(__name, __id)| (*__name == #controller).then_some(__id.clone()))
            .unwrap_or_else(|| #controller.to_string())
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
    if next.choice.is_some() && out.choice.replace(next.choice.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question choice attribute"));
    }
    if next.repeated.is_some() && out.repeated.replace(next.repeated.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question repeated attribute"));
    }
    if next.min.is_some() && out.min.replace(next.min.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question min attribute"));
    }
    if next.max.is_some() && out.max.replace(next.max.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question max attribute"));
    }
    if next.active_when.is_some() && out.active_when.replace(next.active_when.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question active_when attribute"));
    }
    if next.default_with.is_some()
        && out
            .default_with
            .replace(next.default_with.unwrap())
            .is_some()
    {
        return Err(Error::new(
            span,
            "duplicate question default_with attribute",
        ));
    }
    if next.validate.is_some() && out.validate.replace(next.validate.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question validate attribute"));
    }
    if next.revision.is_some() && out.revision.replace(next.revision.unwrap()).is_some() {
        return Err(Error::new(span, "duplicate question revision attribute"));
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

fn usize_lit(expr: &Expr, message: &str) -> Result<(usize, Span)> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(lit), ..
        }) => Ok((lit.base10_parse()?, lit.span())),
        _ => Err(Error::new(expr.span(), message)),
    }
}

fn path_expr(expr: &Expr, message: &str) -> Result<(Path, Span)> {
    match expr {
        Expr::Path(ExprPath { path, .. }) => Ok((path.clone(), path.span())),
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

/// Main implementation of the QuestionnaireChoices derive macro.
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
        impl ::standout_input::questionnaire::QuestionnaireChoices for #enum_name {
            fn choices() -> &'static [&'static str] {
                &[#(#choice_literals),*]
            }
        }

        impl ::core::str::FromStr for #enum_name {
            type Err = ::standout_input::questionnaire::QuestionnaireChoiceParseError;

            fn from_str(value: &str) -> ::core::result::Result<Self, Self::Err> {
                match value.trim() {
                    #(#parse_arms,)*
                    _ => ::core::result::Result::Err(
                        ::standout_input::questionnaire::QuestionnaireChoiceParseError::new(
                            <Self as ::standout_input::questionnaire::QuestionnaireChoices>::choices()
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

    Ok(expanded)
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
        let attrs = parse_choice_attrs(&variant.attrs)?;
        let (choice, choice_span) = attrs.rename.unwrap_or_else(|| {
            (
                to_kebab_case(&variant.ident.to_string()),
                variant.ident.span(),
            )
        });
        Ok(Self {
            ident: variant.ident.clone(),
            choice,
            choice_span,
        })
    }
}

fn parse_choice_attrs(attrs: &[Attribute]) -> Result<ChoiceAttr> {
    let mut out = ChoiceAttr::default();
    for attr in attrs {
        if attr.path().is_ident("question") {
            let parsed = attr.parse_args::<ChoiceAttr>()?;
            if parsed.rename.is_some() && out.rename.replace(parsed.rename.unwrap()).is_some() {
                return Err(Error::new(
                    attr.span(),
                    "duplicate question rename attribute",
                ));
            }
        }
    }
    Ok(out)
}

fn to_kebab_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_was_separator = true;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch.is_whitespace() {
            if !out.is_empty() && !prev_was_separator {
                out.push('-');
            }
            prev_was_separator = true;
            continue;
        }
        if ch.is_uppercase() {
            if !out.is_empty() && !prev_was_separator {
                out.push('-');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
        prev_was_separator = false;
    }
    out.trim_matches('-').to_string()
}
