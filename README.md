# Configulator

[![codecov](https://codecov.io/github/usa-reddragon/configulator-rs/graph/badge.svg?token=VcJUr2qOGS)](https://codecov.io/github/usa-reddragon/configulator-rs) [![License](https://badgen.net/github/license/USA-RedDragon/configulator-rs)](https://github.com/USA-RedDragon/configulator-rs/blob/main/LICENSE) [![GitHub contributors](https://badgen.net/github/contributors/USA-RedDragon/configulator-rs)](https://github.com/USA-RedDragon/configulator-rs/graphs/contributors/)

A simple configuration manager for Rust applications with derive macro support.
This is the Rust version of [configulator](https://github.com/USA-RedDragon/configulator) (Go), and both are tested against the same spec.

## Features

- Supports configuration from multiple sources with clear precedence:
  1. Default values (lowest)
  2. Config files (any serde format via `serde_loader`)
  3. Environment variables
  4. CLI flags (highest)
- `#[derive(Config)]` macro for declarative configuration structs
- Any serde-compatible file format - YAML, TOML, JSON, with a one-liner
- Nested structs, `Vec<T>`, maps, `Vec<Struct>`, and `Option<T>` fields
- Custom types - anything implementing `FromStr`, no serde impls needed
- Per-field origin report: which source set each value
- `secret` redaction, `required` fields, and a generated `print_config()`
- Go-style `Duration` (`"30s"`, `"1h30m"`)
- Optional validation via the `Validate` trait
- Boolean CLI flags (`--debug` sets true, `--debug false` sets false)

## Supported Types

- All primitive scalars (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`, `bool`, `String`)
- `PathBuf`, `configulator::Duration`, and any other `FromStr` type
- `Option<T>` of a scalar or nested struct. It stays `None` unless a source sets it, and `null` in a file counts as unset.
- `Vec<T>` of scalars, and `Vec<T>` of nested structs (file only)
- `HashMap`/`BTreeMap` with scalar or nested struct values (file only)
- Nested structs (must also derive `Config` and be marked `nested`)

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
configulator-rs = "0.2"
```

> [!NOTE]
> The same option is spelled differently per source (`http.host` in a config file is `HTTP__HOST` in env vars), so two fields in the same struct can't have names that only differ by case or `-`/`_`. That's a compile error.

### Derive Attributes

Fields use the `#[configulator(...)]` attribute:

|              Key               |                          Description                          |
| ------------------------------ | ------------------------------------------------------------- |
| `name`                         | Config key name (defaults to the field name)                  |
| `default`                      | Default value as a string literal                             |
| `description`                  | Help text shown in CLI `--help` output                        |
| `nested`                       | The field's type (or `Vec`/map element type) derives `Config` |
| `env = "-"` / `env = "NAME"`   | Skip env, or override this field's env segment                |
| `flag = "-"` / `flag = "NAME"` | Skip CLI, or override this field's flag segment               |
| `short = 'p'`                  | Single-character CLI shorthand                                |
| `secret`                       | Redacted in `print_config()` and parse errors                 |
| `required`                     | Some source must set this field                               |

On the struct: `#[configulator(crate = "path")]` if you renamed the crate, and `#[configulator(allow_unknown_fields)]` to allow unknown keys in config files.

### Example

```rust
use configulator::{
    CLIFlagOptions, Config, Configulator, ConfigulatorError,
    EnvironmentVariableOptions, FileOptions, Validate,
    serde_loader,
};

#[derive(Config, Debug)]
struct AppConfig {
    #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
    host: String,

    #[configulator(name = "port", default = "8080", description = "Listen port")]
    port: u16,

    #[configulator(name = "database", nested)]
    database: DatabaseConfig,
}

#[derive(Config, Debug)]
struct DatabaseConfig {
    #[configulator(name = "url", default = "postgres://localhost/mydb")]
    url: String,
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
    let result = Configulator::<AppConfig>::new()
        .with_file(FileOptions {
            paths: vec!["config.yaml".into(), "/etc/myapp/config.yaml".into()],
            ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
        })
        // MYAPP__HOST, MYAPP__DATABASE__URL, ...
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: "MYAPP__".into(),
            separator: "__".into(),
        })
        // --host, --database.url, --config/-c, ...
        .with_cli_flags(CLIFlagOptions { separator: ".".into() })
        .load_with_report();

    // --help, --version, and bad args come back as a clap::Error
    let (config, report) = match result {
        Ok(ok) => ok,
        Err(ConfigulatorError::CLIError(e)) => e.exit(),
        Err(e) => return Err(e.into()),
    };

    println!("host: {}", config.host);
    for path in report.paths() {
        let o = report.origin(path).unwrap();
        println!("{path} came from {} ({})", o.layer, o.detail);
    }
    Ok(())
}
```

See [`examples/`](configulator/examples/) for more, including custom `FromStr` types.

### Configuration Sources

#### Config Files

Pass any serde-compatible deserializer via `serde_loader`, or implement the `FileLoader` trait yourself.

Provide a list of paths to search. The first file found is used.

```rust
// YAML
.with_file(FileOptions {
    paths: vec!["config.yaml".into()],
    ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
})

// TOML
.with_file(FileOptions {
    paths: vec!["config.toml".into()],
    ..FileOptions::new(serde_loader(|s| toml::from_str(s)))
})
```

The CLI also accepts `--config` / `-c` to specify a config file path at runtime (requires `.with_file()`).
If that file doesn't exist or can't be parsed, loading fails instead of falling back to the search paths. `FileOptions::explicit` works the same way.

Unknown keys in config files are an error unless the struct has `allow_unknown_fields`.

#### Environment Variables

Environment variables are `prefix` + each config name in the path (uppercased, dashes become underscores) joined by `separator`.
The prefix is used as-is, so include the trailing separator, and it must be uppercase.

```rust
.with_environment_variables(EnvironmentVariableOptions {
    prefix: "MYAPP__".into(),
    separator: "__".into(),
})
```

For example, a field named `max-connections` under a `database` parent would be `MYAPP__DATABASE__MAX_CONNECTIONS`.

#### CLI Flags

Nested fields use the separator to form flag names (e.g. `--database.host`).

```rust
.with_cli_flags(CLIFlagOptions {
    separator: ".".into(),
})
```

List fields can be repeated (`--ports 80 --ports 443`). Collections of structs and maps are file only.

You can also provide a custom `clap::Command` to set the app name, version, or add your own flags:

```rust
.with_cli_command(clap::Command::new("myapp").version("1.0"))
```

### Validation

Implement the `Validate` trait and call `.load()` or `.load_with_report()` to validate after loading. Use `.load_without_validation()` to skip validation.

### Schema and Sample Config

[`configulator-cli`](configulator-cli/) prints a JSON Schema, sample config, or Markdown table of options for your config struct.

### Feature Flags

All features are enabled by default except `testing`.

| Feature   | Description                                                          | Dependencies |
|-----------|----------------------------------------------------------------------|--------------|
| `file`    | Config file loading (`FileOptions`, `serde_loader`, `--config` flag) | `serde`      |
| `cli`     | CLI flag parsing via clap                                            | `clap`       |
| `env`     | Environment variable loading                                         | -            |
| `testing` | `with_cli_args()` / `with_env_vars()` for tests                      | `clap`       |

To opt out of features you don't need:

```toml
[dependencies]
configulator-rs = { version = "0.2", default-features = false, features = ["env"] }
```
