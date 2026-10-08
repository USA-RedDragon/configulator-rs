//! # Configulator
//!
//! A simple configuration manager for Rust applications with derive macro
//! support.
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
//! - Nested structs, `Vec<T>`, maps, `Vec<Struct>`, and `Option<T>` fields
//! - Custom types - anything implementing [`FromStr`](std::str::FromStr)
//! - Per-field origin [`Report`]: which source set each value
//! - `secret` redaction, `required` fields, and a generated `print_config()`
//! - Go-style [`Duration`] (`"30s"`, `"1h30m"`)
//! - Optional validation via the [`Validate`] trait
//! - Boolean CLI flags (`--debug` sets true, `--debug false` sets false)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use configulator::{Config, Configulator};
//!
//! #[derive(Config, Debug)]
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
//! #[derive(Config, Debug)]
//! struct AppConfig {
//!     #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
//!     host: String,
//!
//!     #[configulator(name = "port", default = "8080", description = "Listen port")]
//!     port: u16,
//!
//!     #[configulator(name = "allowed-origins", default = "localhost,example.com")]
//!     allowed_origins: Vec<String>,
//!
//!     #[configulator(name = "database", nested)]
//!     database: DatabaseConfig,
//! }
//!
//! #[derive(Config, Debug)]
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
//!     let result = Configulator::<AppConfig>::new()
//!         .with_file(FileOptions {
//!             paths: vec!["config.yaml".into(), "/etc/myapp/config.yaml".into()],
//!             // Any serde-compatible format works: serde_json, toml, etc.
//!             ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
//!         })
//!         // Env vars: MYAPP__HOST, MYAPP__DATABASE__MAX_CONNECTIONS, etc.
//!         .with_environment_variables(EnvironmentVariableOptions {
//!             prefix: "MYAPP__".into(),
//!             separator: "__".into(),
//!         })
//!         // CLI flags: --host, --database.url, etc.
//!         .with_cli_flags(CLIFlagOptions {
//!             separator: ".".into(),
//!         })
//!         .load_with_report();
//!     // --help, --version, and bad args come back as a clap::Error
//!     let (config, report) = match result {
//!         Ok(ok) => ok,
//!         Err(configulator::ConfigulatorError::CLIError(e)) => e.exit(),
//!         Err(e) => return Err(e.into()),
//!     };
//!
//!     println!("Host: {}", config.host);
//!     for path in report.paths() {
//!         let origin = report.origin(path).unwrap();
//!         println!("{path} came from {} ({})", origin.layer, origin.detail);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Derive Attributes
//!
//! Fields are annotated with `#[configulator(...)]`:
//!
//! | Key           | Description                                                     |
//! |---------------|-----------------------------------------------------------------|
//! | `name`        | Config key name (defaults to the field name)                    |
//! | `default`     | Default value as a string literal                               |
//! | `description` | Help text shown in CLI `--help` output                          |
//! | `nested`      | The field's type (or `Vec`/map element type) derives `Config`   |
//! | `env = "-"` / `env = "NAME"` | Skip env, or override this field's env segment   |
//! | `flag = "-"` / `flag = "NAME"` | Skip CLI, or override this field's flag segment |
//! | `short = 'p'` | Single-character CLI shorthand                                  |
//! | `secret`      | Redacted in `print_config()` and parse errors                   |
//! | `required`    | Some source must set this field                                 |
//!
//! On the struct: `#[configulator(crate = "path")]` if you renamed the
//! crate, and `#[configulator(allow_unknown_fields)]` to allow unknown keys
//! in config files.
//!
//! ## Supported Types
//!
//! - All primitive scalars (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`, `bool`,
//!   [`String`])
//! - [`PathBuf`](std::path::PathBuf) and any other
//!   [`FromStr`](std::str::FromStr) type, including [`Duration`]
//! - `Option<T>` of a scalar or nested struct. It stays `None` unless a
//!   source sets it, and `null` in a file counts as unset.
//! - [`Vec<T>`](Vec) of scalars, and `Vec<T>` of nested structs (file only)
//! - Maps with scalar or nested struct values (file only)
//! - Nested structs (must also derive `Config` and be marked `nested`)
//!
//! ## Environment Variables
//!
//! Variable names are `prefix` + each config name in the path (uppercased,
//! dashes become underscores) joined by `separator`. The prefix is used
//! as-is, so include the trailing separator, and it must be uppercase.
//!
//! ## Validation
//!
//! Implement [`Validate`] and call [`Configulator::load`] (or
//! [`load_with_report`](Configulator::load_with_report)) to validate after
//! loading. Use
//! [`load_without_validation`](Configulator::load_without_validation) to
//! skip it.
//!
//! ## Feature Flags
//!
//! | Feature | Description                                                          | Dependencies |
//! |---------|----------------------------------------------------------------------|--------------|
//! | `file`  | Config file loading (`FileOptions`, `serde_loader`, `--config` flag) | `serde`      |
//! | `cli`   | CLI flag parsing via clap                                            | `clap`       |
//! | `env`   | Environment variable loading                                         | -            |

#![warn(clippy::all)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "cli")]
mod cli;
mod configulator;
mod duration;
mod error;
mod field_info;
#[cfg(feature = "file")]
mod file;
mod options;
mod report;
mod shadow;

pub use configulator_derive::Config;

pub use crate::configulator::Configulator;
pub use crate::duration::Duration;
pub use crate::error::ConfigulatorError;
#[cfg(feature = "file")]
pub use crate::file::{serde_loader, FileLoader};
#[cfg(feature = "cli")]
pub use crate::options::CLIFlagOptions;
#[cfg(feature = "env")]
pub use crate::options::EnvironmentVariableOptions;
#[cfg(feature = "file")]
pub use crate::options::FileOptions;
pub use crate::report::{Layer, Origin, Report};
pub use crate::shadow::HasShadow;

#[doc(hidden)]
pub use crate::field_info::{FieldInfo, FieldType, ScalarHint};
#[doc(hidden)]
pub use crate::shadow::__private;

/// Trait for user-defined config validation.
///
/// Implement this on your config struct to add validation logic that runs
/// after all sources are merged.
pub trait Validate {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
