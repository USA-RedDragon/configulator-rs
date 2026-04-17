use crate::error::ConfigulatorError;
use crate::options::FileOptions;
use crate::value_map::{ConfigValue, ValueMap};

/// Load configuration from the first YAML file found in the given paths.
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

    let yaml_value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents)
        .map_err(|e| ConfigulatorError::FileError(e.to_string()))?;

    Ok(yaml_to_value_map(&yaml_value))
}

fn yaml_to_value_map(value: &serde_yaml_ng::Value) -> ValueMap {
    let mut map = ValueMap::new();
    if let serde_yaml_ng::Value::Mapping(mapping) = value {
        for (k, v) in mapping {
            let key = match k {
                serde_yaml_ng::Value::String(s) => s.clone(),
                other => format!("{other:?}"),
            };
            map.insert(key, yaml_value_to_config_value(v));
        }
    }
    map
}

fn yaml_value_to_config_value(value: &serde_yaml_ng::Value) -> ConfigValue {
    match value {
        serde_yaml_ng::Value::Mapping(_) => ConfigValue::Nested(yaml_to_value_map(value)),
        serde_yaml_ng::Value::Sequence(seq) => {
            let items: Vec<String> = seq
                .iter()
                .map(yaml_scalar_to_string)
                .collect();
            ConfigValue::List(items)
        }
        other => ConfigValue::Scalar(yaml_scalar_to_string(other)),
    }
}

fn yaml_scalar_to_string(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Null => String::new(),
        // Tagged values, sequences, and mappings in scalar position are
        // unexpected — surface them as debug strings rather than silently
        // dropping them. The calling code will likely fail at parse time,
        // giving the user a visible error.
        other => format!("{other:?}"),
    }
}
