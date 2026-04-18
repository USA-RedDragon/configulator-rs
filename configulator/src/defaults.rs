use crate::field_info::{FieldInfo, FieldType};
use crate::value_map::{ConfigValue, ValueMap};

/// Build a `ValueMap` from the `default` attributes on config fields.
pub fn load_defaults(fields: &[FieldInfo]) -> ValueMap {
    let mut map = ValueMap::new();
    for field in fields {
        let key = field.config_name.to_string();
        match &field.field_type {
            FieldType::Struct(sub_fields) => {
                let nested = load_defaults(sub_fields);
                // Empty nested maps are omitted; the struct will get T::default()
                // via `parse_nested` when the key is absent from the map.
                if !nested.is_empty() {
                    map.insert(key, ConfigValue::Nested(nested));
                }
            }
            FieldType::List => {
                if let Some(default_str) = field.default_value {
                    let parts: Vec<String> = default_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    map.insert(key, ConfigValue::List(parts));
                }
            }
            FieldType::Bool | FieldType::Scalar => {
                if let Some(default_str) = field.default_value {
                    map.insert(key, ConfigValue::Scalar(default_str.to_string()));
                }
            }
        }
    }
    map
}
