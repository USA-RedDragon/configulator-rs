use std::path::PathBuf;

#[cfg(feature = "file")]
use crate::file::FileLoader;

/// Options for loading configuration from a file.
///
/// Users must supply a [`FileLoader`] implementation that knows how to
/// parse the file contents.
/// Use [`serde_loader`](crate::serde_loader) for any serde-compatible format.
#[cfg(feature = "file")]
pub struct FileOptions {
    /// List of file paths to search. The first one found is used.
    pub paths: Vec<PathBuf>,
    /// If true, return an error if no config file is found.
    pub error_if_not_found: bool,
    /// The loader that parses file contents into configuration values.
    pub loader: Box<dyn FileLoader>,
}

#[cfg(feature = "file")]
impl std::fmt::Debug for FileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileOptions")
            .field("paths", &self.paths)
            .field("error_if_not_found", &self.error_if_not_found)
            .field("loader", &"<dyn FileLoader>")
            .finish()
    }
}

/// Options for loading configuration from environment variables.
#[cfg(feature = "env")]
#[derive(Debug, Clone)]
pub struct EnvironmentVariableOptions {
    /// Prefix for env vars (e.g. `"APP"` with separator `"__"` → `APP__PORT`).
    pub prefix: String,
    /// Separator between prefix and field names (e.g. `"__"` → `APP__DATABASE__HOST`).
    pub separator: String,
}

/// Options for loading configuration from CLI flags.
#[cfg(feature = "cli")]
#[derive(Debug, Clone)]
pub struct CLIFlagOptions {
    /// Separator for nested struct fields in flag names (e.g. `"."` → `--database.host`).
    pub separator: String,
}
