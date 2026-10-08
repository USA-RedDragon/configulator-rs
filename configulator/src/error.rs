use std::fmt;

/// Errors that can occur during configuration loading.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigulatorError {
    /// No config file was found in any of the specified paths and
    /// `error_if_not_found` was set.
    #[cfg(feature = "file")]
    FileNotFound,
    /// Failed to read or parse a config file found via the search paths.
    #[cfg(feature = "file")]
    FileError(String),
    /// An explicitly named config file (`--config` or
    /// [`FileOptions::explicit`](crate::FileOptions::explicit)) could not
    /// be read.
    #[cfg(feature = "file")]
    ExplicitFileError {
        /// The path as given.
        path: std::path::PathBuf,
        /// The underlying IO failure.
        message: String,
    },
    /// Failed to parse a value from a string.
    ParseError {
        /// The field path, env var, or flag the value was destined for.
        field: String,
        /// The offending value; `(redacted)` for `secret` fields.
        value: String,
        /// The underlying parse failure.
        message: String,
    },
    /// Invalid [`EnvironmentVariableOptions`](crate::EnvironmentVariableOptions):
    /// the prefix must be uppercase and the separator must not contain `-`.
    #[cfg(feature = "env")]
    BadEnvOptions(String),
    /// A `required` field was not set by any layer.
    Required {
        /// The dotted config path of the missing field.
        path: String,
    },
    /// Validation failed.
    ValidationError(Box<dyn std::error::Error + Send + Sync>),
    /// CLI parsing failed, or `--help`/`--version` was passed. Call
    /// [`clap::Error::exit`] on it to print and exit like a normal clap
    /// app:
    ///
    /// ```rust,ignore
    /// let config = match configulator.load() {
    ///     Ok(c) => c,
    ///     Err(ConfigulatorError::CLIError(e)) => e.exit(),
    ///     Err(e) => return Err(e.into()),
    /// };
    /// ```
    #[cfg(feature = "cli")]
    CLIError(clap::Error),
    /// Two CLI flags share a long name or shorthand: two fields, a field
    /// and `--config`, or a field and an arg on a custom `clap::Command`.
    #[cfg(feature = "cli")]
    FlagConflict(String),
}

impl fmt::Display for ConfigulatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "file")]
            Self::FileNotFound => write!(f, "config file not found"),
            #[cfg(feature = "file")]
            Self::FileError(msg) => write!(f, "file error: {msg}"),
            #[cfg(feature = "file")]
            Self::ExplicitFileError { path, message } => {
                write!(f, "config file {}: {message}", path.display())
            }
            Self::ParseError {
                field,
                value,
                message,
            } => {
                write!(
                    f,
                    "failed to parse field '{field}' value '{value}': {message}"
                )
            }
            #[cfg(feature = "env")]
            Self::BadEnvOptions(msg) => write!(f, "bad environment variable options: {msg}"),
            Self::Required { path } => {
                write!(f, "required field '{path}' was not set by any layer")
            }
            Self::ValidationError(err) => write!(f, "validation error: {err}"),
            #[cfg(feature = "cli")]
            Self::CLIError(err) => write!(f, "{err}"),
            #[cfg(feature = "cli")]
            Self::FlagConflict(flag) => write!(f, "CLI flag {flag} is defined more than once"),
        }
    }
}

impl std::error::Error for ConfigulatorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ValidationError(err) => Some(err.as_ref()),
            #[cfg(feature = "cli")]
            Self::CLIError(err) => Some(err),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}
