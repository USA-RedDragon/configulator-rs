//! Internal helpers used by code generated from the `Config` derive macro.
//!
//! These types and functions are `pub` so the macro-generated code can reference
//! them, but they are `#[doc(hidden)]` at the re-export site and not intended
//! for direct use.

use std::marker::PhantomData;
use std::str::FromStr;

use crate::error::ConfigulatorError;
use crate::field_info::FieldType;
use crate::value_map::{ConfigValue, ValueMap};
use crate::{ConfigFields, FromValueMap};

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a scalar value from the value map.
pub fn parse_scalar<T: FromStr + Default>(
    map: &ValueMap,
    key: &str,
) -> Result<T, ConfigulatorError>
where
    T::Err: std::fmt::Display,
{
    match map.get(key) {
        Some(ConfigValue::Scalar(s)) => {
            if s.is_empty() {
                return Ok(T::default());
            }
            s.parse::<T>().map_err(|e| ConfigulatorError::ParseError {
                field: key.to_string(),
                value: s.clone(),
                message: e.to_string(),
            })
        }
        None => Ok(T::default()),
        Some(other) => Err(ConfigulatorError::ParseError {
            field: key.to_string(),
            value: format!("{other:?}"),
            message: "expected scalar value".to_string(),
        }),
    }
}

/// Parse a list of values from the value map.
pub fn parse_list<T: FromStr + Default>(
    map: &ValueMap,
    key: &str,
) -> Result<Vec<T>, ConfigulatorError>
where
    T::Err: std::fmt::Display,
{
    match map.get(key) {
        Some(ConfigValue::List(items)) => {
            let mut result = Vec::with_capacity(items.len());
            for (i, s) in items.iter().enumerate() {
                let val = s.parse::<T>().map_err(|e| ConfigulatorError::ParseError {
                    field: format!("{key}[{i}]"),
                    value: s.clone(),
                    message: e.to_string(),
                })?;
                result.push(val);
            }
            Ok(result)
        }
        // A single scalar can be treated as a one-element list
        Some(ConfigValue::Scalar(s)) => {
            if s.is_empty() {
                return Ok(Vec::new());
            }
            let val = s.parse::<T>().map_err(|e| ConfigulatorError::ParseError {
                field: key.to_string(),
                value: s.clone(),
                message: e.to_string(),
            })?;
            Ok(vec![val])
        }
        None => Ok(Vec::new()),
        Some(other) => Err(ConfigulatorError::ParseError {
            field: key.to_string(),
            value: format!("{other:?}"),
            message: "expected list value".to_string(),
        }),
    }
}

/// Parse a nested struct from the value map.
pub fn parse_nested<T: FromValueMap + Default>(
    map: &ValueMap,
    key: &str,
) -> Result<T, ConfigulatorError> {
    match map.get(key) {
        Some(ConfigValue::Nested(nested_map)) => T::from_value_map(nested_map),
        None => Ok(T::default()),
        Some(other) => Err(ConfigulatorError::ParseError {
            field: key.to_string(),
            value: format!("{other:?}"),
            message: "expected nested struct value".to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Compile-time struct vs scalar detection
// ---------------------------------------------------------------------------
//
// Inherent methods on `ConfigDetect<T>` take priority over trait methods in
// Rust's method resolution. When `T: FromValueMap + ConfigFields + Default`
// (i.e. a nested struct that derives Config), the inherent methods are
// available and win. For all other types, the inherent methods are absent
// (bounds not satisfied), so Rust falls through to `ConfiguratorScalar`
// trait methods.

/// Helper type for compile-time detection of nested structs vs scalars.
/// Used by generated code — not intended for direct use.
pub struct ConfigDetect<T>(pub PhantomData<T>);

/// Inherent methods for nested struct types. These take priority over the
/// `ConfiguratorScalar` trait methods when `T` derives `Config`.
impl<T: FromValueMap + ConfigFields + Default> ConfigDetect<T> {
    pub fn __configulator_parse(&self, map: &ValueMap, key: &str) -> Result<T, ConfigulatorError> {
        parse_nested::<T>(map, key)
    }
    pub fn __configulator_field_type(&self) -> FieldType {
        FieldType::Struct(T::configulator_fields())
    }
}

/// Fallback trait for scalar types that implement `FromStr`.
/// Used by generated code when the inherent methods on `ConfigDetect` are not available.
pub trait ConfiguratorScalar {
    type Output;
    fn __configulator_parse(&self, map: &ValueMap, key: &str) -> Result<Self::Output, ConfigulatorError>;
    fn __configulator_field_type(&self) -> FieldType;
}

impl<T> ConfiguratorScalar for ConfigDetect<T>
where
    T: FromStr + Default,
    T::Err: std::fmt::Display,
{
    type Output = T;
    fn __configulator_parse(&self, map: &ValueMap, key: &str) -> Result<T, ConfigulatorError> {
        parse_scalar::<T>(map, key)
    }
    fn __configulator_field_type(&self) -> FieldType {
        FieldType::Scalar
    }
}
