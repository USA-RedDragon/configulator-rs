//! # Configulator
//!
//! A configuration library that merges values from multiple sources into a
//! typed struct, with clear precedence:
//!
//! 1. **Default values** (lowest priority)
//! 2. **YAML config files**
//! 3. **Environment variables**
//! 4. **CLI flags** (highest priority)
//!
//! ## Features
//!
//! - `#[derive(Config)]` macro for declarative configuration structs
//! - Nested struct support with automatic detection
//! - `Vec<T>` list fields
//! - Custom types — anything implementing `FromStr + Default`
//! - Optional validation via the [`Validate`] trait
//! - Boolean CLI flags (`--debug` sets true, `--debug false` sets false)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use configulator::{Config, Configulator, Validate};
//!
//! #[derive(Config, Default, Debug)]
//! struct AppConfig {
//!     #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
//!     host: String,
//!
//!     #[configulator(name = "port", default = "8080", description = "Listen port")]
//!     port: u16,
//!
//!     #[configulator(name = "debug", default = "false", description = "Enable debug mode")]
//!     debug: bool,
//! }
//!
//! fn main() {
//!     let config: AppConfig = Configulator::new()
//!         .build()
//!         .expect("failed to load config");
//!     println!("{config:?}");
//! }
//! ```
//!
//! ## Configuration Sources
//!
//! Enable as many or as few sources as you need via the builder:
//!
//! ```rust,no_run
//! use configulator::{
//!     CLIFlagOptions, Config, Configulator, EnvironmentVariableOptions, FileOptions, Validate,
//! };
//!
//! #[derive(Config, Default, Debug)]
//! struct AppConfig {
//!     #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
//!     host: String,
//!
//!     #[configulator(name = "port", default = "8080", description = "Listen port")]
//!     port: u16,
//!
//!     #[configulator(name = "debug", default = "false", description = "Enable debug mode")]
//!     debug: bool,
//!
//!     #[configulator(name = "allowed-origins", default = "localhost,example.com")]
//!     allowed_origins: Vec<String>,
//!
//!     #[configulator(name = "database")]
//!     database: DatabaseConfig,
//! }
//!
//! #[derive(Config, Default, Debug)]
//! struct DatabaseConfig {
//!     #[configulator(name = "url", default = "postgres://localhost/mydb")]
//!     url: String,
//!
//!     #[configulator(name = "max-connections", default = "10")]
//!     max_connections: u32,
//! }
//!
//! impl Validate for AppConfig {
//!     fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         if self.port == 0 {
//!             return Err("port must be non-zero".into());
//!         }
//!         if self.database.max_connections == 0 {
//!             return Err("max-connections must be non-zero".into());
//!         }
//!         Ok(())
//!     }
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Configulator::<AppConfig>::new()
//!         // YAML file — first path found wins; no error if missing
//!         .with_file(FileOptions {
//!             paths: vec!["config.yaml".into(), "/etc/myapp/config.yaml".into()],
//!             error_if_not_found: false,
//!         })
//!         // Env vars: MYAPP__HOST, MYAPP__DATABASE__MAX_CONNECTIONS, etc.
//!         .with_environment_variables(EnvironmentVariableOptions {
//!             prefix: "MYAPP".into(),
//!             separator: "__".into(),
//!         })
//!         // CLI flags: --host, --database.url, --debug, etc.
//!         .with_cli_flags(CLIFlagOptions {
//!             separator: ".".into(),
//!         })
//!         // .load() validates; use .load_without_validation() to skip
//!         .load()?;
//!
//!     println!("Host: {}", config.host);
//!     println!("Database URL: {}", config.database.url);
//!     Ok(())
//! }
//! ```
//!
//! ## Derive Attributes
//!
//! Fields are annotated with `#[configulator(...)]` using these keys:
//!
//! | Key           | Description                                    |
//! |---------------|------------------------------------------------|
//! | `name`        | Config key name (defaults to the field name)   |
//! | `default`     | Default value as a string literal              |
//! | `description` | Help text shown in CLI `--help` output         |
//!
//! ## Supported Types
//!
//! - All primitive scalars (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`, `bool`, `String`)
//! - `PathBuf` and any other `FromStr + Default` type
//! - Custom enums (implement `FromStr + Default`)
//! - `Vec<T>` for list values (comma-separated defaults, repeated CLI flags)
//! - Nested structs (must also derive `Config`)

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
pub use crate::options::{CLIFlagOptions, EnvironmentVariableOptions, FileOptions};

// Re-export derive-macro internals (used by generated code, not public API)
#[doc(hidden)]
pub use crate::derive_helpers::{
    parse_list, parse_nested, parse_scalar, ConfigDetect, ConfiguratorScalar,
};
#[doc(hidden)]
pub use crate::field_info::{FieldInfo, FieldType};
#[doc(hidden)]
pub use crate::value_map::{ConfigValue, ValueMap};

/// Trait implemented by the `Config` derive macro. Provides field metadata.
#[doc(hidden)]
pub trait ConfigFields {
    fn configulator_fields() -> Vec<FieldInfo>;
}

/// Trait implemented by the `Config` derive macro. Constructs a struct from a `ValueMap`.
#[doc(hidden)]
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
