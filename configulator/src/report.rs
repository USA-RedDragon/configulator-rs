use std::collections::BTreeMap;
use std::fmt;

/// The configuration layer a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// A `default` attribute (including element defaults inside collections).
    Default,
    /// The config file.
    File,
    /// An environment variable.
    Env,
    /// A CLI flag.
    Cli,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Layer::Default => "default",
            Layer::File => "file",
            Layer::Env => "env",
            Layer::Cli => "cli",
        })
    }
}

/// Where one field's final value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The layer that set the value.
    pub layer: Layer,
    /// Layer-specific detail: the file path, the env var name, or the flag.
    pub detail: String,
}

/// Per-field origin report: which layer set each field, and from where.
///
/// Paths are dotted config names (`http.host`), with list indices as
/// `servers[0].addr` and map keys as `labels.team` (quoted when the key
/// contains `.` or `[`). A field never written by any layer has no entry.
#[derive(Debug, Default, Clone)]
pub struct Report {
    origins: BTreeMap<String, Origin>,
    file: Option<String>,
}

impl Report {
    /// The origin of the value at `path`, if any layer set it.
    pub fn origin(&self, path: &str) -> Option<&Origin> {
        self.origins.get(path)
    }

    /// Every recorded path, sorted.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.origins.keys().map(String::as_str)
    }

    /// The config file that was loaded, even if it set nothing or every
    /// value in it was overridden.
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    #[doc(hidden)]
    pub fn __set_file(&mut self, path: &str) {
        self.file = Some(path.to_string());
    }

    #[doc(hidden)]
    pub fn __set(&mut self, path: &str, layer: Layer, detail: String) {
        self.origins
            .insert(path.to_string(), Origin { layer, detail });
    }
}
