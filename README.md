# Configulator

[![codecov](https://codecov.io/github/usa-reddragon/configulator-rs/graph/badge.svg?token=VcJUr2qOGS)](https://codecov.io/github/usa-reddragon/configulator-rs) [![License](https://badgen.net/github/license/USA-RedDragon/configulator-rs)](https://github.com/USA-RedDragon/configulator-rs/blob/main/LICENSE) [![GitHub contributors](https://badgen.net/github/contributors/USA-RedDragon/configulator-rs)](https://github.com/USA-RedDragon/configulator-rs/graphs/contributors/)

A simple configuration manager for Rust applications with derive macro support.

## Features

- Supports configuration from multiple sources with clear precedence:
  1. Default values (lowest)
  2. Config files (any serde format via `serde_loader`)
  3. Environment variables
  4. CLI flags (highest)
- `#[derive(Config)]` macro for declarative configuration structs
- Any serde-compatible file format - YAML, TOML, JSON, with a one-liner
- Pluggable file format support - bring your own parser via [`FileLoader`]
- Nested struct support
- [`Vec<T>`](Vec) list fields
- Custom types - anything implementing [`FromStr`](std::str::FromStr) + [`Default`]
- Optional validation via the [`Validate`] trait
- Boolean CLI flags (`--debug` sets true, `--debug false` sets false)

## Supported Types

- All primitive scalars (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`, `bool`, `String`)
- `PathBuf` and any other `FromStr + Default` type
- Custom enums (with `FromStr` implementation)
- `Vec<T>` for list values
- Nested structs (must also derive `Config`)

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
configulator-rs = "0.1"
```

> [!NOTE]
> Because the configuration options are expressed as different cases (i.e. `http.host` in a config file would be `HTTP__HOST` in environment variables), this library cannot be used for configurations that contain the same field name in different cases.

### Derive Attributes

This library uses the `#[configulator(...)]` attribute with the following keys:

- `name` - The config key name (defaults to the field name)
- `default` - Default value as a string literal
- `description` - Help text shown in CLI `--help` output

```rust
// Field appears in config files, env vars, and CLI flags as "my-name".
#[configulator(name = "my-name")]
my_name: String,

// Field has a description shown in CLI --help
#[configulator(name = "my-name", description = "this text appears in --help")]
my_name: String,

// Field has a default value of 1
#[configulator(name = "my-name", default = "1")]
my_name: u32,
```

### Example

```rust
use configulator::{
    CLIFlagOptions, Config, Configulator,
    EnvironmentVariableOptions, FileOptions, Validate,
    serde_loader,
};

#[derive(Config, Default, Debug)]
struct AppConfig {
    #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
    host: String,

    #[configulator(name = "port", default = "8080", description = "Listen port")]
    port: u16,

    #[configulator(name = "debug", default = "false", description = "Enable debug mode")]
    debug: bool,

    #[configulator(name = "allowed-origins", default = "localhost,example.com")]
    allowed_origins: Vec<String>,

    #[configulator(name = "database")]
    database: DatabaseConfig,
}

#[derive(Config, Default, Debug)]
struct DatabaseConfig {
    #[configulator(name = "url", default = "postgres://localhost/mydb")]
    url: String,

    #[configulator(name = "max-connections", default = "10")]
    max_connections: u32,
}

impl Validate for AppConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.port == 0 {
            return Err("port must be non-zero".into());
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configulator::<AppConfig>::new()
        .with_file(FileOptions {
            paths: vec!["config.yaml".into(), "/etc/myapp/config.yaml".into()],
            error_if_not_found: false,
            // Any serde-compatible format works: serde_json, toml, etc.
            loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
        })
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: "MYAPP".into(),
            separator: "__".into(),
        })
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .load()?;

    println!("Host: {}", config.host);
    println!("Database URL: {}", config.database.url);
    Ok(())
}
```

### Configuration Sources

#### Config Files

Configulator is format-agnostic, pass any serde-compatible deserializer via
`serde_loader`, or implement the `FileLoader` trait for full control.
YAML, TOML, JSON, and any other serde format work out of the box.

Provide a list of paths to search. The first file found is used.

```rust
// YAML
.with_file(FileOptions {
    paths: vec!["config.yaml".into()],
    error_if_not_found: false,
    loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
})

// TOML
.with_file(FileOptions {
    paths: vec!["config.toml".into()],
    error_if_not_found: false,
    loader: serde_loader(|s| toml::from_str(s)),
})

// JSON
.with_file(FileOptions {
    paths: vec!["config.json".into()],
    error_if_not_found: false,
    loader: serde_loader(|s| serde_json::from_str(s)),
})
```

The CLI also accepts `--config` / `-c` to specify a config file path at runtime
(requires calling `.with_file()` first).

#### Environment Variables

Environment variables are formed as `PREFIX` + `SEPARATOR` + `FIELD_NAME` (uppercased, dashes become underscores).

```rust
.with_environment_variables(EnvironmentVariableOptions {
    prefix: "MYAPP".into(),
    separator: "__".into(),
})
```

For example, a field named `max-connections` under a `database` parent with prefix `MYAPP` and separator `__` would be `MYAPP__DATABASE__MAX_CONNECTIONS`.

#### CLI Flags

Nested fields use the separator to form flag names (e.g. `--database.host`).

```rust
.with_cli_flags(CLIFlagOptions {
    separator: ".".into(),
})
```

Boolean fields work as flags (`--debug` sets to true, `--debug false` sets to false). List fields can be repeated (`--ports 80 --ports 443`).

You can also provide a custom `clap::Command` to set the app name, version, or add your own flags:

```rust
.with_cli_command(clap::Command::new("myapp").version("1.0"))
.with_cli_flags(CLIFlagOptions {
    separator: ".".into(),
})
```

### Validation

Implement the `Validate` trait and call `.load()` to validate after loading. Use `.load_without_validation()` to skip validation.

### Feature Flags

Configulator uses feature flags to keep dependencies minimal. All features are
enabled by default.

| Feature | Description                                                            | Dependencies |
|---------|------------------------------------------------------------------------|--------------|
| `file`  | Config file loading (`FileOptions`, `serde_loader`, `--config` flag)   | `serde`      |
| `cli`   | CLI flag parsing via clap                                              | `clap`       |
| `env`   | Environment variable loading                                           | -            |

To opt out of features you don't need:

```toml
[dependencies]
configulator-rs = { version = "0.1", default-features = false, features = ["env"] }
```
