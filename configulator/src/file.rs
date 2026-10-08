use std::path::Path;

use crate::error::ConfigulatorError;
use crate::options::FileOptions;
use crate::shadow::HasShadow;

/// Parses file contents into a config's shadow type.
///
/// Implement this for full control over a format; for anything
/// serde-compatible use [`serde_loader`] instead.
pub trait FileLoader<C: HasShadow>: Send + Sync {
    /// Parse the raw file contents into the config's shadow.
    fn load(&self, contents: &str) -> Result<C::Shadow, ConfigulatorError>;
}

struct SerdeLoader<F>(F);

impl<C, F, E> FileLoader<C> for SerdeLoader<F>
where
    C: HasShadow,
    F: Fn(&str) -> Result<C::Shadow, E> + Send + Sync,
    E: std::fmt::Display,
{
    fn load(&self, contents: &str) -> Result<C::Shadow, ConfigulatorError> {
        (self.0)(contents).map_err(|e| ConfigulatorError::DecodeError {
            path: std::path::PathBuf::new(),
            message: e.to_string(),
        })
    }
}

/// Create a [`FileLoader`] from any serde-compatible deserializer.
///
/// Pass a closure that calls the format crate's `from_str`:
///
/// ```rust,ignore
/// loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
/// loader: serde_loader(|s| toml::from_str(s)),
/// loader: serde_loader(|s| serde_json::from_str(s)),
/// ```
pub fn serde_loader<C, F, E>(f: F) -> Box<dyn FileLoader<C>>
where
    C: HasShadow,
    F: Fn(&str) -> Result<C::Shadow, E> + Send + Sync + 'static,
    E: std::fmt::Display + 'static,
{
    Box::new(SerdeLoader(f))
}

/// Load the file layer. Returns the parsed shadow and the display path of
/// the file that was read, or `None` when no file matched softly.
///
/// `cli_explicit` (from `--config`) wins over `opts.explicit`; either must
/// exist and parse, with no fallback to the search paths.
pub(crate) fn load<C: HasShadow>(
    opts: &FileOptions<C>,
    cli_explicit: Option<&Path>,
) -> Result<Option<(C::Shadow, String)>, ConfigulatorError> {
    let explicit = cli_explicit.or(opts.explicit.as_deref());
    if let Some(path) = explicit {
        let contents =
            std::fs::read_to_string(path).map_err(|e| ConfigulatorError::ExplicitFileError {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?;
        return Ok(Some((
            parse(opts, path, &contents)?,
            path.display().to_string(),
        )));
    }

    for path in &opts.paths {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                return Ok(Some((
                    parse(opts, path, &contents)?,
                    path.display().to_string(),
                )));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(ConfigulatorError::SearchPathUnreadable {
                    path: path.clone(),
                    message: e.to_string(),
                });
            }
        }
    }

    if opts.error_if_not_found {
        return Err(ConfigulatorError::FileNotFound {
            searched: opts.paths.clone(),
        });
    }
    Ok(None)
}

fn parse<C: HasShadow>(
    opts: &FileOptions<C>,
    path: &Path,
    contents: &str,
) -> Result<C::Shadow, ConfigulatorError> {
    if contents.is_empty() {
        return Ok(C::Shadow::default());
    }
    crate::shadow::__private::de_reset();
    opts.loader.load(contents).map_err(|e| match e {
        ConfigulatorError::DecodeError { message, .. } => ConfigulatorError::DecodeError {
            path: path.to_path_buf(),
            message: tidy_decode_message(message),
        },
        other => other,
    })
}

fn tidy_decode_message(message: String) -> String {
    use crate::shadow::__private::{de_message, de_pathed, unknown_field};
    if !de_pathed() {
        if let Some(key) = unknown_field(&message) {
            return format!("{key}: {message}");
        }
        return message;
    }
    let ours = de_message();
    if ours.contains(" at line ") {
        return ours;
    }
    match message.rfind(" at line ") {
        Some(i) => format!("{ours}{}", &message[i..]),
        None => ours,
    }
}
