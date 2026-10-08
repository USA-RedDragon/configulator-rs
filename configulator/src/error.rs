use std::fmt;

/// Errors that can occur during configuration loading.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigulatorError {
    /// No config file was found in any of the search paths and
    /// `error_if_not_found` was set.
    #[cfg(feature = "file")]
    FileNotFound {
        /// The search paths, in order.
        searched: Vec<std::path::PathBuf>,
    },
    /// A search path exists but could not be read, for example because it
    /// is a directory. Never skipped like a missing path.
    #[cfg(feature = "file")]
    SearchPathUnreadable {
        /// The search path.
        path: std::path::PathBuf,
        /// The underlying IO failure.
        message: String,
    },
    /// A config file was read but its contents could not be decoded.
    #[cfg(feature = "file")]
    DecodeError {
        /// The file that failed to decode.
        path: std::path::PathBuf,
        /// The decoder's error.
        message: String,
    },
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
    /// A value from the environment, a flag, or a `default` attribute could
    /// not be parsed.
    ParseError {
        /// The dotted config path, such as `http.port`.
        path: String,
        /// Where the value came from: the env var, the `--flag`, or
        /// `default tag`.
        source: String,
        /// The offending value; `(redacted)` for `secret` fields.
        value: String,
        /// The underlying parse failure.
        message: String,
    },
    /// Invalid [`EnvironmentVariableOptions`](crate::EnvironmentVariableOptions):
    /// the prefix must be uppercase and the separator must not contain `-`.
    #[cfg(feature = "env")]
    BadEnvOptions {
        /// `Prefix` or `Separator`.
        field: &'static str,
        /// The rejected value.
        value: String,
        /// Why it was rejected.
        reason: &'static str,
    },
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
    FlagConflict {
        /// The flag being added.
        flag: String,
        /// The clashing shorthand, or `None` when the long name clashes.
        shorthand: Option<char>,
        /// The flag that already has the name or shorthand.
        existing: String,
    },
}

impl fmt::Display for ConfigulatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "file")]
            Self::FileNotFound { searched } => {
                let paths: Vec<String> = searched.iter().map(|p| p.display().to_string()).collect();
                write!(f, "no config file found; searched [{}]", paths.join(" "))
            }
            #[cfg(feature = "file")]
            Self::SearchPathUnreadable { path, message } => {
                write!(f, "config search path {}: {message}", path.display())
            }
            #[cfg(feature = "file")]
            Self::DecodeError { path, message } => {
                write!(f, "decoding {}: {message}", path.display())
            }
            #[cfg(feature = "file")]
            Self::ExplicitFileError { path, message } => {
                write!(f, "config file {}: {message}", path.display())
            }
            Self::ParseError {
                path,
                source,
                value,
                message,
            } => write!(f, "{path}: cannot parse {value:?} from {source}: {message}"),
            #[cfg(feature = "env")]
            Self::BadEnvOptions {
                field,
                value,
                reason,
            } => write!(f, "EnvironmentVariableOptions.{field} {value:?}: {reason}"),
            Self::Required { path } => write!(f, "{path}: required but not set by any layer"),
            Self::ValidationError(err) => write!(f, "validation error: {err}"),
            #[cfg(feature = "cli")]
            Self::CLIError(err) => write!(f, "{err}"),
            #[cfg(feature = "cli")]
            Self::FlagConflict {
                flag,
                shorthand: Some(c),
                existing,
            } => write!(
                f,
                "flag {flag:?}: shorthand {:?} is already used by flag {existing:?}",
                c.to_string()
            ),
            #[cfg(feature = "cli")]
            Self::FlagConflict { flag, .. } => write!(
                f,
                "flag {flag:?} is already defined; rename one of them (flag = \"name\", FileOptions::flag_name) or skip the field with flag = \"-\""
            ),
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
