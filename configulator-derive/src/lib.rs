//! # Configulator Derive Macro
//!
//! Derive macro for
//! [`configulator-rs`](https://crates.io/crates/configulator-rs).
//! This crate is not intended to be used directly, add
//! `configulator-rs` as a dependency instead.

#![warn(clippy::all)]
#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Data, Fields, Type, PathArguments, GenericArgument,
    punctuated::Punctuated, Token,
};

/// Derive macro that generates `ConfigFields` and `FromValueMap` implementations for a struct.
///
/// Supports `#[configulator(name = "...", default = "...", description = "...")]` attributes.
/// Falls back to field name if no `name` attribute is specified.
///
/// Scalar field types must implement [`FromStr`](std::str::FromStr) + `Default`. Nested struct
/// types must also derive `Config`, detection is automatic at compile time.
#[proc_macro_derive(Config, attributes(configulator))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_config_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_config_impl(input: &DeriveInput) -> Result<proc_macro2::TokenStream, syn::Error> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = extract_named_fields(input)?;

    let mut field_info_tokens = Vec::new();
    let mut from_map_tokens = Vec::new();

    for field in fields.iter() {
        let field_ident = field.ident.as_ref().unwrap();
        let field_name_str = field_ident.to_string();
        let field_ty = &field.ty;

        let attrs = parse_configulator_attrs(&field.attrs)?;

        let config_name_str = attrs.config_name.unwrap_or_else(|| field_name_str.clone());
        let field_type_token = field_type_to_tokens(field_ty);

        let default_tokens = match &attrs.default_val {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };
        let desc_tokens = match &attrs.description {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };

        field_info_tokens.push(quote! {
            configulator::FieldInfo {
                field_name: #field_name_str,
                config_name: #config_name_str,
                default_value: #default_tokens,
                description: #desc_tokens,
                field_type: #field_type_token,
            }
        });

        let from_map_field = gen_from_value_map_field(field_ident, &config_name_str, field_ty);
        from_map_tokens.push(from_map_field);
    }

    let expanded = quote! {
        impl #impl_generics configulator::ConfigFields for #name #ty_generics #where_clause {
            fn configulator_fields() -> Vec<configulator::FieldInfo> {
                // Import trait so fallback scalar dispatch can resolve
                use configulator::ConfiguratorScalar as _;
                vec![
                    #(#field_info_tokens),*
                ]
            }
        }

        impl #impl_generics configulator::FromValueMap for #name #ty_generics #where_clause {
            fn from_value_map(
                map: &configulator::ValueMap,
            ) -> Result<Self, configulator::ConfigulatorError> {
                // Import trait so fallback scalar dispatch can resolve
                use configulator::ConfiguratorScalar as _;
                Ok(Self {
                    #(#from_map_tokens),*
                })
            }
        }
    };

    Ok(expanded)
}

/// Extract named fields from a `DeriveInput`, returning an error for non-structs
/// or structs without named fields.
fn extract_named_fields(
    input: &DeriveInput,
) -> Result<&Punctuated<syn::Field, Token![,]>, syn::Error> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(&fields.named),
            _ => Err(syn::Error::new_spanned(
                &input.ident,
                "Config can only be derived for structs with named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            &input.ident,
            "Config can only be derived for structs",
        )),
    }
}

#[derive(Debug)]
struct FieldConfigAttrs {
    config_name: Option<String>,
    default_val: Option<String>,
    description: Option<String>,
}

/// Parse `#[configulator(...)]` attributes from a field's attribute list.
/// Non-configulator attributes are skipped. Returns an error if attribute
/// syntax is malformed.
fn parse_configulator_attrs(attrs: &[syn::Attribute]) -> Result<FieldConfigAttrs, syn::Error> {
    let mut result = FieldConfigAttrs {
        config_name: None,
        default_val: None,
        description: None,
    };
    for attr in attrs {
        if !attr.path().is_ident("configulator") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                result.config_name = Some(lit.value());
            } else if meta.path.is_ident("default") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                result.default_val = Some(lit.value());
            } else if meta.path.is_ident("description") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                result.description = Some(lit.value());
            } else {
                let name = meta.path.get_ident()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "?".to_string());
                return Err(meta.error(format_args!(
                    "unknown configulator attribute `{name}`; \
                     expected `name`, `default`, or `description`",
                )));
            }
            Ok(())
        })?;
    }
    Ok(result)
}

/// Map a Rust type to the simplified `FieldType` enum (Bool, Scalar, List, Struct).
/// For non-bool, non-Vec types, uses compile-time autoref dispatch to detect
/// whether the type is a nested struct or a scalar.
fn field_type_to_tokens(ty: &Type) -> proc_macro2::TokenStream {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "bool" {
                return quote! { configulator::FieldType::Bool };
            }
            if segment.ident == "Vec" {
                if let PathArguments::AngleBracketed(_) = &segment.arguments {
                    return quote! { configulator::FieldType::List };
                }
                return quote! {
                    compile_error!("Vec fields must have a type argument, e.g. Vec<String>")
                };
            }
        }
    }
    gen_config_detect_tokens(ty)
}

/// Generate the `ConfigDetect` autoref dispatch expression for a type.
/// At compile time this resolves to either `FieldType::Struct` (for nested
/// config structs) or `FieldType::Scalar` (for `FromStr` types).
fn gen_config_detect_tokens(ty: &Type) -> proc_macro2::TokenStream {
    quote! {
        {
            let __m = configulator::ConfigDetect::<#ty>(::std::marker::PhantomData);
            __m.__configulator_field_type()
        }
    }
}

/// Generate the field assignment for `FromValueMap::from_value_map`.
fn gen_from_value_map_field(
    field_ident: &syn::Ident,
    config_name: &str,
    ty: &Type,
) -> proc_macro2::TokenStream {
    let kind = classify_type(ty);
    match kind {
        TypeKind::Bool => {
            quote! {
                #field_ident: configulator::parse_scalar::<bool>(map, #config_name)?
            }
        }
        TypeKind::Vec(inner_ty) => {
            quote! {
                #field_ident: configulator::parse_list::<#inner_ty>(map, #config_name)?
            }
        }
        TypeKind::Other => {
            quote! {
                #field_ident: {
                    let __m = configulator::ConfigDetect::<#ty>(::std::marker::PhantomData);
                    __m.__configulator_parse(map, #config_name)?
                }
            }
        }
    }
}

#[derive(Debug)]
enum TypeKind {
    Bool,
    Vec(Box<Type>),
    Other,
}

fn classify_type(ty: &Type) -> TypeKind {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "bool" {
                return TypeKind::Bool;
            }
            if segment.ident == "Vec" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        return TypeKind::Vec(Box::new(inner.clone()));
                    }
                }
                // Vec without type argument — emit a clear error
                return TypeKind::Other;
            }
        }
    }
    TypeKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    // ── derive_config_impl tests ──

    #[test]
    fn derive_config_impl_valid_struct() {
        let input: DeriveInput = parse_str("struct Foo { x: u32 }").unwrap();
        assert!(derive_config_impl(&input).is_ok());
    }

    #[test]
    fn derive_config_impl_rejects_enum() {
        let input: DeriveInput = parse_str("enum Foo { A, B }").unwrap();
        let err = derive_config_impl(&input).unwrap_err();
        assert!(err.to_string().contains("only be derived for structs"));
    }

    #[test]
    fn derive_config_impl_rejects_tuple_struct() {
        let input: DeriveInput = parse_str("struct Foo(u32);").unwrap();
        let err = derive_config_impl(&input).unwrap_err();
        assert!(err.to_string().contains("named fields"));
    }

    #[test]
    fn derive_config_impl_rejects_bad_attr() {
        let input: DeriveInput = parse_str(
            r#"struct Foo { #[configulator(name = 42)] f: String }"#,
        )
        .unwrap();
        assert!(derive_config_impl(&input).is_err());
    }

    // ── extract_named_fields tests ──

    #[test]
    fn extract_named_fields_accepts_named_struct() {
        let input: DeriveInput = parse_str("struct Foo { x: u32 }").unwrap();
        assert!(extract_named_fields(&input).is_ok());
    }

    #[test]
    fn extract_named_fields_rejects_tuple_struct() {
        let input: DeriveInput = parse_str("struct Foo(u32);").unwrap();
        let err = extract_named_fields(&input).unwrap_err();
        assert!(
            err.to_string().contains("named fields"),
            "expected 'named fields' error, got: {err}"
        );
    }

    #[test]
    fn extract_named_fields_rejects_unit_struct() {
        let input: DeriveInput = parse_str("struct Foo;").unwrap();
        let err = extract_named_fields(&input).unwrap_err();
        assert!(
            err.to_string().contains("named fields"),
            "expected 'named fields' error, got: {err}"
        );
    }

    #[test]
    fn extract_named_fields_rejects_enum() {
        let input: DeriveInput = parse_str("enum Foo { A, B }").unwrap();
        let err = extract_named_fields(&input).unwrap_err();
        assert!(
            err.to_string().contains("only be derived for structs"),
            "expected 'only be derived for structs' error, got: {err}"
        );
    }

    // ── parse_configulator_attrs tests ──

    #[test]
    fn parse_attrs_extracts_all_keys() {
        let input: DeriveInput = parse_str(
            r#"struct Foo { #[configulator(name = "n", default = "d", description = "desc")] f: u32 }"#,
        )
        .unwrap();
        let fields = extract_named_fields(&input).unwrap();
        let attrs = parse_configulator_attrs(&fields.first().unwrap().attrs).unwrap();
        assert_eq!(attrs.config_name.as_deref(), Some("n"));
        assert_eq!(attrs.default_val.as_deref(), Some("d"));
        assert_eq!(attrs.description.as_deref(), Some("desc"));
    }

    #[test]
    fn parse_attrs_skips_non_configulator() {
        let input: DeriveInput = parse_str(
            r#"struct Foo { #[allow(unused)] #[configulator(name = "bar")] f: String }"#,
        )
        .unwrap();
        let fields = extract_named_fields(&input).unwrap();
        let attrs = parse_configulator_attrs(&fields.first().unwrap().attrs).unwrap();
        assert_eq!(attrs.config_name.as_deref(), Some("bar"));
    }

    #[test]
    fn parse_attrs_rejects_unknown_key() {
        let input: DeriveInput = parse_str(
            r#"struct Foo { #[configulator(name = "bar", extra)] f: String }"#,
        )
        .unwrap();
        let fields = extract_named_fields(&input).unwrap();
        let err = parse_configulator_attrs(&fields.first().unwrap().attrs).unwrap_err();
        assert!(
            err.to_string().contains("unknown configulator attribute"),
            "expected 'unknown configulator attribute' error, got: {err}"
        );
    }

    #[test]
    fn parse_attrs_error_on_bad_value_type() {
        let input: DeriveInput = parse_str(
            r#"struct Foo { #[configulator(name = 42)] f: String }"#,
        )
        .unwrap();
        let fields = extract_named_fields(&input).unwrap();
        assert!(parse_configulator_attrs(&fields.first().unwrap().attrs).is_err());
    }

    #[test]
    fn parse_attrs_no_attrs_returns_none() {
        let input: DeriveInput = parse_str("struct Foo { f: String }").unwrap();
        let fields = extract_named_fields(&input).unwrap();
        let attrs = parse_configulator_attrs(&fields.first().unwrap().attrs).unwrap();
        assert!(attrs.config_name.is_none());
        assert!(attrs.default_val.is_none());
        assert!(attrs.description.is_none());
    }

    // ── classify_type tests ──

    #[test]
    fn classify_type_bool() {
        let ty: Type = parse_str("bool").unwrap();
        assert!(matches!(classify_type(&ty), TypeKind::Bool));
    }

    #[test]
    fn classify_type_vec_with_inner() {
        let ty: Type = parse_str("Vec<String>").unwrap();
        assert!(matches!(classify_type(&ty), TypeKind::Vec(_)));
    }

    #[test]
    fn classify_type_scalar_string() {
        let ty: Type = parse_str("String").unwrap();
        assert!(matches!(classify_type(&ty), TypeKind::Other));
    }

    #[test]
    fn classify_type_reference_is_other() {
        let ty: Type = parse_str("&str").unwrap();
        assert!(matches!(classify_type(&ty), TypeKind::Other));
    }

    #[test]
    fn classify_type_tuple_is_other() {
        let ty: Type = parse_str("(i32, i32)").unwrap();
        assert!(matches!(classify_type(&ty), TypeKind::Other));
    }

    #[test]
    fn classify_type_bare_vec_without_type_args() {
        let ty: Type = parse_str("Vec").unwrap();
        assert!(matches!(classify_type(&ty), TypeKind::Other));
    }

    // ── field_type_to_tokens tests ──

    #[test]
    fn field_type_to_tokens_bool() {
        let ty: Type = parse_str("bool").unwrap();
        let tokens = field_type_to_tokens(&ty).to_string();
        assert!(tokens.contains("FieldType"), "expected FieldType in: {tokens}");
        assert!(tokens.contains("Bool"), "expected Bool in: {tokens}");
    }

    #[test]
    fn field_type_to_tokens_vec() {
        let ty: Type = parse_str("Vec<u32>").unwrap();
        let tokens = field_type_to_tokens(&ty).to_string();
        assert!(tokens.contains("FieldType"), "expected FieldType in: {tokens}");
        assert!(tokens.contains("List"), "expected List in: {tokens}");
    }

    #[test]
    fn field_type_to_tokens_scalar() {
        let ty: Type = parse_str("String").unwrap();
        let tokens = field_type_to_tokens(&ty).to_string();
        assert!(
            tokens.contains("ConfigDetect"),
            "expected ConfigDetect dispatch in: {tokens}"
        );
    }

    #[test]
    fn field_type_to_tokens_non_path_fallback() {
        let ty: Type = parse_str("&str").unwrap();
        let tokens = field_type_to_tokens(&ty).to_string();
        assert!(
            tokens.contains("ConfigDetect"),
            "expected ConfigDetect dispatch in fallback: {tokens}"
        );
    }
}
