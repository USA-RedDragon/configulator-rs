use std::fmt;

/// Errors that can occur during configuration loading.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigulatorError {
    /// No config file was found in any of the specified paths.
    #[cfg(feature = "file")]
    FileNotFound,
    /// Failed to read or parse a config file.
    #[cfg(feature = "file")]
    FileError(String),
    /// Failed to parse a value from a string.
    ParseError { field: String, value: String, message: String },
    /// Validation failed.
    ValidationError(Box<dyn std::error::Error + Send + Sync>),
    /// A required configuration source had an issue.
    #[cfg(feature = "cli")]
    CLIError(String),
}

impl fmt::Display for ConfigulatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "file")]
            Self::FileNotFound => write!(f, "config file not found"),
            #[cfg(feature = "file")]
            Self::FileError(msg) => write!(f, "file error: {msg}"),
            Self::ParseError { field, value, message } => {
                write!(f, "failed to parse field '{field}' value '{value}': {message}")
            }
            Self::ValidationError(err) => write!(f, "validation error: {err}"),
            #[cfg(feature = "cli")]
            Self::CLIError(msg) => write!(f, "CLI error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigulatorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ValidationError(err) => Some(err.as_ref()),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}
