use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    DeriveInput, Error, Expr, Meta, Path, Result, Token,
};

#[derive(Default)]
pub(super) struct ContainerAttrs {
    pub(super) handlers: Option<Path>,
}

#[derive(Default)]
pub(super) struct VariantAttrs {
    pub(super) name: Option<String>,
    pub(super) handler: Option<Path>,
    pub(super) template_name: Option<String>,
    pub(super) inputs: Option<Path>,
    pub(super) silent: bool,
    pub(super) binary: bool,
    pub(super) structured_only: bool,
    pub(super) pre_dispatch: Option<Vec<Path>>,
    pub(super) post_dispatch: Option<Vec<Path>>,
    pub(super) post_output: Option<Vec<Path>>,
    pub(super) questionnaire: Option<Path>,
    pub(super) nested: bool,
    pub(super) skip: bool,
    pub(super) default: bool,
    pub(super) list_view: bool,
    pub(super) item_type: Option<String>,
    pub(super) pipe_to: Option<String>,
    pub(super) pipe_through: Option<String>,
    pub(super) pipe_to_clipboard: bool,
    pub(super) simple: bool,
    pub(super) pure: bool,
    pub(super) pageable: bool,
    pub(super) no_config: bool,
}

impl Parse for ContainerAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = ContainerAttrs::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                Meta::NameValue(nv) if nv.path.is_ident("handlers") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.handlers = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown attribute, expected `handlers = path`",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

fn hook_paths(key: &str, meta: &Meta) -> Result<Vec<Path>> {
    match meta {
        Meta::NameValue(nv) => match &nv.value {
            Expr::Path(expr_path) => Ok(vec![expr_path.path.clone()]),
            other => Err(Error::new(other.span(), "expected path")),
        },
        Meta::List(list) => {
            let paths: Punctuated<Path, Token![,]> =
                list.parse_args_with(Punctuated::parse_terminated)?;
            if paths.is_empty() {
                return Err(Error::new(
                    list.span(),
                    format!("`{key}` needs at least one path"),
                ));
            }
            Ok(paths.into_iter().collect())
        }
        Meta::Path(path) => Err(Error::new(
            path.span(),
            format!("expected `{key} = path` or `{key}(first, second)`"),
        )),
    }
}

fn hook_repeat_error(key: &str, span: proc_macro2::Span) -> Error {
    Error::new(
        span,
        format!(
            "`{key}` appears twice on this variant. Name every {key} hook in one list, in the \
             order they run: `{key}(first, second)`"
        ),
    )
}

fn set_hook_paths(slot: &mut Option<Vec<Path>>, key: &str, meta: &Meta) -> Result<()> {
    if slot.is_some() {
        return Err(hook_repeat_error(key, meta.span()));
    }
    *slot = Some(hook_paths(key, meta)?);
    Ok(())
}

fn merge_hook_paths(
    slot: &mut Option<Vec<Path>>,
    other: Option<Vec<Path>>,
    key: &str,
    span: proc_macro2::Span,
) -> Result<()> {
    let Some(paths) = other else { return Ok(()) };
    if slot.is_some() {
        return Err(hook_repeat_error(key, span));
    }
    *slot = Some(paths);
    Ok(())
}

impl Parse for VariantAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = VariantAttrs::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                Meta::NameValue(nv) if nv.path.is_ident("name") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            let value = lit_str.value();
                            if value.is_empty() {
                                return Err(Error::new(nv.value.span(), "`name` cannot be empty"));
                            }
                            if value.contains('.') {
                                return Err(Error::new(
                                    nv.value.span(),
                                    "`name` cannot contain `.`: dispatch splits a registration \
                                     path on `.`, so `parent.child` registers a nested path no \
                                     clap subcommand declares. Name the single command the \
                                     variant is, and nest with `#[dispatch(nested)]`",
                                ));
                            }
                            attrs.name = Some(value);
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("handler") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.handler = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("template_name") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.template_name = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("inputs") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.inputs = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::Path(p) if p.is_ident("silent") => {
                    attrs.silent = true;
                }
                Meta::Path(p) if p.is_ident("binary") => {
                    attrs.binary = true;
                }
                Meta::Path(p) if p.is_ident("structured_only") => {
                    attrs.structured_only = true;
                }
                Meta::Path(p) if p.is_ident("pageable") => {
                    attrs.pageable = true;
                }
                Meta::Path(p) if p.is_ident("no_config") => {
                    attrs.no_config = true;
                }
                _ if meta.path().is_ident("pre_dispatch") => {
                    set_hook_paths(&mut attrs.pre_dispatch, "pre_dispatch", &meta)?;
                }
                _ if meta.path().is_ident("post_dispatch") => {
                    set_hook_paths(&mut attrs.post_dispatch, "post_dispatch", &meta)?;
                }
                _ if meta.path().is_ident("post_output") => {
                    set_hook_paths(&mut attrs.post_output, "post_output", &meta)?;
                }
                Meta::NameValue(nv) if nv.path.is_ident("questionnaire") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.questionnaire = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::Path(p) if p.is_ident("nested") => {
                    attrs.nested = true;
                }
                Meta::Path(p) if p.is_ident("skip") => {
                    attrs.skip = true;
                }
                Meta::Path(p) if p.is_ident("default") => {
                    attrs.default = true;
                }
                Meta::Path(p) if p.is_ident("list_view") => {
                    attrs.list_view = true;
                }
                Meta::NameValue(nv) if nv.path.is_ident("item_type") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.item_type = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("pipe_to") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.pipe_to = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("pipe_through") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.pipe_through = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::Path(p) if p.is_ident("pipe_to_clipboard") => {
                    attrs.pipe_to_clipboard = true;
                }
                Meta::Path(p) if p.is_ident("simple") => {
                    attrs.simple = true;
                }
                Meta::Path(p) if p.is_ident("pure") => {
                    attrs.pure = true;
                }
                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown attribute, expected one of: name, handler, template_name, inputs, silent, binary, structured_only, pageable, no_config, pre_dispatch, post_dispatch, post_output, questionnaire, nested, skip, default, list_view, item_type, pipe_to, pipe_through, pipe_to_clipboard, simple, pure",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

impl VariantAttrs {
    /// Several `#[dispatch(...)]` on one variant fold into one set before validation.
    fn merge(&mut self, other: VariantAttrs, span: proc_macro2::Span) -> Result<()> {
        let VariantAttrs {
            name,
            handler,
            template_name,
            inputs,
            silent,
            binary,
            structured_only,
            pre_dispatch,
            post_dispatch,
            post_output,
            questionnaire,
            nested,
            skip,
            default,
            list_view,
            item_type,
            pipe_to,
            pipe_through,
            pipe_to_clipboard,
            simple,
            pure,
            pageable,
            no_config,
        } = other;

        self.name = name.or(self.name.take());
        self.handler = handler.or(self.handler.take());
        self.template_name = template_name.or(self.template_name.take());
        self.inputs = inputs.or(self.inputs.take());
        merge_hook_paths(&mut self.pre_dispatch, pre_dispatch, "pre_dispatch", span)?;
        merge_hook_paths(
            &mut self.post_dispatch,
            post_dispatch,
            "post_dispatch",
            span,
        )?;
        merge_hook_paths(&mut self.post_output, post_output, "post_output", span)?;
        self.questionnaire = questionnaire.or(self.questionnaire.take());
        self.item_type = item_type.or(self.item_type.take());
        self.pipe_to = pipe_to.or(self.pipe_to.take());
        self.pipe_through = pipe_through.or(self.pipe_through.take());
        self.silent |= silent;
        self.binary |= binary;
        self.structured_only |= structured_only;
        self.nested |= nested;
        self.skip |= skip;
        self.default |= default;
        self.list_view |= list_view;
        self.pipe_to_clipboard |= pipe_to_clipboard;
        self.simple |= simple;
        self.pure |= pure;
        self.pageable |= pageable;
        self.no_config |= no_config;
        Ok(())
    }

    /// Runs after `merge`, so a conflicting pair split across two attributes is
    /// still rejected.
    fn validate(&self, span: proc_macro2::Span) -> Result<()> {
        if self.pure && self.handler.is_some() {
            return Err(Error::new(
                span,
                "`pure` and `handler` cannot be used together: `pure` registers the wrapper `#[handler]` generates for the variant's own function, while `handler = path` names a function to register. Drop `pure` and point `handler` at the wrapper (`handler = handlers::name__handler`) to register another `#[handler]` function",
            ));
        }
        if self.pure && self.simple {
            return Err(Error::new(
                span,
                "`pure` and `simple` cannot be used together: `simple` registers a function taking only `&ArgMatches`, while the wrapper `#[handler]` generates always takes `(&ArgMatches, &CommandContext)`. Drop `simple`; a `#[handler]` function declares what it needs through its parameter attributes",
            ));
        }
        let absence_count =
            usize::from(self.silent) + usize::from(self.binary) + usize::from(self.structured_only);
        if absence_count > 1 {
            return Err(Error::new(
                span,
                "`silent`, `binary`, and `structured_only` cannot be used together",
            ));
        }
        if absence_count == 1 && self.template_name.is_some() {
            return Err(Error::new(
                span,
                "`silent`, `binary`, and `structured_only` cannot be combined with `template_name`",
            ));
        }

        Ok(())
    }
}

pub(super) fn parse_container_attrs(input: &DeriveInput) -> Result<ContainerAttrs> {
    let mut merged = ContainerAttrs::default();
    let mut found = false;

    for attr in &input.attrs {
        if attr.path().is_ident("dispatch") {
            let attrs = attr.parse_args::<ContainerAttrs>()?;
            merged.handlers = attrs.handlers.or(merged.handlers);
            found = true;
        }
    }

    if found {
        return Ok(merged);
    }

    Err(Error::new(
        input.span(),
        "missing `#[dispatch(handlers = path)]` attribute",
    ))
}

pub(super) fn parse_variant_attrs(attrs: &[syn::Attribute]) -> Result<VariantAttrs> {
    let mut merged = VariantAttrs::default();
    let mut span = None;

    for attr in attrs {
        if attr.path().is_ident("dispatch") {
            merged.merge(attr.parse_args::<VariantAttrs>()?, attr.span())?;
            span.get_or_insert_with(|| attr.span());
        }
    }

    if let Some(span) = span {
        merged.validate(span)?;
    }

    Ok(merged)
}
