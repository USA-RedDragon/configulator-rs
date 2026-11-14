use crate::error::ConfigulatorError;
use crate::field_info::{FieldInfo, FieldType};

/// The `--config` flag: its name, shorthand, and the default shown in help.
#[derive(Clone)]
pub(crate) struct ConfigFlag {
    pub name: String,
    pub short: char,
    pub default: Option<String>,
}

/// Build the clap `Command` for a config's fields. Nested structs use the
/// configured separator in flag names (`--database.host`). Collections
/// and `flag = "-"` fields are skipped. Every config arg is global, so it
/// is accepted before or after a subcommand.
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
    if cmd.get_bin_name().is_none() {
        let name = cmd.get_name().to_string();
        cmd = cmd.bin_name(name);
    }

    if let Some(flag) = config_flag {
        check_free(&cmd, &flag.name, Some(flag.short))?;
        let mut arg = clap::Arg::new(flag.name.clone())
            .short(flag.short)
            .long(flag.name)
            .help("config file")
            .value_hint(clap::ValueHint::FilePath)
            .num_args(1)
            .global(true);
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
    check_free_in(cmd, long, short)?;
    for sub in cmd.get_subcommands() {
        check_free(sub, long, short)?;
    }
    Ok(())
}

fn check_free_in(
    cmd: &clap::Command,
    long: &str,
    short: Option<char>,
) -> Result<(), ConfigulatorError> {
    let conflict = |shorthand: Option<char>, existing: &str| ConfigulatorError::FlagConflict {
        flag: long.to_string(),
        shorthand,
        existing: existing.to_string(),
    };
    for a in cmd.get_arguments() {
        let existing = a.get_long().unwrap_or(a.get_id().as_str());
        if a.get_id() == long || a.get_long() == Some(long) {
            return Err(conflict(None, existing));
        }
        if let Some(c) = short.filter(|c| a.get_short() == Some(*c)) {
            return Err(conflict(Some(c), existing));
        }
    }
    if !cmd.is_disable_help_flag_set() {
        if long == "help" {
            return Err(conflict(None, "help"));
        }
        if short == Some('h') {
            return Err(conflict(Some('h'), "help"));
        }
    }
    if cmd.get_version().is_some() && !cmd.is_disable_version_flag_set() {
        if long == "version" {
            return Err(conflict(None, "version"));
        }
        if short == Some('V') {
            return Err(conflict(Some('V'), "version"));
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
                .require_equals(true),
            FieldType::Scalar => clap::Arg::new(flag_name.clone())
                .long(flag_name.clone())
                .num_args(1),
            FieldType::List => clap::Arg::new(flag_name.clone())
                .long(flag_name.clone())
                .num_args(1)
                .action(clap::ArgAction::Append),
        };
        check_free(cmd, &flag_name, field.short)?;
        let mut arg = arg.global(true);
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
        if field.path {
            arg = arg.value_hint(clap::ValueHint::AnyPath);
        }
        *cmd = std::mem::take(cmd).arg(arg);
    }
    Ok(())
}

/// Parse args. clap errors, including `--help`/`--version`, are returned
/// as `CLIError` for the app to handle.
pub(crate) fn parse(
    cmd: clap::Command,
    args: &[String],
) -> Result<clap::ArgMatches, ConfigulatorError> {
    cmd.try_get_matches_from(args)
        .map_err(ConfigulatorError::CLIError)
}

/// The `--config` path, if it was given on the command line.
pub(crate) fn config_path(matches: &clap::ArgMatches, config_flag: Option<&str>) -> Option<String> {
    let name = config_flag?;
    if matches.value_source(name) == Some(clap::parser::ValueSource::CommandLine) {
        matches.get_one::<String>(name).cloned()
    } else {
        None
    }
}
