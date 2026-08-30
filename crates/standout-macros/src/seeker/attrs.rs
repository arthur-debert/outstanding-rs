use proc_macro2::Span;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Error, Ident, Lit, Meta, Result, Token,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekType {
    String,
    Number,
    Timestamp,
    Enum,
    Bool,
}

impl SeekType {
    pub fn from_ident(ident: &Ident) -> Result<Self> {
        match ident.to_string().as_str() {
            "String" | "string" => Ok(SeekType::String),
            "Number" | "number" => Ok(SeekType::Number),
            "Timestamp" | "timestamp" => Ok(SeekType::Timestamp),
            "Enum" | "enumeration" => Ok(SeekType::Enum),
            "Bool" | "boolean" | "bool" => Ok(SeekType::Bool),
            other => Err(Error::new(
                ident.span(),
                format!(
                    "unknown seek type: '{}'. Expected one of: String, Number, Timestamp, Enum, Bool",
                    other
                ),
            )),
        }
    }

    pub fn from_str(s: &str, span: Span) -> Result<Self> {
        match s {
            "string" | "String" => Ok(SeekType::String),
            "number" | "Number" => Ok(SeekType::Number),
            "timestamp" | "Timestamp" => Ok(SeekType::Timestamp),
            "enum" | "Enum" => Ok(SeekType::Enum),
            "bool" | "Bool" => Ok(SeekType::Bool),
            other => Err(Error::new(
                span,
                format!(
                    "unknown seek type: '{}'. Expected one of: string, number, timestamp, enum, bool",
                    other
                ),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SeekAttr {
    pub seek_type: Option<SeekType>,
    pub skip: bool,
    pub rename: Option<String>,
    pub span: Span,
}

impl Default for SeekAttr {
    fn default() -> Self {
        SeekAttr {
            seek_type: None,
            skip: false,
            rename: None,
            span: Span::call_site(),
        }
    }
}

impl Parse for SeekAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attr = SeekAttr::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                Meta::Path(p) => {
                    if p.is_ident("skip") {
                        attr.skip = true;
                    } else if let Some(ident) = p.get_ident() {
                        attr.seek_type = Some(SeekType::from_ident(ident)?);
                        attr.span = ident.span();
                    } else {
                        return Err(Error::new(
                            p.span(),
                            "expected seek type: String, Number, Timestamp, Enum, Bool, or skip",
                        ));
                    }
                }

                Meta::NameValue(nv) => {
                    if nv.path.is_ident("rename") {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: Lit::Str(s), ..
                        }) = &nv.value
                        {
                            attr.rename = Some(s.value());
                        } else {
                            return Err(Error::new(
                                nv.value.span(),
                                "rename must be a string literal",
                            ));
                        }
                    } else if nv.path.is_ident("ty") {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: Lit::Str(s), ..
                        }) = &nv.value
                        {
                            attr.seek_type = Some(SeekType::from_str(&s.value(), s.span())?);
                            attr.span = s.span();
                        } else {
                            return Err(Error::new(nv.value.span(), "ty must be a string literal"));
                        }
                    } else {
                        return Err(Error::new(
                            nv.path.span(),
                            "unknown attribute. Expected: rename or ty",
                        ));
                    }
                }

                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown seek attribute. Expected: String, Number, Timestamp, Enum, Bool, skip, rename = \"...\", or ty = \"...\"",
                    ));
                }
            }
        }

        Ok(attr)
    }
}

pub fn parse_seek_attrs(attrs: &[Attribute]) -> Result<SeekAttr> {
    for attr in attrs {
        if attr.path().is_ident("seek") {
            return attr.parse_args::<SeekAttr>();
        }
    }
    Ok(SeekAttr::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_seek(tokens: &str) -> Result<SeekAttr> {
        syn::parse_str::<SeekAttr>(tokens)
    }

    #[test]
    fn test_seek_string() {
        let attr = parse_seek("String").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::String));
        assert!(!attr.skip);
    }

    #[test]
    fn test_seek_string_lowercase() {
        let attr = parse_seek("string").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::String));
    }

    #[test]
    fn test_seek_number() {
        let attr = parse_seek("Number").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Number));
    }

    #[test]
    fn test_seek_timestamp() {
        let attr = parse_seek("Timestamp").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Timestamp));
    }

    #[test]
    fn test_seek_enum_via_ty() {
        let attr = parse_seek(r#"ty = "enum""#).unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Enum));
    }

    #[test]
    fn test_seek_enum_capitalized() {
        let attr = parse_seek("Enum").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Enum));
    }

    #[test]
    fn test_seek_bool() {
        let attr = parse_seek("Bool").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Bool));
    }

    #[test]
    fn test_seek_bool_lowercase() {
        let attr = parse_seek("boolean").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Bool));
    }

    #[test]
    fn test_seek_skip() {
        let attr = parse_seek("skip").unwrap();
        assert!(attr.skip);
        assert_eq!(attr.seek_type, None);
    }

    #[test]
    fn test_seek_rename() {
        let attr = parse_seek(r#"String, rename = "custom_name""#).unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::String));
        assert_eq!(attr.rename, Some("custom_name".to_string()));
    }

    #[test]
    fn test_seek_ty_with_rename() {
        let attr = parse_seek(r#"ty = "enum", rename = "status""#).unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Enum));
        assert_eq!(attr.rename, Some("status".to_string()));
    }

    #[test]
    fn test_seek_invalid_type() {
        let result = parse_seek("invalid");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown seek type"));
    }

    #[test]
    fn test_seek_enumeration_alias() {
        let attr = parse_seek("enumeration").unwrap();
        assert_eq!(attr.seek_type, Some(SeekType::Enum));
    }
}
