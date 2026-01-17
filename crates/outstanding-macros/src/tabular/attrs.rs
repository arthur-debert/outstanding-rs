//! Attribute parsing for tabular derive macros.
//!
//! This module provides parsers for the `#[col(...)]` and `#[tabular(...)]` attributes
//! used by the `Tabular` and `TabularRow` derive macros.

// Allow dead_code during incremental development - these will be used in Phase 2 and 3
#![allow(dead_code)]

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Error, Expr, Lit, Meta, Result, Token,
};

/// Field-level attributes from `#[col(...)]`.
#[derive(Debug, Default, Clone)]
pub struct ColAttr {
    /// Fixed width: `width = 8`
    pub width_fixed: Option<usize>,
    /// Fill width: `width = "fill"`
    pub width_fill: bool,
    /// Fraction width: `width = "2fr"`
    pub width_fraction: Option<usize>,
    /// Minimum width: `min = 10`
    pub min: Option<usize>,
    /// Maximum width: `max = 30`
    pub max: Option<usize>,
    /// Alignment: `align = "right"`
    pub align: Option<String>,
    /// Anchor: `anchor = "right"`
    pub anchor: Option<String>,
    /// Overflow mode: `overflow = "wrap"`
    pub overflow: Option<String>,
    /// Truncation position: `truncate_at = "middle"`
    pub truncate_at: Option<String>,
    /// Style name: `style = "muted"`
    pub style: Option<String>,
    /// Style from value: `style_from_value`
    pub style_from_value: bool,
    /// Header title: `header = "Due Date"`
    pub header: Option<String>,
    /// Null representation: `null_repr = "N/A"`
    pub null_repr: Option<String>,
    /// Key override: `key = "user.name"`
    pub key: Option<String>,
    /// Skip this field: `skip`
    pub skip: bool,
}

/// Container-level attributes from `#[tabular(...)]`.
#[derive(Debug, Default, Clone)]
pub struct TabularAttr {
    /// Column separator: `separator = " │ "`
    pub separator: Option<String>,
    /// Row prefix: `prefix = "│ "`
    pub prefix: Option<String>,
    /// Row suffix: `suffix = " │"`
    pub suffix: Option<String>,
}

impl Parse for ColAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attr = ColAttr::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                // width = 8 or width = "fill" or width = "2fr"
                Meta::NameValue(nv) if nv.path.is_ident("width") => {
                    parse_width_value(&nv.value, &mut attr)?;
                }

                // min = 10
                Meta::NameValue(nv) if nv.path.is_ident("min") => {
                    attr.min = Some(parse_usize_expr(&nv.value)?);
                }

                // max = 30
                Meta::NameValue(nv) if nv.path.is_ident("max") => {
                    attr.max = Some(parse_usize_expr(&nv.value)?);
                }

                // align = "right"
                Meta::NameValue(nv) if nv.path.is_ident("align") => {
                    attr.align = Some(parse_string_expr(&nv.value)?);
                }

                // anchor = "right"
                Meta::NameValue(nv) if nv.path.is_ident("anchor") => {
                    attr.anchor = Some(parse_string_expr(&nv.value)?);
                }

                // overflow = "wrap"
                Meta::NameValue(nv) if nv.path.is_ident("overflow") => {
                    attr.overflow = Some(parse_string_expr(&nv.value)?);
                }

                // truncate_at = "middle"
                Meta::NameValue(nv) if nv.path.is_ident("truncate_at") => {
                    attr.truncate_at = Some(parse_string_expr(&nv.value)?);
                }

                // style = "muted"
                Meta::NameValue(nv) if nv.path.is_ident("style") => {
                    attr.style = Some(parse_string_expr(&nv.value)?);
                }

                // style_from_value (flag)
                Meta::Path(p) if p.is_ident("style_from_value") => {
                    attr.style_from_value = true;
                }

                // header = "Due Date"
                Meta::NameValue(nv) if nv.path.is_ident("header") => {
                    attr.header = Some(parse_string_expr(&nv.value)?);
                }

                // null_repr = "N/A"
                Meta::NameValue(nv) if nv.path.is_ident("null_repr") => {
                    attr.null_repr = Some(parse_string_expr(&nv.value)?);
                }

                // key = "user.name"
                Meta::NameValue(nv) if nv.path.is_ident("key") => {
                    attr.key = Some(parse_string_expr(&nv.value)?);
                }

                // skip (flag)
                Meta::Path(p) if p.is_ident("skip") => {
                    attr.skip = true;
                }

                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown col attribute: expected one of: width, min, max, align, \
                             anchor, overflow, truncate_at, style, style_from_value, header, \
                             null_repr, key, skip"
                            .to_string(),
                    ));
                }
            }
        }

        Ok(attr)
    }
}

impl Parse for TabularAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attr = TabularAttr::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                // separator = " │ "
                Meta::NameValue(nv) if nv.path.is_ident("separator") => {
                    attr.separator = Some(parse_string_expr(&nv.value)?);
                }

                // prefix = "│ "
                Meta::NameValue(nv) if nv.path.is_ident("prefix") => {
                    attr.prefix = Some(parse_string_expr(&nv.value)?);
                }

                // suffix = " │"
                Meta::NameValue(nv) if nv.path.is_ident("suffix") => {
                    attr.suffix = Some(parse_string_expr(&nv.value)?);
                }

                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown tabular attribute: expected one of: separator, prefix, suffix",
                    ));
                }
            }
        }

        Ok(attr)
    }
}

/// Parse width value which can be:
/// - Integer: `width = 8` → Fixed(8)
/// - String "fill": `width = "fill"` → Fill
/// - String "Nfr": `width = "2fr"` → Fraction(2)
fn parse_width_value(expr: &Expr, attr: &mut ColAttr) -> Result<()> {
    match expr {
        // width = 8
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            Lit::Int(lit_int) => {
                attr.width_fixed = Some(lit_int.base10_parse()?);
            }
            Lit::Str(lit_str) => {
                let s = lit_str.value();
                if s == "fill" {
                    attr.width_fill = true;
                } else if s.ends_with("fr") {
                    let num_str = s.trim_end_matches("fr");
                    let n: usize = num_str.parse().map_err(|_| {
                        Error::new(
                            lit_str.span(),
                            format!("invalid fraction: '{}'. Expected format like '2fr'", s),
                        )
                    })?;
                    attr.width_fraction = Some(n);
                } else {
                    return Err(Error::new(
                        lit_str.span(),
                        format!("invalid width string: '{}'. Expected 'fill' or '<n>fr'", s),
                    ));
                }
            }
            _ => {
                return Err(Error::new(
                    expr_lit.span(),
                    "width must be an integer or string ('fill' or '<n>fr')",
                ));
            }
        },
        _ => {
            return Err(Error::new(
                expr.span(),
                "width must be an integer or string literal",
            ));
        }
    }
    Ok(())
}

/// Parse a usize from an expression.
fn parse_usize_expr(expr: &Expr) -> Result<usize> {
    if let Expr::Lit(expr_lit) = expr {
        if let Lit::Int(lit_int) = &expr_lit.lit {
            return lit_int.base10_parse();
        }
    }
    Err(Error::new(expr.span(), "expected integer literal"))
}

/// Parse a string from an expression.
fn parse_string_expr(expr: &Expr) -> Result<String> {
    if let Expr::Lit(expr_lit) = expr {
        if let Lit::Str(lit_str) = &expr_lit.lit {
            return Ok(lit_str.value());
        }
    }
    Err(Error::new(expr.span(), "expected string literal"))
}

/// Extract `#[col(...)]` attributes from a field's attributes.
pub fn parse_col_attrs(attrs: &[Attribute]) -> Result<ColAttr> {
    for attr in attrs {
        if attr.path().is_ident("col") {
            return attr.parse_args::<ColAttr>();
        }
    }
    Ok(ColAttr::default())
}

/// Extract `#[tabular(...)]` attributes from a struct's attributes.
pub fn parse_tabular_attrs(attrs: &[Attribute]) -> Result<TabularAttr> {
    for attr in attrs {
        if attr.path().is_ident("tabular") {
            return attr.parse_args::<TabularAttr>();
        }
    }
    Ok(TabularAttr::default())
}

/// Generate token stream for Width enum variant.
pub fn generate_width_tokens(attr: &ColAttr) -> TokenStream {
    // Priority: width_fixed > width_fill > width_fraction > bounded(min, max) > default
    if let Some(w) = attr.width_fixed {
        quote! { ::outstanding::tabular::Width::Fixed(#w) }
    } else if attr.width_fill {
        quote! { ::outstanding::tabular::Width::Fill }
    } else if let Some(n) = attr.width_fraction {
        quote! { ::outstanding::tabular::Width::Fraction(#n) }
    } else if attr.min.is_some() || attr.max.is_some() {
        let min = attr
            .min
            .map(|m| quote! { Some(#m) })
            .unwrap_or(quote! { None });
        let max = attr
            .max
            .map(|m| quote! { Some(#m) })
            .unwrap_or(quote! { None });
        quote! { ::outstanding::tabular::Width::Bounded { min: #min, max: #max } }
    } else {
        // Default: unbounded
        quote! { ::outstanding::tabular::Width::default() }
    }
}

/// Generate token stream for Align enum variant.
pub fn generate_align_tokens(align: &Option<String>) -> Result<TokenStream> {
    match align.as_deref() {
        None => Ok(quote! { ::outstanding::tabular::Align::default() }),
        Some("left") => Ok(quote! { ::outstanding::tabular::Align::Left }),
        Some("right") => Ok(quote! { ::outstanding::tabular::Align::Right }),
        Some("center") => Ok(quote! { ::outstanding::tabular::Align::Center }),
        Some(other) => Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "invalid align value: '{}'. Expected 'left', 'right', or 'center'",
                other
            ),
        )),
    }
}

/// Generate token stream for Anchor enum variant.
pub fn generate_anchor_tokens(anchor: &Option<String>) -> Result<TokenStream> {
    match anchor.as_deref() {
        None => Ok(quote! { ::outstanding::tabular::Anchor::default() }),
        Some("left") => Ok(quote! { ::outstanding::tabular::Anchor::Left }),
        Some("right") => Ok(quote! { ::outstanding::tabular::Anchor::Right }),
        Some(other) => Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "invalid anchor value: '{}'. Expected 'left' or 'right'",
                other
            ),
        )),
    }
}

/// Generate token stream for Overflow enum variant.
pub fn generate_overflow_tokens(attr: &ColAttr) -> Result<TokenStream> {
    let truncate_at = match attr.truncate_at.as_deref() {
        None | Some("end") => quote! { ::outstanding::tabular::TruncateAt::End },
        Some("start") => quote! { ::outstanding::tabular::TruncateAt::Start },
        Some("middle") => quote! { ::outstanding::tabular::TruncateAt::Middle },
        Some(other) => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "invalid truncate_at value: '{}'. Expected 'end', 'start', or 'middle'",
                    other
                ),
            ));
        }
    };

    match attr.overflow.as_deref() {
        None | Some("truncate") => Ok(quote! {
            ::outstanding::tabular::Overflow::Truncate {
                at: #truncate_at,
                marker: "…".to_string(),
            }
        }),
        Some("wrap") => Ok(quote! { ::outstanding::tabular::Overflow::wrap() }),
        Some("clip") => Ok(quote! { ::outstanding::tabular::Overflow::Clip }),
        Some("expand") => Ok(quote! { ::outstanding::tabular::Overflow::Expand }),
        Some(other) => Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "invalid overflow value: '{}'. Expected 'truncate', 'wrap', 'clip', or 'expand'",
                other
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to parse ColAttr from tokens
    fn parse_col(tokens: &str) -> Result<ColAttr> {
        syn::parse_str::<ColAttr>(tokens)
    }

    // Helper to parse TabularAttr from tokens
    fn parse_tabular(tokens: &str) -> Result<TabularAttr> {
        syn::parse_str::<TabularAttr>(tokens)
    }

    #[test]
    fn test_col_width_fixed() {
        let attr = parse_col("width = 8").unwrap();
        assert_eq!(attr.width_fixed, Some(8));
        assert!(!attr.width_fill);
        assert_eq!(attr.width_fraction, None);
    }

    #[test]
    fn test_col_width_fill() {
        let attr = parse_col(r#"width = "fill""#).unwrap();
        assert!(attr.width_fill);
        assert_eq!(attr.width_fixed, None);
    }

    #[test]
    fn test_col_width_fraction() {
        let attr = parse_col(r#"width = "2fr""#).unwrap();
        assert_eq!(attr.width_fraction, Some(2));
    }

    #[test]
    fn test_col_min_max() {
        let attr = parse_col("min = 10, max = 30").unwrap();
        assert_eq!(attr.min, Some(10));
        assert_eq!(attr.max, Some(30));
    }

    #[test]
    fn test_col_align() {
        let attr = parse_col(r#"align = "right""#).unwrap();
        assert_eq!(attr.align, Some("right".to_string()));
    }

    #[test]
    fn test_col_anchor() {
        let attr = parse_col(r#"anchor = "right""#).unwrap();
        assert_eq!(attr.anchor, Some("right".to_string()));
    }

    #[test]
    fn test_col_overflow() {
        let attr = parse_col(r#"overflow = "wrap""#).unwrap();
        assert_eq!(attr.overflow, Some("wrap".to_string()));
    }

    #[test]
    fn test_col_truncate_at() {
        let attr = parse_col(r#"truncate_at = "middle""#).unwrap();
        assert_eq!(attr.truncate_at, Some("middle".to_string()));
    }

    #[test]
    fn test_col_style() {
        let attr = parse_col(r#"style = "muted""#).unwrap();
        assert_eq!(attr.style, Some("muted".to_string()));
    }

    #[test]
    fn test_col_style_from_value() {
        let attr = parse_col("style_from_value").unwrap();
        assert!(attr.style_from_value);
    }

    #[test]
    fn test_col_header() {
        let attr = parse_col(r#"header = "Due Date""#).unwrap();
        assert_eq!(attr.header, Some("Due Date".to_string()));
    }

    #[test]
    fn test_col_null_repr() {
        let attr = parse_col(r#"null_repr = "N/A""#).unwrap();
        assert_eq!(attr.null_repr, Some("N/A".to_string()));
    }

    #[test]
    fn test_col_key() {
        let attr = parse_col(r#"key = "user.name""#).unwrap();
        assert_eq!(attr.key, Some("user.name".to_string()));
    }

    #[test]
    fn test_col_skip() {
        let attr = parse_col("skip").unwrap();
        assert!(attr.skip);
    }

    #[test]
    fn test_col_combined() {
        let attr =
            parse_col(r#"width = 8, align = "right", style = "muted", header = "ID""#).unwrap();
        assert_eq!(attr.width_fixed, Some(8));
        assert_eq!(attr.align, Some("right".to_string()));
        assert_eq!(attr.style, Some("muted".to_string()));
        assert_eq!(attr.header, Some("ID".to_string()));
    }

    #[test]
    fn test_col_invalid_width_string() {
        let result = parse_col(r#"width = "invalid""#);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid width string"));
    }

    #[test]
    fn test_col_unknown_attribute() {
        let result = parse_col("unknown = 5");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown col attribute"));
    }

    #[test]
    fn test_tabular_separator() {
        let attr = parse_tabular(r#"separator = " │ ""#).unwrap();
        assert_eq!(attr.separator, Some(" │ ".to_string()));
    }

    #[test]
    fn test_tabular_prefix_suffix() {
        let attr = parse_tabular(r#"prefix = "│ ", suffix = " │""#).unwrap();
        assert_eq!(attr.prefix, Some("│ ".to_string()));
        assert_eq!(attr.suffix, Some(" │".to_string()));
    }

    #[test]
    fn test_tabular_unknown_attribute() {
        let result = parse_tabular("unknown = 5");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown tabular attribute"));
    }

    #[test]
    fn test_generate_width_fixed() {
        let attr = ColAttr {
            width_fixed: Some(10),
            ..Default::default()
        };
        let tokens = generate_width_tokens(&attr);
        let output = tokens.to_string();
        // Check that it contains the key parts
        assert!(output.contains("outstanding"));
        assert!(output.contains("tabular"));
        assert!(output.contains("Width"));
        assert!(output.contains("Fixed"));
        assert!(output.contains("10"));
    }

    #[test]
    fn test_generate_width_fill() {
        let attr = ColAttr {
            width_fill: true,
            ..Default::default()
        };
        let tokens = generate_width_tokens(&attr);
        assert!(tokens.to_string().contains("Fill"));
    }

    #[test]
    fn test_generate_width_fraction() {
        let attr = ColAttr {
            width_fraction: Some(3),
            ..Default::default()
        };
        let tokens = generate_width_tokens(&attr);
        assert!(tokens.to_string().contains("Fraction"));
        assert!(tokens.to_string().contains("3"));
    }

    #[test]
    fn test_generate_width_bounded() {
        let attr = ColAttr {
            min: Some(5),
            max: Some(20),
            ..Default::default()
        };
        let tokens = generate_width_tokens(&attr);
        assert!(tokens.to_string().contains("Bounded"));
    }

    #[test]
    fn test_generate_align() {
        assert!(generate_align_tokens(&Some("left".to_string())).is_ok());
        assert!(generate_align_tokens(&Some("right".to_string())).is_ok());
        assert!(generate_align_tokens(&Some("center".to_string())).is_ok());
        assert!(generate_align_tokens(&Some("invalid".to_string())).is_err());
    }

    #[test]
    fn test_generate_anchor() {
        assert!(generate_anchor_tokens(&Some("left".to_string())).is_ok());
        assert!(generate_anchor_tokens(&Some("right".to_string())).is_ok());
        assert!(generate_anchor_tokens(&Some("invalid".to_string())).is_err());
    }

    #[test]
    fn test_generate_overflow() {
        let attr = ColAttr {
            overflow: Some("wrap".to_string()),
            ..Default::default()
        };
        assert!(generate_overflow_tokens(&attr).is_ok());

        let attr = ColAttr {
            overflow: Some("clip".to_string()),
            ..Default::default()
        };
        assert!(generate_overflow_tokens(&attr).is_ok());

        let attr = ColAttr {
            overflow: Some("expand".to_string()),
            ..Default::default()
        };
        assert!(generate_overflow_tokens(&attr).is_ok());

        let attr = ColAttr {
            overflow: Some("truncate".to_string()),
            truncate_at: Some("middle".to_string()),
            ..Default::default()
        };
        assert!(generate_overflow_tokens(&attr).is_ok());

        let attr = ColAttr {
            overflow: Some("invalid".to_string()),
            ..Default::default()
        };
        assert!(generate_overflow_tokens(&attr).is_err());
    }
}
