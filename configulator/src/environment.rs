use crate::field_info::{FieldInfo, FieldType};
use crate::options::EnvironmentVariableOptions;
use crate::value_map::{parse_comma_list, ConfigValue, ValueMap};

/// Load configuration values from environment variables.
///
/// Env var names are constructed as: `PREFIX` + `SEPARATOR` + `FIELDNAME`,
/// all uppercased, with dashes replaced by underscores.
///
/// For example, with prefix `"APP"` and separator `"__"`:
/// - field `port` → `APP__PORT`
/// - nested field `database.host` → `APP__DATABASE__HOST`
pub fn load_from_env(
    opts: &EnvironmentVariableOptions,
    fields: &[FieldInfo],
) -> ValueMap {
    let prefix = opts.prefix.to_uppercase();
    load_fields_from_env(fields, &prefix, &opts.separator)
}

fn load_fields_from_env(
    fields: &[FieldInfo],
    prefix: &str,
    separator: &str,
) -> ValueMap {
    let mut map = ValueMap::new();

    for field in fields {
        let env_key = format!(
            "{prefix}{sep}{name}",
            sep = if prefix.is_empty() { "" } else { separator },
            name = field.config_name.to_uppercase().replace('-', "_"),
        );

        match &field.field_type {
            FieldType::Struct(sub_fields) => {
                let nested = load_fields_from_env(sub_fields, &env_key, separator);
                // Empty nested maps are omitted; the struct will get T::default()
                // via `parse_nested` when the key is absent from the map.
                if !nested.is_empty() {
                    map.insert(field.config_name.to_string(), ConfigValue::Nested(nested));
                }
            }
            FieldType::List => {
                if let Ok(val) = std::env::var(&env_key) {
                    map.insert(field.config_name.to_string(), ConfigValue::List(parse_comma_list(&val)));
                }
            }
            FieldType::Bool | FieldType::Scalar => {
                if let Ok(val) = std::env::var(&env_key) {
                    map.insert(field.config_name.to_string(), ConfigValue::Scalar(val));
                }
            }
        }
    }

    map
}
