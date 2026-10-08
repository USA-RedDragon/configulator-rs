use crate::error::ConfigulatorError;
use crate::field_info::{FieldInfo, FieldType};

/// Build the clap `Command` for a config's fields. Nested structs use the
/// configured separator in flag names (`--database.host`). Collections
/// and `flag = "-"` fields are skipped (SPEC rule 6).
/// The `--config` flag: its name, shorthand, and the default shown in help.
#[derive(Clone)]
pub(crate) struct ConfigFlag {
    pub name: String,
    pub short: char,
    pub default: Option<String>,
}

pub(crate) fn build_command(
    base: Option<clap::Command>,
    fields: &[FieldInfo],
    separator: &str,
    config_flag: Option<ConfigFlag>,
    default_name: &str,
) -> Result<clap::Command, ConfigulatorError> {
    let mut cmd = base
        .unwrap_or_else(|| clap::Command::new(default_name.to_string()))
        .no_binary_name(true);

    if let Some(flag) = config_flag {
        check_free(&cmd, &flag.name, Some(flag.short))?;
        let mut arg = clap::Arg::new(flag.name.clone())
            .short(flag.short)
            .long(flag.name)
            .help("config file")
            .num_args(1);
        if let Some(default) = flag.default {
            arg = arg.default_value(default);
        }
        cmd = cmd.arg(arg);
    }

    register_args(&mut cmd, fields, "", separator)?;
    Ok(cmd)
}

fn check_free(
    cmd: &clap::Command,
    long: &str,
    short: Option<char>,
) -> Result<(), ConfigulatorError> {
    for a in cmd.get_arguments() {
        if a.get_id() == long || a.get_long() == Some(long) {
            return Err(ConfigulatorError::FlagConflict(format!("--{long}")));
        }
        if let Some(c) = short.filter(|c| a.get_short() == Some(*c)) {
            return Err(ConfigulatorError::FlagConflict(format!("-{c}")));
        }
    }
    if !cmd.is_disable_help_flag_set() {
        if long == "help" {
            return Err(ConfigulatorError::FlagConflict("--help".into()));
        }
        if short == Some('h') {
            return Err(ConfigulatorError::FlagConflict("-h".into()));
        }
    }
    if cmd.get_version().is_some() && !cmd.is_disable_version_flag_set() {
        if long == "version" {
            return Err(ConfigulatorError::FlagConflict("--version".into()));
        }
        if short == Some('V') {
            return Err(ConfigulatorError::FlagConflict("-V".into()));
        }
    }
    Ok(())
}

fn register_args(
    cmd: &mut clap::Command,
    fields: &[FieldInfo],
    prefix: &str,
    separator: &str,
) -> Result<(), ConfigulatorError> {
    for field in fields {
        if field.skip_cli {
            continue;
        }
        let flag_name = if prefix.is_empty() {
            field.flag_segment.to_string()
        } else {
            format!("{prefix}{separator}{}", field.flag_segment)
        };

        let arg = match &field.field_type {
            FieldType::Struct(sub) => {
                register_args(cmd, sub, &flag_name, separator)?;
                continue;
            }
            FieldType::Map | FieldType::StructList(_) | FieldType::StructMap(_) => continue,
            FieldType::Bool => clap::Arg::new(flag_name.clone())
                .long(flag_name.clone())
                .num_args(0..=1)
                .default_missing_value("true")
                .require_equals(false),
            FieldType::Scalar => clap::Arg::new(flag_name.clone())
                .long(flag_name.clone())
                .num_args(1),
            FieldType::List => clap::Arg::new(flag_name.clone())
                .long(flag_name.clone())
                .num_args(1)
                .action(clap::ArgAction::Append),
        };
        check_free(cmd, &flag_name, field.short)?;
        let mut arg = arg;
        if let Some(desc) = field.description {
            arg = arg.help(desc);
        }
        if let Some(default) = field
            .default_value
            .filter(|d| !d.is_empty() && !field.secret)
        {
            arg = arg.default_value(default);
        }
        if let Some(short) = field.short {
            arg = arg.short(short);
        }
        *cmd = std::mem::take(cmd).arg(arg);
    }
    Ok(())
}

/// Parse args, returning the matches and the `--config` path if set.
///
/// clap errors, including `--help`/`--version`, are returned as
/// `CLIError` for the app to handle.
pub(crate) fn parse(
    cmd: clap::Command,
    args: &[String],
    config_flag: Option<&str>,
) -> Result<(clap::ArgMatches, Option<String>), ConfigulatorError> {
    let matches = cmd
        .try_get_matches_from(args)
        .map_err(ConfigulatorError::CLIError)?;
    let config_path = config_flag.and_then(|name| {
        if matches.value_source(name) == Some(clap::parser::ValueSource::CommandLine) {
            matches.get_one::<String>(name).cloned()
        } else {
            None
        }
    });
    Ok((matches, config_path))
}
