use crate::error::ConfigulatorError;
use crate::field_info::{FieldInfo, FieldType};
use crate::options::CLIFlagOptions;
use crate::value_map::{ConfigValue, ValueMap};

/// Load configuration values from CLI arguments using clap.
///
/// Builds a clap `Command` dynamically from the field metadata and parses
/// the provided args. Flag names for nested structs use the configured separator
/// (e.g. `--database.host` with separator `.`).
pub fn load_from_cli(
    opts: &CLIFlagOptions,
    fields: &[FieldInfo],
    args: &[String],
    config_file_flag: bool,
    base_cmd: Option<clap::Command>,
) -> Result<ValueMap, ConfigulatorError> {
    let mut cmd = base_cmd
        .unwrap_or_else(|| clap::Command::new("app"))
        .no_binary_name(true)
        .disable_version_flag(true);

    if config_file_flag {
        cmd = cmd.arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .help("Path to configuration file")
                .num_args(1),
        );
    }

    cmd = register_args(cmd, fields, "", &opts.separator);

    let matches = cmd
        .try_get_matches_from(args)
        .map_err(|e| ConfigulatorError::CLIError(e.to_string()))?;

    let mut map = extract_values(&matches, fields, "", &opts.separator);

    // If a config file path was given via --config / -c, store it specially
    if config_file_flag {
        if let Some(path) = matches.get_one::<String>("config") {
            map.insert(
                "__config_file__".to_string(),
                ConfigValue::Scalar(path.clone()),
            );
        }
    }

    Ok(map)
}

fn register_args(
    mut cmd: clap::Command,
    fields: &[FieldInfo],
    prefix: &str,
    separator: &str,
) -> clap::Command {
    for field in fields {
        let flag_name = if prefix.is_empty() {
            field.config_name.to_string()
        } else {
            format!("{prefix}{separator}{}", field.config_name)
        };

        match &field.field_type {
            FieldType::Struct(sub_fields) => {
                cmd = register_args(cmd, sub_fields, &flag_name, separator);
            }
            FieldType::Bool => {
                let mut arg = clap::Arg::new(flag_name.clone())
                    .long(flag_name)
                    .num_args(0..=1)
                    .default_missing_value("true")
                    .require_equals(false);
                if let Some(desc) = field.description {
                    arg = arg.help(desc);
                }
                cmd = cmd.arg(arg);
            }
            FieldType::Scalar => {
                let mut arg = clap::Arg::new(flag_name.clone())
                    .long(flag_name)
                    .num_args(1);
                if let Some(desc) = field.description {
                    arg = arg.help(desc);
                }
                cmd = cmd.arg(arg);
            }
            FieldType::List => {
                let mut arg = clap::Arg::new(flag_name.clone())
                    .long(flag_name)
                    .num_args(1)
                    .action(clap::ArgAction::Append);
                if let Some(desc) = field.description {
                    arg = arg.help(desc);
                }
                cmd = cmd.arg(arg);
            }
        }
    }
    cmd
}

fn extract_values(
    matches: &clap::ArgMatches,
    fields: &[FieldInfo],
    prefix: &str,
    separator: &str,
) -> ValueMap {
    let mut map = ValueMap::new();

    for field in fields {
        let flag_name = if prefix.is_empty() {
            field.config_name.to_string()
        } else {
            format!("{prefix}{separator}{}", field.config_name)
        };

        match &field.field_type {
            FieldType::Struct(sub_fields) => {
                let nested = extract_values(matches, sub_fields, &flag_name, separator);
                if !nested.is_empty() {
                    map.insert(field.config_name.to_string(), ConfigValue::Nested(nested));
                }
            }
            FieldType::Bool => {
                if matches.contains_id(&flag_name) && matches.value_source(&flag_name)
                    == Some(clap::parser::ValueSource::CommandLine)
                {
                    let val = matches
                        .get_one::<String>(&flag_name)
                        .map(|s| s.as_str())
                        .unwrap_or("true");
                    map.insert(field.config_name.to_string(), ConfigValue::Scalar(val.to_string()));
                }
            }
            FieldType::Scalar => {
                if let Some(val) = matches.get_one::<String>(&flag_name) {
                    if matches.value_source(&flag_name)
                        == Some(clap::parser::ValueSource::CommandLine)
                    {
                        map.insert(
                            field.config_name.to_string(),
                            ConfigValue::Scalar(val.clone()),
                        );
                    }
                }
            }
            FieldType::List => {
                if let Some(vals) = matches.get_many::<String>(&flag_name) {
                    if matches.value_source(&flag_name)
                        == Some(clap::parser::ValueSource::CommandLine)
                    {
                        let items: Vec<String> = vals.cloned().collect();
                        if !items.is_empty() {
                            map.insert(field.config_name.to_string(), ConfigValue::List(items));
                        }
                    }
                }
            }
        }
    }

    map
}
