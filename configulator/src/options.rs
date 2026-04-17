use std::path::PathBuf;

/// Options for loading configuration from a YAML file.
#[derive(Debug, Clone)]
pub struct FileOptions {
    /// List of file paths to search. The first one found is used.
    pub paths: Vec<PathBuf>,
    /// If true, return an error if no config file is found.
    pub error_if_not_found: bool,
}

/// Options for loading configuration from environment variables.
#[derive(Debug, Clone)]
pub struct EnvironmentVariableOptions {
    /// Prefix for env vars (e.g. `"APP"` → `APP_PORT`).
    pub prefix: String,
    /// Separator for nested struct fields (e.g. `"__"` → `APP_DATABASE__HOST`).
    pub separator: String,
}

/// Options for loading configuration from CLI flags.
#[derive(Debug, Clone)]
pub struct CLIFlagOptions {
    /// Separator for nested struct fields in flag names (e.g. `"."` → `--database.host`).
    pub separator: String,
}
