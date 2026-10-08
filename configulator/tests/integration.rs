#![cfg(all(feature = "file", feature = "cli", feature = "env"))]

use std::collections::HashMap;
use std::io::Write;

use configulator::{
    serde_loader, CLIFlagOptions, Config, Configulator, ConfigulatorError, Duration,
    EnvironmentVariableOptions, FileOptions, Layer, Validate,
};

#[derive(Config, Debug, PartialEq)]
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

#[derive(Config, Debug, PartialEq)]
struct NestedConfig {
    #[configulator(name = "app-name", default = "myapp")]
    app_name: String,

    #[configulator(name = "database", nested)]
    database: DatabaseConfig,
}

impl Validate for NestedConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[derive(Config, Debug, PartialEq)]
struct DatabaseConfig {
    #[configulator(name = "url", default = "postgres://localhost/db")]
    url: String,

    #[configulator(name = "max-connections", default = "10")]
    max_connections: u32,

    #[configulator(name = "pool", nested)]
    pool: PoolConfig,
}

#[derive(Config, Debug, PartialEq)]
struct PoolConfig {
    #[configulator(name = "size", default = "5")]
    size: u16,
}

#[derive(Config, Debug, PartialEq)]
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

#[derive(Config, Debug, PartialEq)]
struct CollectionsConfig {
    #[configulator(name = "labels")]
    labels: HashMap<String, String>,

    #[configulator(name = "servers", nested)]
    servers: Vec<ServerConfig>,

    #[configulator(name = "pools", nested)]
    pools: HashMap<String, PoolConfig>,
}

impl Validate for CollectionsConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[derive(Config, Debug, PartialEq)]
struct ServerConfig {
    #[configulator(name = "addr")]
    addr: String,

    #[configulator(name = "weight", default = "1")]
    weight: u16,
}

#[derive(Config, Debug, PartialEq)]
struct OptionalsConfig {
    #[configulator(name = "port")]
    port: Option<u16>,

    #[configulator(name = "name", default = "opt-name")]
    name: Option<String>,

    #[configulator(name = "tls", nested)]
    tls: Option<TlsConfig>,
}

impl Validate for OptionalsConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[derive(Config, Debug, PartialEq)]
struct TlsConfig {
    #[configulator(name = "cert")]
    cert: String,

    #[configulator(name = "min-version", default = "12")]
    min_version: u16,
}

#[derive(Debug, Default, PartialEq, Clone)]
enum LogLevel {
    #[default]
    Info,
    Debug,
}

impl std::str::FromStr for LogLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            other => Err(format!("bad log level: {other}")),
        }
    }
}

fn yaml_file(contents: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

fn yaml_opts<C: configulator::HasShadow>(path: &std::path::Path) -> FileOptions<C>
where
    C::Shadow: for<'de> serde::Deserialize<'de>,
{
    FileOptions {
        paths: vec![path.to_path_buf()],
        ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
    }
}

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn env_opts(prefix: &str) -> EnvironmentVariableOptions {
    EnvironmentVariableOptions {
        prefix: prefix.to_string(),
        separator: "_".to_string(),
    }
}

#[test]
fn defaults_scalars() {
    let config = Configulator::<SimpleConfig>::new().load().unwrap();
    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 8080);
    assert!(!config.debug);
}

#[test]
fn defaults_nested() {
    let config = Configulator::<NestedConfig>::new()
        .load_without_validation()
        .unwrap();
    assert_eq!(config.app_name, "myapp");
    assert_eq!(config.database.url, "postgres://localhost/db");
    assert_eq!(config.database.max_connections, 10);
    assert_eq!(config.database.pool.size, 5);
}

#[test]
fn defaults_list_and_optionals() {
    let config = Configulator::<ListConfig>::new().load().unwrap();
    assert_eq!(config.tags, vec!["a", "b", "c"]);
    assert!(config.ports.is_empty());

    let config = Configulator::<OptionalsConfig>::new().load().unwrap();
    assert_eq!(config.port, None);
    assert_eq!(config.name.as_deref(), Some("opt-name"));
    assert_eq!(config.tls, None);
}

#[test]
fn defaults_only_constructor() {
    let config = Configulator::<SimpleConfig>::defaults_only().unwrap();
    assert_eq!(config.port, 8080);
}

#[test]
fn defaults_report_origins() {
    let (_, report) = Configulator::<SimpleConfig>::new()
        .load_with_report()
        .unwrap();
    let origin = report.origin("port").unwrap();
    assert_eq!(origin.layer, Layer::Default);
    assert_eq!(origin.detail, "default");
}

#[test]
fn validation_runs_on_load() {
    let err = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--port", "0"]))
        .load()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::ValidationError(_)));

    let ok = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--port", "0"]))
        .load_without_validation()
        .unwrap();
    assert_eq!(ok.port, 0);
}

#[test]
fn file_yaml_basic_and_report() {
    let f = yaml_file("host: example.com\nport: 9090\n");
    let (config, report) = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load_with_report()
        .unwrap();
    assert_eq!(config.host, "example.com");
    assert_eq!(config.port, 9090);
    assert!(!config.debug);
    assert_eq!(report.origin("port").unwrap().layer, Layer::File);
    assert_eq!(report.file().unwrap(), f.path().display().to_string());
}

#[test]
fn file_quoted_numbers_parse() {
    let f = yaml_file("port: \"9999\"\n");
    let config = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(config.port, 9999);
}

#[test]
fn file_null_is_absent() {
    let f = yaml_file("port: ~\nhost: kept\n");
    let config = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(config.port, 8080, "null keeps the default");
    assert_eq!(config.host, "kept");
}

#[test]
fn file_unknown_key_rejected() {
    let f = yaml_file("nope: 1\n");
    let err = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::FileError(msg) => assert!(msg.contains("nope"), "got: {msg}"),
        other => panic!("expected FileError, got {other:?}"),
    }
}

#[test]
fn file_parse_error_names_field() {
    let f = yaml_file("port: not-a-number\n");
    let err = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::FileError(msg) => assert!(msg.contains("port"), "got: {msg}"),
        other => panic!("expected FileError, got {other:?}"),
    }
}

#[test]
fn file_nested_deep_merge() {
    let f = yaml_file("database:\n  pool:\n    size: 99\n");
    let config = Configulator::<NestedConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load_without_validation()
        .unwrap();
    assert_eq!(config.database.pool.size, 99);
    assert_eq!(
        config.database.url, "postgres://localhost/db",
        "sibling default survives"
    );
}

#[test]
fn file_not_found_soft_and_hard() {
    let config = Configulator::<SimpleConfig>::new()
        .with_file(FileOptions {
            paths: vec!["/nonexistent/config.yaml".into()],
            ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
        })
        .load()
        .unwrap();
    assert_eq!(config.port, 8080, "soft miss boots on defaults");

    let err = Configulator::<SimpleConfig>::new()
        .with_file(FileOptions {
            paths: vec!["/nonexistent/config.yaml".into()],
            error_if_not_found: true,
            ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
        })
        .load()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::FileNotFound));
}

#[test]
fn file_explicit_must_exist_no_fallback() {
    let good = yaml_file("host: from-search\n");
    let err = Configulator::<SimpleConfig>::new()
        .with_file(FileOptions {
            paths: vec![good.path().to_path_buf()],
            explicit: Some("/nonexistent/app.yaml".into()),
            ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
        })
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::ExplicitFileError { path, .. } => {
            assert_eq!(path, std::path::PathBuf::from("/nonexistent/app.yaml"));
        }
        other => panic!("expected ExplicitFileError, got {other:?}"),
    }
}

#[test]
fn file_config_flag_is_explicit() {
    let good = yaml_file("host: from-search\n");
    let err = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts::<SimpleConfig>(good.path()))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--config", "/nonexistent/app.yaml"]))
        .load()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::ExplicitFileError { .. }));

    let cli_file = yaml_file("host: from-cli-file\n");
    let config = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts::<SimpleConfig>(good.path()))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--config", &cli_file.path().display().to_string()]))
        .load()
        .unwrap();
    assert_eq!(config.host, "from-cli-file");
}

#[test]
fn file_collections() {
    let f = yaml_file(
        "labels:\n  team: infra\nservers:\n  - addr: a:1\n  - addr: b:2\n    weight: 5\npools:\n  primary: {}\n",
    );
    let (config, report) = Configulator::<CollectionsConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load_with_report()
        .unwrap();
    assert_eq!(config.labels["team"], "infra");
    assert_eq!(config.servers.len(), 2);
    assert_eq!(config.servers[0].weight, 1, "element default");
    assert_eq!(config.servers[1].weight, 5);
    assert_eq!(config.pools["primary"].size, 5, "map element default");

    assert_eq!(report.origin("servers[0].addr").unwrap().layer, Layer::File);
    let elem_default = report.origin("servers[0].weight").unwrap();
    assert_eq!(elem_default.layer, Layer::Default);
    assert_eq!(elem_default.detail, "element default");
    assert_eq!(
        report.origin("pools.primary.size").unwrap().detail,
        "element default"
    );
}

#[test]
fn file_optional_struct_allocated_with_element_defaults() {
    let f = yaml_file("tls:\n  cert: c.pem\n");
    let config = Configulator::<OptionalsConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    let tls = config.tls.expect("tls allocated by file layer");
    assert_eq!(tls.cert, "c.pem");
    assert_eq!(tls.min_version, 12, "field default applies on allocation");
}

#[test]
fn file_custom_fromstr_leaf() {
    #[derive(Config, Debug)]
    struct LevelConfig {
        #[configulator(name = "level", default = "info")]
        level: LogLevel,
    }
    impl Validate for LevelConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    let f = yaml_file("level: debug\n");
    let config = Configulator::<LevelConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(config.level, LogLevel::Debug);

    let f = yaml_file("level: nope\n");
    let err = Configulator::<LevelConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::FileError(msg) => {
            assert!(msg.contains("bad log level"), "got: {msg}");
            assert!(msg.contains("level"), "field name in error: {msg}");
        }
        other => panic!("expected FileError, got {other:?}"),
    }
}

#[test]
fn env_basic_and_nested_naming() {
    let (config, report) = Configulator::<NestedConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[
            ("APP_APP_NAME", "from-env"),
            ("APP_DATABASE_POOL_SIZE", "42"),
        ]))
        .load_with_report()
        .unwrap();
    assert_eq!(config.app_name, "from-env", "dash folds to underscore");
    assert_eq!(config.database.pool.size, 42);
    let origin = report.origin("database.pool.size").unwrap();
    assert_eq!(origin.layer, Layer::Env);
    assert_eq!(origin.detail, "APP_DATABASE_POOL_SIZE");
}

#[test]
fn env_prefix_is_verbatim() {
    let config = Configulator::<SimpleConfig>::new()
        .with_environment_variables(env_opts("X"))
        .with_env_vars(env(&[("XHOST", "glued")]))
        .load()
        .unwrap();
    assert_eq!(config.host, "glued");
}

#[test]
fn env_empty_string_is_a_value() {
    let config = Configulator::<SimpleConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_HOST", "")]))
        .load()
        .unwrap();
    assert_eq!(
        config.host, "",
        "present but empty is the empty string, not the default"
    );
}

#[test]
fn env_list_no_trimming() {
    let config = Configulator::<ListConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_TAGS", "p, q")]))
        .load()
        .unwrap();
    assert_eq!(config.tags, vec!["p", " q"], "no trimming (SPEC)");
}

#[test]
fn env_array_separator() {
    let config = Configulator::<ListConfig>::new()
        .with_array_separator(";")
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_TAGS", "x;y")]))
        .load()
        .unwrap();
    assert_eq!(config.tags, vec!["x", "y"]);

    #[derive(Config, Debug)]
    struct SepConfig {
        #[configulator(name = "vals", default = "1;2;3")]
        vals: Vec<u16>,
    }
    impl Validate for SepConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let config = Configulator::<SepConfig>::new()
        .with_array_separator(";")
        .load()
        .unwrap();
    assert_eq!(config.vals, vec![1, 2, 3]);
}

#[test]
fn env_parse_error_names_var() {
    let err = Configulator::<SimpleConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_PORT", "lots")]))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::ParseError { field, value, .. } => {
            assert_eq!(field, "APP_PORT");
            assert_eq!(value, "lots");
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

#[test]
fn env_bad_options() {
    let err = Configulator::<SimpleConfig>::new()
        .with_environment_variables(env_opts("app_"))
        .load()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::BadEnvOptions(_)));

    let err = Configulator::<SimpleConfig>::new()
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: "APP".into(),
            separator: "-".into(),
        })
        .load()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::BadEnvOptions(_)));
}

#[test]
fn env_override_and_skip() {
    #[derive(Config, Debug)]
    #[allow(dead_code)]
    struct EnvAttrConfig {
        #[configulator(name = "host", env = "HOSTNAME_OVERRIDE")]
        host: String,

        #[configulator(name = "internal", env = "-", default = "hidden")]
        internal: String,
    }
    impl Validate for EnvAttrConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    let (config, report) = Configulator::<EnvAttrConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[
            ("APP_HOSTNAME_OVERRIDE", "from-override"),
            ("APP_INTERNAL", "should-be-ignored"),
        ]))
        .load_with_report()
        .unwrap();
    assert_eq!(config.host, "from-override");
    assert_eq!(config.internal, "hidden", "env = \"-\" skips the env layer");
    assert_eq!(
        report.origin("host").unwrap().detail,
        "APP_HOSTNAME_OVERRIDE"
    );
}

#[test]
fn env_optionals() {
    let config = Configulator::<OptionalsConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_PORT", "123"), ("APP_TLS_CERT", "e.pem")]))
        .load()
        .unwrap();
    assert_eq!(config.port, Some(123));
    let tls = config.tls.expect("env allocates the optional struct");
    assert_eq!(tls.cert, "e.pem");
    assert_eq!(tls.min_version, 12);
}

#[test]
fn env_untouched_optional_struct_stays_none() {
    let config = Configulator::<OptionalsConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_PORT", "123")]))
        .load()
        .unwrap();
    assert_eq!(config.tls, None, "no tls var set: stays None");
}

#[test]
fn cli_flags_scalars_and_bool() {
    let config = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--host", "cli.example", "--debug"]))
        .load()
        .unwrap();
    assert_eq!(config.host, "cli.example");
    assert!(config.debug, "bare --debug sets true");

    let config = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--debug", "false"]))
        .load()
        .unwrap();
    assert!(!config.debug, "--debug false sets false");
}

#[test]
fn cli_nested_and_report() {
    let (config, report) = Configulator::<NestedConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--database.pool.size", "77"]))
        .load_with_report()
        .unwrap();
    assert_eq!(config.database.pool.size, 77);
    let origin = report.origin("database.pool.size").unwrap();
    assert_eq!(origin.layer, Layer::Cli);
    assert_eq!(origin.detail, "--database.pool.size");
}

#[test]
fn cli_list_repeated() {
    let config = Configulator::<ListConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--ports", "80", "--ports", "443"]))
        .load()
        .unwrap();
    assert_eq!(config.ports, vec![80, 443]);
}

#[test]
fn cli_unknown_flag_errors() {
    let err = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--nope", "1"]))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::CLIError(e) => {
            assert_eq!(e.kind(), clap::error::ErrorKind::UnknownArgument);
        }
        other => panic!("expected CLIError, got {other:?}"),
    }
}

#[test]
fn cli_help_passes_through_as_clap_error() {
    let err = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--help"]))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::CLIError(e) => {
            assert_eq!(e.kind(), clap::error::ErrorKind::DisplayHelp);
            let rendered = e.to_string();
            assert!(rendered.contains("--port"), "help lists flags: {rendered}");
        }
        other => panic!("expected CLIError, got {other:?}"),
    }
}

#[test]
fn cli_parse_error_names_flag() {
    let err = Configulator::<SimpleConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--port", "lots"]))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::ParseError { field, .. } => assert_eq!(field, "--port"),
        other => panic!("expected ParseError, got {other:?}"),
    }
}

#[test]
fn cli_short_flag() {
    #[derive(Config, Debug)]
    struct ShortConfig {
        #[configulator(name = "port", default = "1", short = 'p')]
        port: u16,
    }
    impl Validate for ShortConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let config = Configulator::<ShortConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["-p", "99"]))
        .load()
        .unwrap();
    assert_eq!(config.port, 99);
}

#[test]
fn cli_flag_override_and_skip() {
    #[derive(Config, Debug)]
    #[allow(dead_code)]
    struct FlagAttrConfig {
        #[configulator(name = "host", flag = "hostname")]
        host: String,

        #[configulator(name = "internal", flag = "-", default = "hidden")]
        internal: String,
    }
    impl Validate for FlagAttrConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let config = Configulator::<FlagAttrConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--hostname", "renamed"]))
        .load()
        .unwrap();
    assert_eq!(config.host, "renamed");

    let err = Configulator::<FlagAttrConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--internal", "x"]))
        .load()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::CLIError(_)),
        "flag = \"-\" unregisters the flag"
    );
}

#[test]
fn cli_custom_command() {
    let config = Configulator::<SimpleConfig>::new()
        .with_cli_command(clap::Command::new("myapp").version("1.0"))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--port", "1234"]))
        .load()
        .unwrap();
    assert_eq!(config.port, 1234);
}

#[test]
fn cli_custom_command_version() {
    let err = Configulator::<SimpleConfig>::new()
        .with_cli_command(clap::Command::new("myapp").version("1.0"))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--version"]))
        .load()
        .unwrap_err();
    let ConfigulatorError::CLIError(e) = err else {
        panic!("expected CLIError, got {err}");
    };
    assert_eq!(e.kind(), clap::error::ErrorKind::DisplayVersion);
    assert_eq!(e.to_string(), "myapp 1.0\n");
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct VersionFieldConfig {
    #[configulator(name = "version", default = "1")]
    version: u32,
    #[configulator(name = "verbose", short = 'V', flag = "-", default = "false")]
    verbose: bool,
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct ShortVConfig {
    #[configulator(name = "verbose", short = 'V', default = "false")]
    verbose: bool,
}

#[test]
fn version_flag_conflicts_only_when_the_command_has_a_version() {
    let config = Configulator::<VersionFieldConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--version", "3"]))
        .load_without_validation()
        .unwrap();
    assert_eq!(config.version, 3);

    let err = Configulator::<VersionFieldConfig>::new()
        .with_cli_command(clap::Command::new("myapp").version("1.0"))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&[]))
        .load_without_validation()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict(ref f) if f == "--version"),
        "{err}"
    );

    let err = Configulator::<ShortVConfig>::new()
        .with_cli_command(clap::Command::new("myapp").version("1.0"))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&[]))
        .load_without_validation()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict(ref f) if f == "-V"),
        "{err}"
    );
}

#[test]
fn precedence_defaults_file_env_cli() {
    let f = yaml_file("host: from-file\nport: 1000\n");
    let (config, report) = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_PORT", "2000"), ("APP_DEBUG", "true")]))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--port", "3000"]))
        .load_with_report()
        .unwrap();
    assert_eq!(config.host, "from-file", "file beats default");
    assert!(config.debug, "env beats file");
    assert_eq!(config.port, 3000, "cli beats env");
    assert_eq!(report.origin("port").unwrap().layer, Layer::Cli);
    assert_eq!(report.origin("host").unwrap().layer, Layer::File);
    assert_eq!(report.origin("debug").unwrap().layer, Layer::Env);
}

#[test]
fn precedence_deep_merge_across_layers() {
    let f = yaml_file("database:\n  url: postgres://file/db\n");
    let config = Configulator::<NestedConfig>::new()
        .with_file(yaml_opts(f.path()))
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_DATABASE_MAX_CONNECTIONS", "50")]))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--database.pool.size", "7"]))
        .load_without_validation()
        .unwrap();
    assert_eq!(config.database.url, "postgres://file/db");
    assert_eq!(config.database.max_connections, 50);
    assert_eq!(config.database.pool.size, 7);
}

#[test]
fn list_replaces_wholesale() {
    let f = yaml_file("tags: [x, y, z]\n");
    let config = Configulator::<ListConfig>::new()
        .with_file(yaml_opts(f.path()))
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_TAGS", "p,q")]))
        .load()
        .unwrap();
    assert_eq!(
        config.tags,
        vec!["p", "q"],
        "env list replaces file list wholesale"
    );
}

#[test]
fn required_field() {
    #[derive(Config, Debug)]
    struct RequiredConfig {
        #[configulator(name = "token", required)]
        token: String,
    }
    impl Validate for RequiredConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let err = Configulator::<RequiredConfig>::new().load().unwrap_err();
    match err {
        ConfigulatorError::Required { path } => assert_eq!(path, "token"),
        other => panic!("expected Required, got {other:?}"),
    }

    let config = Configulator::<RequiredConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_TOKEN", "sekrit")]))
        .load()
        .unwrap();
    assert_eq!(config.token, "sekrit");
}

#[test]
fn secret_redacted_in_errors_and_print() {
    #[derive(Config, Debug)]
    #[allow(dead_code)]
    struct SecretConfig {
        #[configulator(name = "api-key", secret, default = "topsecret")]
        api_key: String,

        #[configulator(name = "port", default = "8080")]
        port: u16,

        #[configulator(name = "burst", secret, default = "1")]
        burst: u16,
    }
    impl Validate for SecretConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    let printed = Configulator::<SecretConfig>::defaults_only()
        .unwrap()
        .print_config();
    assert!(printed.contains("api-key = (redacted)"), "got: {printed}");
    assert!(printed.contains("port = 8080"), "got: {printed}");
    assert!(!printed.contains("topsecret"), "got: {printed}");

    let err = Configulator::<SecretConfig>::new()
        .with_environment_variables(env_opts("APP_"))
        .with_env_vars(env(&[("APP_BURST", "not-a-number")]))
        .load()
        .unwrap_err();
    match err {
        ConfigulatorError::ParseError { value, .. } => assert_eq!(value, "(redacted)"),
        other => panic!("expected ParseError, got {other:?}"),
    }
}

#[test]
fn print_config_nested_and_collections() {
    let f = yaml_file("labels:\n  team: infra\nservers:\n  - addr: a:1\n");
    let config = Configulator::<CollectionsConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    let printed = config.print_config();
    assert!(
        printed.contains("labels.team = \"infra\""),
        "got: {printed}"
    );
    assert!(
        printed.contains("servers[0].addr = \"a:1\""),
        "got: {printed}"
    );
    assert!(printed.contains("servers[0].weight = 1"), "got: {printed}");
    assert!(printed.contains("pools = {}"), "got: {printed}");
}

#[test]
fn duration_field() {
    #[derive(Config, Debug)]
    struct DurationConfig {
        #[configulator(name = "timeout", default = "30s")]
        timeout: Duration,
    }
    impl Validate for DurationConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let config = Configulator::<DurationConfig>::new().load().unwrap();
    assert_eq!(config.timeout.get(), std::time::Duration::from_secs(30));

    let f = yaml_file("timeout: 1h30m\n");
    let config = Configulator::<DurationConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(config.timeout.get(), std::time::Duration::from_secs(5400));
    assert_eq!(config.timeout.to_string(), "1h30m0s");
}

#[test]
fn field_named_like_old_sentinel_survives() {
    #[derive(Config, Debug)]
    struct SentinelConfig {
        #[configulator(name = "__config_file__", default = "keep-me")]
        config_file: String,
    }
    impl Validate for SentinelConfig {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let config = Configulator::<SentinelConfig>::new().load().unwrap();
    assert_eq!(config.config_file, "keep-me");
}

#[test]
fn schema_and_sample() {
    #[derive(Config, Debug)]
    #[allow(dead_code)]
    struct SchemaCfg {
        #[configulator(name = "port", default = "8080", required, description = "listen port")]
        port: u16,

        #[configulator(name = "key", secret)]
        key: String,

        #[configulator(name = "sub", nested)]
        sub: SchemaSub,

        #[configulator(name = "tags", default = "a,b")]
        tags: Vec<String>,
    }
    #[derive(Config, Debug)]
    #[allow(dead_code)]
    struct SchemaSub {
        #[configulator(name = "host", default = "localhost", description = "bind host")]
        host: String,
    }
    impl Validate for SchemaCfg {
        fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    let schema = Configulator::<SchemaCfg>::json_schema();
    for want in [
        "\"required\"",
        "\"port\"",
        "\"listen port\"",
        "\"additionalProperties\": false",
        "\"default\": 8080",
        "\"title\": \"Configuration\"",
        "\"type\": \"integer\"",
    ] {
        assert!(schema.contains(want), "schema missing {want}:\n{schema}");
    }

    let sample = Configulator::<SchemaCfg>::sample_config();
    for want in [
        "port: 8080",
        "key: \"(secret)\"",
        "# bind host",
        "host: \"localhost\"",
        "tags: [\"a\", \"b\"]",
    ] {
        assert!(sample.contains(want), "sample missing {want}:\n{sample}");
    }
}

#[derive(Config, Debug)]
struct RequiredNestedConfig {
    #[configulator(name = "db", nested, required)]
    db: RequiredDb,
}

#[derive(Config, Debug)]
struct RequiredDb {
    #[configulator(name = "host")]
    host: String,
}

#[test]
fn required_nested_struct_is_set_by_its_fields() {
    let config = Configulator::<RequiredNestedConfig>::new()
        .with_environment_variables(env_opts("T_"))
        .with_env_vars(env(&[("T_DB_HOST", "h")]))
        .load_without_validation()
        .unwrap();
    assert_eq!(config.db.host, "h");

    let err = Configulator::<RequiredNestedConfig>::new()
        .with_environment_variables(env_opts("T_"))
        .with_env_vars(env(&[]))
        .load_without_validation()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::Required { path } if path == "db"));
}

#[derive(Config, Debug)]
struct OptionalSectionConfig {
    #[configulator(name = "db", nested)]
    db: Option<OptionalSectionDb>,
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct OptionalSectionDb {
    #[configulator(name = "host", required)]
    host: String,
    #[configulator(name = "port")]
    port: Option<u16>,
}

#[test]
fn unset_optional_section_skips_its_required_fields() {
    let config = Configulator::<OptionalSectionConfig>::new()
        .with_environment_variables(env_opts("T_"))
        .with_env_vars(env(&[]))
        .load_without_validation()
        .unwrap();
    assert!(config.db.is_none());

    let err = Configulator::<OptionalSectionConfig>::new()
        .with_environment_variables(env_opts("T_"))
        .with_env_vars(env(&[("T_DB_PORT", "5432")]))
        .load_without_validation()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::Required { path } if path == "db.host"));
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct ShortCConfig {
    #[configulator(name = "concurrency", short = 'c', default = "1")]
    concurrency: u32,
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct DuplicateShortConfig {
    #[configulator(name = "port", short = 'p', default = "1")]
    port: u16,
    #[configulator(name = "path", short = 'p', default = "x")]
    path: String,
}

#[test]
fn flag_conflicts_are_errors_not_panics() {
    let f = yaml_file("concurrency: 2\n");
    let err = Configulator::<ShortCConfig>::new()
        .with_file(yaml_opts(f.path()))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&[]))
        .load_without_validation()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict(ref f) if f == "-c"),
        "{err}"
    );

    let err = Configulator::<DuplicateShortConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&[]))
        .load_without_validation()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict(ref f) if f == "-p"),
        "{err}"
    );

    let err = Configulator::<SimpleConfig>::new()
        .with_cli_command(clap::Command::new("app").arg(clap::Arg::new("port").long("port")))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&[]))
        .load_without_validation()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict(ref f) if f == "--port"),
        "{err}"
    );
}

#[derive(Config, Debug)]
struct RawIdentConfig {
    #[configulator(default = "a")]
    r#type: String,
}

#[test]
fn raw_identifier_fields() {
    let f = yaml_file("type: b\n");
    let config = Configulator::<RawIdentConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load_without_validation()
        .unwrap();
    assert_eq!(config.r#type, "b");

    let config = Configulator::<RawIdentConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--type", "c"]))
        .load_without_validation()
        .unwrap();
    assert_eq!(config.r#type, "c");
}

#[derive(Config, Debug)]
struct ElementSepConfig {
    #[configulator(name = "servers", nested)]
    servers: Vec<ElementSepServer>,
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct ElementSepServer {
    #[configulator(name = "addr")]
    addr: String,
    #[configulator(name = "tags", default = "a;b")]
    tags: Vec<String>,
}

#[test]
fn element_defaults_use_the_array_separator() {
    let f = yaml_file("servers:\n  - addr: x\n");
    let config = Configulator::<ElementSepConfig>::new()
        .with_file(yaml_opts(f.path()))
        .with_array_separator(";")
        .load_without_validation()
        .unwrap();
    assert_eq!(config.servers[0].tags, vec!["a", "b"]);
}

#[allow(dead_code)]
#[derive(Config, Debug)]
#[configulator(allow_unknown_fields)]
struct LooseConfig {
    #[configulator(name = "a", default = "1")]
    a: u16,
    #[configulator(name = "strict", nested)]
    strict: PoolConfig,
}

#[test]
fn schema_follows_allow_unknown_fields() {
    let schema = Configulator::<LooseConfig>::json_schema();
    assert_eq!(
        schema.matches("\"additionalProperties\": false").count(),
        1,
        "{schema}"
    );
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct EnvSkipConfig {
    #[configulator(name = "db", nested, env = "-")]
    db: PoolConfig,
}

#[test]
fn markdown_skips_env_for_children_of_env_skipped_struct() {
    use configulator::HasShadow;
    let md = configulator::__schema::markdown(&EnvSkipConfig::fields(), ".", "T_", "_");
    assert!(!md.contains("T_DB_SIZE"), "{md}");
    assert!(md.contains("--db.size"), "{md}");
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct EmptyDefaultConfig {
    #[configulator(name = "user", default = "")]
    user: String,
}

#[test]
fn markdown_leaves_empty_default_cell_blank() {
    use configulator::HasShadow;
    let md = configulator::__schema::markdown(&EmptyDefaultConfig::fields(), ".", "", "_");
    assert!(!md.contains("``"), "{md}");
}

#[derive(Default)]
struct NoDebug(String);

impl std::str::FromStr for NoDebug {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(NoDebug(s.to_string()))
    }
}

#[derive(Config)]
struct NoDebugConfig {
    #[configulator(name = "token", default = "x")]
    token: NoDebug,
    #[configulator(name = "port", default = "1")]
    port: u16,
}

#[test]
fn print_config_without_debug() {
    let config = Configulator::<NoDebugConfig>::new()
        .load_without_validation()
        .unwrap();
    assert_eq!(config.token.0, "x");
    let out = config.print_config();
    assert!(out.contains("token = (no Debug impl)"), "{out}");
    assert!(out.contains("port = 1"), "{out}");
}

#[derive(Config, Debug, PartialEq)]
struct NcRule {
    #[configulator(name = "from")]
    from: i32,
    #[configulator(name = "range", default = "1")]
    range: i32,
    #[configulator(name = "on", default = "true")]
    on: bool,
}

#[derive(Config, Debug, PartialEq)]
struct NcLevel {
    #[configulator(name = "level", default = "7")]
    level: i32,
}

#[derive(Config, Debug, PartialEq)]
struct NcPeer {
    #[configulator(name = "name")]
    name: String,
    #[configulator(name = "slots", default = "3")]
    slots: i32,
    #[configulator(name = "rules", nested)]
    rules: Vec<NcRule>,
    #[configulator(name = "tags", nested)]
    tags: HashMap<String, NcRule>,
    #[configulator(name = "inner", nested)]
    inner: NcLevel,
    #[configulator(name = "opt", nested)]
    opt: Option<NcLevel>,
}

#[derive(Config, Debug, PartialEq)]
struct NcCfg {
    #[configulator(name = "peers", nested)]
    peers: Vec<NcPeer>,
    #[configulator(name = "by-name", nested)]
    by_name: HashMap<String, NcPeer>,
}

impl Validate for NcCfg {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[test]
fn nested_collections_inside_collection_elements() {
    let f = yaml_file(
        r#"{"peers":[{"name":"a","rules":[{"from":1},{"from":2,"range":5,"on":false}],"tags":{"x":{"from":9}},"opt":{}}],"by-name":{"b":{"name":"b","rules":[{"from":4}]}}}"#,
    );
    let (config, report) = Configulator::<NcCfg>::new()
        .with_file(yaml_opts(f.path()))
        .load_with_report()
        .unwrap();
    let rule = |from, range, on| NcRule { from, range, on };
    let p = &config.peers[0];
    assert_eq!(p.name, "a");
    assert_eq!(p.slots, 3);
    assert_eq!(p.rules, vec![rule(1, 1, true), rule(2, 5, false)]);
    assert_eq!(p.tags["x"], rule(9, 1, true));
    assert_eq!(p.inner.level, 7);
    assert_eq!(p.opt, Some(NcLevel { level: 7 }));
    let b = &config.by_name["b"];
    assert_eq!(b.slots, 3);
    assert_eq!(b.rules, vec![rule(4, 1, true)]);

    let o = |path: &str| report.origin(path).map(|o| (o.layer, o.detail.clone()));
    assert_eq!(
        o("peers[0].rules[0].range"),
        Some((Layer::Default, "element default".into()))
    );
    assert_eq!(o("peers[0].rules[1].range").map(|x| x.0), Some(Layer::File));
    assert_eq!(
        o("by-name.b.rules[0].on").map(|x| x.0),
        Some(Layer::Default)
    );
    assert_eq!(o("peers[0].opt.level").map(|x| x.0), Some(Layer::Default));
}

#[derive(Config, Debug, PartialEq)]
struct SampleCfg {
    #[configulator(name = "peers", nested, description = "the peers")]
    peers: Vec<NcPeer>,
    #[configulator(name = "by-name", nested)]
    by_name: HashMap<String, NcPeer>,
    #[configulator(name = "labels")]
    labels: HashMap<String, String>,
    #[configulator(name = "tls", nested)]
    tls: Option<TlsConfig>,
    #[configulator(name = "token", secret)]
    token: Option<String>,
    #[configulator(name = "mode", default = "fast")]
    mode: Option<String>,
    #[configulator(name = "on")]
    on: Option<bool>,
    #[configulator(name = "port", default = "8080")]
    port: u16,
}

impl Validate for SampleCfg {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

fn load_sample(text: &str) -> SampleCfg {
    let f = yaml_file(text);
    Configulator::<SampleCfg>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap_or_else(|e| panic!("{e}\n{text}"))
}

#[test]
fn sample_loads_as_is_and_uncommented() {
    let sample = Configulator::<SampleCfg>::sample_config();
    let config = load_sample(&sample);
    assert!(config.peers.is_empty() && config.by_name.is_empty() && config.labels.is_empty());
    assert_eq!(config.tls, None);
    assert_eq!(config.token, None);
    assert_eq!(config.mode.as_deref(), Some("fast"));
    assert_eq!(config.on, None);
    assert_eq!(config.port, 8080);

    let key = |s: &str| {
        let s = s.trim_start_matches("- ");
        s.split_once(':').is_some_and(|(k, _)| {
            !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    };
    let uncommented: String = sample
        .lines()
        .map(|l| {
            let body = l.trim_start();
            let ind = &l[..l.len() - body.len()];
            match body.strip_prefix("# ") {
                Some(rest) if key(rest.trim_start()) => format!("{ind}{rest}\n"),
                _ => format!("{l}\n"),
            }
        })
        .collect();
    let config = load_sample(&uncommented);
    let peer = &config.peers[0];
    assert_eq!(peer.slots, 3);
    assert_eq!(peer.rules[0].range, 1);
    assert!(peer.rules[0].on);
    assert_eq!(peer.tags["example"].range, 1);
    assert_eq!(peer.opt, Some(NcLevel { level: 7 }));
    assert_eq!(config.by_name["example"].inner.level, 7);
    assert_eq!(config.labels["example"], "");
    assert_eq!(config.tls.as_ref().unwrap().min_version, 12);
    assert_eq!(config.token.as_deref(), Some("(secret)"));
    assert_eq!(config.on, Some(false));
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct SecretDefaultConfig {
    #[configulator(name = "token", secret, default = "hunter2")]
    token: String,
}

impl Validate for SecretDefaultConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[test]
fn secret_defaults_stay_out_of_generated_docs() {
    use configulator::HasShadow;
    let schema = Configulator::<SecretDefaultConfig>::json_schema();
    let sample = Configulator::<SecretDefaultConfig>::sample_config();
    let md = configulator::__schema::markdown(&SecretDefaultConfig::fields(), ".", "", "_");
    for out in [&schema, &sample, &md] {
        assert!(!out.contains("hunter2"), "{out}");
    }
    assert!(sample.contains("# token: \"(secret)\""), "{sample}");
    let config = Configulator::<SecretDefaultConfig>::new().load().unwrap();
    assert_eq!(config.token, "hunter2");
    let f = yaml_file(&sample);
    let config = Configulator::<SecretDefaultConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(config.token, "hunter2");
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct ReqServer {
    #[configulator(name = "addr", required)]
    addr: String,
    #[configulator(name = "weight", default = "1")]
    weight: u16,
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct ReqElementsConfig {
    #[configulator(name = "servers", nested)]
    servers: Vec<ReqServer>,
    #[configulator(name = "pools", nested)]
    pools: HashMap<String, ReqServer>,
}

#[test]
fn required_is_checked_per_collection_element() {
    let load = |text: &str| {
        let f = yaml_file(text);
        Configulator::<ReqElementsConfig>::new()
            .with_file(yaml_opts(f.path()))
            .load_without_validation()
    };
    let required_path = |text: &str| match load(text).unwrap_err() {
        ConfigulatorError::Required { path } => path,
        e => panic!("{e}"),
    };
    assert_eq!(
        required_path("servers:\n  - addr: a\n  - weight: 2\n"),
        "servers[1].addr"
    );
    assert_eq!(
        required_path("pools:\n  p:\n    weight: 2\n"),
        "pools.p.addr"
    );
    let config = load("servers:\n  - addr: a\n").unwrap();
    assert_eq!(config.servers[0].addr, "a");
    assert!(load("{}\n").unwrap().servers.is_empty());
}

#[allow(dead_code)]
#[derive(Config, Debug)]
struct ShortFlagConfig {
    #[configulator(name = "port", short = 'p', default = "1")]
    port: u16,
    #[configulator(name = "host", default = "h")]
    host: String,
    #[configulator(name = "hidden", short = 'x', flag = "-", default = "1")]
    hidden: u16,
}

#[test]
fn markdown_flag_column_shows_shorthand() {
    use configulator::HasShadow;
    let md = configulator::__schema::markdown(&ShortFlagConfig::fields(), ".", "", "_");
    assert!(md.contains("| `-p`, `--port` |"), "{md}");
    assert!(md.contains("| `--host` "), "{md}");
    assert!(!md.contains("`-x`"), "{md}");
}

#[derive(Config, Debug, PartialEq)]
struct ListDefaultsConfig {
    #[configulator(name = "origins", default = "*,https://*")]
    origins: Vec<String>,
    #[configulator(name = "ports", default = "80,443")]
    ports: Vec<u16>,
    #[configulator(name = "waits", default = "1s,2m")]
    waits: Vec<Duration>,
}

impl Validate for ListDefaultsConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[test]
fn list_defaults_render_as_valid_yaml_and_typed_schema() {
    let sample = Configulator::<ListDefaultsConfig>::sample_config();
    assert!(
        sample.contains(r#"origins: ["*", "https://*"]"#),
        "{sample}"
    );
    assert!(sample.contains("ports: [80, 443]"), "{sample}");
    assert!(sample.contains(r#"waits: ["1s", "2m"]"#), "{sample}");
    let f = yaml_file(&sample);
    let from_sample = Configulator::<ListDefaultsConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(
        from_sample,
        Configulator::<ListDefaultsConfig>::new().load().unwrap()
    );

    let schema = Configulator::<ListDefaultsConfig>::json_schema();
    let schema = schema.split_whitespace().collect::<String>();
    assert!(schema.contains(r#""default":[80,443]"#), "{schema}");
    assert!(
        schema.contains(r#""default":["*","https://*"]"#),
        "{schema}"
    );
}

#[derive(Config, Debug, PartialEq)]
struct ComplexConfig {
    #[configulator(name = "z", default = "1+2i")]
    z: configulator::Complex128,
    #[configulator(name = "exponent")]
    exponent: configulator::Complex64,
    #[configulator(name = "roots", default = "1i,2")]
    roots: Vec<configulator::Complex128>,
}

impl Validate for ComplexConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[test]
fn complex_numbers_in_every_layer() {
    use configulator::{Complex128, Complex64};
    let config = Configulator::<ComplexConfig>::new().load().unwrap();
    assert_eq!(config.z, Complex128::new(1.0, 2.0));
    assert_eq!(
        config.roots,
        vec![Complex128::new(0.0, 1.0), Complex128::new(2.0, 0.0)]
    );

    let f = yaml_file("z: \"(3-4i)\"\nexponent: 3\nroots: [\"5i\", 6]\n");
    let config = Configulator::<ComplexConfig>::new()
        .with_file(yaml_opts(f.path()))
        .load()
        .unwrap();
    assert_eq!(config.z, Complex128::new(3.0, -4.0));
    assert_eq!(config.exponent, Complex64::new(3.0, 0.0));
    assert_eq!(
        config.roots,
        vec![Complex128::new(0.0, 5.0), Complex128::new(6.0, 0.0)]
    );

    let config = Configulator::<ComplexConfig>::new()
        .with_environment_variables(env_opts("C_"))
        .with_env_vars(env(&[("C_Z", "2i"), ("C_ROOTS", "1+1i,-1-1i")]))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--exponent", "(0.5+0.5i)"]))
        .load()
        .unwrap();
    assert_eq!(config.z, Complex128::new(0.0, 2.0));
    assert_eq!(config.exponent, Complex64::new(0.5, 0.5));
    assert_eq!(
        config.roots,
        vec![Complex128::new(1.0, 1.0), Complex128::new(-1.0, -1.0)]
    );

    let err = Configulator::<ComplexConfig>::new()
        .with_environment_variables(env_opts("C_"))
        .with_env_vars(env(&[("C_Z", "1+2j")]))
        .load()
        .unwrap_err();
    assert!(matches!(err, ConfigulatorError::ParseError { .. }), "{err}");

    let schema = Configulator::<ComplexConfig>::json_schema();
    let schema = schema.split_whitespace().collect::<String>();
    assert!(
        schema.contains(r#""z":{"default":"1+2i","type":"string"}"#),
        "{schema}"
    );
    let sample = Configulator::<ComplexConfig>::sample_config();
    assert!(sample.contains(r#"z: "1+2i""#), "{sample}");
    assert!(sample.contains(r#"roots: ["1i", "2"]"#), "{sample}");
}

#[test]
fn empty_file_loads_as_no_keys_and_is_reported() {
    let f = yaml_file("");
    let (config, report) = Configulator::<SimpleConfig>::new()
        .with_file(FileOptions {
            paths: vec![f.path().to_path_buf()],
            ..FileOptions::new(serde_loader(|s| serde_json::from_str(s)))
        })
        .load_with_report()
        .unwrap();
    assert_eq!(config.port, 8080);
    assert_eq!(report.file(), Some(f.path().display().to_string().as_str()));

    let f = yaml_file("port: 9000\n");
    let (config, report) = Configulator::<SimpleConfig>::new()
        .with_file(yaml_opts(f.path()))
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--port", "9001"]))
        .load_with_report()
        .unwrap();
    assert_eq!(config.port, 9001);
    assert_eq!(report.file(), Some(f.path().display().to_string().as_str()));
}

#[derive(Config, Debug, PartialEq)]
struct ListOverrideConfig {
    #[configulator(name = "tags", env = "MY_TAGS", flag = "my-tags")]
    tags: Vec<String>,
}

impl Validate for ListOverrideConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[test]
fn env_and_flag_overrides_on_scalar_lists() {
    let config = Configulator::<ListOverrideConfig>::new()
        .with_environment_variables(env_opts("T_"))
        .with_env_vars(env(&[("T_MY_TAGS", "a,b")]))
        .load()
        .unwrap();
    assert_eq!(config.tags, vec!["a", "b"]);
    let config = Configulator::<ListOverrideConfig>::new()
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args(&["--my-tags", "c", "--my-tags", "d"]))
        .load()
        .unwrap();
    assert_eq!(config.tags, vec!["c", "d"]);
}
