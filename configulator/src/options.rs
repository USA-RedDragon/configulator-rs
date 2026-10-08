#[cfg(feature = "file")]
use std::path::PathBuf;

#[cfg(feature = "file")]
use crate::file::FileLoader;
#[cfg(feature = "file")]
use crate::shadow::HasShadow;

/// Options for loading configuration from a file.
///
/// Users must supply a [`FileLoader`] implementation that knows how to
/// parse the file contents into the config's shadow type.
/// Use [`serde_loader`](crate::serde_loader) for any serde-compatible format.
///
/// New fields may be added in minor releases; construct with a literal
/// spread over [`FileOptions::new`] to stay source-compatible:
///
/// ```rust,ignore
/// FileOptions {
///     paths: vec!["config.yaml".into()],
///     ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
/// }
/// ```
#[cfg(feature = "file")]
pub struct FileOptions<C: HasShadow> {
    /// Search paths, tried in order; the first readable file wins. A
    /// missing search path is a soft miss; any other read failure is an
    /// error.
    pub paths: Vec<PathBuf>,
    /// If set, this file must exist and parse; `paths` is not searched.
    /// Same for a path given via `--config`.
    pub explicit: Option<PathBuf>,
    /// If true, "no search path matched" is an error. Does not affect
    /// `explicit`, which is always required to exist.
    pub error_if_not_found: bool,
    /// The loader that parses file contents into the config's shadow.
    pub loader: Box<dyn FileLoader<C>>,
    /// The config-file flag name; `None` means `config`.
    pub flag_name: Option<String>,
    /// The config-file flag shorthand; `None` means `c`.
    pub shorthand: Option<char>,
}

#[cfg(feature = "file")]
impl<C: HasShadow> FileOptions<C> {
    /// A `FileOptions` with the given loader and every other field at its
    /// default. Useful as a `..` spread base for literal construction.
    pub fn new(loader: Box<dyn FileLoader<C>>) -> Self {
        Self {
            paths: Vec::new(),
            explicit: None,
            error_if_not_found: false,
            loader,
            flag_name: None,
            shorthand: None,
        }
    }
}

#[cfg(feature = "file")]
impl<C: HasShadow> std::fmt::Debug for FileOptions<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileOptions")
            .field("paths", &self.paths)
            .field("explicit", &self.explicit)
            .field("error_if_not_found", &self.error_if_not_found)
            .field("loader", &"<dyn FileLoader>")
            .field("flag_name", &self.flag_name)
            .field("shorthand", &self.shorthand)
            .finish()
    }
}

/// Options for loading configuration from environment variables.
///
/// A variable name is `prefix` + the uppercased config names of the path,
/// joined by `separator`, with `-` folded to `_` in name-derived segments
/// only. The prefix is used verbatim (include a trailing separator if you
/// want one) and must be uppercase; the separator must not contain `-`.
/// Both are validated at load time.
#[cfg(feature = "env")]
#[derive(Debug, Clone)]
pub struct EnvironmentVariableOptions {
    /// Prepended verbatim to every variable name
    /// (e.g. `"MYAPP__"` → `MYAPP__DATABASE__HOST`).
    pub prefix: String,
    /// Joins nested levels, verbatim. Empty means `"__"`.
    pub separator: String,
}

/// Options for loading configuration from CLI flags.
#[cfg(feature = "cli")]
#[derive(Debug, Clone)]
pub struct CLIFlagOptions {
    /// Separator for nested struct fields in flag names
    /// (e.g. `"."` → `--database.host`).
    pub separator: String,
}
