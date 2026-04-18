use std::collections::HashMap;

#[cfg(feature = "file")]
mod serde_impl {
    use super::ConfigValue;
    use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
    use std::fmt;

    struct ConfigValueVisitor;

    impl<'de> Visitor<'de> for ConfigValueVisitor {
        type Value = ConfigValue;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("any valid configuration value")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(v.to_string()))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(v.to_string()))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(v.to_string()))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(v.to_string()))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(v.to_owned()))
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(v))
        }

        fn visit_none<E: de::Error>(self) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(String::new()))
        }

        fn visit_unit<E: de::Error>(self) -> Result<ConfigValue, E> {
            Ok(ConfigValue::Scalar(String::new()))
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ConfigValue, A::Error> {
            let mut items = Vec::new();
            while let Some(val) = seq.next_element::<ConfigValue>()? {
                match val {
                    ConfigValue::Scalar(s) => items.push(s),
                    other => items.push(format!("{other:?}")),
                }
            }
            Ok(ConfigValue::List(items))
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ConfigValue, A::Error> {
            let mut result = super::ValueMap::new();
            while let Some((key, value)) = map.next_entry::<String, ConfigValue>()? {
                result.insert(key, value);
            }
            Ok(ConfigValue::Nested(result))
        }
    }

    impl<'de> Deserialize<'de> for ConfigValue {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(ConfigValueVisitor)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_expecting() {
            use std::fmt::Write;
            let mut buf = String::new();
            let visitor = ConfigValueVisitor;
            // Call expecting via a Formatter
            write!(buf, "{}", DisplayExpecting(visitor)).unwrap();
            assert_eq!(buf, "any valid configuration value");
        }

        /// Helper that calls the visitor's `expecting` method through Display.
        struct DisplayExpecting<V>(V);
        impl<V: Visitor<'static>> fmt::Display for DisplayExpecting<V> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.expecting(f)
            }
        }

        #[test]
        fn test_visit_string() {
            let visitor = ConfigValueVisitor;
            let result: Result<ConfigValue, serde::de::value::Error> =
                visitor.visit_string("owned".to_owned());
            match result.unwrap() {
                ConfigValue::Scalar(s) => assert_eq!(s, "owned"),
                other => panic!("expected Scalar, got {other:?}"),
            }
        }

        #[test]
        fn test_visit_none() {
            let visitor = ConfigValueVisitor;
            let result: Result<ConfigValue, serde::de::value::Error> = visitor.visit_none();
            match result.unwrap() {
                ConfigValue::Scalar(s) => assert!(s.is_empty()),
                other => panic!("expected empty Scalar, got {other:?}"),
            }
        }
    }
}

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
                    // Source is nested but target isn't, source wins
                    *entry = ConfigValue::Nested(source_nested.clone());
                }
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}
