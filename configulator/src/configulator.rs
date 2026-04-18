use std::marker::PhantomData;
#[cfg(feature = "file")]
use std::path::PathBuf;

#[cfg(feature = "cli")]
use crate::cli;
use crate::defaults;
#[cfg(feature = "env")]
use crate::environment;
use crate::error::ConfigulatorError;
#[cfg(feature = "file")]
use crate::file;
#[cfg(feature = "file")]
use crate::options::FileOptions;
#[cfg(feature = "env")]
use crate::options::EnvironmentVariableOptions;
#[cfg(feature = "cli")]
use crate::options::CLIFlagOptions;
use crate::value_map::{merge_value_maps, ValueMap};
#[cfg(all(feature = "cli", feature = "file"))]
use crate::value_map::ConfigValue;
use crate::{ConfigFields, FromValueMap, Validate};

/// Builder for loading configuration from multiple sources into a typed struct.
///
/// Sources are applied in precedence order: defaults < file < env vars < CLI flags.
///
/// Available sources depend on enabled feature flags (`file`, `env`, `cli`).
pub struct Configulator<C> {
    #[cfg(feature = "file")]
    file_opts: Option<FileOptions>,
    #[cfg(feature = "env")]
    env_opts: Option<EnvironmentVariableOptions>,
    #[cfg(feature = "cli")]
    cli_opts: Option<CLIFlagOptions>,
    #[cfg(feature = "testing")]
    cli_args: Option<Vec<String>>,
    #[cfg(feature = "cli")]
    cli_command: Option<clap::Command>,
    _marker: PhantomData<C>,
}

impl<C: ConfigFields + FromValueMap + Default> Configulator<C> {
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
            #[cfg(feature = "cli")]
            cli_command: None,
            _marker: PhantomData,
        }
    }

    /// Enable loading from a config file.
    ///
    /// The [`FileOptions`] must include a [`FileLoader`](crate::FileLoader)
    /// implementation that parses the file contents. Use [`serde_loader`](crate::serde_loader)
    /// for any serde-compatible format.
    #[cfg(feature = "file")]
    #[must_use]
    pub fn with_file(mut self, opts: FileOptions) -> Self {
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

    /// Override CLI args (for testing). If not called, uses `std::env::args()`.
    #[cfg(feature = "testing")]
    #[must_use]
    pub fn with_cli_args(mut self, args: Vec<String>) -> Self {
        self.cli_args = Some(args);
        self
    }

    /// Provide a custom `clap::Command` as the base for CLI flag parsing.
    ///
    /// Configulator will add its own config arguments to this command,
    /// allowing you to define additional flags, set the app name/version,
    /// or customise help output.
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
        let fields = C::configulator_fields();
        let mut merged = ValueMap::new();

        // 1. Defaults (lowest precedence)
        let defaults = defaults::load_defaults(&fields);
        merge_value_maps(&mut merged, &defaults);

        // 2. Parse CLI once (if configured) to get both config path and values
        #[cfg(feature = "cli")]
        let cli_values = if let Some(ref opts) = self.cli_opts {
            let args = self.get_cli_args();
            let has_file = {
                #[cfg(feature = "file")]
                { self.file_opts.is_some() }
                #[cfg(not(feature = "file"))]
                { false }
            };
            Some(cli::load_from_cli(opts, &fields, &args, has_file, self.cli_command.clone())?)
        } else {
            None
        };
        #[cfg(not(feature = "cli"))]
        let cli_values = None::<ValueMap>;

        // 3. File
        #[cfg(feature = "file")]
        {
            let mut file_opts = self.file_opts;
            // Prepend --config path if CLI provided one and .with_file() was called
            #[cfg(feature = "cli")]
            if let Some(ref mut opts) = file_opts {
                if let Some(ref cli_vals) = cli_values {
                    if let Some(ConfigValue::Scalar(path)) = cli_vals.get("__config_file__") {
                        opts.paths.insert(0, PathBuf::from(path));
                    }
                }
            }
            if let Some(ref opts) = file_opts {
                let file_values = file::load_from_file(opts)?;
                merge_value_maps(&mut merged, &file_values);
            }
        }

        // 4. Environment variables
        #[cfg(feature = "env")]
        if let Some(ref opts) = self.env_opts {
            let env_values = environment::load_from_env(opts, &fields);
            merge_value_maps(&mut merged, &env_values);
        }

        // 5. CLI flags (highest precedence)
        #[cfg(feature = "cli")]
        if let Some(mut cli_values) = cli_values {
            cli_values.remove("__config_file__");
            merge_value_maps(&mut merged, &cli_values);
        }

        C::from_value_map(&merged)
    }

    /// Get the default config (all defaults applied, no other sources).
    ///
    /// Validation is **not** performed. Call
    /// [`Validate::validate`](crate::Validate::validate) on the result if needed.
    pub fn defaults_only() -> Result<C, ConfigulatorError> {
        let fields = C::configulator_fields();
        let defaults = defaults::load_defaults(&fields);
        C::from_value_map(&defaults)
    }

    #[cfg(feature = "cli")]
    fn get_cli_args(&self) -> Vec<String> {
        #[cfg(feature = "testing")]
        if let Some(args) = &self.cli_args {
            return args.clone();
        }
        std::env::args().skip(1).collect()
    }
}

impl<C: ConfigFields + FromValueMap + Default> Default for Configulator<C> {
    fn default() -> Self {
        Self::new()
    }
}
