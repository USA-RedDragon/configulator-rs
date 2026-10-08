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
    fn shadow_defaults(array_sep: &str) -> Result<Self::Shadow, ConfigulatorError>;

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
        array_sep: &str,
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

    /// `print_config` value wrapper. `(&PrintVal(&v)).print_val()` uses
    /// `Debug` when `v` has it and falls back to a placeholder otherwise,
    /// so field types aren't required to implement `Debug`.
    pub struct PrintVal<'a, T>(pub &'a T);

    pub trait PrintDebug {
        fn print_val(&self) -> String;
    }

    impl<T: fmt::Debug> PrintDebug for PrintVal<'_, T> {
        fn print_val(&self) -> String {
            format!("{:?}", self.0)
        }
    }

    pub trait PrintFallback {
        fn print_val(&self) -> String;
    }

    impl<T> PrintFallback for &PrintVal<'_, T> {
        fn print_val(&self) -> String {
            "(no Debug impl)".to_string()
        }
    }

    #[cfg(feature = "cli")]
    pub use clap;

    /// The one leaf parse path: file, env, CLI, and defaults all land here,
    /// so a custom `FromStr` type behaves identically in all four layers.
    pub fn parse_leaf<T: FromStr>(
        s: &str,
        field: &str,
        secret: bool,
    ) -> Result<T, ConfigulatorError>
    where
        T::Err: fmt::Display,
    {
        T::from_str(s).map_err(|e| ConfigulatorError::ParseError {
            field: field.to_string(),
            value: if secret {
                "(redacted)".to_string()
            } else {
                s.to_string()
            },
            message: e.to_string(),
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
    struct ScalarVisitor<T>(std::marker::PhantomData<T>);

    #[cfg(feature = "file")]
    impl<'de, T> serde::de::Visitor<'de> for ScalarVisitor<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        type Value = Option<T>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a scalar")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            T::from_str(v).map(Some).map_err(serde::de::Error::custom)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
            self.visit_str(&v.to_string())
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
            self.visit_str(&v.to_string())
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
            self.visit_str(&v.to_string())
        }
        fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
            self.visit_str(&v.to_string())
        }
        // null in a file means unset
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_any(ScalarVisitor(std::marker::PhantomData))
        }
    }

    /// `deserialize_with` target for shadow leaf fields. `name` is baked in
    /// by the derive so parse errors carry the field name.
    #[cfg(feature = "file")]
    pub fn leaf_named<'de, D, T>(d: D, name: &str) -> Result<Option<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
        T: FromStr,
        T::Err: fmt::Display,
    {
        d.deserialize_any(ScalarVisitor::<T>(std::marker::PhantomData))
            .map_err(|e| serde::de::Error::custom(format_args!("{name}: {e}")))
    }

    #[cfg(feature = "file")]
    impl<'de, T> serde::Deserialize<'de> for Leaf<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            match d.deserialize_any(ScalarVisitor::<T>(std::marker::PhantomData))? {
                Some(v) => Ok(Leaf(v)),
                None => Err(serde::de::Error::custom(
                    "null is not a valid list/map element",
                )),
            }
        }
    }
}
