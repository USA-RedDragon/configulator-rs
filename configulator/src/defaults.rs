use crate::field_info::{FieldInfo, FieldType};
use crate::value_map::{ConfigValue, ValueMap};

/// Build a `ValueMap` from the `default` attributes on config fields.
pub fn load_defaults(fields: &[FieldInfo]) -> ValueMap {
    let mut map = ValueMap::new();
    for field in fields {
        match &field.field_type {
            FieldType::Struct(sub_fields) => {
                let nested = load_defaults(sub_fields);
                if !nested.is_empty() {
                    map.insert(field.config_name.to_string(), ConfigValue::Nested(nested));
                }
            }
            FieldType::List => {
                if let Some(default_str) = field.default_value {
                    let parts: Vec<String> = default_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    map.insert(field.config_name.to_string(), ConfigValue::List(parts));
                }
            }
            FieldType::Bool | FieldType::Scalar => {
                if let Some(default_str) = field.default_value {
                    map.insert(
                        field.config_name.to_string(),
                        ConfigValue::Scalar(default_str.to_string()),
                    );
                }
            }
        }
    }
    map
}
