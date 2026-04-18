use crate::error::ConfigulatorError;
use crate::options::FileOptions;
use crate::value_map::{ConfigValue, ValueMap};

/// Trait for parsing file contents into configuration values.
///
/// Implement this for any configuration format you want to support
/// (YAML, TOML, JSON, etc.).
///
/// For formats supported by [serde](https://serde.rs), use [`serde_loader`]
/// instead of implementing this trait manually.
pub trait FileLoader: Send + Sync {
    /// Parse the raw file contents into configuration values.
    fn load(&self, contents: &str) -> Result<ValueMap, ConfigulatorError>;
}

/// A [`FileLoader`] backed by any serde-compatible deserializer.
///
/// Created via [`serde_loader`]. Accepts a closure that deserializes
/// a `&str` into a [`ConfigValue`].
pub struct SerdeLoader<F>(F);

impl<F, E> FileLoader for SerdeLoader<F>
where
    F: Fn(&str) -> Result<ConfigValue, E> + Send + Sync,
    E: std::fmt::Display,
{
    fn load(&self, contents: &str) -> Result<ValueMap, ConfigulatorError> {
        let value = (self.0)(contents)
            .map_err(|e| ConfigulatorError::FileError(e.to_string()))?;
        match value {
            ConfigValue::Nested(map) => Ok(map),
            _ => Err(ConfigulatorError::FileError(
                "config file root must be a mapping/table".into(),
            )),
        }
    }
}

/// Create a [`FileLoader`] from any serde-compatible deserializer.
///
/// This is the easiest way to support a file format. Pass a closure
/// that calls the format crate's `from_str` function:
///
/// ```rust,no_run
/// use configulator::{serde_loader, FileOptions};
///
/// let opts = FileOptions {
///     paths: vec!["config.yaml".into()],
///     error_if_not_found: false,
///     // YAML
///     loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
/// };
/// ```
///
/// Works with any format: `serde_json::from_str`, `toml::from_str`, etc.
pub fn serde_loader<F, E>(f: F) -> Box<dyn FileLoader>
where
    F: Fn(&str) -> Result<ConfigValue, E> + Send + Sync + 'static,
    E: std::fmt::Display + 'static,
{
    Box::new(SerdeLoader(f))
}

/// Load configuration from the first file found in the given paths,
/// using the loader specified in [`FileOptions`].
pub fn load_from_file(opts: &FileOptions) -> Result<ValueMap, ConfigulatorError> {
    let mut contents = None;
    for path in &opts.paths {
        match std::fs::read_to_string(path) {
            Ok(data) => {
                contents = Some(data);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(ConfigulatorError::FileError(format!(
                    "{}: {e}",
                    path.display()
                )));
            }
        }
    }

    let contents = match contents {
        Some(c) => c,
        None => {
            if opts.error_if_not_found {
                return Err(ConfigulatorError::FileNotFound);
            }
            return Ok(ValueMap::new());
        }
    };

    opts.loader.load(&contents)
}
