//! # Configulator Derive Macro
//!
//! Derive macro for
//! [`configulator-rs`](https://crates.io/crates/configulator-rs).
//! This crate is not intended to be used directly, add
//! `configulator-rs` as a dependency instead.

#![warn(clippy::all)]
#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

/// Derive macro that generates the configulator shadow type and loading
/// machinery for a struct.
///
/// Every field's type must implement [`FromStr`](std::str::FromStr) (and
/// `Default`, unless it is an `Option`), or be marked
/// `#[configulator(nested)]` and itself derive `Config`.
#[proc_macro_derive(Config, attributes(configulator))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_config_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[derive(Default)]
struct StructAttrs {
    crate_path: Option<syn::Path>,
    allow_unknown_fields: bool,
}

fn parse_struct_attrs(attrs: &[syn::Attribute]) -> Result<StructAttrs, syn::Error> {
    let mut out = StructAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("configulator") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.crate_path = Some(lit.parse()?);
            } else if meta.path.is_ident("allow_unknown_fields") {
                out.allow_unknown_fields = true;
            } else {
                return Err(meta.error(
                    "unknown struct-level configulator attribute; \
                     expected `crate` or `allow_unknown_fields`",
                ));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

#[derive(Default)]
struct FieldAttrs {
    name: Option<String>,
    default: Option<String>,
    description: Option<String>,
    nested: bool,
    short: Option<char>,
    secret: bool,
    required: bool,
    env: Option<String>,
    flag: Option<String>,
}

fn parse_field_attrs(attrs: &[syn::Attribute]) -> Result<FieldAttrs, syn::Error> {
    let mut out = FieldAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("configulator") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.name = Some(lit.value());
            } else if meta.path.is_ident("default") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.default = Some(lit.value());
            } else if meta.path.is_ident("description") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.description = Some(lit.value());
            } else if meta.path.is_ident("nested") {
                out.nested = true;
            } else if meta.path.is_ident("secret") {
                out.secret = true;
            } else if meta.path.is_ident("required") {
                out.required = true;
            } else if meta.path.is_ident("short") {
                let value = meta.value()?;
                if let Ok(lit) = value.parse::<syn::LitChar>() {
                    out.short = Some(lit.value());
                } else {
                    let lit: syn::LitStr = value.parse()?;
                    let val = lit.value();
                    let mut chars = val.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => out.short = Some(c),
                        _ => {
                            return Err(
                                meta.error("`short` must be a single character, e.g. short = 'p'")
                            )
                        }
                    }
                }
            } else if meta.path.is_ident("env") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.env = Some(lit.value());
            } else if meta.path.is_ident("flag") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.flag = Some(lit.value());
            } else {
                let name = meta
                    .path
                    .get_ident()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "?".to_string());
                return Err(meta.error(format_args!(
                    "unknown configulator attribute `{name}`; expected `name`, `default`, \
                     `description`, `nested`, `short`, `secret`, `required`, `env`, or `flag`",
                )));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

enum MapKind {
    Hash,
    BTree,
}

enum Shape {
    /// `bool` or `Option<bool>` (CLI-flag special case); `opt` for Option.
    Bool { opt: bool },
    /// A `FromStr` scalar; `opt` for `Option<T>` (`ty` is the inner type).
    Leaf { ty: Type, opt: bool },
    /// `Vec<T>` of scalars.
    VecLeaf { elem: Type },
    /// A map with scalar values.
    MapLeaf { kind: MapKind, key: Type, val: Type },
    /// A nested Config struct; `opt` for `Option<T>`.
    Nested { ty: Type, opt: bool },
    /// `Vec<T>` of nested Config structs.
    VecNested { elem: Type },
    /// A map of nested Config structs.
    MapNested { kind: MapKind, key: Type, val: Type },
}

fn single_type_arg(seg: &syn::PathSegment) -> Option<Type> {
    if let PathArguments::AngleBracketed(args) = &seg.arguments {
        let types: Vec<_> = args
            .args
            .iter()
            .filter_map(|a| match a {
                GenericArgument::Type(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        if types.len() == 1 {
            return Some(types[0].clone());
        }
    }
    None
}

fn two_type_args(seg: &syn::PathSegment) -> Option<(Type, Type)> {
    if let PathArguments::AngleBracketed(args) = &seg.arguments {
        let types: Vec<_> = args
            .args
            .iter()
            .filter_map(|a| match a {
                GenericArgument::Type(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        if types.len() == 2 {
            return Some((types[0].clone(), types[1].clone()));
        }
    }
    None
}

fn last_segment(ty: &Type) -> Option<&syn::PathSegment> {
    match ty {
        Type::Path(p) => p.path.segments.last(),
        _ => None,
    }
}

fn classify(field: &syn::Field, attrs: &FieldAttrs) -> Result<Shape, syn::Error> {
    classify_inner(&field.ty, attrs, field, false)
}

fn classify_inner(
    ty: &Type,
    attrs: &FieldAttrs,
    field: &syn::Field,
    inside_option: bool,
) -> Result<Shape, syn::Error> {
    let err = |msg: &str| Err(syn::Error::new_spanned(field, msg));

    if let Some(seg) = last_segment(ty) {
        if seg.ident == "Option" {
            if inside_option {
                return err("Option<Option<T>> config fields are not supported");
            }
            let inner = single_type_arg(seg).ok_or_else(|| {
                syn::Error::new_spanned(field, "Option must have a type argument")
            })?;
            let inner_shape = classify_inner(&inner, attrs, field, true)?;
            return match inner_shape {
                Shape::Bool { .. } => Ok(Shape::Bool { opt: true }),
                Shape::Leaf { ty, .. } => Ok(Shape::Leaf { ty, opt: true }),
                Shape::Nested { ty, .. } => Ok(Shape::Nested { ty, opt: true }),
                _ => {
                    err("Option of a collection is not supported; use an empty collection instead")
                }
            };
        }
        if seg.ident == "bool" {
            if attrs.nested {
                return err("`nested` cannot be applied to bool");
            }
            return Ok(Shape::Bool { opt: false });
        }
        if seg.ident == "Vec" {
            let elem = single_type_arg(seg)
                .ok_or_else(|| syn::Error::new_spanned(field, "Vec must have a type argument"))?;
            return if attrs.nested {
                Ok(Shape::VecNested { elem })
            } else {
                Ok(Shape::VecLeaf { elem })
            };
        }
        if seg.ident == "HashMap" || seg.ident == "BTreeMap" {
            let kind = if seg.ident == "HashMap" {
                MapKind::Hash
            } else {
                MapKind::BTree
            };
            let (key, val) = two_type_args(seg).ok_or_else(|| {
                syn::Error::new_spanned(field, "maps must have key and value type arguments")
            })?;
            return if attrs.nested {
                Ok(Shape::MapNested { kind, key, val })
            } else {
                Ok(Shape::MapLeaf { kind, key, val })
            };
        }
    }

    if attrs.nested {
        Ok(Shape::Nested {
            ty: ty.clone(),
            opt: false,
        })
    } else {
        Ok(Shape::Leaf {
            ty: ty.clone(),
            opt: false,
        })
    }
}

impl Shape {
    fn is_file_only(&self) -> bool {
        matches!(
            self,
            Shape::MapLeaf { .. } | Shape::VecNested { .. } | Shape::MapNested { .. }
        )
    }
}

fn scalar_hint(ty: &Type) -> &'static str {
    if let Some(seg) = last_segment(ty) {
        let id = seg.ident.to_string();
        match id.as_str() {
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => return "Integer",
            "f32" | "f64" => return "Float",
            "bool" => return "Bool",
            _ => {}
        }
    }
    "String"
}

fn map_type(kind: &MapKind) -> TokenStream2 {
    match kind {
        MapKind::Hash => quote!(::std::collections::HashMap),
        MapKind::BTree => quote!(::std::collections::BTreeMap),
    }
}

struct FieldModel {
    ident: syn::Ident,
    config_name: String,
    env_segment: String,
    flag_segment: String,
    skip_env: bool,
    skip_cli: bool,
    attrs: FieldAttrs,
    shape: Shape,
}

fn build_model(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::Token![,]>,
) -> Result<Vec<FieldModel>, syn::Error> {
    let mut out = Vec::new();
    for field in fields {
        let attrs = parse_field_attrs(&field.attrs)?;
        let shape = classify(field, &attrs)?;
        let ident = field.ident.clone().unwrap();
        let config_name = attrs
            .name
            .clone()
            .unwrap_or_else(|| ident.unraw().to_string());

        if shape.is_file_only() {
            if let Some(env) = &attrs.env {
                if env != "-" {
                    return Err(syn::Error::new_spanned(
                        field,
                        "`env = \"...\"` opt-in on a collection: collections are file-only (SPEC rule 6)",
                    ));
                }
            }
            if let Some(flag) = &attrs.flag {
                if flag != "-" {
                    return Err(syn::Error::new_spanned(
                        field,
                        "`flag = \"...\"` opt-in on a collection: collections are file-only (SPEC rule 6)",
                    ));
                }
            }
        }
        if attrs.default.is_some()
            && matches!(
                shape,
                Shape::Nested { .. }
                    | Shape::VecNested { .. }
                    | Shape::MapNested { .. }
                    | Shape::MapLeaf { .. }
            )
        {
            return Err(syn::Error::new_spanned(
                field,
                "`default` is only supported on scalar and Vec-of-scalar fields",
            ));
        }

        if let Some(env) = &attrs.env {
            if env != "-"
                && (env.is_empty()
                    || !env
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "`env = \"...\"` overrides must be uppercase A-Z, 0-9, and _ \
                     (they are used verbatim in both configulator implementations)",
                ));
            }
        }
        let skip_env = attrs.env.as_deref() == Some("-");
        let skip_cli = attrs.flag.as_deref() == Some("-");
        let env_segment = match &attrs.env {
            Some(e) if e != "-" => e.clone(),
            _ => config_name.to_uppercase().replace('-', "_"),
        };
        let flag_segment = match &attrs.flag {
            Some(f) if f != "-" => f.clone(),
            _ => config_name.clone(),
        };

        out.push(FieldModel {
            ident,
            config_name,
            env_segment,
            flag_segment,
            skip_env,
            skip_cli,
            attrs,
            shape,
        });
    }

    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for m in &out {
        let folded = m.config_name.to_lowercase().replace('-', "_");
        if let Some(prev) = seen.get(&folded) {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "config names {prev:?} and {:?} collide under case and -/_ folding",
                    m.config_name
                ),
            ));
        }
        seen.insert(folded, m.config_name.clone());
    }
    Ok(out)
}

fn derive_config_impl(input: &DeriveInput) -> Result<TokenStream2, syn::Error> {
    let name = &input.ident;
    let vis = &input.vis;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "Config cannot be derived for generic structs",
        ));
    }
    let struct_attrs = parse_struct_attrs(&input.attrs)?;
    let cr: syn::Path = struct_attrs
        .crate_path
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(::configulator));

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    name,
                    "Config can only be derived for structs with named fields",
                ))
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "Config can only be derived for structs",
            ))
        }
    };

    let model = build_model(fields)?;
    let shadow_ident = format_ident!("__{}Shadow", name);

    let shadow_struct = emit_shadow_struct(&cr, vis, &shadow_ident, &model, &struct_attrs);
    let de_impl = emit_de_fns(&cr, &shadow_ident, &model);
    let has_shadow = emit_has_shadow(
        &cr,
        name,
        &shadow_ident,
        &model,
        struct_attrs.allow_unknown_fields,
    );
    let print_impl = emit_print(&cr, name, &model);

    Ok(quote! {
        #shadow_struct
        #de_impl
        #has_shadow
        #print_impl
    })
}

fn shadow_field_type(cr: &syn::Path, shape: &Shape) -> TokenStream2 {
    match shape {
        Shape::Bool { .. } => quote!(::std::option::Option<bool>),
        Shape::Leaf { ty, .. } => quote!(::std::option::Option<#ty>),
        Shape::VecLeaf { elem } => {
            quote!(::std::option::Option<::std::vec::Vec<#cr::__private::Leaf<#elem>>>)
        }
        Shape::MapLeaf { kind, key, val } => {
            let map = map_type(kind);
            quote!(::std::option::Option<#map<#key, #cr::__private::Leaf<#val>>>)
        }
        Shape::Nested { ty, .. } => {
            quote!(::std::option::Option<<#ty as #cr::HasShadow>::Shadow>)
        }
        Shape::VecNested { elem } => {
            quote!(::std::option::Option<::std::vec::Vec<<#elem as #cr::HasShadow>::Shadow>>)
        }
        Shape::MapNested { kind, key, val } => {
            let map = map_type(kind);
            quote!(::std::option::Option<#map<#key, <#val as #cr::HasShadow>::Shadow>>)
        }
    }
}

fn emit_shadow_struct(
    cr: &syn::Path,
    vis: &syn::Visibility,
    shadow_ident: &syn::Ident,
    model: &[FieldModel],
    struct_attrs: &StructAttrs,
) -> TokenStream2 {
    let serde_path_str = {
        let p = quote!(#cr).to_string().replace(' ', "");
        format!("{p}::__private::serde")
    };

    let fields: Vec<TokenStream2> = model
        .iter()
        .map(|m| {
            let ident = &m.ident;
            let ty = shadow_field_type(cr, &m.shape);
            let config_name = &m.config_name;
            if cfg!(feature = "file") {
                let de_with = match m.shape {
                    Shape::Bool { .. } | Shape::Leaf { .. } => {
                        let de_fn = format!("{shadow_ident}::__de_{}", ident.unraw());
                        quote!(#[serde(deserialize_with = #de_fn)])
                    }
                    _ => quote!(),
                };
                quote! {
                    #[serde(rename = #config_name, default)]
                    #de_with
                    pub #ident: #ty
                }
            } else {
                quote! { pub #ident: #ty }
            }
        })
        .collect();

    if cfg!(feature = "file") {
        let deny = if struct_attrs.allow_unknown_fields {
            quote!()
        } else {
            quote!(#[serde(deny_unknown_fields)])
        };
        quote! {
            #[doc(hidden)]
            #[derive(#cr::__private::serde::Deserialize, ::std::default::Default)]
            #[serde(crate = #serde_path_str)]
            #deny
            #[allow(non_camel_case_types)]
            #vis struct #shadow_ident {
                #(#fields),*
            }
        }
    } else {
        quote! {
            #[doc(hidden)]
            #[derive(::std::default::Default)]
            #[allow(non_camel_case_types)]
            #vis struct #shadow_ident {
                #(#fields),*
            }
        }
    }
}

fn emit_de_fns(cr: &syn::Path, shadow_ident: &syn::Ident, model: &[FieldModel]) -> TokenStream2 {
    if !cfg!(feature = "file") {
        return quote!();
    }
    let fns: Vec<TokenStream2> = model
        .iter()
        .filter_map(|m| {
            let inner: TokenStream2 = match &m.shape {
                Shape::Bool { .. } => quote!(bool),
                Shape::Leaf { ty, .. } => quote!(#ty),
                _ => return None,
            };
            let de_ident = format_ident!("__de_{}", m.ident);
            let config_name = &m.config_name;
            let secret = m.attrs.secret;
            Some(quote! {
                #[doc(hidden)]
                pub fn #de_ident<'de, D>(
                    d: D,
                ) -> ::std::result::Result<::std::option::Option<#inner>, D::Error>
                where
                    D: #cr::__private::serde::Deserializer<'de>,
                {
                    #cr::__private::leaf_named(d, #config_name, #secret)
                }
            })
        })
        .collect();
    if fns.is_empty() {
        quote!()
    } else {
        quote! {
            #[allow(non_snake_case)]
            impl #shadow_ident {
                #(#fns)*
            }
        }
    }
}

fn emit_has_shadow(
    cr: &syn::Path,
    name: &syn::Ident,
    shadow_ident: &syn::Ident,
    model: &[FieldModel],
    allow_unknown: bool,
) -> TokenStream2 {
    let defaults_body = emit_defaults(cr, model);
    let overlay_body = emit_overlay(cr, model);
    let build_body = emit_build(cr, name, model);
    let record_body = emit_record_set(cr, model);
    let vacant_body = emit_vacant(model);
    let fields_body = emit_fields(cr, model);

    let from_env = if cfg!(feature = "env") {
        let body = emit_from_env(cr, model);
        quote! {
            fn from_env(
                get: &dyn Fn(&str) -> ::std::option::Option<::std::string::String>,
                prefix: &str,
                sep: &str,
                array_sep: &str,
            ) -> ::std::result::Result<Self::Shadow, #cr::ConfigulatorError> {
                let mut s = <Self::Shadow as ::std::default::Default>::default();
                #body
                ::std::result::Result::Ok(s)
            }
        }
    } else {
        quote!()
    };

    let from_cli = if cfg!(feature = "cli") {
        let body = emit_from_cli(cr, model);
        quote! {
            fn from_cli(
                matches: &#cr::__private::clap::ArgMatches,
                prefix: &str,
                sep: &str,
            ) -> ::std::result::Result<Self::Shadow, #cr::ConfigulatorError> {
                let mut s = <Self::Shadow as ::std::default::Default>::default();
                #body
                ::std::result::Result::Ok(s)
            }
        }
    } else {
        quote!()
    };

    quote! {
        #[automatically_derived]
        impl #cr::HasShadow for #name {
            type Shadow = #shadow_ident;

            fn shadow_defaults(
                array_sep: &str,
            ) -> ::std::result::Result<Self::Shadow, #cr::ConfigulatorError> {
                #[allow(unused_mut, unused_variables)]
                let mut s = <Self::Shadow as ::std::default::Default>::default();
                #defaults_body
                ::std::result::Result::Ok(s)
            }

            #[allow(clippy::redundant_closure_call)]
            fn overlay(
                acc: &mut Self::Shadow,
                other: Self::Shadow,
                prefix: &str,
                layer: #cr::Layer,
                detail: &dyn Fn(&str) -> ::std::string::String,
                report: &mut #cr::Report,
            ) {
                #overlay_body
            }

            const ALLOW_UNKNOWN_FIELDS: bool = #allow_unknown;

            fn build(
                sh: Self::Shadow,
                prefix: &str,
                ctx: &#cr::__private::BuildCtx,
                report: &mut #cr::Report,
            ) -> ::std::result::Result<Self, #cr::ConfigulatorError> {
                #[allow(unused_variables)]
                let (prefix, array_sep) = (prefix, ctx.array_sep.as_str());
                ::std::result::Result::Ok(#build_body)
            }

            fn record_set(
                sh: &Self::Shadow,
                prefix: &str,
                layer: #cr::Layer,
                detail: &dyn Fn(&str) -> ::std::string::String,
                report: &mut #cr::Report,
            ) {
                #record_body
            }

            fn shadow_is_vacant(sh: &Self::Shadow) -> bool {
                #vacant_body
            }

            fn fields() -> ::std::vec::Vec<#cr::FieldInfo> {
                #fields_body
            }

            #from_env
            #from_cli
        }
    }
}

fn emit_defaults(cr: &syn::Path, model: &[FieldModel]) -> TokenStream2 {
    let mut parts = Vec::new();
    for m in model {
        let ident = &m.ident;
        let config_name = &m.config_name;
        let secret = m.attrs.secret;
        match &m.shape {
            Shape::Bool { .. } | Shape::Leaf { .. } => {
                if let Some(default) = &m.attrs.default {
                    let ty = match &m.shape {
                        Shape::Bool { .. } => quote!(bool),
                        Shape::Leaf { ty, .. } => quote!(#ty),
                        _ => unreachable!(),
                    };
                    parts.push(quote! {
                        s.#ident = ::std::option::Option::Some(
                            #cr::__private::parse_leaf::<#ty>(#default, #config_name, #secret)?,
                        );
                    });
                }
            }
            Shape::VecLeaf { elem } => {
                if let Some(default) = &m.attrs.default {
                    parts.push(quote! {
                        s.#ident = ::std::option::Option::Some(
                            #cr::__private::split_list(#default, array_sep)
                                .into_iter()
                                .map(|x| {
                                    #cr::__private::parse_leaf::<#elem>(x, #config_name, #secret)
                                        .map(#cr::__private::Leaf)
                                })
                                .collect::<::std::result::Result<_, _>>()?,
                        );
                    });
                }
            }
            Shape::Nested { ty, opt: false } => {
                parts.push(quote! {
                    s.#ident = ::std::option::Option::Some(
                        <#ty as #cr::HasShadow>::shadow_defaults(array_sep)?,
                    );
                });
            }
            _ => {}
        }
    }
    quote!(#(#parts)*)
}

fn emit_overlay(cr: &syn::Path, model: &[FieldModel]) -> TokenStream2 {
    let mut parts = Vec::new();
    for m in model {
        let ident = &m.ident;
        let config_name = &m.config_name;
        match &m.shape {
            Shape::Bool { .. }
            | Shape::Leaf { .. }
            | Shape::VecLeaf { .. }
            | Shape::MapLeaf { .. } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(v) = other.#ident {
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, layer, detail(&p));
                        acc.#ident = ::std::option::Option::Some(v);
                    }
                });
            }
            Shape::Nested { ty, .. } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(ov) = other.#ident {
                        let p = #cr::__private::join(prefix, #config_name);
                        match acc.#ident.as_mut() {
                            ::std::option::Option::Some(cur) => {
                                <#ty as #cr::HasShadow>::overlay(cur, ov, &p, layer, detail, report);
                            }
                            ::std::option::Option::None => {
                                let mut cur = <<#ty as #cr::HasShadow>::Shadow as ::std::default::Default>::default();
                                <#ty as #cr::HasShadow>::overlay(&mut cur, ov, &p, layer, detail, report);
                                acc.#ident = ::std::option::Option::Some(cur);
                            }
                        }
                    }
                });
            }
            Shape::VecNested { elem } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(v) = other.#ident {
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, layer, detail(&p));
                        for (i, e) in v.iter().enumerate() {
                            <#elem as #cr::HasShadow>::record_set(
                                e, &format!("{p}[{i}]"), layer, detail, report,
                            );
                        }
                        acc.#ident = ::std::option::Option::Some(v);
                    }
                });
            }
            Shape::MapNested { val, .. } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(mp) = other.#ident {
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, layer, detail(&p));
                        for (k, e) in mp.iter() {
                            let kp = format!("{p}.{}", #cr::__private::quote_key(&k.to_string()));
                            <#val as #cr::HasShadow>::record_set(e, &kp, layer, detail, report);
                        }
                        acc.#ident = ::std::option::Option::Some(mp);
                    }
                });
            }
        }
    }
    quote!(#(#parts)*)
}

fn emit_build(cr: &syn::Path, name: &syn::Ident, model: &[FieldModel]) -> TokenStream2 {
    let mut inits = Vec::new();
    for m in model {
        let ident = &m.ident;
        let config_name = &m.config_name;
        let secret = m.attrs.secret;
        let missing = |fallback: TokenStream2| {
            quote! {{
                if ctx.check_required {
                    return ::std::result::Result::Err(#cr::ConfigulatorError::Required {
                        path: #cr::__private::join(prefix, #config_name),
                    });
                }
                #fallback
            }}
        };
        let required = m.attrs.required && m.attrs.default.is_none();
        let init = match &m.shape {
            Shape::Bool { opt } | Shape::Leaf { opt, .. } => {
                let ty = match &m.shape {
                    Shape::Bool { .. } => quote!(bool),
                    Shape::Leaf { ty, .. } => quote!(#ty),
                    _ => unreachable!(),
                };
                let none_arm = match (&m.attrs.default, opt) {
                    (Some(default), false) => quote! {{
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, #cr::Layer::Default, "element default".to_string());
                        #cr::__private::parse_leaf::<#ty>(#default, #config_name, #secret)?
                    }},
                    (Some(default), true) => quote! {{
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, #cr::Layer::Default, "element default".to_string());
                        ::std::option::Option::Some(
                            #cr::__private::parse_leaf::<#ty>(#default, #config_name, #secret)?,
                        )
                    }},
                    (None, false) if required => {
                        missing(quote!(<#ty as ::std::default::Default>::default()))
                    }
                    (None, true) if required => missing(quote!(::std::option::Option::None)),
                    (None, false) => quote!(<#ty as ::std::default::Default>::default()),
                    (None, true) => quote!(::std::option::Option::None),
                };
                let some_arm = if *opt {
                    quote!(::std::option::Option::Some(v))
                } else {
                    quote!(v)
                };
                quote! {
                    #ident: match sh.#ident {
                        ::std::option::Option::Some(v) => #some_arm,
                        ::std::option::Option::None => #none_arm,
                    }
                }
            }
            Shape::VecLeaf { elem } => {
                let none_arm = match &m.attrs.default {
                    Some(default) => quote! {{
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, #cr::Layer::Default, "element default".to_string());
                        #cr::__private::split_list(#default, array_sep)
                            .into_iter()
                            .map(|x| #cr::__private::parse_leaf::<#elem>(x, #config_name, #secret))
                            .collect::<::std::result::Result<_, _>>()?
                    }},
                    None if required => missing(quote!(::std::vec::Vec::new())),
                    None => quote!(::std::vec::Vec::new()),
                };
                quote! {
                    #ident: match sh.#ident {
                        ::std::option::Option::Some(v) => {
                            v.into_iter().map(|#cr::__private::Leaf(x)| x).collect()
                        }
                        ::std::option::Option::None => #none_arm,
                    }
                }
            }
            Shape::MapLeaf { kind, .. } => {
                let map = map_type(kind);
                let none_arm = if required {
                    missing(quote!(#map::new()))
                } else {
                    quote!(#map::new())
                };
                quote! {
                    #ident: match sh.#ident {
                        ::std::option::Option::Some(mp) => mp
                            .into_iter()
                            .map(|(k, #cr::__private::Leaf(v))| (k, v))
                            .collect(),
                        ::std::option::Option::None => #none_arm,
                    }
                }
            }
            Shape::Nested { ty, opt: false } => quote! {
                #ident: <#ty as #cr::HasShadow>::build(
                    match sh.#ident {
                        ::std::option::Option::Some(x) => x,
                        ::std::option::Option::None => {
                            <<#ty as #cr::HasShadow>::Shadow as ::std::default::Default>::default()
                        }
                    },
                    &#cr::__private::join(prefix, #config_name),
                    ctx,
                    report,
                )?
            },
            Shape::Nested { ty, opt: true } => quote! {
                #ident: match sh.#ident {
                    ::std::option::Option::Some(x) => ::std::option::Option::Some(
                        <#ty as #cr::HasShadow>::build(
                            x,
                            &#cr::__private::join(prefix, #config_name),
                            ctx,
                            report,
                        )?,
                    ),
                    ::std::option::Option::None => ::std::option::Option::None,
                }
            },
            Shape::VecNested { elem } => quote! {
                #ident: match sh.#ident {
                    ::std::option::Option::Some(v) => {
                        let p = #cr::__private::join(prefix, #config_name);
                        let mut out = ::std::vec::Vec::with_capacity(v.len());
                        for (i, e) in v.into_iter().enumerate() {
                            out.push(<#elem as #cr::HasShadow>::build(
                                e,
                                &format!("{p}[{i}]"),
                                ctx,
                                report,
                            )?);
                        }
                        out
                    }
                    ::std::option::Option::None => ::std::vec::Vec::new(),
                }
            },
            Shape::MapNested { kind, val, .. } => {
                let map = map_type(kind);
                quote! {
                    #ident: match sh.#ident {
                        ::std::option::Option::Some(mp) => {
                            let p = #cr::__private::join(prefix, #config_name);
                            let mut out = #map::new();
                            for (k, e) in mp.into_iter() {
                                let kp = format!(
                                    "{p}.{}",
                                    #cr::__private::quote_key(&k.to_string()),
                                );
                                out.insert(
                                    k,
                                    <#val as #cr::HasShadow>::build(e, &kp, ctx, report)?,
                                );
                            }
                            out
                        }
                        ::std::option::Option::None => #map::new(),
                    }
                }
            }
        };
        inits.push(init);
    }
    quote! {
        #name {
            #(#inits),*
        }
    }
}

fn emit_record_set(cr: &syn::Path, model: &[FieldModel]) -> TokenStream2 {
    let mut parts = Vec::new();
    for m in model {
        let ident = &m.ident;
        let config_name = &m.config_name;
        match &m.shape {
            Shape::Bool { .. }
            | Shape::Leaf { .. }
            | Shape::VecLeaf { .. }
            | Shape::MapLeaf { .. } => {
                parts.push(quote! {
                    if sh.#ident.is_some() {
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, layer, detail(&p));
                    }
                });
            }
            Shape::Nested { ty, .. } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(x) = &sh.#ident {
                        <#ty as #cr::HasShadow>::record_set(
                            x,
                            &#cr::__private::join(prefix, #config_name),
                            layer,
                            detail,
                            report,
                        );
                    }
                });
            }
            Shape::VecNested { elem } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(v) = &sh.#ident {
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, layer, detail(&p));
                        for (i, e) in v.iter().enumerate() {
                            <#elem as #cr::HasShadow>::record_set(
                                e, &format!("{p}[{i}]"), layer, detail, report,
                            );
                        }
                    }
                });
            }
            Shape::MapNested { val, .. } => {
                parts.push(quote! {
                    if let ::std::option::Option::Some(mp) = &sh.#ident {
                        let p = #cr::__private::join(prefix, #config_name);
                        report.__set(&p, layer, detail(&p));
                        for (k, e) in mp.iter() {
                            let kp = format!("{p}.{}", #cr::__private::quote_key(&k.to_string()));
                            <#val as #cr::HasShadow>::record_set(e, &kp, layer, detail, report);
                        }
                    }
                });
            }
        }
    }
    quote!(#(#parts)*)
}

fn emit_vacant(model: &[FieldModel]) -> TokenStream2 {
    let checks: Vec<TokenStream2> = model
        .iter()
        .map(|m| {
            let ident = &m.ident;
            quote!(sh.#ident.is_none())
        })
        .collect();
    if checks.is_empty() {
        quote!(true)
    } else {
        quote!(#(#checks)&&*)
    }
}

fn emit_fields(cr: &syn::Path, model: &[FieldModel]) -> TokenStream2 {
    let items: Vec<TokenStream2> = model
        .iter()
        .map(|m| {
            let field_name = m.ident.unraw().to_string();
            let config_name = &m.config_name;
            let env_segment = &m.env_segment;
            let flag_segment = &m.flag_segment;
            let skip_env = m.skip_env;
            let skip_cli = m.skip_cli;
            let short = match m.attrs.short {
                Some(c) => quote!(::std::option::Option::Some(#c)),
                None => quote!(::std::option::Option::None),
            };
            let secret = m.attrs.secret;
            let required = m.attrs.required;
            let default_value = match &m.attrs.default {
                Some(d) => quote!(::std::option::Option::Some(#d)),
                None => quote!(::std::option::Option::None),
            };
            let description = match &m.attrs.description {
                Some(d) => quote!(::std::option::Option::Some(#d)),
                None => quote!(::std::option::Option::None),
            };
            let hint = match &m.shape {
                Shape::Bool { .. } => "Bool",
                Shape::Leaf { ty, .. } => scalar_hint(ty),
                Shape::VecLeaf { elem } => scalar_hint(elem),
                Shape::MapLeaf { val, .. } => scalar_hint(val),
                _ => "String",
            };
            let hint = format_ident!("{hint}");
            let optional = matches!(
                m.shape,
                Shape::Bool { opt: true }
                    | Shape::Leaf { opt: true, .. }
                    | Shape::Nested { opt: true, .. }
            );
            let allow_unknown = match &m.shape {
                Shape::Nested { ty, .. } => quote!(<#ty as #cr::HasShadow>::ALLOW_UNKNOWN_FIELDS),
                Shape::VecNested { elem } => {
                    quote!(<#elem as #cr::HasShadow>::ALLOW_UNKNOWN_FIELDS)
                }
                Shape::MapNested { val, .. } => {
                    quote!(<#val as #cr::HasShadow>::ALLOW_UNKNOWN_FIELDS)
                }
                _ => quote!(false),
            };
            let field_type = match &m.shape {
                Shape::Bool { .. } => quote!(#cr::FieldType::Bool),
                Shape::Leaf { .. } => quote!(#cr::FieldType::Scalar),
                Shape::VecLeaf { .. } => quote!(#cr::FieldType::List),
                Shape::MapLeaf { .. } => quote!(#cr::FieldType::Map),
                Shape::Nested { ty, .. } => {
                    quote!(#cr::FieldType::Struct(<#ty as #cr::HasShadow>::fields()))
                }
                Shape::VecNested { elem } => {
                    quote!(#cr::FieldType::StructList(<#elem as #cr::HasShadow>::fields()))
                }
                Shape::MapNested { val, .. } => {
                    quote!(#cr::FieldType::StructMap(<#val as #cr::HasShadow>::fields()))
                }
            };
            quote! {
                #cr::FieldInfo {
                    field_name: #field_name,
                    config_name: #config_name,
                    env_segment: #env_segment,
                    flag_segment: #flag_segment,
                    skip_env: #skip_env,
                    skip_cli: #skip_cli,
                    short: #short,
                    secret: #secret,
                    required: #required,
                    default_value: #default_value,
                    description: #description,
                    scalar: #cr::ScalarHint::#hint,
                    optional: #optional,
                    allow_unknown_fields: #allow_unknown,
                    field_type: #field_type,
                }
            }
        })
        .collect();
    quote! {
        ::std::vec![#(#items),*]
    }
}

fn emit_from_env(cr: &syn::Path, model: &[FieldModel]) -> TokenStream2 {
    let mut parts = Vec::new();
    for m in model {
        if m.skip_env {
            continue;
        }
        let ident = &m.ident;
        let env_segment = &m.env_segment;
        let secret = m.attrs.secret;
        match &m.shape {
            Shape::Bool { .. } | Shape::Leaf { .. } => {
                let ty = match &m.shape {
                    Shape::Bool { .. } => quote!(bool),
                    Shape::Leaf { ty, .. } => quote!(#ty),
                    _ => unreachable!(),
                };
                parts.push(quote! {
                    {
                        let var = format!("{prefix}{}", #env_segment);
                        if let ::std::option::Option::Some(v) = get(&var) {
                            s.#ident = ::std::option::Option::Some(
                                #cr::__private::parse_leaf::<#ty>(&v, &var, #secret)?,
                            );
                        }
                    }
                });
            }
            Shape::VecLeaf { elem } => {
                parts.push(quote! {
                    {
                        let var = format!("{prefix}{}", #env_segment);
                        if let ::std::option::Option::Some(v) = get(&var) {
                            s.#ident = ::std::option::Option::Some(
                                #cr::__private::split_list(&v, array_sep)
                                    .into_iter()
                                    .map(|x| {
                                        #cr::__private::parse_leaf::<#elem>(x, &var, #secret)
                                            .map(#cr::__private::Leaf)
                                    })
                                    .collect::<::std::result::Result<_, _>>()?,
                            );
                        }
                    }
                });
            }
            Shape::MapLeaf { .. } | Shape::VecNested { .. } | Shape::MapNested { .. } => {}
            Shape::Nested { ty, .. } => {
                parts.push(quote! {
                    {
                        let sub = <#ty as #cr::HasShadow>::from_env(
                            get,
                            &format!("{prefix}{}{sep}", #env_segment),
                            sep,
                            array_sep,
                        )?;
                        if !<#ty as #cr::HasShadow>::shadow_is_vacant(&sub) {
                            s.#ident = ::std::option::Option::Some(sub);
                        }
                    }
                });
            }
        }
    }
    quote!(#(#parts)*)
}

fn emit_from_cli(cr: &syn::Path, model: &[FieldModel]) -> TokenStream2 {
    let mut parts = Vec::new();
    for m in model {
        if m.skip_cli {
            continue;
        }
        let ident = &m.ident;
        let flag_segment = &m.flag_segment;
        let secret = m.attrs.secret;
        let flag_expr = quote! {
            if prefix.is_empty() {
                #flag_segment.to_string()
            } else {
                format!("{prefix}{sep}{}", #flag_segment)
            }
        };
        match &m.shape {
            Shape::Bool { .. } | Shape::Leaf { .. } => {
                let ty = match &m.shape {
                    Shape::Bool { .. } => quote!(bool),
                    Shape::Leaf { ty, .. } => quote!(#ty),
                    _ => unreachable!(),
                };
                parts.push(quote! {
                    {
                        let flag = #flag_expr;
                        if matches.value_source(&flag)
                            == ::std::option::Option::Some(
                                #cr::__private::clap::parser::ValueSource::CommandLine,
                            )
                        {
                            if let ::std::option::Option::Some(v) =
                                matches.get_one::<::std::string::String>(&flag)
                            {
                                s.#ident = ::std::option::Option::Some(
                                    #cr::__private::parse_leaf::<#ty>(
                                        v,
                                        &format!("--{flag}"),
                                        #secret,
                                    )?,
                                );
                            }
                        }
                    }
                });
            }
            Shape::VecLeaf { elem } => {
                parts.push(quote! {
                    {
                        let flag = #flag_expr;
                        if matches.value_source(&flag)
                            == ::std::option::Option::Some(
                                #cr::__private::clap::parser::ValueSource::CommandLine,
                            )
                        {
                            if let ::std::option::Option::Some(vals) =
                                matches.get_many::<::std::string::String>(&flag)
                            {
                                s.#ident = ::std::option::Option::Some(
                                    vals.map(|v| {
                                        #cr::__private::parse_leaf::<#elem>(
                                            v,
                                            &format!("--{flag}"),
                                            #secret,
                                        )
                                        .map(#cr::__private::Leaf)
                                    })
                                    .collect::<::std::result::Result<_, _>>()?,
                                );
                            }
                        }
                    }
                });
            }
            Shape::MapLeaf { .. } | Shape::VecNested { .. } | Shape::MapNested { .. } => {}
            Shape::Nested { ty, .. } => {
                parts.push(quote! {
                    {
                        let flag = #flag_expr;
                        let sub = <#ty as #cr::HasShadow>::from_cli(matches, &flag, sep)?;
                        if !<#ty as #cr::HasShadow>::shadow_is_vacant(&sub) {
                            s.#ident = ::std::option::Option::Some(sub);
                        }
                    }
                });
            }
        }
    }
    quote!(#(#parts)*)
}

fn go_leaf(cr: &syn::Path, ty: &Type, expr: TokenStream2) -> TokenStream2 {
    let name = last_segment(ty).map(|seg| seg.ident.to_string());
    match name.as_deref() {
        Some("f64") => quote!(#cr::__private::go_float(*#expr as f64, false)),
        Some("f32") => quote!(#cr::__private::go_float(*#expr as f64, true)),
        Some("PathBuf") => quote!(#expr.display().to_string()),
        _ => quote!((&&&#cr::__private::PrintVal(#expr)).print_val()),
    }
}

fn go_value(cr: &syn::Path, m: &FieldModel, expr: TokenStream2) -> TokenStream2 {
    if m.attrs.secret {
        return quote!(::std::string::String::from("(redacted)"));
    }
    match &m.shape {
        Shape::Bool { opt: false } => go_leaf(cr, &syn::parse_quote!(bool), expr),
        Shape::Leaf { ty, opt: false } => go_leaf(cr, ty, expr),
        Shape::Bool { opt: true } | Shape::Leaf { opt: true, .. } => {
            let ty: Type = match &m.shape {
                Shape::Leaf { ty, .. } => ty.clone(),
                _ => syn::parse_quote!(bool),
            };
            let inner = go_leaf(cr, &ty, quote!(x));
            quote! {
                match #expr {
                    ::std::option::Option::Some(x) => #inner,
                    ::std::option::Option::None => ::std::string::String::from("<nil>"),
                }
            }
        }
        Shape::VecLeaf { elem } => {
            let item = go_leaf(cr, elem, quote!(x));
            quote!(#cr::__private::go_list(#expr.iter().map(|x| #item)))
        }
        Shape::MapLeaf { val, .. } => {
            let item = go_leaf(cr, val, quote!(v));
            quote! {
                #cr::__private::go_map(#expr.iter().map(|(k, v)| {
                    ((&&&#cr::__private::PrintVal(k)).print_val(), #item)
                }))
            }
        }
        Shape::Nested { opt: false, .. } => quote!(#expr.__go_value()),
        Shape::Nested { opt: true, .. } => quote! {
            match #expr {
                ::std::option::Option::Some(x) => x.__go_value(),
                ::std::option::Option::None => ::std::string::String::from("<nil>"),
            }
        },
        Shape::VecNested { .. } => {
            quote!(#cr::__private::go_list(#expr.iter().map(|x| x.__go_value())))
        }
        Shape::MapNested { .. } => quote! {
            #cr::__private::go_map(#expr.iter().map(|(k, v)| {
                ((&&&#cr::__private::PrintVal(k)).print_val(), v.__go_value())
            }))
        },
    }
}

fn emit_print(cr: &syn::Path, name: &syn::Ident, model: &[FieldModel]) -> TokenStream2 {
    let mut lines = Vec::new();
    let mut values = Vec::new();
    for m in model {
        let ident = &m.ident;
        let config_name = &m.config_name;
        let value = go_value(cr, m, quote!((&self.#ident)));
        values.push(value.clone());
        let line = |v: TokenStream2| {
            quote! {
                out.push_str(&::std::format!(
                    "{} = {}\n",
                    #cr::__private::join(prefix, #config_name),
                    #v,
                ));
            }
        };
        let part = match &m.shape {
            Shape::Nested { opt: false, .. } if !m.attrs.secret => quote! {
                self.#ident.__print_into(&#cr::__private::join(prefix, #config_name), out);
            },
            Shape::Nested { opt: true, .. } if !m.attrs.secret => {
                let unset = line(quote!("<unset>"));
                quote! {
                    match &self.#ident {
                        ::std::option::Option::Some(x) => {
                            x.__print_into(&#cr::__private::join(prefix, #config_name), out)
                        }
                        ::std::option::Option::None => { #unset }
                    }
                }
            }
            Shape::Bool { opt: true } | Shape::Leaf { opt: true, .. } if !m.attrs.secret => {
                let ty: Type = match &m.shape {
                    Shape::Leaf { ty, .. } => ty.clone(),
                    _ => syn::parse_quote!(bool),
                };
                let inner = go_leaf(cr, &ty, quote!(x));
                line(quote! {
                    match &self.#ident {
                        ::std::option::Option::Some(x) => #inner,
                        ::std::option::Option::None => ::std::string::String::from("<unset>"),
                    }
                })
            }
            Shape::VecNested { elem: ty } | Shape::MapNested { val: ty, .. } if !m.attrs.secret => {
                let redacted = line(quote!("(redacted)"));
                let shown = line(value);
                quote! {
                    if #cr::__private::has_secret(&<#ty as #cr::HasShadow>::fields()) {
                        #redacted
                    } else {
                        #shown
                    }
                }
            }
            _ => line(value),
        };
        lines.push(part);
    }
    quote! {
        #[automatically_derived]
        impl #name {
            /// Render every field as `path = value` lines, the way Go's
            /// `PrintConfig` does, redacting fields marked `secret`.
            pub fn print_config(&self) -> ::std::string::String {
                let mut out = ::std::string::String::new();
                self.__print_into("", &mut out);
                out
            }

            #[doc(hidden)]
            pub fn __print_into(&self, prefix: &str, out: &mut ::std::string::String) {
                #[allow(unused_imports)]
                use #cr::__private::{PrintDebug as _, PrintDisplay as _, PrintFallback as _};
                #(#lines)*
            }

            #[doc(hidden)]
            pub fn __go_value(&self) -> ::std::string::String {
                #[allow(unused_imports)]
                use #cr::__private::{PrintDebug as _, PrintDisplay as _, PrintFallback as _};
                let parts: ::std::vec::Vec<::std::string::String> = ::std::vec![#(#values),*];
                ::std::format!("{{{}}}", parts.join(" "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    fn derive(src: &str) -> Result<TokenStream2, syn::Error> {
        derive_config_impl(&parse_str::<DeriveInput>(src).unwrap())
    }

    #[test]
    fn accepts_named_struct() {
        assert!(derive(r#"struct Foo { #[configulator(name = "x")] x: u32 }"#).is_ok());
    }

    #[test]
    fn rejects_enum_and_tuple_struct() {
        let err = derive("enum Foo { A, B }").unwrap_err();
        assert!(err.to_string().contains("only be derived for structs"));
        let err = derive("struct Foo(u32);").unwrap_err();
        assert!(err.to_string().contains("named fields"));
    }

    #[test]
    fn rejects_unknown_attribute() {
        let err =
            derive(r#"struct Foo { #[configulator(name = "x", extra)] x: u32 }"#).unwrap_err();
        assert!(err.to_string().contains("unknown configulator attribute"));
    }

    #[test]
    fn rejects_collision_under_folding() {
        let err = derive(
            r#"struct Foo {
                #[configulator(name = "a-b")] x: u32,
                #[configulator(name = "A_B")] y: u32,
            }"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("collide"));
    }

    #[test]
    fn rejects_env_opt_in_on_collection() {
        let err = derive(
            r#"struct Foo { #[configulator(name = "t", env = "T")] t: std::collections::HashMap<String, String> }"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("file-only"));
        assert!(derive(
            r#"struct Foo { #[configulator(name = "t", env = "T", flag = "tt")] t: Vec<String> }"#
        )
        .is_ok());
    }

    #[test]
    fn rejects_lowercase_env_override() {
        let err =
            derive(r#"struct Foo { #[configulator(name = "x", env = "lower-case")] x: u32 }"#)
                .unwrap_err();
        assert!(err.to_string().contains("uppercase"));
    }

    #[test]
    fn config_name_defaults_to_field_name() {
        assert!(derive("struct Foo { some_field: u32 }").is_ok());
    }

    #[test]
    fn classifies_shapes() {
        assert!(derive(
            r#"struct Foo {
                a: bool,
                b: Option<u16>,
                c: Vec<String>,
                d: std::collections::HashMap<String, String>,
                #[configulator(nested)] e: Bar,
                #[configulator(nested)] f: Option<Bar>,
                #[configulator(nested)] g: Vec<Bar>,
                #[configulator(nested)] h: std::collections::BTreeMap<String, Bar>,
            }"#,
        )
        .is_ok());
    }
}
