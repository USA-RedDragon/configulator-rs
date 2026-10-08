#[cfg(any(feature = "env", feature = "cli", feature = "testing"))]
use std::collections::HashMap;
use std::marker::PhantomData;

#[cfg(feature = "cli")]
use crate::cli;
use crate::error::ConfigulatorError;
use crate::field_info::{FieldInfo, FieldType};
#[cfg(feature = "file")]
use crate::file;
#[cfg(feature = "cli")]
use crate::options::CLIFlagOptions;
#[cfg(feature = "env")]
use crate::options::EnvironmentVariableOptions;
#[cfg(feature = "file")]
use crate::options::FileOptions;
use crate::report::{Layer, Report};
use crate::shadow::HasShadow;
use crate::Validate;

#[cfg(feature = "env")]
type EnvGetter<'a> = Box<dyn Fn(&str) -> Option<String> + 'a>;

/// Builder for loading configuration from multiple sources into a typed
/// struct.
///
/// Sources are applied in precedence order:
/// defaults < file < env vars < CLI flags.
///
/// Available sources depend on enabled feature flags (`file`, `env`, `cli`).
pub struct Configulator<C: HasShadow> {
    #[cfg(feature = "file")]
    file_opts: Option<FileOptions<C>>,
    #[cfg(feature = "env")]
    env_opts: Option<EnvironmentVariableOptions>,
    #[cfg(feature = "cli")]
    cli_opts: Option<CLIFlagOptions>,
    #[cfg(feature = "testing")]
    cli_args: Option<Vec<String>>,
    #[cfg(feature = "testing")]
    env_vars: Option<HashMap<String, String>>,
    #[cfg(feature = "cli")]
    cli_command: Option<clap::Command>,
    array_separator: String,
    _marker: PhantomData<C>,
}

impl<C: HasShadow> Configulator<C> {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "file")]
            file_opts: None,
            #[cfg(feature = "env")]
            env_opts: None,
            #[cfg(feature = "cli")]
            cli_opts: None,
            #[cfg(feature = "testing")]
            cli_args: None,
            #[cfg(feature = "testing")]
            env_vars: None,
            #[cfg(feature = "cli")]
            cli_command: None,
            array_separator: ",".to_string(),
            _marker: PhantomData,
        }
    }

    /// Enable loading from a config file.
    ///
    /// The [`FileOptions`] must include a [`FileLoader`](crate::FileLoader)
    /// implementation that parses the file contents. Use
    /// [`serde_loader`](crate::serde_loader) for any serde-compatible format.
    #[cfg(feature = "file")]
    #[must_use]
    pub fn with_file(mut self, opts: FileOptions<C>) -> Self {
        self.file_opts = Some(opts);
        self
    }

    /// Enable loading from environment variables.
    #[cfg(feature = "env")]
    #[must_use]
    pub fn with_environment_variables(mut self, opts: EnvironmentVariableOptions) -> Self {
        self.env_opts = Some(opts);
        self
    }

    /// Enable loading from CLI flags.
    #[cfg(feature = "cli")]
    #[must_use]
    pub fn with_cli_flags(mut self, opts: CLIFlagOptions) -> Self {
        self.cli_opts = Some(opts);
        self
    }

    /// Set the separator used to split list values in env vars and
    /// `default` attributes (`,` by default). CLI list flags are repeated
    /// instead and are not affected.
    #[must_use]
    pub fn with_array_separator(mut self, sep: impl Into<String>) -> Self {
        self.array_separator = sep.into();
        self
    }

    /// Override CLI args (for testing). If not called, uses `std::env::args()`.
    #[cfg(feature = "testing")]
    #[must_use]
    pub fn with_cli_args(mut self, args: Vec<String>) -> Self {
        self.cli_args = Some(args);
        self
    }

    /// Override the environment (for testing). If not called, uses
    /// `std::env::var`.
    #[cfg(feature = "testing")]
    #[must_use]
    pub fn with_env_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.env_vars = Some(vars);
        self
    }

    /// Provide a custom `clap::Command` as the base for CLI flag parsing.
    ///
    /// Configulator will add its own config arguments to this command,
    /// allowing you to define additional flags, set the app name/version,
    /// or customise help output.
    ///
    /// Custom args must not share a long name or shorthand with a config
    /// flag. Loading returns [`ConfigulatorError::FlagConflict`] if one does.
    #[cfg(feature = "cli")]
    #[must_use]
    pub fn with_cli_command(mut self, cmd: clap::Command) -> Self {
        self.cli_command = Some(cmd);
        self
    }

    /// Load configuration, applying validation.
    pub fn load(self) -> Result<C, ConfigulatorError>
    where
        C: Validate,
    {
        let config = self.load_without_validation()?;
        config
            .validate()
            .map_err(ConfigulatorError::ValidationError)?;
        Ok(config)
    }

    /// Load configuration without running validation.
    pub fn load_without_validation(self) -> Result<C, ConfigulatorError> {
        self.load_impl().map(|(c, _)| c)
    }

    /// Load configuration (with validation) alongside the per-field origin
    /// [`Report`].
    pub fn load_with_report(self) -> Result<(C, Report), ConfigulatorError>
    where
        C: Validate,
    {
        let (config, report) = self.load_impl()?;
        config
            .validate()
            .map_err(ConfigulatorError::ValidationError)?;
        Ok((config, report))
    }

    fn load_impl(self) -> Result<(C, Report), ConfigulatorError> {
        let mut report = Report::default();
        let mut acc = C::Shadow::default();

        let defaults = C::shadow_defaults(&self.array_separator)?;
        C::overlay(
            &mut acc,
            defaults,
            "",
            Layer::Default,
            &|_| "default tag".to_string(),
            &mut report,
        );

        // CLI is parsed first to find --config, but its values merge last.
        #[cfg(feature = "cli")]
        #[cfg_attr(not(feature = "file"), allow(unused_variables))]
        let (cli_shadow, cli_config_path, cli_sep) = if let Some(ref opts) = self.cli_opts {
            let args = self.get_cli_args();
            let config_flag = self.config_flag_spec();
            let cmd = cli::build_command(
                self.cli_command.clone(),
                &C::fields(),
                &opts.separator,
                config_flag.clone(),
                &binary_name(),
            )?;
            let (matches, config_path) =
                cli::parse(cmd, &args, config_flag.as_ref().map(|(n, _)| n.as_str()))?;
            let shadow = C::from_cli(&matches, "", &opts.separator)?;
            (Some(shadow), config_path, opts.separator.clone())
        } else {
            (None, None, String::new())
        };

        #[cfg(feature = "file")]
        if let Some(ref opts) = self.file_opts {
            #[cfg(feature = "cli")]
            let cli_explicit = cli_config_path.as_deref().map(std::path::Path::new);
            #[cfg(not(feature = "cli"))]
            let cli_explicit = None;
            if let Some((shadow, path)) = file::load::<C>(opts, cli_explicit)? {
                report.__set_file(&path);
                C::overlay(
                    &mut acc,
                    shadow,
                    "",
                    Layer::File,
                    &|_| path.clone(),
                    &mut report,
                );
            }
        }

        #[cfg(feature = "env")]
        if let Some(ref opts) = self.env_opts {
            validate_env_options(opts)?;
            let sep = if opts.separator.is_empty() {
                "__".to_string()
            } else {
                opts.separator.clone()
            };
            let getter = self.env_getter();
            let shadow = C::from_env(getter.as_ref(), &opts.prefix, &sep, &self.array_separator)?;
            let names = env_name_map(&C::fields(), &opts.prefix, &sep);
            C::overlay(
                &mut acc,
                shadow,
                "",
                Layer::Env,
                &|p| names.get(p).cloned().unwrap_or_else(|| p.to_string()),
                &mut report,
            );
        }

        #[cfg(feature = "cli")]
        if let Some(shadow) = cli_shadow {
            let names = flag_name_map(&C::fields(), &cli_sep);
            C::overlay(
                &mut acc,
                shadow,
                "",
                Layer::Cli,
                &|p| names.get(p).cloned().unwrap_or_else(|| format!("--{p}")),
                &mut report,
            );
        }

        let ctx = crate::shadow::__private::BuildCtx {
            array_sep: self.array_separator.clone(),
            check_required: true,
        };
        let config = C::build(acc, "", &ctx, &mut report)?;
        check_required(&C::fields(), "", &report)?;
        Ok((config, report))
    }

    /// Render a JSON Schema (draft-07 subset) for the config type:
    /// types, descriptions, defaults, `required`, and
    /// `additionalProperties: false` unless the struct has
    /// `allow_unknown_fields`. Same output as the Go generator's `-schema`
    /// flag.
    pub fn json_schema() -> String {
        crate::schema::json_schema(&C::fields(), C::ALLOW_UNKNOWN_FIELDS)
    }

    /// Render a commented YAML sample config: every key at its default,
    /// descriptions as comments, secrets redacted. Same output as the Go
    /// generator's `-sample` flag.
    pub fn sample_config() -> String {
        crate::schema::sample_config(&C::fields())
    }

    /// The config with only defaults applied, using this builder's array
    /// separator. Neither `required` nor [`Validate`] is checked, like Go's
    /// `Default()`.
    pub fn defaults(&self) -> Result<C, ConfigulatorError> {
        defaults_with::<C>(&self.array_separator)
    }

    /// [`defaults`](Self::defaults) with the default `,` array separator.
    pub fn defaults_only() -> Result<C, ConfigulatorError> {
        defaults_with::<C>(",")
    }

    #[cfg(feature = "cli")]
    fn get_cli_args(&self) -> Vec<String> {
        #[cfg(feature = "testing")]
        if let Some(args) = &self.cli_args {
            return args.clone();
        }
        std::env::args().skip(1).collect()
    }

    /// The --config flag (name, short), registered only when the file
    /// layer is configured.
    #[cfg(feature = "cli")]
    fn config_flag_spec(&self) -> Option<(String, char)> {
        #[cfg(feature = "file")]
        {
            self.file_opts.as_ref().map(|fo| {
                (
                    fo.flag_name.clone().unwrap_or_else(|| "config".to_string()),
                    fo.shorthand.unwrap_or('c'),
                )
            })
        }
        #[cfg(not(feature = "file"))]
        {
            None
        }
    }

    #[cfg(feature = "env")]
    fn env_getter(&self) -> EnvGetter<'_> {
        #[cfg(feature = "testing")]
        if let Some(vars) = &self.env_vars {
            return Box::new(move |k: &str| vars.get(k).cloned());
        }
        Box::new(|k: &str| std::env::var(k).ok())
    }
}

impl<C: HasShadow> Default for Configulator<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// The usage-line name when no custom command is provided: the binary
/// name, falling back to "app".
#[cfg(feature = "cli")]
fn binary_name() -> String {
    std::env::args()
        .next()
        .as_deref()
        .map(std::path::Path::new)
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".to_string())
}

#[cfg(feature = "env")]
fn validate_env_options(opts: &EnvironmentVariableOptions) -> Result<(), ConfigulatorError> {
    if opts.prefix != opts.prefix.to_uppercase() {
        return Err(ConfigulatorError::BadEnvOptions(format!(
            "prefix {:?} must be uppercase",
            opts.prefix
        )));
    }
    if opts.separator.contains('-') {
        return Err(ConfigulatorError::BadEnvOptions(format!(
            "separator {:?} must not contain '-'",
            opts.separator
        )));
    }
    Ok(())
}

/// Map dotted config paths to their env var names, honoring `env = "..."`
/// overrides, for origin details.
#[cfg(feature = "env")]
fn env_name_map(fields: &[FieldInfo], prefix: &str, sep: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    fn walk(
        fields: &[FieldInfo],
        var_prefix: &str,
        path_prefix: &str,
        sep: &str,
        out: &mut HashMap<String, String>,
    ) {
        for f in fields {
            if f.skip_env {
                continue;
            }
            let var = format!("{var_prefix}{}", f.env_segment);
            let path = crate::shadow::__private::join(path_prefix, f.config_name);
            match &f.field_type {
                FieldType::Struct(sub) => {
                    walk(sub, &format!("{var}{sep}"), &path, sep, out);
                }
                FieldType::Map | FieldType::StructList(_) | FieldType::StructMap(_) => {}
                _ => {
                    out.insert(path, var);
                }
            }
        }
    }
    walk(fields, prefix, "", sep, &mut out);
    out
}

/// Map dotted config paths to `--flag` spellings, honoring `flag = "..."`
/// overrides, for origin details.
#[cfg(feature = "cli")]
fn flag_name_map(fields: &[FieldInfo], sep: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    fn walk(
        fields: &[FieldInfo],
        flag_prefix: &str,
        path_prefix: &str,
        sep: &str,
        out: &mut HashMap<String, String>,
    ) {
        for f in fields {
            if f.skip_cli {
                continue;
            }
            let flag = if flag_prefix.is_empty() {
                f.flag_segment.to_string()
            } else {
                format!("{flag_prefix}{sep}{}", f.flag_segment)
            };
            let path = crate::shadow::__private::join(path_prefix, f.config_name);
            match &f.field_type {
                FieldType::Struct(sub) => walk(sub, &flag, &path, sep, out),
                FieldType::Map | FieldType::StructList(_) | FieldType::StructMap(_) => {}
                _ => {
                    out.insert(path, format!("--{flag}"));
                }
            }
        }
    }
    walk(fields, "", "", sep, &mut out);
    out
}

/// Check `required` fields against the report: some layer (including a
/// `default` attribute) must have set each one. A nested struct counts as
/// set when any field under it is set, and the fields of an unset
/// `Option` struct are not checked. Required leaves inside collection
/// elements are checked per element when each element is built.
fn check_required(
    fields: &[FieldInfo],
    path_prefix: &str,
    report: &Report,
) -> Result<(), ConfigulatorError> {
    for f in fields {
        let path = crate::shadow::__private::join(path_prefix, f.config_name);
        let set = || {
            report.origin(&path).is_some() || {
                let (dot, bracket) = (format!("{path}."), format!("{path}["));
                report
                    .paths()
                    .any(|p| p.starts_with(&dot) || p.starts_with(&bracket))
            }
        };
        if f.required && !set() {
            return Err(ConfigulatorError::Required { path });
        }
        if let FieldType::Struct(sub) = &f.field_type {
            if !f.optional || set() {
                check_required(sub, &path, report)?;
            }
        }
    }
    Ok(())
}

fn defaults_with<C: HasShadow>(array_sep: &str) -> Result<C, ConfigulatorError> {
    let mut report = Report::default();
    let mut acc = C::Shadow::default();
    let defaults = C::shadow_defaults(array_sep)?;
    C::overlay(
        &mut acc,
        defaults,
        "",
        Layer::Default,
        &|_| "default tag".to_string(),
        &mut report,
    );
    let ctx = crate::shadow::__private::BuildCtx {
        array_sep: array_sep.to_string(),
        check_required: false,
    };
    C::build(acc, "", &ctx, &mut report)
}
