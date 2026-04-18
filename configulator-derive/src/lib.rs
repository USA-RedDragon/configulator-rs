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
use syn::{parse_macro_input, DeriveInput, Data, Fields, Type, PathArguments, GenericArgument};

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
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "Config can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "Config can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let mut field_info_tokens = Vec::new();
    let mut from_map_tokens = Vec::new();

    for field in fields.iter() {
        let field_ident = field.ident.as_ref().unwrap();
        let field_name_str = field_ident.to_string();
        let field_ty = &field.ty;

        let mut config_name: Option<String> = None;
        let mut default_val: Option<String> = None;
        let mut description: Option<String> = None;

        for attr in &field.attrs {
            if !attr.path().is_ident("configulator") {
                continue;
            }
            let parse_result = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    config_name = Some(lit.value());
                } else if meta.path.is_ident("default") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    default_val = Some(lit.value());
                } else if meta.path.is_ident("description") {
                    let value = meta.value()?;
                    let lit: syn::LitStr = value.parse()?;
                    description = Some(lit.value());
                }
                Ok(())
            });
            if let Err(err) = parse_result {
                return err.to_compile_error().into();
            }
        }

        let config_name_str = config_name.unwrap_or_else(|| field_name_str.clone());
        let field_type_token = field_type_to_tokens(field_ty);

        let default_tokens = match &default_val {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };
        let desc_tokens = match &description {
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

    TokenStream::from(expanded)
}

/// Map a Rust type to the simplified `FieldType` enum (Bool, Scalar, List, Struct).
/// For non-bool, non-Vec types, uses compile-time autoref dispatch to detect
/// whether the type is a nested struct or a scalar.
fn field_type_to_tokens(ty: &Type) -> proc_macro2::TokenStream {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident_str = segment.ident.to_string();

            return match ident_str.as_str() {
                "bool" => quote! { configulator::FieldType::Bool },
                "Vec" => quote! { configulator::FieldType::List },
                _ => quote! {
                    {
                        let __m = configulator::ConfigDetect::<#ty>(::std::marker::PhantomData);
                        __m.__configulator_field_type()
                    }
                },
            };
        }
    }
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
            let ident_str = segment.ident.to_string();

            if ident_str == "bool" {
                return TypeKind::Bool;
            }
            if ident_str == "Vec" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        return TypeKind::Vec(Box::new(inner.clone()));
                    }
                }
                // Vec without type argument will produce a compile error downstream
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
        // A bare `Vec` with no angle brackets — exercises the Vec-without-type-arg branch
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
        // A reference type is not Type::Path — exercises the fallback branch
        let ty: Type = parse_str("&str").unwrap();
        let tokens = field_type_to_tokens(&ty).to_string();
        assert!(
            tokens.contains("ConfigDetect"),
            "expected ConfigDetect dispatch in fallback: {tokens}"
        );
    }
}
