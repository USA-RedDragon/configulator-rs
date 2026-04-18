#![warn(clippy::all)]
#![forbid(unsafe_code)]

mod cli;
mod configulator;
mod defaults;
mod derive_helpers;
mod environment;
mod error;
mod field_info;
mod file;
mod options;
mod value_map;

// Re-export the derive macro
pub use configulator_derive::Config;

// Re-export public types
pub use crate::configulator::Configulator;
pub use crate::error::ConfigulatorError;
pub use crate::field_info::{FieldInfo, FieldType};
pub use crate::options::{CLIFlagOptions, EnvironmentVariableOptions, FileOptions};
pub use crate::value_map::{ConfigValue, ValueMap};

// Re-export derive-macro helpers (used by generated code, not public API)
#[doc(hidden)]
pub use crate::derive_helpers::{
    parse_list, parse_nested, parse_scalar, ConfigDetect, ConfiguratorScalar,
};

/// Trait implemented by the `Config` derive macro. Provides field metadata.
pub trait ConfigFields {
    fn configulator_fields() -> Vec<FieldInfo>;
}

/// Trait implemented by the `Config` derive macro. Constructs a struct from a `ValueMap`.
pub trait FromValueMap: Sized {
    fn from_value_map(map: &ValueMap) -> Result<Self, ConfigulatorError>;
}

/// Trait for user-defined config validation.
///
/// Implement this on your config struct to add validation logic that runs
/// after all sources are merged.
pub trait Validate {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
