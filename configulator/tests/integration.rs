#[cfg(test)]
mod tests {
    use configulator::*;
    use std::io::Write;
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

    #[test]
    fn test_load_without_validation_skips_check() {
        let result = Configulator::<SimpleConfig>::new()
            .with_cli_flags(CLIFlagOptions { separator: ".".into() })
            .with_cli_args(vec!["--port".into(), "0".into()])
            .load_without_validation();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().port, 0);
    }

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
            })
            .load()
            .unwrap();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3000);
        assert!(config.debug);
    }

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
            })
            .load()
            .unwrap();

        assert_eq!(config.app_name, "production");
        assert_eq!(config.database.url, "postgres://prod/db");
        assert_eq!(config.database.max_connections, 50);
    }

    #[test]
    fn test_yaml_file_not_found_no_error() {
        let config = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![PathBuf::from("/nonexistent/config.yaml")],
                error_if_not_found: false,
            })
            .load()
            .unwrap();

        // Falls back to defaults
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn test_yaml_file_not_found_error() {
        let result = Configulator::<SimpleConfig>::new()
            .with_file(FileOptions {
                paths: vec![PathBuf::from("/nonexistent/config.yaml")],
                error_if_not_found: true,
            })
            .load();

        assert!(result.is_err());
    }

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
            })
            .load()
            .unwrap();

        assert_eq!(config.port, 1111);
    }

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
            })
            .load()
            .unwrap();

        // port from file, host from default
        assert_eq!(config.port, 5555);
        assert_eq!(config.host, "127.0.0.1");
    }

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
}
