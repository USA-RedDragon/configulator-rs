use std::collections::HashMap;

/// Intermediate representation of configuration values from any source.
/// Used to merge values from files, env vars, and CLI flags before
/// converting to the final config struct.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ConfigValue {
    /// A raw string value to be parsed into the target type via `FromStr`.
    Scalar(String),
    /// A nested struct represented as a map of field names to values.
    Nested(ValueMap),
    /// A list of raw string values (for Vec fields).
    List(Vec<String>),
}

/// A map from config field names to their values.
pub type ValueMap = HashMap<String, ConfigValue>;

/// Merge `source` into `target`. Values in `source` overwrite values in `target`.
pub fn merge_value_maps(target: &mut ValueMap, source: &ValueMap) {
    for (key, value) in source {
        match value {
            ConfigValue::Nested(source_nested) => {
                let entry = target
                    .entry(key.clone())
                    .or_insert_with(|| ConfigValue::Nested(ValueMap::new()));
                if let ConfigValue::Nested(target_nested) = entry {
                    merge_value_maps(target_nested, source_nested);
                } else {
                    // Source is nested but target isn't — source wins
                    *entry = ConfigValue::Nested(source_nested.clone());
                }
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}
