use proc_macro2::Span;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Error, Expr, ExprLit, ExprPath, Lit, Meta, Path, Result, Token,
};

#[derive(Default)]
pub(super) struct QuestionAttr {
    pub(super) id: Option<(String, Span)>,
    pub(super) default: Option<(String, Span)>,
    pub(super) prose: Option<Span>,
    pub(super) choice: Option<Span>,
    pub(super) min: Option<(usize, Span)>,
    pub(super) max: Option<(usize, Span)>,
    pub(super) active_when: Option<ActiveWhenAttr>,
    pub(super) default_with: Option<(Path, Span)>,
    pub(super) validate: Option<(Path, Span)>,
    pub(super) revision: Option<(String, Span)>,
}

#[derive(Clone)]
pub(super) struct ActiveWhenAttr {
    pub(super) field: String,
    pub(super) expected: String,
    pub(super) span: Span,
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

fn set_once<T>(slot: &mut Option<T>, value: T, span: Span, name: &str) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(Error::new(
            span,
            format!("duplicate question {name} attribute"),
        ));
    }
    Ok(())
}

fn question_metas(attrs: &[Attribute]) -> Result<Vec<Meta>> {
    let mut metas = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("question") {
            metas.extend(attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?);
        }
    }
    Ok(metas)
}

impl QuestionAttr {
    pub(super) fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
        let mut attr = QuestionAttr::default();

        for meta in question_metas(attrs)? {
            match meta {
                Meta::NameValue(nv) if nv.path.is_ident("id") => {
                    let value = string_lit(&nv.value, "id must be a string literal")?;
                    set_once(&mut attr.id, value, nv.path.span(), "id")?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("default") => {
                    let value = string_lit(&nv.value, "default must be a string literal")?;
                    set_once(&mut attr.default, value, nv.path.span(), "default")?;
                }
                Meta::Path(path) if path.is_ident("prose") => {
                    let span = path.span();
                    set_once(&mut attr.prose, span, span, "prose")?;
                }
                Meta::Path(path) if path.is_ident("choice") => {
                    let span = path.span();
                    set_once(&mut attr.choice, span, span, "choice")?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("min") => {
                    let value = usize_lit(&nv.value, "min must be an integer literal")?;
                    set_once(&mut attr.min, value, nv.path.span(), "min")?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("max") => {
                    let value = usize_lit(&nv.value, "max must be an integer literal")?;
                    set_once(&mut attr.max, value, nv.path.span(), "max")?;
                }
                Meta::List(list) if list.path.is_ident("active_when") => {
                    let mut parsed = list.parse_args::<ActiveWhenAttr>()?;
                    parsed.span = list.path.span();
                    set_once(
                        &mut attr.active_when,
                        parsed,
                        list.path.span(),
                        "active_when",
                    )?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("default_with") => {
                    let value = path_expr(&nv.value, "default_with must be a function path")?;
                    set_once(
                        &mut attr.default_with,
                        value,
                        nv.path.span(),
                        "default_with",
                    )?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("validate") => {
                    let value = path_expr(&nv.value, "validate must be a function path")?;
                    set_once(&mut attr.validate, value, nv.path.span(), "validate")?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("revision") => {
                    let value = string_lit(&nv.value, "revision must be a string literal")?;
                    set_once(&mut attr.revision, value, nv.path.span(), "revision")?;
                }
                other => {
                    return Err(Error::new(
                        other.span(),
                        "unknown question attribute: expected id, default, prose, choice, min, max, active_when, default_with, validate, or revision",
                    ));
                }
            }
        }

        Ok(attr)
    }
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

pub(super) fn doc_prompt(attrs: &[Attribute]) -> String {
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

pub(super) fn parse_choice_rename(attrs: &[Attribute]) -> Result<Option<(String, Span)>> {
    let mut rename = None;
    for meta in question_metas(attrs)? {
        match meta {
            Meta::NameValue(nv) if nv.path.is_ident("rename") => {
                let value = string_lit(&nv.value, "rename must be a string literal")?;
                set_once(&mut rename, value, nv.path.span(), "rename")?;
            }
            other => {
                return Err(Error::new(
                    other.span(),
                    "unknown question attribute: expected rename",
                ));
            }
        }
    }
    Ok(rename)
}
