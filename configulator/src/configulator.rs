use std::marker::PhantomData;
use std::path::PathBuf;

use crate::cli;
use crate::defaults;
use crate::environment;
use crate::error::ConfigulatorError;
use crate::file;
use crate::options::{CLIFlagOptions, EnvironmentVariableOptions, FileOptions};
use crate::value_map::{merge_value_maps, ConfigValue, ValueMap};
use crate::{ConfigFields, FromValueMap, Validate};

/// Builder for loading configuration from multiple sources into a typed struct.
///
/// Sources are applied in precedence order: defaults < file < env vars < CLI flags.
pub struct Configulator<C> {
    file_opts: Option<FileOptions>,
    env_opts: Option<EnvironmentVariableOptions>,
    cli_opts: Option<CLIFlagOptions>,
    cli_args: Option<Vec<String>>,
    cli_command: Option<clap::Command>,
    _marker: PhantomData<C>,
}

impl<C: ConfigFields + FromValueMap + Default> Configulator<C> {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            file_opts: None,
            env_opts: None,
            cli_opts: None,
            cli_args: None,
            cli_command: None,
            _marker: PhantomData,
        }
    }

    /// Enable loading from a YAML config file.
    #[must_use]
    pub fn with_file(mut self, opts: FileOptions) -> Self {
        self.file_opts = Some(opts);
        self
    }

    /// Enable loading from environment variables.
    #[must_use]
    pub fn with_environment_variables(mut self, opts: EnvironmentVariableOptions) -> Self {
        self.env_opts = Some(opts);
        self
    }

    /// Enable loading from CLI flags.
    #[must_use]
    pub fn with_cli_flags(mut self, opts: CLIFlagOptions) -> Self {
        self.cli_opts = Some(opts);
        self
    }

    /// Override CLI args (for testing). If not called, uses `std::env::args()`.
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
        let cli_values = if let Some(ref opts) = self.cli_opts {
            let args = self.get_cli_args();
            let has_file = self.file_opts.is_some();
            Some(cli::load_from_cli(opts, &fields, &args, has_file, self.cli_command.clone())?)
        } else {
            None
        };

        // 3. File — prepend --config path if CLI provided one
        let mut file_opts = self.file_opts;
        if let Some(ref cli_vals) = cli_values {
            if let Some(ConfigValue::Scalar(path)) = cli_vals.get("__config_file__") {
                let opts = file_opts.get_or_insert_with(|| FileOptions {
                    paths: Vec::new(),
                    error_if_not_found: false,
                });
                opts.paths.insert(0, PathBuf::from(path));
            }
        }
        if let Some(ref opts) = file_opts {
            let file_values = file::load_from_file(opts)?;
            merge_value_maps(&mut merged, &file_values);
        }

        // 4. Environment variables
        if let Some(ref opts) = self.env_opts {
            let env_values = environment::load_from_env(opts, &fields);
            merge_value_maps(&mut merged, &env_values);
        }

        // 5. CLI flags (highest precedence)
        if let Some(mut cli_values) = cli_values {
            cli_values.remove("__config_file__");
            merge_value_maps(&mut merged, &cli_values);
        }

        C::from_value_map(&merged)
    }

    /// Get the default config (all defaults applied, no other sources).
    pub fn defaults_only() -> Result<C, ConfigulatorError> {
        let fields = C::configulator_fields();
        let defaults = defaults::load_defaults(&fields);
        C::from_value_map(&defaults)
    }

    fn get_cli_args(&self) -> Vec<String> {
        match &self.cli_args {
            Some(args) => args.clone(),
            None => std::env::args().skip(1).collect(),
        }
    }
}

impl<C: ConfigFields + FromValueMap + Default> Default for Configulator<C> {
    fn default() -> Self {
        Self::new()
    }
}
