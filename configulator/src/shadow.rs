//! The shadow trait: every `#[derive(Config)]` struct gets a generated
//! mirror ("shadow") whose fields are all `Option`-typed. Every layer
//! (defaults, file, env, CLI) produces a shadow; merging is `overlay`;
//! a single `build` constructs the real struct at the end.

use crate::error::ConfigulatorError;
use crate::report::{Layer, Report};

/// Implemented by `#[derive(Config)]`. Associates a config struct with its
/// generated shadow type.
///
/// All methods are implementation detail of the derive macro; user code
/// only ever names the trait as a bound (e.g. in a hand-written
/// [`FileLoader`](crate::FileLoader)).
pub trait HasShadow: Sized {
    /// The generated presence-aware mirror of this struct.
    type Shadow: Default;

    #[doc(hidden)]
    fn shadow_defaults(prefix: &str, array_sep: &str) -> Result<Self::Shadow, ConfigulatorError>;

    #[doc(hidden)]
    fn overlay(
        acc: &mut Self::Shadow,
        other: Self::Shadow,
        prefix: &str,
        layer: Layer,
        detail: &dyn Fn(&str) -> String,
        report: &mut Report,
    );

    #[doc(hidden)]
    fn build(
        sh: Self::Shadow,
        prefix: &str,
        ctx: &__private::BuildCtx,
        report: &mut Report,
    ) -> Result<Self, ConfigulatorError>;

    /// `#[configulator(allow_unknown_fields)]` was set on the struct.
    #[doc(hidden)]
    const ALLOW_UNKNOWN_FIELDS: bool = false;

    /// Record origins for every set field of `sh` (used for elements of
    /// collections, which replace wholesale rather than deep-merging).
    #[doc(hidden)]
    fn record_set(
        sh: &Self::Shadow,
        prefix: &str,
        layer: Layer,
        detail: &dyn Fn(&str) -> String,
        report: &mut Report,
    );

    /// True when no field of `sh` is set (used to avoid allocating
    /// optional nested structs a layer never touched).
    #[doc(hidden)]
    fn shadow_is_vacant(sh: &Self::Shadow) -> bool;

    #[doc(hidden)]
    fn fields() -> Vec<crate::field_info::FieldInfo>;

    #[doc(hidden)]
    #[cfg(feature = "env")]
    fn from_env(
        get: &dyn Fn(&str) -> Option<String>,
        prefix: &str,
        sep: &str,
        array_sep: &str,
    ) -> Result<Self::Shadow, ConfigulatorError>;

    #[doc(hidden)]
    #[cfg(feature = "cli")]
    fn from_cli(
        matches: &clap::ArgMatches,
        prefix: &str,
        sep: &str,
    ) -> Result<Self::Shadow, ConfigulatorError>;
}

#[doc(hidden)]
pub mod __private {
    use super::*;
    use std::fmt;
    use std::str::FromStr;

    #[cfg(feature = "file")]
    pub use serde;

    /// Settings for [`HasShadow::build`].
    pub struct BuildCtx {
        /// Separator for list defaults.
        pub array_sep: String,
        /// Fail on unset `required` fields.
        pub check_required: bool,
    }

    /// `print_config` value wrapper. `(&&&PrintVal(&v)).print_val()` uses
    /// `Display` when `v` has it, then `Debug`, then a placeholder, so field
    /// types need neither.
    pub struct PrintVal<'a, T: ?Sized>(pub &'a T);

    pub trait PrintDisplay {
        fn print_val(&self) -> String;
    }

    impl<T: fmt::Display + ?Sized> PrintDisplay for &&PrintVal<'_, T> {
        fn print_val(&self) -> String {
            self.0.to_string()
        }
    }

    pub trait PrintDebug {
        fn print_val(&self) -> String;
    }

    impl<T: fmt::Debug + ?Sized> PrintDebug for &PrintVal<'_, T> {
        fn print_val(&self) -> String {
            format!("{:?}", self.0)
        }
    }

    pub trait PrintFallback {
        fn print_val(&self) -> String;
    }

    impl<T: ?Sized> PrintFallback for PrintVal<'_, T> {
        fn print_val(&self) -> String {
            "(no Debug impl)".to_string()
        }
    }

    /// A float as Go's `%v` prints it.
    pub fn go_float(v: f64, is_f32: bool) -> String {
        crate::complex::format_g(v, if is_f32 { 9 } else { 17 })
    }

    /// A Go `%v` list: `[a b]`.
    pub fn go_list(items: impl IntoIterator<Item = String>) -> String {
        format!("[{}]", items.into_iter().collect::<Vec<_>>().join(" "))
    }

    /// A Go `%v` map with keys sorted: `map[a:x b:y]`.
    pub fn go_map(entries: impl IntoIterator<Item = (String, String)>) -> String {
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort();
        let body: Vec<String> = entries
            .into_iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        format!("map[{}]", body.join(" "))
    }

    /// True if any field in the tree is `secret`.
    pub fn has_secret(fields: &[crate::field_info::FieldInfo]) -> bool {
        use crate::field_info::FieldType;
        fields.iter().any(|f| {
            f.secret
                || match &f.field_type {
                    FieldType::Struct(sub)
                    | FieldType::StructList(sub)
                    | FieldType::StructMap(sub) => has_secret(sub),
                    _ => false,
                }
        })
    }

    #[cfg(feature = "cli")]
    pub use clap;

    /// The one leaf parse path: file, env, CLI, and defaults all land here,
    /// so a custom `FromStr` type behaves identically in all four layers.
    pub fn parse_leaf<T: FromStr + 'static>(
        s: &str,
        path: &str,
        source: &str,
        secret: bool,
    ) -> Result<T, ConfigulatorError>
    where
        T::Err: fmt::Display,
    {
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            let parsed = match s {
                "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
                "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
                _ => None,
            };
            return match parsed {
                Some(b) => Ok(*(Box::new(b) as Box<dyn std::any::Any>)
                    .downcast::<T>()
                    .unwrap()),
                None => Err(ConfigulatorError::ParseError {
                    path: path.to_string(),
                    source: source.to_string(),
                    value: if secret {
                        "(redacted)".to_string()
                    } else {
                        s.to_string()
                    },
                    message: "invalid boolean".to_string(),
                }),
            };
        }
        T::from_str(s).map_err(|e| ConfigulatorError::ParseError {
            path: path.to_string(),
            source: source.to_string(),
            value: if secret {
                "(redacted)".to_string()
            } else {
                s.to_string()
            },
            message: if secret {
                "invalid value".to_string()
            } else {
                e.to_string()
            },
        })
    }

    /// Split a list-valued env or default string on the configured
    /// separator. No trimming, no empty-element dropping (SPEC); a fully
    /// empty string is the empty list.
    pub fn split_list<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
        if s.is_empty() {
            return Vec::new();
        }
        s.split(sep).collect()
    }

    /// Join a path prefix and a field name with a dot.
    pub fn join(prefix: &str, name: &str) -> String {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        }
    }

    /// Quote a map key in an origin path when it contains `.` or `[`.
    pub fn quote_key(k: &str) -> String {
        if k.contains('.') || k.contains('[') {
            format!("{k:?}")
        } else {
            k.to_string()
        }
    }

    /// Wrapper for scalar elements of `Vec`/map fields in a shadow: routes
    /// deserialization through the same `FromStr` path as every other
    /// leaf. Inside a collection, `null` is an error, not "absent".
    #[derive(Debug, Clone, PartialEq, Default)]
    pub struct Leaf<T>(pub T);

    #[cfg(feature = "file")]
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Int,
        Float,
        Bool,
        Complex,
        Text,
    }

    #[cfg(feature = "file")]
    fn kind_of<T: 'static>() -> Kind {
        use std::any::TypeId;
        let t = TypeId::of::<T>();
        let any = |ids: &[TypeId]| ids.contains(&t);
        if any(&[
            TypeId::of::<i8>(),
            TypeId::of::<i16>(),
            TypeId::of::<i32>(),
            TypeId::of::<i64>(),
            TypeId::of::<i128>(),
            TypeId::of::<isize>(),
            TypeId::of::<u8>(),
            TypeId::of::<u16>(),
            TypeId::of::<u32>(),
            TypeId::of::<u64>(),
            TypeId::of::<u128>(),
            TypeId::of::<usize>(),
        ]) {
            Kind::Int
        } else if any(&[TypeId::of::<f32>(), TypeId::of::<f64>()]) {
            Kind::Float
        } else if t == TypeId::of::<bool>() {
            Kind::Bool
        } else if any(&[
            TypeId::of::<crate::Complex64>(),
            TypeId::of::<crate::Complex128>(),
        ]) {
            Kind::Complex
        } else {
            Kind::Text
        }
    }

    #[cfg(feature = "file")]
    struct ScalarVisitor<T>(Kind, std::marker::PhantomData<T>);

    #[cfg(feature = "file")]
    impl<T: 'static> ScalarVisitor<T> {
        fn new() -> Self {
            ScalarVisitor(kind_of::<T>(), std::marker::PhantomData)
        }
    }

    #[cfg(feature = "file")]
    impl<'de, T> serde::de::Visitor<'de> for ScalarVisitor<T>
    where
        T: FromStr + 'static,
        T::Err: fmt::Display,
    {
        type Value = Option<T>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str(match self.0 {
                Kind::Int => "an integer",
                Kind::Float => "a number",
                Kind::Bool => "a boolean",
                Kind::Complex => "a number or a string",
                Kind::Text => "a string",
            })
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if matches!(self.0, Kind::Int | Kind::Float | Kind::Bool) {
                return Err(E::invalid_type(serde::de::Unexpected::Str(v), &self));
            }
            T::from_str(v).map(Some).map_err(E::custom)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
            if !matches!(self.0, Kind::Int | Kind::Float | Kind::Complex) {
                return Err(E::invalid_type(serde::de::Unexpected::Signed(v), &self));
            }
            T::from_str(&v.to_string()).map(Some).map_err(E::custom)
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
            if !matches!(self.0, Kind::Int | Kind::Float | Kind::Complex) {
                return Err(E::invalid_type(serde::de::Unexpected::Unsigned(v), &self));
            }
            T::from_str(&v.to_string()).map(Some).map_err(E::custom)
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
            if !matches!(self.0, Kind::Float | Kind::Complex) {
                return Err(E::invalid_type(serde::de::Unexpected::Float(v), &self));
            }
            T::from_str(&v.to_string()).map(Some).map_err(E::custom)
        }
        fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
            if self.0 != Kind::Bool {
                return Err(E::invalid_type(serde::de::Unexpected::Bool(v), &self));
            }
            T::from_str(&v.to_string()).map(Some).map_err(E::custom)
        }
        // null in a file means unset
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_any(ScalarVisitor::<T>::new())
        }
    }

    /// `deserialize_with` target for shadow leaf fields. `name` is baked in
    /// by the derive so parse errors carry the field name.
    #[cfg(feature = "file")]
    pub fn leaf_named<'de, D, T>(d: D, name: &str, secret: bool) -> Result<Option<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
        T: FromStr + 'static,
        T::Err: fmt::Display,
    {
        d.deserialize_any(ScalarVisitor::<T>::new()).map_err(|e| {
            if secret {
                serde::de::Error::custom(format_args!("{name}: invalid value"))
            } else {
                serde::de::Error::custom(format_args!("{name}: {e}"))
            }
        })
    }

    #[cfg(feature = "file")]
    impl<'de, T> serde::Deserialize<'de> for Leaf<T>
    where
        T: FromStr + 'static,
        T::Err: fmt::Display,
    {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match d.deserialize_any(ScalarVisitor::<T>::new())? {
                Some(v) => Ok(Leaf(v)),
                None => Err(serde::de::Error::custom(
                    "null is not a valid list/map element",
                )),
            }
        }
    }
}
