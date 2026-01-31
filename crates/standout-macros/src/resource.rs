//! Derive macro for Resource operations.
//!
//! This module provides the [`Resource`] derive macro that generates CLI commands
//! and handlers for Create, Read, Update, Delete operations on structs.
//!
//! # Example
//!
//! ```rust,ignore
//! use standout_macros::Resource;
//!
//! #[derive(Clone, Resource)]
//! #[resource(object = "task", store = TaskStore)]
//! pub struct Task {
//!     #[resource(id)]
//!     pub id: String,
//!
//!     #[resource(arg(short, long), form(required))]
//!     pub title: String,
//!
//!     #[resource(arg(short, long), choices = ["pending", "done"])]
//!     pub status: String,
//!
//!     #[resource(readonly)]
//!     pub created_at: DateTime<Utc>,
//! }
//! ```
//!
//! This generates:
//! - `TaskCommands` enum with List, View, Create, Update, Delete variants
//! - Handler module `__task_resource_handlers` with implementations
//! - Dispatch configuration via `TaskCommands::dispatch_config()`

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Data, DeriveInput, Error, Expr, Fields, Ident, Meta, Result, Token, Type,
};

/// Container-level attributes for #[resource(...)]
#[derive(Default)]
struct ResourceContainerAttrs {
    /// Required: singular object name (e.g., "task")
    object: Option<String>,
    /// Required: store type implementing ResourceStore
    store: Option<syn::Path>,
    /// Optional: plural name (defaults to "{object}s")
    plural: Option<String>,
    /// Optional: subset of operations to generate
    operations: Option<Vec<ResourceOperation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceOperation {
    List,
    View,
    Create,
    Update,
    Delete,
}

impl ResourceOperation {
    fn all() -> Vec<Self> {
        vec![
            Self::List,
            Self::View,
            Self::Create,
            Self::Update,
            Self::Delete,
        ]
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "list" => Some(Self::List),
            "view" => Some(Self::View),
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Field-level attributes for #[resource(...)]
#[derive(Default)]
struct ResourceFieldAttrs {
    /// This field is the primary identifier
    id: bool,
    /// Exclude from create/update operations
    readonly: bool,
    /// Exclude from all Resource operations
    skip: bool,
    /// Default value expression for create
    default_expr: Option<String>,
    /// Constrained values for this field
    choices: Option<Vec<String>>,
}

/// Information about a field for Resource operations
struct ResourceFieldInfo {
    ident: Ident,
    ty: Type,
    attrs: ResourceFieldAttrs,
}

impl Parse for ResourceContainerAttrs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = ResourceContainerAttrs::default();

        let content: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;

        for meta in content {
            match &meta {
                Meta::NameValue(nv) if nv.path.is_ident("object") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.object = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("store") => {
                    if let Expr::Path(expr_path) = &nv.value {
                        attrs.store = Some(expr_path.path.clone());
                    } else {
                        return Err(Error::new(nv.value.span(), "expected path"));
                    }
                }
                Meta::NameValue(nv) if nv.path.is_ident("plural") => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            attrs.plural = Some(lit_str.value());
                        } else {
                            return Err(Error::new(nv.value.span(), "expected string literal"));
                        }
                    } else {
                        return Err(Error::new(nv.value.span(), "expected string literal"));
                    }
                }
                Meta::List(list) if list.path.is_ident("operations") => {
                    let mut ops = Vec::new();
                    // Parse the inner tokens as comma-separated identifiers
                    let inner: Punctuated<Ident, Token![,]> =
                        list.parse_args_with(Punctuated::parse_terminated)?;
                    for ident in inner {
                        if let Some(op) = ResourceOperation::from_str(&ident.to_string()) {
                            ops.push(op);
                        } else {
                            return Err(Error::new(
                                ident.span(),
                                format!("unknown operation '{}', expected one of: list, view, create, update, delete", ident),
                            ));
                        }
                    }
                    attrs.operations = Some(ops);
                }
                _ => {
                    return Err(Error::new(
                        meta.span(),
                        "unknown attribute, expected one of: object, store, plural, operations",
                    ));
                }
            }
        }

        Ok(attrs)
    }
}

/// Parse field-level #[resource(...)] attributes
fn parse_field_attrs(attrs: &[syn::Attribute]) -> Result<ResourceFieldAttrs> {
    let mut field_attrs = ResourceFieldAttrs::default();

    for attr in attrs {
        if attr.path().is_ident("resource") {
            let nested: Punctuated<Meta, Token![,]> =
                attr.parse_args_with(Punctuated::parse_terminated)?;

            for meta in nested {
                match &meta {
                    Meta::Path(p) if p.is_ident("id") => {
                        field_attrs.id = true;
                    }
                    Meta::Path(p) if p.is_ident("readonly") => {
                        field_attrs.readonly = true;
                    }
                    Meta::Path(p) if p.is_ident("skip") => {
                        field_attrs.skip = true;
                    }
                    Meta::NameValue(nv) if nv.path.is_ident("default") => {
                        if let Expr::Lit(expr_lit) = &nv.value {
                            if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                                field_attrs.default_expr = Some(lit_str.value());
                            }
                        }
                    }
                    Meta::List(list) if list.path.is_ident("choices") => {
                        let inner: Punctuated<syn::LitStr, Token![,]> =
                            list.parse_args_with(Punctuated::parse_terminated)?;
                        let choices: Vec<String> = inner.iter().map(|l| l.value()).collect();
                        field_attrs.choices = Some(choices);
                    }
                    // Ignore arg, form, validate - they're for future expansion
                    Meta::List(list) if list.path.is_ident("arg") => {}
                    Meta::List(list) if list.path.is_ident("form") => {}
                    Meta::List(list) if list.path.is_ident("validate") => {}
                    _ => {
                        // Ignore unrecognized attributes for forward compatibility
                    }
                }
            }
        }
    }

    Ok(field_attrs)
}

/// Parse container-level #[resource(...)] attributes
fn parse_container_attrs(input: &DeriveInput) -> Result<ResourceContainerAttrs> {
    for attr in &input.attrs {
        if attr.path().is_ident("resource") {
            return attr.parse_args::<ResourceContainerAttrs>();
        }
    }

    Err(Error::new(
        input.span(),
        "missing `#[resource(object = \"...\", store = ...)]` attribute",
    ))
}

/// Extract field information from struct
fn extract_fields(fields: &Fields) -> Result<Vec<ResourceFieldInfo>> {
    let named_fields = match fields {
        Fields::Named(named) => &named.named,
        _ => {
            return Err(Error::new(
                fields.span(),
                "Resource can only be derived for structs with named fields",
            ));
        }
    };

    let mut result = Vec::new();
    for field in named_fields {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new(field.span(), "expected named field"))?;
        let attrs = parse_field_attrs(&field.attrs)?;

        result.push(ResourceFieldInfo {
            ident,
            ty: field.ty.clone(),
            attrs,
        });
    }

    Ok(result)
}

/// Main implementation of the Resource derive macro
pub fn resource_derive_impl(input: DeriveInput) -> Result<TokenStream> {
    let container_attrs = parse_container_attrs(&input)?;

    let object_name = container_attrs
        .object
        .ok_or_else(|| Error::new(input.span(), "missing `object` in #[resource(...)]"))?;

    let store_type = container_attrs
        .store
        .ok_or_else(|| Error::new(input.span(), "missing `store` in #[resource(...)]"))?;

    let _plural_name = container_attrs
        .plural
        .unwrap_or_else(|| format!("{}s", object_name));

    let operations = container_attrs
        .operations
        .unwrap_or_else(ResourceOperation::all);

    let struct_name = &input.ident;

    // Extract struct fields
    let fields = match &input.data {
        Data::Struct(data) => extract_fields(&data.fields)?,
        _ => {
            return Err(Error::new(
                input.span(),
                "Resource can only be derived for structs",
            ));
        }
    };

    // Find the ID field
    let _id_field = fields.iter().find(|f| f.attrs.id).ok_or_else(|| {
        Error::new(
            input.span(),
            "no field marked with #[resource(id)] - one field must be the identifier",
        )
    })?;

    // Generate names
    let commands_enum_name = format_ident!("{}Commands", struct_name);
    let handlers_module_name = format_ident!("__{}_resource_handlers", object_name);
    let object_name_upper = {
        let mut chars = object_name.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default()
            + chars.as_str()
    };

    // Fields for create/update (excluding id, readonly, and skip)
    let mutable_fields: Vec<&ResourceFieldInfo> = fields
        .iter()
        .filter(|f| !f.attrs.id && !f.attrs.readonly && !f.attrs.skip)
        .collect();

    // Generate clap args for create command
    let create_args: Vec<TokenStream> = mutable_fields
        .iter()
        .map(|f| {
            let name = &f.ident;
            let ty = &f.ty;
            let name_str = name.to_string();
            let long_name = name_str.replace('_', "-");

            if let Some(choices) = &f.attrs.choices {
                let choice_values: Vec<&String> = choices.iter().collect();
                quote! {
                    #[arg(long = #long_name, value_parser = clap::builder::PossibleValuesParser::new([#(#choice_values),*]))]
                    pub #name: Option<String>,
                }
            } else {
                quote! {
                    #[arg(long = #long_name)]
                    pub #name: Option<#ty>,
                }
            }
        })
        .collect();

    // Generate clap args for update command (all optional)
    let update_args: Vec<TokenStream> = mutable_fields
        .iter()
        .map(|f| {
            let name = &f.ident;
            let ty = &f.ty;
            let name_str = name.to_string();
            let long_name = name_str.replace('_', "-");

            if let Some(choices) = &f.attrs.choices {
                let choice_values: Vec<&String> = choices.iter().collect();
                quote! {
                    #[arg(long = #long_name, value_parser = clap::builder::PossibleValuesParser::new([#(#choice_values),*]))]
                    pub #name: Option<String>,
                }
            } else {
                quote! {
                    #[arg(long = #long_name)]
                    pub #name: Option<#ty>,
                }
            }
        })
        .collect();

    // Generate JSON field builders for create handler
    let create_json_fields: Vec<TokenStream> = mutable_fields
        .iter()
        .map(|f| {
            let name = &f.ident;
            let name_str = name.to_string();
            let long_name = name_str.replace('_', "-");
            quote! {
                if let Some(val) = matches.get_one::<String>(#long_name) {
                    __data[#name_str] = ::serde_json::json!(val);
                }
            }
        })
        .collect();

    // Generate JSON field builders for update handler
    let update_json_fields: Vec<TokenStream> = mutable_fields
        .iter()
        .map(|f| {
            let name = &f.ident;
            let name_str = name.to_string();
            let long_name = name_str.replace('_', "-");
            quote! {
                if let Some(val) = matches.get_one::<String>(#long_name) {
                    __data[#name_str] = ::serde_json::json!(val);
                    __changed.push(#name_str.to_string());
                }
            }
        })
        .collect();

    // Generate command enum variants based on selected operations
    let mut command_variants = Vec::new();
    let mut dispatch_commands = Vec::new();

    if operations.contains(&ResourceOperation::List) {
        command_variants.push(quote! {
            /// List all items
            List {
                #[arg(long)]
                filter: Option<String>,
                #[arg(long)]
                sort: Option<String>,
                #[arg(long)]
                limit: Option<usize>,
            },
        });
        dispatch_commands.push(quote! {
            let __builder = __builder.command_with(
                "list",
                #handlers_module_name::list,
                |cfg| cfg.template("standout/list-view")
            );
        });
    }

    if operations.contains(&ResourceOperation::View) {
        command_variants.push(quote! {
            /// View a single item
            View {
                /// The ID of the item to view
                id: String,
            },
        });
        dispatch_commands.push(quote! {
            let __builder = __builder.command_with(
                "view",
                #handlers_module_name::view,
                |cfg| cfg.template("standout/detail-view")
            );
        });
    }

    if operations.contains(&ResourceOperation::Create) {
        command_variants.push(quote! {
            /// Create a new item
            Create {
                #(#create_args)*
                #[arg(long)]
                dry_run: bool,
            },
        });
        dispatch_commands.push(quote! {
            let __builder = __builder.command_with(
                "create",
                #handlers_module_name::create,
                |cfg| cfg.template("standout/create-view")
            );
        });
    }

    if operations.contains(&ResourceOperation::Update) {
        command_variants.push(quote! {
            /// Update an existing item
            Update {
                /// The ID of the item to update
                id: String,
                #(#update_args)*
                #[arg(long)]
                dry_run: bool,
            },
        });
        dispatch_commands.push(quote! {
            let __builder = __builder.command_with(
                "update",
                #handlers_module_name::update,
                |cfg| cfg.template("standout/update-view")
            );
        });
    }

    if operations.contains(&ResourceOperation::Delete) {
        command_variants.push(quote! {
            /// Delete an item
            Delete {
                /// The ID of the item to delete
                id: String,
                #[arg(long)]
                confirm: bool,
                #[arg(long)]
                force: bool,
            },
        });
        dispatch_commands.push(quote! {
            let __builder = __builder.command_with(
                "delete",
                #handlers_module_name::delete,
                |cfg| cfg.template("standout/delete-view")
            );
        });
    }

    // Generate handler implementations
    let list_handler = if operations.contains(&ResourceOperation::List) {
        quote! {
            pub fn list(
                matches: &::clap::ArgMatches,
                ctx: &::standout::cli::CommandContext,
            ) -> ::standout::cli::HandlerResult<::standout::views::ListViewResult<#struct_name>> {
                let store = ctx.app_state.get_required::<#store_type>()?;

                let query = {
                    let filter = matches.get_one::<String>("filter").cloned();
                    let sort = matches.get_one::<String>("sort").cloned();
                    let limit = matches.get_one::<usize>("limit").cloned();

                    if filter.is_some() || sort.is_some() || limit.is_some() {
                        let mut q = ::standout::cli::ResourceQuery::new();
                        if let Some(f) = filter { q = q.filter(f); }
                        if let Some(s) = sort { q = q.sort(s); }
                        if let Some(l) = limit { q = q.limit(l); }
                        Some(q)
                    } else {
                        None
                    }
                };

                let items = store.list(query.as_ref())?;
                let total = items.len();

                Ok(::standout::cli::Output::Render(
                    ::standout::views::list_view(items)
                        .total_count(total)
                        .tabular_spec(<#struct_name as ::standout::tabular::Tabular>::tabular_spec())
                        .build()
                ))
            }
        }
    } else {
        quote! {}
    };

    let view_handler = if operations.contains(&ResourceOperation::View) {
        quote! {
            pub fn view(
                matches: &::clap::ArgMatches,
                ctx: &::standout::cli::CommandContext,
            ) -> ::standout::cli::HandlerResult<::standout::views::DetailViewResult<#struct_name>> {
                let store = ctx.app_state.get_required::<#store_type>()?;
                let id_str = matches.get_one::<String>("id").unwrap();
                let id = store.parse_id(id_str)?;
                let item = store.resolve(&id)?;

                Ok(::standout::cli::Output::Render(
                    ::standout::views::detail_view(item)
                        .title(#object_name_upper)
                        .subtitle(id_str)
                        .action("Update", format!("{} update {}", #object_name, id_str))
                        .action("Delete", format!("{} delete {}", #object_name, id_str))
                        .build()
                ))
            }
        }
    } else {
        quote! {}
    };

    let create_handler = if operations.contains(&ResourceOperation::Create) {
        quote! {
            pub fn create(
                matches: &::clap::ArgMatches,
                ctx: &::standout::cli::CommandContext,
            ) -> ::standout::cli::HandlerResult<::standout::views::CreateViewResult<#struct_name>> {
                let store = ctx.app_state.get_required::<#store_type>()?;
                let dry_run = matches.get_flag("dry_run");

                // Build JSON data from matches
                let mut __data = ::serde_json::json!({});
                #(#create_json_fields)*

                if dry_run {
                    // For dry-run, try to deserialize to show what would be created
                    match ::serde_json::from_value::<#struct_name>(__data) {
                        Ok(preview) => {
                            Ok(::standout::cli::Output::Render(
                                ::standout::views::create_view(preview)
                                    .dry_run()
                                    .info("Dry run - no changes made")
                                    .build()
                            ))
                        }
                        Err(e) => {
                            Err(::anyhow::anyhow!("Invalid data: {}", e))
                        }
                    }
                } else {
                    let item = store.create(__data)?;
                    Ok(::standout::cli::Output::Render(
                        ::standout::views::create_view(item)
                            .success(format!("{} created", #object_name_upper))
                            .build()
                    ))
                }
            }
        }
    } else {
        quote! {}
    };

    let update_handler = if operations.contains(&ResourceOperation::Update) {
        quote! {
            pub fn update(
                matches: &::clap::ArgMatches,
                ctx: &::standout::cli::CommandContext,
            ) -> ::standout::cli::HandlerResult<::standout::views::UpdateViewResult<#struct_name>> {
                let store = ctx.app_state.get_required::<#store_type>()?;
                let id_str = matches.get_one::<String>("id").unwrap();
                let id = store.parse_id(id_str)?;
                let dry_run = matches.get_flag("dry_run");

                // Get the current state before update
                let before = store.resolve(&id)?;

                // Build JSON data and track changed fields
                let mut __data = ::serde_json::json!({});
                let mut __changed: Vec<String> = Vec::new();
                #(#update_json_fields)*

                if dry_run {
                    Ok(::standout::cli::Output::Render(
                        ::standout::views::update_view(before.clone())
                            .before(before)
                            .changed_fields(__changed)
                            .dry_run()
                            .info("Dry run - no changes made")
                            .build()
                    ))
                } else if __changed.is_empty() {
                    Ok(::standout::cli::Output::Render(
                        ::standout::views::update_view(before)
                            .info("No changes specified")
                            .build()
                    ))
                } else {
                    let after = store.update(&id, __data)?;
                    Ok(::standout::cli::Output::Render(
                        ::standout::views::update_view(after)
                            .before(before)
                            .changed_fields(__changed)
                            .success(format!("{} updated", #object_name_upper))
                            .build()
                    ))
                }
            }
        }
    } else {
        quote! {}
    };

    let delete_handler = if operations.contains(&ResourceOperation::Delete) {
        quote! {
            pub fn delete(
                matches: &::clap::ArgMatches,
                ctx: &::standout::cli::CommandContext,
            ) -> ::standout::cli::HandlerResult<::standout::views::DeleteViewResult<#struct_name>> {
                let store = ctx.app_state.get_required::<#store_type>()?;
                let id_str = matches.get_one::<String>("id").unwrap();
                let id = store.parse_id(id_str)?;
                let confirm = matches.get_flag("confirm");
                let force = matches.get_flag("force");

                // Get the item to show what will be/was deleted
                let item = store.resolve(&id)?;

                if !confirm && !force {
                    // Show confirmation prompt
                    Ok(::standout::cli::Output::Render(
                        ::standout::views::delete_view(item)
                            .warning(format!("Use --confirm to delete this {}", #object_name))
                            .build()
                    ))
                } else {
                    store.delete(&id)?;
                    Ok(::standout::cli::Output::Render(
                        ::standout::views::delete_view(item)
                            .confirmed()
                            .success(format!("{} deleted", #object_name_upper))
                            .build()
                    ))
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate the full output
    let expanded = quote! {
        /// Commands enum for Resource operations on #struct_name
        #[derive(::clap::Subcommand, Clone, Debug)]
        pub enum #commands_enum_name {
            #(#command_variants)*
        }

        impl #commands_enum_name {
            /// Returns the dispatch configuration for these Resource commands
            pub fn dispatch_config() -> impl FnOnce(::standout::cli::GroupBuilder) -> ::standout::cli::GroupBuilder {
                |__builder| {
                    #(#dispatch_commands)*
                    __builder
                }
            }
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #handlers_module_name {
            use super::*;

            #list_handler
            #view_handler
            #create_handler
            #update_handler
            #delete_handler
        }
    };

    Ok(expanded)
}
