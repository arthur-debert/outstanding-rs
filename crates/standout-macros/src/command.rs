use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Error, Expr, FnArg, ItemFn, Lit, Meta, Pat, PatType, Result, Token, Type,
};

#[derive(Default)]
struct CommandAttrs {
    name: Option<String>,
    about: Option<String>,
    long_about: Option<String>,
    visible_alias: Option<String>,
    hide: bool,
    template: Option<String>,
}

impl Parse for CommandAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = CommandAttrs::default();

        if input.is_empty() {
            return Ok(attrs);
        }

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match meta {
                Meta::NameValue(nv) => {
                    let ident = nv.path.get_ident().map(|i| i.to_string());
                    match ident.as_deref() {
                        Some("name") => {
                            attrs.name = Some(parse_string_value(&nv.value)?);
                        }
                        Some("about") => {
                            attrs.about = Some(parse_string_value(&nv.value)?);
                        }
                        Some("long_about") => {
                            attrs.long_about = Some(parse_string_value(&nv.value)?);
                        }
                        Some("visible_alias") => {
                            attrs.visible_alias = Some(parse_string_value(&nv.value)?);
                        }
                        Some("template") => {
                            attrs.template = Some(parse_string_value(&nv.value)?);
                        }
                        Some("hide") => {
                            attrs.hide = parse_bool_value(&nv.value)?;
                        }
                        Some(other) => {
                            return Err(Error::new(
                                nv.path.span(),
                                format!("unknown command attribute `{}`", other),
                            ));
                        }
                        None => {
                            return Err(Error::new(nv.path.span(), "expected identifier"));
                        }
                    }
                }
                Meta::Path(path) => {
                    if path.is_ident("hide") {
                        attrs.hide = true;
                    } else {
                        return Err(Error::new(
                            path.span(),
                            "expected `name = \"...\"` style attribute",
                        ));
                    }
                }
                Meta::List(_) => {
                    return Err(Error::new(
                        meta.span(),
                        "unexpected attribute format, use `key = value`",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

#[derive(Debug, Clone)]
enum ParamKind {
    Flag(FlagAttrs),
    Arg(ArgAttrs),
    Ctx,
    Matches,
    None,
}

#[derive(Debug, Clone, Default)]
struct FlagAttrs {
    short: Option<char>,
    long: Option<String>,
    help: Option<String>,
    hide: bool,
}

#[derive(Debug, Clone, Default)]
struct ArgAttrs {
    short: Option<char>,
    long: Option<String>,
    help: Option<String>,
    value_name: Option<String>,
    default: Option<String>,
    hide: bool,
    positional: bool,
    allow_negative_numbers: bool,
}

struct ParamInfo {
    rust_name: String,
    cli_name: String,
    ty: Type,
    kind: ParamKind,
}

fn parse_string_value(expr: &Expr) -> Result<String> {
    if let Expr::Lit(expr_lit) = expr {
        if let Lit::Str(lit_str) = &expr_lit.lit {
            return Ok(lit_str.value());
        }
    }
    Err(Error::new(expr.span(), "expected string literal"))
}

fn parse_bool_value(expr: &Expr) -> Result<bool> {
    if let Expr::Lit(expr_lit) = expr {
        if let Lit::Bool(lit_bool) = &expr_lit.lit {
            return Ok(lit_bool.value());
        }
    }
    Err(Error::new(expr.span(), "expected boolean literal"))
}

#[allow(dead_code)]
fn parse_char_value(expr: &Expr) -> Result<char> {
    if let Expr::Lit(expr_lit) = expr {
        if let Lit::Char(lit_char) = &expr_lit.lit {
            return Ok(lit_char.value());
        }
    }
    Err(Error::new(expr.span(), "expected character literal"))
}

fn parse_flag_attrs(attr: &syn::Attribute) -> Result<FlagAttrs> {
    let mut attrs = FlagAttrs::default();

    if attr.meta.require_path_only().is_ok() {
        return Ok(attrs);
    }

    attr.parse_nested_meta(|meta| {
        let ident = meta.path.get_ident().map(|i| i.to_string());
        match ident.as_deref() {
            Some("short") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Char(c) = value {
                    attrs.short = Some(c.value());
                } else {
                    return Err(Error::new(value.span(), "expected character literal"));
                }
            }
            Some("long") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.long = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some("help") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.help = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some("hide") => {
                if meta.input.peek(Token![=]) {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Bool(b) = value {
                        attrs.hide = b.value();
                    } else {
                        return Err(Error::new(value.span(), "expected boolean literal"));
                    }
                } else {
                    attrs.hide = true;
                }
            }
            Some("name") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.long = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some(other) => {
                return Err(Error::new(
                    meta.path.span(),
                    format!("unknown flag attribute `{}`", other),
                ));
            }
            None => {
                return Err(Error::new(meta.path.span(), "expected identifier"));
            }
        }
        Ok(())
    })?;

    Ok(attrs)
}

fn parse_arg_attrs(attr: &syn::Attribute) -> Result<ArgAttrs> {
    let mut attrs = ArgAttrs::default();

    if attr.meta.require_path_only().is_ok() {
        return Ok(attrs);
    }

    attr.parse_nested_meta(|meta| {
        let ident = meta.path.get_ident().map(|i| i.to_string());
        match ident.as_deref() {
            Some("short") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Char(c) = value {
                    attrs.short = Some(c.value());
                } else {
                    return Err(Error::new(value.span(), "expected character literal"));
                }
            }
            Some("long") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.long = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some("help") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.help = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some("value_name") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.value_name = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some("default") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.default = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some("hide") => {
                if meta.input.peek(Token![=]) {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Bool(b) = value {
                        attrs.hide = b.value();
                    } else {
                        return Err(Error::new(value.span(), "expected boolean literal"));
                    }
                } else {
                    attrs.hide = true;
                }
            }
            Some("positional") => {
                if meta.input.peek(Token![=]) {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Bool(b) = value {
                        attrs.positional = b.value();
                    } else {
                        return Err(Error::new(value.span(), "expected boolean literal"));
                    }
                } else {
                    attrs.positional = true;
                }
            }
            Some("allow_negative_numbers") => {
                if meta.input.peek(Token![=]) {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Bool(b) = value {
                        attrs.allow_negative_numbers = b.value();
                    } else {
                        return Err(Error::new(value.span(), "expected boolean literal"));
                    }
                } else {
                    attrs.allow_negative_numbers = true;
                }
            }
            Some("name") => {
                let value: Lit = meta.value()?.parse()?;
                if let Lit::Str(s) = value {
                    attrs.long = Some(s.value());
                } else {
                    return Err(Error::new(value.span(), "expected string literal"));
                }
            }
            Some(other) => {
                return Err(Error::new(
                    meta.path.span(),
                    format!("unknown arg attribute `{}`", other),
                ));
            }
            None => {
                return Err(Error::new(meta.path.span(), "expected identifier"));
            }
        }
        Ok(())
    })?;

    Ok(attrs)
}

fn parse_param_kind(pat_type: &PatType) -> Result<ParamKind> {
    for attr in &pat_type.attrs {
        if attr.path().is_ident("flag") {
            return Ok(ParamKind::Flag(parse_flag_attrs(attr)?));
        }
        if attr.path().is_ident("arg") {
            return Ok(ParamKind::Arg(parse_arg_attrs(attr)?));
        }
        if attr.path().is_ident("ctx") {
            return Ok(ParamKind::Ctx);
        }
        if attr.path().is_ident("matches") {
            return Ok(ParamKind::Matches);
        }
    }
    Ok(ParamKind::None)
}

fn extract_param_name(pat: &Pat) -> Result<String> {
    match pat {
        Pat::Ident(ident) => Ok(ident.ident.to_string()),
        _ => Err(Error::new(
            pat.span(),
            "expected identifier pattern for parameter",
        )),
    }
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

fn is_vec_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Vec";
        }
    }
    false
}

fn extract_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner);
                }
            }
        }
    }
    None
}

fn is_reference_type(ty: &Type) -> bool {
    matches!(ty, Type::Reference(_))
}

fn get_base_type(ty: &Type) -> Option<&Type> {
    if is_option_type(ty) || is_vec_type(ty) {
        extract_inner_type(ty)
    } else {
        Some(ty)
    }
}

fn needs_value_parser(ty: &Type) -> bool {
    let base = match get_base_type(ty) {
        Some(t) => t,
        None => return false,
    };

    if let Type::Path(type_path) = base {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();
            if type_name == "String" || type_name == "OsString" {
                return false;
            }
            return true;
        }
    }

    false
}

fn generate_value_parser(ty: &Type) -> Option<TokenStream> {
    let base = get_base_type(ty)?;

    if !needs_value_parser(ty) {
        return None;
    }

    Some(quote! { .value_parser(::clap::value_parser!(#base)) })
}

fn is_unit_result(fn_item: &ItemFn) -> bool {
    matches!(extract_result_ok_type(fn_item), Some(Type::Tuple(t)) if t.elems.is_empty())
}

fn extract_result_ok_type(fn_item: &ItemFn) -> Option<Type> {
    if let syn::ReturnType::Type(_, ty) = &fn_item.sig.output {
        if let Type::Path(type_path) = ty.as_ref() {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(ok_type)) = args.args.first() {
                            return Some(ok_type.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_output_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Output" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return Some(inner);
                    }
                }
            }
        }
    }
    None
}

fn generate_extraction(param: &ParamInfo) -> TokenStream {
    let rust_name = format_ident!("{}", param.rust_name);
    let cli_name = &param.cli_name;
    let ty = &param.ty;

    match &param.kind {
        ParamKind::Flag(_) => {
            quote! {
                let #rust_name: bool = __matches.get_flag(#cli_name);
            }
        }
        ParamKind::Arg(_) => {
            if is_option_type(ty) {
                let inner = extract_inner_type(ty).unwrap_or(ty);
                quote! {
                    let #rust_name: #ty = __matches.get_one::<#inner>(#cli_name).cloned();
                }
            } else if is_vec_type(ty) {
                let inner = extract_inner_type(ty).unwrap_or(ty);
                quote! {
                    let #rust_name: #ty = __matches
                        .get_many::<#inner>(#cli_name)
                        .map(|v| v.cloned().collect())
                        .unwrap_or_default();
                }
            } else {
                quote! {
                    let #rust_name: #ty = __matches.get_one::<#ty>(#cli_name)
                        .expect(concat!("Missing required argument '", #cli_name, "' - ensure clap definition matches handler"))
                        .clone();
                }
            }
        }
        ParamKind::Ctx | ParamKind::Matches | ParamKind::None => {
            quote! {}
        }
    }
}

fn generate_call_arg(param: &ParamInfo) -> TokenStream {
    let rust_name = format_ident!("{}", param.rust_name);

    match &param.kind {
        ParamKind::Flag(_) | ParamKind::Arg(_) => {
            quote! { #rust_name }
        }
        ParamKind::Ctx => {
            quote! { __ctx }
        }
        ParamKind::Matches => {
            quote! { __matches }
        }
        ParamKind::None => {
            quote! { #rust_name }
        }
    }
}

fn generate_expected_arg(param: &ParamInfo) -> Option<TokenStream> {
    let cli_name = &param.cli_name;
    let rust_name = &param.rust_name;

    match &param.kind {
        ParamKind::Flag(_) => Some(quote! {
            ::standout_dispatch::verify::ExpectedArg::flag(#cli_name, #rust_name)
        }),
        ParamKind::Arg(_) => {
            let ty = &param.ty;
            if is_option_type(ty) {
                Some(quote! {
                    ::standout_dispatch::verify::ExpectedArg::optional_arg(#cli_name, #rust_name)
                })
            } else if is_vec_type(ty) {
                Some(quote! {
                    ::standout_dispatch::verify::ExpectedArg::vec_arg(#cli_name, #rust_name)
                })
            } else {
                Some(quote! {
                    ::standout_dispatch::verify::ExpectedArg::required_arg(#cli_name, #rust_name)
                })
            }
        }
        ParamKind::Ctx | ParamKind::Matches | ParamKind::None => None,
    }
}

fn generate_clap_arg(param: &ParamInfo) -> Option<TokenStream> {
    let cli_name = &param.cli_name;

    match &param.kind {
        ParamKind::Flag(attrs) => {
            let mut arg = quote! {
                ::clap::Arg::new(#cli_name)
                    .action(::clap::ArgAction::SetTrue)
            };

            if let Some(short) = attrs.short {
                arg = quote! { #arg.short(#short) };
            }

            let long_name = attrs.long.as_deref().unwrap_or(cli_name);
            arg = quote! { #arg.long(#long_name) };

            if let Some(ref help) = attrs.help {
                arg = quote! { #arg.help(#help) };
            }

            if attrs.hide {
                arg = quote! { #arg.hide(true) };
            }

            Some(quote! { .arg(#arg) })
        }
        ParamKind::Arg(attrs) => {
            let ty = &param.ty;
            let is_optional = is_option_type(ty);
            let is_vec = is_vec_type(ty);

            let mut arg = quote! { ::clap::Arg::new(#cli_name) };

            if is_vec {
                arg = quote! { #arg.action(::clap::ArgAction::Append) };
            } else {
                arg = quote! { #arg.action(::clap::ArgAction::Set) };
            }

            if let Some(value_parser) = generate_value_parser(ty) {
                arg = quote! { #arg #value_parser };
            }

            if !is_optional && !is_vec && attrs.default.is_none() {
                arg = quote! { #arg.required(true) };
            }

            if let Some(short) = attrs.short {
                arg = quote! { #arg.short(#short) };
            }

            if !attrs.positional {
                let long_name = attrs.long.as_deref().unwrap_or(cli_name);
                arg = quote! { #arg.long(#long_name) };
            }

            if let Some(ref help) = attrs.help {
                arg = quote! { #arg.help(#help) };
            }

            if let Some(ref value_name) = attrs.value_name {
                arg = quote! { #arg.value_name(#value_name) };
            }

            if let Some(ref default) = attrs.default {
                arg = quote! { #arg.default_value(#default) };
            }

            if attrs.hide {
                arg = quote! { #arg.hide(true) };
            }

            if attrs.allow_negative_numbers {
                arg = quote! { #arg.allow_negative_numbers(true) };
            }

            Some(quote! { .arg(#arg) })
        }
        ParamKind::Ctx | ParamKind::Matches | ParamKind::None => None,
    }
}

pub fn command_impl(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let cmd_attrs: CommandAttrs = syn::parse2(attr)?;

    let command_name = cmd_attrs.name.ok_or_else(|| {
        Error::new(
            proc_macro2::Span::call_site(),
            "#[command] requires `name` attribute: #[command(name = \"...\")]",
        )
    })?;

    let template_name = cmd_attrs.template.unwrap_or_else(|| command_name.clone());

    let fn_item: ItemFn = syn::parse2(item)?;
    let fn_name = &fn_item.sig.ident;
    let fn_vis = &fn_item.vis;

    let handler_fn_name = format_ident!("{}__handler", fn_name);
    let expected_args_fn_name = format_ident!("{}__expected_args", fn_name);
    let command_fn_name = format_ident!("{}__command", fn_name);
    let template_fn_name = format_ident!("{}__template", fn_name);
    let handler_struct_name = format_ident!("{}_Handler", fn_name);

    let mut params: Vec<ParamInfo> = Vec::new();

    for fn_arg in &fn_item.sig.inputs {
        match fn_arg {
            FnArg::Typed(pat_type) => {
                let kind = parse_param_kind(pat_type)?;
                let rust_name = extract_param_name(&pat_type.pat)?;

                let cli_name = match &kind {
                    ParamKind::Flag(attrs) => attrs
                        .long
                        .clone()
                        .unwrap_or_else(|| rust_name.replace('_', "-")),
                    ParamKind::Arg(attrs) => attrs
                        .long
                        .clone()
                        .unwrap_or_else(|| rust_name.replace('_', "-")),
                    _ => rust_name.clone(),
                };

                if matches!(kind, ParamKind::None) && !is_reference_type(&pat_type.ty) {
                    return Err(Error::new(
                        pat_type.span(),
                        "parameter must have #[flag], #[arg], #[ctx], or #[matches] annotation",
                    ));
                }

                params.push(ParamInfo {
                    rust_name,
                    cli_name,
                    ty: (*pat_type.ty).clone(),
                    kind,
                });
            }
            FnArg::Receiver(_) => {
                return Err(Error::new(
                    fn_arg.span(),
                    "#[command] functions cannot have self parameter",
                ));
            }
        }
    }

    let extractions: Vec<TokenStream> = params.iter().map(generate_extraction).collect();

    let call_args: Vec<TokenStream> = params.iter().map(generate_call_arg).collect();

    let expected_args: Vec<TokenStream> = params.iter().filter_map(generate_expected_arg).collect();

    let clap_args: Vec<TokenStream> = params.iter().filter_map(generate_clap_arg).collect();

    let return_type = &fn_item.sig.output;

    let call_and_return = if is_unit_result(&fn_item) {
        quote! {
            #fn_name(#(#call_args),*)?;
            Ok(::standout_dispatch::Output::Silent)
        }
    } else {
        quote! {
            #fn_name(#(#call_args),*)
        }
    };

    let wrapper_return_type = if is_unit_result(&fn_item) {
        quote! { -> ::standout_dispatch::HandlerResult<()> }
    } else {
        quote! { #return_type }
    };

    let ok_type = extract_result_ok_type(&fn_item).ok_or_else(|| {
        Error::new(
            fn_item.sig.output.span(),
            "handler must return Result<T, E>",
        )
    })?;

    let output_type = if is_unit_result(&fn_item) {
        quote! { () }
    } else if let Some(inner) = extract_output_type(&ok_type) {
        quote! { #inner }
    } else {
        quote! { #ok_type }
    };

    let mut clean_fn = fn_item.clone();
    for fn_arg in &mut clean_fn.sig.inputs {
        if let FnArg::Typed(pat_type) = fn_arg {
            pat_type.attrs.retain(|attr| {
                !attr.path().is_ident("flag")
                    && !attr.path().is_ident("arg")
                    && !attr.path().is_ident("ctx")
                    && !attr.path().is_ident("matches")
            });
        }
    }

    let mut cmd_builder = quote! {
        ::clap::Command::new(#command_name)
    };

    if let Some(ref about) = cmd_attrs.about {
        cmd_builder = quote! { #cmd_builder.about(#about) };
    }

    if let Some(ref long_about) = cmd_attrs.long_about {
        cmd_builder = quote! { #cmd_builder.long_about(#long_about) };
    }

    if let Some(ref alias) = cmd_attrs.visible_alias {
        cmd_builder = quote! { #cmd_builder.visible_alias(#alias) };
    }

    if cmd_attrs.hide {
        cmd_builder = quote! { #cmd_builder.hide(true) };
    }

    for arg in &clap_args {
        cmd_builder = quote! { #cmd_builder #arg };
    }

    Ok(quote! {
        #clean_fn

        #fn_vis fn #handler_fn_name(
            __matches: &::clap::ArgMatches,
            __ctx: &::standout_dispatch::CommandContext
        ) #wrapper_return_type {
            #(#extractions)*
            #call_and_return
        }

        #fn_vis fn #expected_args_fn_name() -> ::std::vec::Vec<::standout_dispatch::verify::ExpectedArg> {
            vec![#(#expected_args),*]
        }

        #fn_vis fn #command_fn_name() -> ::clap::Command {
            #cmd_builder
        }

        #fn_vis fn #template_fn_name() -> &'static str {
            #template_name
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy)]
        #fn_vis struct #handler_struct_name;

        impl ::standout_dispatch::Handler for #handler_struct_name {
            type Output = #output_type;

            fn handle(
                &mut self,
                matches: &::clap::ArgMatches,
                ctx: &::standout_dispatch::CommandContext
            ) -> ::standout_dispatch::HandlerResult<Self::Output> {
                ::standout_dispatch::IntoHandlerResult::into_handler_result(
                    #handler_fn_name(matches, ctx)
                )
            }

            fn expected_args(&self) -> ::std::vec::Vec<::standout_dispatch::verify::ExpectedArg> {
                #expected_args_fn_name()
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_attrs() {
        let tokens: TokenStream = quote! {
            name = "list", about = "List items", template = "custom_list"
        };

        let attrs: CommandAttrs = syn::parse2(tokens).unwrap();
        assert_eq!(attrs.name, Some("list".to_string()));
        assert_eq!(attrs.about, Some("List items".to_string()));
        assert_eq!(attrs.template, Some("custom_list".to_string()));
    }

    #[test]
    fn test_parse_command_attrs_minimal() {
        let tokens: TokenStream = quote! {
            name = "add"
        };

        let attrs: CommandAttrs = syn::parse2(tokens).unwrap();
        assert_eq!(attrs.name, Some("add".to_string()));
        assert_eq!(attrs.about, None);
        assert_eq!(attrs.template, None);
    }
}
