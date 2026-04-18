#[cfg(test)]
mod tests {
    use configulator::*;
    #[cfg(feature = "file")]
    use std::io::Write;
    #[cfg(feature = "file")]
    use std::path::PathBuf;

    // ---- Test structs ----

    #[derive(Config, Default, Debug, PartialEq)]
    struct SimpleConfig {
        #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
        host: String,

        #[configulator(name = "port", default = "8080", description = "Listen port")]
        port: u16,

        #[configulator(name = "debug", default = "false", description = "Debug mode")]
        debug: bool,
    }

    impl Validate for SimpleConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            if self.port == 0 {
                return Err("port must be non-zero".into());
            }
            Ok(())
        }
    }

    #[derive(Config, Default, Debug, PartialEq)]
    struct NestedConfig {
        #[configulator(name = "app-name", default = "myapp")]
        app_name: String,

        #[configulator(name = "database")]
        database: DatabaseConfig,
    }

    impl Validate for NestedConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    #[derive(Config, Default, Debug, PartialEq)]
    struct DatabaseConfig {
        #[configulator(name = "url", default = "postgres://localhost/db")]
        url: String,

        #[configulator(name = "max-connections", default = "10")]
        max_connections: u32,
    }

    #[derive(Config, Default, Debug, PartialEq)]
    struct ListConfig {
        #[configulator(name = "tags", default = "a,b,c")]
        tags: Vec<String>,

        #[configulator(name = "ports")]
        ports: Vec<u16>,
    }

    impl Validate for ListConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    // ---- Helpers for safe env var manipulation ----

    /// SAFETY: Tests that use env vars must run serially (not in parallel with
    /// other tests that read/write the same env vars). In practice this is
    /// acceptable for integration tests in a dedicated test binary.
    unsafe fn set_env(key: &str, val: &str) {
        unsafe { std::env::set_var(key, val) };
    }

    unsafe fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) };
    }

    // ---- Tests ----

    #[test]
    fn test_defaults_only() {
        let config = Configulator::<SimpleConfig>::new()
            .load()
            .unwrap();

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(!config.debug);
    }

    #[test]
    fn test_defaults_nested() {
        let config = Configulator::<NestedConfig>::new()
            .load_without_validation()
            .unwrap();

        assert_eq!(config.app_name, "myapp");
        assert_eq!(config.database.url, "postgres://localhost/db");
        assert_eq!(config.database.max_connections, 10);
    }

    #[test]
    fn test_defaults_list() {
        let config = Configulator::<ListConfig>::new()
            .load()
            .unwrap();

        assert_eq!(config.tags, vec!["a", "b", "c"]);
        assert!(config.ports.is_empty());
    }

    #[test]
    fn test_validation_passes() {
        let result = Configulator::<SimpleConfig>::new().load();
        assert!(result.is_ok());
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_validation_fails() {
        // port=0 should fail validation
        let result = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--port".into(), "0".into()])
            .load();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("port must be non-zero"));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_load_without_validation_skips_check() {
        let result = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--port".into(), "0".into()])
            .load_without_validation();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().port, 0);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_yaml_file_loading() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: 0.0.0.0").unwrap();
        writeln!(f, "port: 3000").unwrap();
        writeln!(f, "debug: true").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3000);
        assert!(config.debug);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_yaml_file_nested() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "app-name: production").unwrap();
        writeln!(f, "database:").unwrap();
        writeln!(f, "  url: postgres://prod/db").unwrap();
        writeln!(f, "  max-connections: 50").unwrap();

        let config = Configulator::<NestedConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.app_name, "production");
        assert_eq!(config.database.url, "postgres://prod/db");
        assert_eq!(config.database.max_connections, 50);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_yaml_file_not_found_no_error() {
        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![PathBuf::from("/nonexistent/config.yaml")],
                error_if_not_found: false,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        // Falls back to defaults
        assert_eq!(config.host, "127.0.0.1");
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_yaml_file_not_found_error() {
        let result = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![PathBuf::from("/nonexistent/config.yaml")],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load();

        assert!(result.is_err());
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_yaml_file_first_found_wins() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("first.yaml");
        let file2 = dir.path().join("second.yaml");

        let mut f1 = std::fs::File::create(&file1).unwrap();
        writeln!(f1, "port: 1111").unwrap();

        let mut f2 = std::fs::File::create(&file2).unwrap();
        writeln!(f2, "port: 2222").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file1.to_path_buf(), file2.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.port, 1111);
    }

    #[cfg(feature = "env")]
    #[test]
    fn test_env_vars() {
        // SAFETY: No other test uses the TEST1_ prefix concurrently.
        unsafe {
            set_env("TEST1_HOST", "envhost");
            set_env("TEST1_PORT", "9090");
            set_env("TEST1_DEBUG", "true");
        }

        let config = Configulator::<SimpleConfig>::new()
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "TEST1".into(),
                separator: "_".into(),
            })
            .load()
            .unwrap();

        assert_eq!(config.host, "envhost");
        assert_eq!(config.port, 9090);
        assert!(config.debug);

        // Cleanup
        unsafe {
            remove_env("TEST1_HOST");
            remove_env("TEST1_PORT");
            remove_env("TEST1_DEBUG");
        }
    }

    #[cfg(feature = "env")]
    #[test]
    fn test_env_vars_nested() {
        // SAFETY: No other test uses the TEST2__ prefix concurrently.
        unsafe {
            set_env("TEST2__APP_NAME", "envapp");
            set_env("TEST2__DATABASE__URL", "postgres://env/db");
            set_env("TEST2__DATABASE__MAX_CONNECTIONS", "25");
        }

        let config = Configulator::<NestedConfig>::new()
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "TEST2".into(),
                separator: "__".into(),
            })
            .load()
            .unwrap();

        assert_eq!(config.app_name, "envapp");
        assert_eq!(config.database.url, "postgres://env/db");
        assert_eq!(config.database.max_connections, 25);

        unsafe {
            remove_env("TEST2__APP_NAME");
            remove_env("TEST2__DATABASE__URL");
            remove_env("TEST2__DATABASE__MAX_CONNECTIONS");
        }
    }

    #[cfg(feature = "env")]
    #[test]
    fn test_env_vars_list() {
        // SAFETY: No other test uses the TEST3_ prefix concurrently.
        unsafe {
            set_env("TEST3_TAGS", "x,y,z");
            set_env("TEST3_PORTS", "80,443");
        }

        let config = Configulator::<ListConfig>::new()
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "TEST3".into(),
                separator: "_".into(),
            })
            .load()
            .unwrap();

        assert_eq!(config.tags, vec!["x", "y", "z"]);
        assert_eq!(config.ports, vec![80, 443]);

        unsafe {
            remove_env("TEST3_TAGS");
            remove_env("TEST3_PORTS");
        }
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_flags_simple() {
        let config = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec![
                "--host".into(), "clihost".into(),
                "--port".into(), "4444".into(),
                "--debug".into(),
            ])
            .load()
            .unwrap();

        assert_eq!(config.host, "clihost");
        assert_eq!(config.port, 4444);
        assert!(config.debug);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_flags_nested() {
        let config = Configulator::<NestedConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec![
                "--app-name".into(), "cliflag-app".into(),
                "--database.url".into(), "postgres://cli/db".into(),
                "--database.max-connections".into(), "99".into(),
            ])
            .load()
            .unwrap();

        assert_eq!(config.app_name, "cliflag-app");
        assert_eq!(config.database.url, "postgres://cli/db");
        assert_eq!(config.database.max_connections, 99);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_precedence_file_over_default() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "port: 5555").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        // port from file, host from default
        assert_eq!(config.port, 5555);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[cfg(all(feature = "file", feature = "env"))]
    #[test]
    fn test_precedence_env_over_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: filehost").unwrap();
        writeln!(f, "port: 5555").unwrap();

        // SAFETY: No other test uses the TEST4_ prefix concurrently.
        unsafe { set_env("TEST4_PORT", "7777") };

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "TEST4".into(),
                separator: "_".into(),
            })
            .load()
            .unwrap();

        // host from file, port overridden by env
        assert_eq!(config.host, "filehost");
        assert_eq!(config.port, 7777);

        unsafe { remove_env("TEST4_PORT") };
    }

    #[cfg(all(feature = "file", feature = "env", feature = "cli"))]
    #[test]
    fn test_precedence_cli_over_all() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: filehost").unwrap();
        writeln!(f, "port: 5555").unwrap();

        // SAFETY: No other test uses the TEST5_ prefix concurrently.
        unsafe { set_env("TEST5_PORT", "7777") };

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "TEST5".into(),
                separator: "_".into(),
            })
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--port".into(), "9999".into()])
            .load()
            .unwrap();

        // host from file, port overridden by CLI (highest precedence)
        assert_eq!(config.host, "filehost");
        assert_eq!(config.port, 9999);

        unsafe { remove_env("TEST5_PORT") };
    }

    #[test]
    fn test_config_fields_metadata() {
        let fields = SimpleConfig::configulator_fields();
        assert_eq!(fields.len(), 3);

        assert_eq!(fields[0].config_name, "host");
        assert_eq!(fields[0].default_value, Some("127.0.0.1"));
        assert_eq!(fields[0].field_type, FieldType::Scalar);

        assert_eq!(fields[1].config_name, "port");
        assert_eq!(fields[1].field_type, FieldType::Scalar);

        assert_eq!(fields[2].config_name, "debug");
        assert_eq!(fields[2].field_type, FieldType::Bool);
    }

    #[test]
    fn test_config_fields_nested_metadata() {
        let fields = NestedConfig::configulator_fields();
        assert_eq!(fields.len(), 2);

        assert_eq!(fields[0].config_name, "app-name");
        assert_eq!(fields[0].field_type, FieldType::Scalar);

        if let FieldType::Struct(sub) = &fields[1].field_type {
            assert_eq!(sub.len(), 2);
            assert_eq!(sub[0].config_name, "url");
            assert_eq!(sub[1].config_name, "max-connections");
        } else {
            panic!("Expected database field to be Struct type");
        }
    }

    #[test]
    fn test_from_value_map_directly() {
        let mut map = ValueMap::new();
        map.insert("host".into(), ConfigValue::Scalar("direct".into()));
        map.insert("port".into(), ConfigValue::Scalar("1234".into()));
        map.insert("debug".into(), ConfigValue::Scalar("true".into()));

        let config = SimpleConfig::from_value_map(&map).unwrap();
        assert_eq!(config.host, "direct");
        assert_eq!(config.port, 1234);
        assert!(config.debug);
    }

    #[test]
    fn test_parse_error_bad_type() {
        let mut map = ValueMap::new();
        map.insert("host".into(), ConfigValue::Scalar("ok".into()));
        map.insert("port".into(), ConfigValue::Scalar("not_a_number".into()));

        let result = SimpleConfig::from_value_map(&map);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("not_a_number"));
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_yaml_list_field() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "tags:").unwrap();
        writeln!(f, "  - alpha").unwrap();
        writeln!(f, "  - beta").unwrap();
        writeln!(f, "ports:").unwrap();
        writeln!(f, "  - 80").unwrap();
        writeln!(f, "  - 443").unwrap();

        let config = Configulator::<ListConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.tags, vec!["alpha", "beta"]);
        assert_eq!(config.ports, vec![80, 443]);
    }

    #[test]
    fn test_defaults_only_method() {
        let config = Configulator::<SimpleConfig>::defaults_only()
            .unwrap();

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(!config.debug);
    }

    #[test]
    fn test_no_configulator_attr_uses_field_name() {
        #[derive(Config, Default, Debug, PartialEq)]
        struct Bare {
            my_field: String,
            count: u32,
        }

        let fields = Bare::configulator_fields();
        assert_eq!(fields[0].config_name, "my_field");
        assert_eq!(fields[1].config_name, "count");
    }

    #[cfg(all(feature = "file", feature = "cli"))]
    #[test]
    fn test_cli_config_flag() {
        // When with_file is called first, --config / -c gets registered
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: from-cli-config").unwrap();
        writeln!(f, "port: 6060").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![],
                error_if_not_found: false,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec![
                "--config".into(),
                file_path.to_string_lossy().to_string(),
            ])
            .load()
            .unwrap();

        assert_eq!(config.host, "from-cli-config");
        assert_eq!(config.port, 6060);
    }

    #[cfg(feature = "env")]
    #[test]
    fn test_env_dashes_to_underscores() {
        // config name "app-name" should become env var PREFIX_APP_NAME
        // SAFETY: No other test uses the TEST6_ prefix concurrently.
        unsafe { set_env("TEST6_APP_NAME", "dashed") };

        let config = Configulator::<NestedConfig>::new()
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "TEST6".into(),
                separator: "_".into(),
            })
            .load_without_validation()
            .unwrap();

        assert_eq!(config.app_name, "dashed");

        unsafe { remove_env("TEST6_APP_NAME") };
    }

    #[test]
    fn test_missing_fields_use_default() {
        // Only set some values, others should be defaults
        let mut map = ValueMap::new();
        map.insert("host".into(), ConfigValue::Scalar("partial".into()));

        let config = SimpleConfig::from_value_map(&map).unwrap();
        assert_eq!(config.host, "partial");
        assert_eq!(config.port, 0); // Default::default() for u16
        assert!(!config.debug);    // Default::default() for bool
    }

    // ---- Phase 1: error.rs Display/Error + options.rs Debug ----

    #[test]
    fn test_error_display_parse_error() {
        let err = ConfigulatorError::ParseError {
            field: "port".into(),
            value: "abc".into(),
            message: "invalid digit".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("port"));
        assert!(msg.contains("abc"));
        assert!(msg.contains("invalid digit"));
    }

    #[test]
    fn test_error_display_validation_error() {
        let inner: Box<dyn std::error::Error + Send + Sync> = "bad config".into();
        let err = ConfigulatorError::ValidationError(inner);
        assert!(err.to_string().contains("validation error"));
        assert!(err.to_string().contains("bad config"));
    }

    #[test]
    fn test_error_source_validation() {
        use std::error::Error;
        let inner: Box<dyn std::error::Error + Send + Sync> = "inner error".into();
        let err = ConfigulatorError::ValidationError(inner);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_error_source_parse_error_is_none() {
        use std::error::Error;
        let err = ConfigulatorError::ParseError {
            field: "f".into(),
            value: "v".into(),
            message: "m".into(),
        };
        assert!(err.source().is_none());
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_error_display_file_not_found() {
        let err = ConfigulatorError::FileNotFound;
        assert_eq!(err.to_string(), "config file not found");
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_error_display_file_error() {
        let err = ConfigulatorError::FileError("bad yaml".into());
        assert!(err.to_string().contains("file error"));
        assert!(err.to_string().contains("bad yaml"));
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_error_source_file_variants_are_none() {
        use std::error::Error;
        let err1 = ConfigulatorError::FileNotFound;
        assert!(err1.source().is_none());
        let err2 = ConfigulatorError::FileError("x".into());
        assert!(err2.source().is_none());
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_error_display_cli_error() {
        let err = ConfigulatorError::CLIError("unknown flag".into());
        assert!(err.to_string().contains("CLI error"));
        assert!(err.to_string().contains("unknown flag"));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_error_source_cli_error_is_none() {
        use std::error::Error;
        let err = ConfigulatorError::CLIError("x".into());
        assert!(err.source().is_none());
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_file_options_debug() {
        let opts = FileOptions {
            paths: vec![PathBuf::from("config.yaml")],
            error_if_not_found: true,
            loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
        };
        let debug_str = format!("{:?}", opts);
        assert!(debug_str.contains("FileOptions"));
        assert!(debug_str.contains("config.yaml"));
        assert!(debug_str.contains("error_if_not_found"));
        assert!(debug_str.contains("<dyn FileLoader>"));
    }

    // ---- Phase 2: value_map.rs serde visitors + merge ----

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_bool() {
        // Exercises visit_bool (value_map.rs lines 14-16)
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "debug: true").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert!(config.debug);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_integer() {
        // Exercises visit_i64/visit_u64 (value_map.rs lines 22-24, 30-32)
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "port: 9999").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.port, 9999);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_float() {
        // Exercises visit_f64 (value_map.rs lines 38-40)
        #[derive(Config, Default, Debug, PartialEq)]
        struct FloatConfig {
            #[configulator(name = "ratio", default = "1.0")]
            ratio: f64,
        }
        impl Validate for FloatConfig {
            fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                Ok(())
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "ratio: 3.14").unwrap();

        let config = Configulator::<FloatConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert!((config.ratio - std::f64::consts::PI).abs() < f64::EPSILON);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_null() {
        // Exercises visit_none/visit_unit (value_map.rs lines 42-44, 46-48)
        // YAML null should produce an empty scalar, field falls back to default
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: ~").unwrap(); // YAML null

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        // Null → empty scalar → String::default() = ""
        assert_eq!(config.host, "");
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_seq_and_map() {
        // Exercises visit_seq (line 55) and visit_map (value_map.rs lines 105-107 via nested)
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "tags:").unwrap();
        writeln!(f, "  - one").unwrap();
        writeln!(f, "  - two").unwrap();
        writeln!(f, "ports:").unwrap();
        writeln!(f, "  - 8080").unwrap();

        let config = Configulator::<ListConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.tags, vec!["one", "two"]);
        assert_eq!(config.ports, vec![8080]);
    }

    #[test]
    fn test_merge_value_maps_nested_overwrites_scalar() {
        // Exercises merge_value_maps type-mismatch branch (value_map.rs lines 105-107)
        // When target has Scalar("x") and source has Nested({...}) for the same key,
        // source should win.
        let mut target = ValueMap::new();
        target.insert("db".into(), ConfigValue::Scalar("old".into()));

        let mut inner = ValueMap::new();
        inner.insert("host".into(), ConfigValue::Scalar("localhost".into()));

        let mut source = ValueMap::new();
        source.insert("db".into(), ConfigValue::Nested(inner));

        merge_value_maps(&mut target, &source);

        match target.get("db") {
            Some(ConfigValue::Nested(nested)) => {
                match nested.get("host") {
                    Some(ConfigValue::Scalar(s)) => assert_eq!(s, "localhost"),
                    other => panic!("Expected Scalar(localhost), got {other:?}"),
                }
            }
            other => panic!("Expected Nested, got {other:?}"),
        }
    }

    // ---- Phase 3: file.rs edge cases ----

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_loader_non_nested_root_error() {
        // Exercises SerdeLoader returning non-Nested at root (file.rs lines 33-35)
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "just a string").unwrap();

        let result = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("root must be a mapping/table"));
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_file_io_error_non_not_found() {
        // Exercises the non-NotFound I/O error branch (file.rs lines 76-80)
        // Reading a directory triggers an I/O error that isn't NotFound
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();

        let result = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![dir_path.clone()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load();

        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("file error"), "unexpected error: {msg}");
    }

    // ---- Phase 4: environment.rs empty prefix ----

    #[cfg(feature = "env")]
    #[test]
    fn test_env_empty_prefix() {
        // Exercises the empty prefix branch (environment.rs line 46)
        // With prefix="" and separator="_", env key should be just the field name
        // SAFETY: No other test uses these exact bare env var names concurrently.
        unsafe {
            set_env("HOST", "emptyprefix");
            set_env("PORT", "1234");
        }

        let config = Configulator::<SimpleConfig>::new()
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "".into(),
                separator: "_".into(),
            })
            .load()
            .unwrap();

        assert_eq!(config.host, "emptyprefix");
        assert_eq!(config.port, 1234);

        unsafe {
            remove_env("HOST");
            remove_env("PORT");
        }
    }

    // ---- Phase 5: derive_helpers parse edge cases ----

    #[test]
    fn test_parse_scalar_with_list_value() {
        // Exercises parse_scalar wrong type branch (derive_helpers.rs line 30)
        let mut map = ValueMap::new();
        map.insert("port".into(), ConfigValue::List(vec!["1".into(), "2".into()]));

        let result = parse_scalar::<u16>(&map, "port");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected scalar value"));
    }

    #[test]
    fn test_parse_list_with_nested_value() {
        // Exercises parse_list wrong type branch (derive_helpers.rs lines 81-85)
        let mut map = ValueMap::new();
        let mut nested = ValueMap::new();
        nested.insert("x".into(), ConfigValue::Scalar("1".into()));
        map.insert("tags".into(), ConfigValue::Nested(nested));

        let result = parse_list::<String>(&map, "tags");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected list value"));
    }

    #[test]
    fn test_parse_list_single_scalar_as_one_element() {
        // Exercises single scalar → one-element list (derive_helpers.rs lines 69-78)
        let mut map = ValueMap::new();
        map.insert("ports".into(), ConfigValue::Scalar("8080".into()));

        let result = parse_list::<u16>(&map, "ports").unwrap();
        assert_eq!(result, vec![8080]);
    }

    #[test]
    fn test_parse_list_empty_scalar_returns_empty() {
        // Exercises empty scalar → empty vec (derive_helpers.rs line 69)
        let mut map = ValueMap::new();
        map.insert("ports".into(), ConfigValue::Scalar("".into()));

        let result = parse_list::<u16>(&map, "ports").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_list_item_parse_error() {
        // Exercises parse error within list items (derive_helpers.rs lines 60-63)
        let mut map = ValueMap::new();
        map.insert("ports".into(), ConfigValue::List(vec!["80".into(), "not_a_port".into()]));

        let result = parse_list::<u16>(&map, "ports");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("ports[1]"), "Error should mention index: {err}");
        assert!(err.contains("not_a_port"));
    }

    #[test]
    fn test_parse_nested_with_scalar_value() {
        // Exercises parse_nested wrong type (derive_helpers.rs lines 96-101)
        let mut map = ValueMap::new();
        map.insert("database".into(), ConfigValue::Scalar("not_a_struct".into()));

        let result = DatabaseConfig::from_value_map(&ValueMap::new());
        assert!(result.is_ok()); // baseline

        let result = parse_nested::<DatabaseConfig>(&map, "database");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected nested struct value"));
    }

    #[test]
    fn test_parse_scalar_empty_returns_default() {
        // Exercises empty scalar → T::default() (derive_helpers.rs line 39-43)
        let mut map = ValueMap::new();
        map.insert("port".into(), ConfigValue::Scalar("".into()));

        let result = parse_scalar::<u16>(&map, "port").unwrap();
        assert_eq!(result, 0); // u16::default()
    }

    // ---- Phase 6: cli.rs edge cases ----

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_bool_explicit_true() {
        // Exercises bool flag with explicit "true" value (cli.rs lines 92-99)
        let config = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--debug".into(), "true".into()])
            .load()
            .unwrap();

        assert!(config.debug);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_bool_explicit_false() {
        // Exercises bool flag with explicit "false" value (cli.rs lines 92-99)
        let config = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--debug".into(), "false".into()])
            .load()
            .unwrap();

        assert!(!config.debug);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_list_flags_repeated() {
        // Exercises list extraction with get_many (cli.rs lines 152-154, 156-161)
        let config = Configulator::<ListConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec![
                "--tags".into(), "x".into(),
                "--tags".into(), "y".into(),
                "--ports".into(), "80".into(),
                "--ports".into(), "443".into(),
            ])
            .load()
            .unwrap();

        assert_eq!(config.tags, vec!["x", "y"]);
        assert_eq!(config.ports, vec![80, 443]);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_unknown_flag_error() {
        // Exercises CLIError from clap parse failure (cli.rs line 18)
        let result = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--nonexistent-flag".into(), "val".into()])
            .load();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("CLI error"), "unexpected error: {}", err);
    }

    // ---- Phase 7: configulator.rs builder edge cases ----

    #[test]
    fn test_configulator_default_trait() {
        // Exercises Configulator::default() (configulator.rs lines 206-208)
        let config = Configulator::<SimpleConfig>::default()
            .load()
            .unwrap();

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(!config.debug);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_without_file_opts() {
        // Exercises has_file=false path (configulator.rs lines 105-108)
        // CLI configured without .with_file()
        let config = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--host".into(), "clionly".into()])
            .load()
            .unwrap();

        assert_eq!(config.host, "clionly");
        assert_eq!(config.port, 8080); // from default
    }

    #[cfg(all(feature = "file", feature = "cli"))]
    #[test]
    fn test_config_file_key_does_not_leak() {
        // Exercises __config_file__ removal (configulator.rs line 195)
        // After loading with --config, the internal key should not appear
        // in the final config struct fields
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: from-config-file").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![],
                error_if_not_found: false,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec![
                "--config".into(),
                file_path.to_string_lossy().to_string(),
            ])
            .load()
            .unwrap();

        // The config loaded correctly and __config_file__ didn't cause issues
        assert_eq!(config.host, "from-config-file");
        assert_eq!(config.port, 8080); // default
    }

    // ---- Additional coverage for remaining gaps ----

    // Struct with described list fields — needed for cli.rs line 97
    #[derive(Config, Default, Debug, PartialEq)]
    struct DescribedListConfig {
        #[configulator(name = "items", description = "List of items")]
        items: Vec<String>,
    }

    impl Validate for DescribedListConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_list_with_description() {
        // Exercises cli.rs line 97: `arg = arg.help(desc)` in the List branch
        // of register_args — requires a list field with a description attribute
        let config = Configulator::<DescribedListConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--items".into(), "a".into(), "--items".into(), "b".into()])
            .load()
            .unwrap();

        assert_eq!(config.items, vec!["a", "b"]);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_list_not_provided() {
        // Exercises cli.rs lines 160-161: List field is registered in clap
        // but no values are provided on the command line, so get_many returns None
        let config = Configulator::<ListConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec![]) // no list args provided
            .load()
            .unwrap();

        // Should fall back to defaults: tags=["a","b","c"], ports=[]
        assert_eq!(config.tags, vec!["a", "b", "c"]);
        assert!(config.ports.is_empty());
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_with_cli_command() {
        // Exercises configulator.rs lines 105-108: with_cli_command
        let cmd = clap::Command::new("myapp").version("1.0.0");

        let config = Configulator::<SimpleConfig>::new()
            .with_cli_command(cmd)
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--host".into(), "custom-cmd".into()])
            .load()
            .unwrap();

        assert_eq!(config.host, "custom-cmd");
        assert_eq!(config.port, 8080);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_cli_no_explicit_args_falls_back_to_env_args() {
        // Exercises configulator.rs line 195: the None branch of get_cli_args
        // where with_cli_args() was NOT called. std::env::args() from the test
        // runner will be passed to clap, which will likely fail because the test
        // runner passes unknown flags (like --test-threads).
        let result = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            // Deliberately NOT calling .with_cli_args() to exercise None branch
            .load();

        // This may or may not succeed depending on test runner args;
        // the point is that line 195 is executed either way.
        // Check it returns either Ok or a CLIError (both are valid).
        match result {
            Ok(_) => {} // no extra test runner args happened to conflict
            Err(ref e) => {
                assert!(
                    e.to_string().contains("CLI error"),
                    "Expected CLIError from unknown test runner args, got: {e}"
                );
            }
        }
    }

    #[test]
    fn test_parse_list_scalar_parse_failure() {
        // Exercises derive_helpers.rs lines 74-77: parse error in the
        // single-scalar-treated-as-one-element-list path
        let mut map = ValueMap::new();
        map.insert("ports".into(), ConfigValue::Scalar("not_a_u16".into()));

        let result = parse_list::<u16>(&map, "ports");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("ports"), "Error should mention field: {err}");
        assert!(err.contains("not_a_u16"), "Error should mention value: {err}");
    }

    #[test]
    fn test_parse_nested_with_nested_value() {
        // Exercises derive_helpers.rs line 96: parse_nested happy path
        // where the value IS a ConfigValue::Nested
        let mut inner = ValueMap::new();
        inner.insert("url".into(), ConfigValue::Scalar("postgres://test/db".into()));
        inner.insert("max-connections".into(), ConfigValue::Scalar("42".into()));

        let mut map = ValueMap::new();
        map.insert("database".into(), ConfigValue::Nested(inner));

        let db = parse_nested::<DatabaseConfig>(&map, "database").unwrap();
        assert_eq!(db.url, "postgres://test/db");
        assert_eq!(db.max_connections, 42);
    }

    #[cfg(feature = "env")]
    #[test]
    fn test_env_list_with_empty_prefix() {
        // Exercises environment.rs line 46: List insertion with empty prefix
        // SAFETY: No other test uses these exact bare env var names concurrently.
        unsafe {
            set_env("TAGS", "env1,env2,env3");
            set_env("PORTS", "3000,4000");
        }

        let config = Configulator::<ListConfig>::new()
            .with_environment_variables(EnvironmentVariableOptions {
                prefix: "".into(),
                separator: "_".into(),
            })
            .load()
            .unwrap();

        assert_eq!(config.tags, vec!["env1", "env2", "env3"]);
        assert_eq!(config.ports, vec![3000, 4000]);

        unsafe {
            remove_env("TAGS");
            remove_env("PORTS");
        }
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_negative_integer() {
        // Exercises value_map.rs lines 22-24: visit_i64
        // serde_yaml_ng uses visit_u64 for positive ints but visit_i64 for negative
        #[derive(Config, Default, Debug, PartialEq)]
        struct SignedConfig {
            #[configulator(name = "offset", default = "0")]
            offset: i64,
        }
        impl Validate for SignedConfig {
            fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                Ok(())
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "offset: -42").unwrap();

        let config = Configulator::<SignedConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.offset, -42);
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_explicit_null_keyword() {
        // Exercises value_map.rs lines 42-44: visit_none
        // Uses explicit `null` keyword instead of `~` to cover the other null path
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "host: null").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.host, "");
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_string_value() {
        // Exercises value_map.rs lines 38-40: visit_string (owned string)
        // Uses a quoted YAML string which may trigger visit_string in some serde impls
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        // Quoted string with special chars to force owned String allocation
        writeln!(f, "host: \"hello\\nworld\"").unwrap();

        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load()
            .unwrap();

        assert_eq!(config.host, "hello\nworld");
    }

    #[cfg(feature = "file")]
    #[test]
    fn test_serde_visitor_seq_rejects_nested_elements() {
        // Exercises value_map.rs visit_seq: non-scalar elements in a sequence
        // must produce an error rather than silently degrading to debug strings.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "tags:").unwrap();
        writeln!(f, "  - simple").unwrap();
        writeln!(f, "  - key: value").unwrap(); // nested map inside a seq

        let result = Configulator::<ListConfig>::new()
            .with_file(FileOptions {
                paths: vec![file_path.to_path_buf()],
                error_if_not_found: true,
                loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
            })
            .load();

        assert!(result.is_err(), "expected error for nested value in sequence");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("nested values inside sequences are not supported"),
            "unexpected error message: {err}"
        );
    }
}
