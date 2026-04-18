//! # Configulator
//!
//! A simple configuration manager for Rust applications with derive macro support.
//!
//! Supports configuration from multiple sources with clear precedence:
//!
//! 1. **Default values** (lowest priority)
//! 2. **Config files** (any serde format via [`serde_loader`])
//! 3. **Environment variables**
//! 4. **CLI flags** (highest priority)
//!
//! ## Features
//!
//! - `#[derive(Config)]` macro for declarative configuration structs
//! - Any serde-compatible file format - YAML, TOML, JSON, with a one-liner
//! - Pluggable file format support - bring your own parser via [`FileLoader`]
//! - Nested struct support
//! - [`Vec<T>`](Vec) list fields
//! - Custom types - anything implementing [`FromStr`](std::str::FromStr) + [`Default`]
//! - Optional validation via the [`Validate`] trait
//! - Boolean CLI flags (`--debug` sets true, `--debug false` sets false)
//!
//! ## Usage
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! configulator-rs = "0.1"
//! ```
//!
//! > **Note:** Because the configuration options are expressed as different cases
//! > (i.e. `http.host` in a config file would be `HTTP__HOST` in environment
//! > variables), this library cannot be used for configurations that contain the
//! > same field name in different cases.
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
//!         .load_without_validation()
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
//!     CLIFlagOptions, Config, Configulator,
//!     EnvironmentVariableOptions, FileOptions, Validate,
//!     serde_loader,
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
//!         Ok(())
//!     }
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Configulator::<AppConfig>::new()
//!         .with_file(FileOptions {
//!             paths: vec!["config.yaml".into(), "/etc/myapp/config.yaml".into()],
//!             error_if_not_found: false,
//!             // Any serde-compatible format works: serde_json, toml, etc.
//!             loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
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
//! ```rust,ignore
//! // Field appears in config files, env vars, and CLI flags as "my-name".
//! #[configulator(name = "my-name")]
//! my_name: String,
//!
//! // Field has a description shown in CLI --help
//! #[configulator(name = "my-name", description = "this text appears in --help")]
//! my_name: String,
//!
//! // Field has a default value of 1
//! #[configulator(name = "my-name", default = "1")]
//! my_name: u32,
//! ```
//!
//! ## Supported Types
//!
//! - All primitive scalars (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`, `bool`, [`String`])
//! - [`PathBuf`](std::path::PathBuf) and any other [`FromStr`](std::str::FromStr) + [`Default`] type
//! - Custom enums (implement [`FromStr`](std::str::FromStr) + [`Default`])
//! - [`Vec<T>`](Vec) for list values (comma-separated defaults, repeated CLI flags)
//! - Nested structs (must also derive `Config`)
//!
//! ## Configuration Sources
//!
//! ### Config Files
//!
//! Configulator is format-agnostic, pass any serde-compatible deserializer via
//! [`serde_loader`], or implement the [`FileLoader`] trait for full control.
//! YAML, TOML, JSON, and any other serde format work out of the box.
//!
//! Provide a list of paths to search. The first file found is used.
//!
//! ```rust,ignore
//! // YAML
//! .with_file(FileOptions {
//!     paths: vec!["config.yaml".into()],
//!     error_if_not_found: false,
//!     loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
//! })
//!
//! // TOML
//! .with_file(FileOptions {
//!     paths: vec!["config.toml".into()],
//!     error_if_not_found: false,
//!     loader: serde_loader(|s| toml::from_str(s)),
//! })
//!
//! // JSON
//! .with_file(FileOptions {
//!     paths: vec!["config.json".into()],
//!     error_if_not_found: false,
//!     loader: serde_loader(|s| serde_json::from_str(s)),
//! })
//! ```
//!
//! The CLI also accepts `--config` / `-c` to specify a config file path at runtime
//! (requires calling [`.with_file()`](Configulator::with_file) first).
//!
//! ### Environment Variables
//!
//! Environment variables are formed as `PREFIX` + `SEPARATOR` + `FIELD_NAME`
//! (uppercased, dashes become underscores).
//!
//! ```rust,ignore
//! .with_environment_variables(EnvironmentVariableOptions {
//!     prefix: "MYAPP".into(),
//!     separator: "__".into(),
//! })
//! ```
//!
//! For example, a field named `max-connections` under a `database` parent with
//! prefix `MYAPP` and separator `__` would be `MYAPP__DATABASE__MAX_CONNECTIONS`.
//!
//! ### CLI Flags
//!
//! Nested fields use the separator to form flag names (e.g. `--database.host`).
//!
//! ```rust,ignore
//! .with_cli_flags(CLIFlagOptions {
//!     separator: ".".into(),
//! })
//! ```
//!
//! Boolean fields work as flags (`--debug` sets to true, `--debug false` sets
//! to false). List fields can be repeated (`--ports 80 --ports 443`).
//!
//! You can also provide a custom `clap::Command` to set the app name, version,
//! or add your own flags:
//!
//! ```rust,ignore
//! .with_cli_command(clap::Command::new("myapp").version("1.0"))
//! .with_cli_flags(CLIFlagOptions {
//!     separator: ".".into(),
//! })
//! ```
//!
//! ## Validation
//!
//! Implement the [`Validate`] trait and call [`.load()`](Configulator::load) to validate after loading.
//! Use [`.load_without_validation()`](Configulator::load_without_validation) to skip validation.
//!
//! ## Feature Flags
//!
//! Configulator uses feature flags to keep dependencies minimal. All features
//! are enabled by default.
//!
//! | Feature | Description                                                          | Dependencies |
//! |---------|----------------------------------------------------------------------|--------------|
//! | `file`  | Config file loading (`FileOptions`, `serde_loader`, `--config` flag) | `serde`      |
//! | `cli`   | CLI flag parsing via clap                                            | `clap`       |
//! | `env`   | Environment variable loading                                         | -            |
//!
//! To opt out of features you don't need:
//!
//! ```toml
//! [dependencies]
//! configulator-rs = { version = "0.1", default-features = false, features = ["env"] }
//! ```

#![warn(clippy::all)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "cli")]
mod cli;
mod configulator;
mod defaults;
mod derive_helpers;
#[cfg(feature = "env")]
mod environment;
mod error;
mod field_info;
#[cfg(feature = "file")]
mod file;
mod options;
mod value_map;

// Re-export the derive macro
pub use configulator_derive::Config;

// Re-export public types
pub use crate::configulator::Configulator;
pub use crate::error::ConfigulatorError;
#[cfg(feature = "cli")]
pub use crate::options::CLIFlagOptions;
#[cfg(feature = "env")]
pub use crate::options::EnvironmentVariableOptions;
#[cfg(feature = "file")]
pub use crate::file::FileLoader;
#[cfg(feature = "file")]
pub use crate::file::serde_loader;
#[cfg(feature = "file")]
pub use crate::options::FileOptions;

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
