# Configulator

[![crates.io](https://img.shields.io/crates/v/configulator-rs.svg)](https://crates.io/crates/configulator-rs) [![codecov](https://codecov.io/github/usa-reddragon/configulator-rs/graph/badge.svg?token=VcJUr2qOGS)](https://codecov.io/github/usa-reddragon/configulator-rs) [![License](https://badgen.net/github/license/USA-RedDragon/configulator-rs)](https://github.com/USA-RedDragon/configulator-rs/blob/main/LICENSE)

Load a Rust config struct from defaults, a config file, environment variables
and command-line flags, using `#[derive(Config)]`.

This is the Rust version of
[configulator](https://github.com/USA-RedDragon/configulator) (Go). Both
follow the same [spec](https://github.com/USA-RedDragon/configulator/blob/main/spec/SPEC.md).

## Installation

Requires Rust 1.85 or later.

```sh
cargo add configulator-rs
cargo add serde_yaml_ng  # or any other serde format for config files
```

## Getting started

```rust
use configulator::{
    CLIFlagOptions, Config, Configulator, ConfigulatorError, EnvironmentVariableOptions,
    FileOptions, Validate, serde_loader,
};

#[derive(Config, Debug)]
struct AppConfig {
    #[configulator(name = "log-level", default = "info", description = "log level")]
    log_level: String,

    #[configulator(name = "http", nested)]
    http: Http,
}

#[derive(Config, Debug)]
struct Http {
    #[configulator(name = "host", default = "localhost", description = "listen address")]
    host: String,

    #[configulator(name = "port", default = "8080", description = "listen port")]
    port: u16,
}

impl Validate for AppConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = Configulator::<AppConfig>::new()
        .with_file(FileOptions {
            paths: vec!["config.yaml".into()],
            ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
        })
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: "MYAPP_".into(),
            separator: "_".into(),
        })
        .with_cli_flags(CLIFlagOptions { separator: ".".into() })
        .load();

    let config = match result {
        Ok(config) => config,
        Err(ConfigulatorError::CLIError(e)) => e.exit(),
        Err(e) => return Err(e.into()),
    };
    println!("{} {}:{}", config.log_level, config.http.host, config.http.port);
    Ok(())
}
```

Now `http.port` can be set with `http: {port: 9000}` in `config.yaml`,
`MYAPP_HTTP_PORT=9000`, or `--http.port 9000`, and `--config` picks a
different file. `--help`, `--version` and bad flags come back as
`ConfigulatorError::CLIError`, and `e.exit()` prints them the way clap does.

More in [configulator/examples](configulator/examples): [basic](configulator/examples/basic.rs)
and [advanced](configulator/examples/advanced.rs) (custom `FromStr` types).

## Features

- Configuration from, lowest to highest priority:
  - Defaults in `#[configulator(default = "...")]`
  - Files, in any serde format: pass the format's `from_str` to
    `serde_loader`, or implement `FileLoader`
  - Environment variables
  - Command-line flags, using clap
- Supported types:
  - Every scalar, `String`, `PathBuf`, and anything else that implements
    `FromStr`. File values go through `FromStr` too, so your types don't need
    `Deserialize`. Bools accept Go's spellings (`1`, `t`, `TRUE`, `0`, `f`,
    `False` and so on) from env, flags and defaults
  - Nested structs, and `Option<T>` of a scalar or struct for optional values
  - `Vec<T>` of scalars
  - `Vec` and `HashMap`/`BTreeMap` of structs, and maps of scalars (files
    only)
  - `configulator::Duration` for Go-style durations (`30s`, `1h30m`)
  - `configulator::Complex64` and `Complex128`, parsed like Go
    (`1+2i`, `(1-2i)`, `3`). The `num-complex` feature converts them to and
    from `num_complex::Complex`
- `load_with_report()` returns a `Report` of where each field's value came
  from: its default, the config file, an environment variable or a flag,
  naming which one
- `secret` fields never show their value: not in `print_config()`, error
  messages or flag help, and their defaults are left out of the JSON Schema,
  the Markdown table and the samples
- `required` fields make loading fail if nothing sets them. A nested struct
  counts as set when any field in it is set. Inside an `Option` struct,
  required fields are checked only when the struct is set, and inside lists
  and maps they are checked per element (`servers[1].addr`)
- Config files are strict: unknown keys are an error unless the struct has
  `#[configulator(allow_unknown_fields)]`, and a value of the wrong type
  (`port: "80"`, `name: 5`) is an error. An empty file loads as no keys
- `print_config()` prints one `path = value` line per field in Go's
  `PrintConfig` format, and `defaults()` returns the config with only
  defaults applied
- `command()` returns the clap `Command` that loading parses, for shell
  completions with `clap_complete`. `--config` and `PathBuf` fields complete
  as paths
- [configulator-cli](configulator-cli) prints a JSON Schema, a sample config
  file (YAML, JSON or TOML), or a Markdown table of every option

## Attributes

| Attribute | Meaning |
| --- | --- |
| `name = "key"` | Key in files, env and flags. Defaults to the field name |
| `default = "value"` | Default value, parsed with `FromStr`. An empty default is a compile error |
| `description = "text"` | Flag help text, and the description in generated docs |
| `nested` | The field (or its `Vec`/map element, or `Option` inner type) is a struct that derives `Config` |
| `env = "NAME"` | Use `NAME` for this field's part of the env var name. `env = "-"` skips env |
| `flag = "name"` | Use `name` for this field's part of the flag name. `flag = "-"` skips flags |
| `short = 'p'` | Flag shorthand |
| `secret` | Never show the value in output, errors, help or generated docs |
| `required` | Loading fails if nothing sets it |

On the struct: `#[configulator(allow_unknown_fields)]`, and
`#[configulator(crate = "path")]` if you renamed the crate.

Env var names are the prefix plus each level's name, uppercased, with `-`
turned into `_`, joined by the separator (`_` if left empty). The prefix is
used as-is, so include its trailing separator: with prefix `MYAPP_`,
`http.listen-port` is `MYAPP_HTTP_LISTEN_PORT`.

## Go types

configulator (Go) has built-in support for some standard library types. In
Rust, use these:

| Go | Rust |
| --- | --- |
| `time.Duration` | `configulator::Duration` (no negative durations) |
| `complex64`, `complex128` | `configulator::Complex64`, `Complex128` |
| `net.IP`, `netip.Addr` | `std::net::IpAddr` |
| `net.TCPAddr`, `net.UDPAddr` | `std::net::SocketAddr` (no hostnames) |
| `net.IPNet` | `ipnet::IpNet` (keeps host bits) |
| `url.URL` | `url::Url` (absolute only) |
| `time.Month`, `*time.Location` | `chrono::Month`, `chrono_tz::Tz` |

Any of them works as a field type because it implements `FromStr`. Go reads
a config file with a decoder chosen by its extension; Rust uses the one
`FileLoader` you pass.

## Cargo features

All on by default except `testing` and `num-complex`.

| Feature | Meaning |
| --- | --- |
| `file` | Config files (`FileOptions`, `serde_loader`, the `--config` flag) |
| `env` | Environment variables |
| `cli` | Command-line flags, using clap |
| `testing` | `with_cli_args()` and `with_env_vars()` for tests |
| `num-complex` | Conversions between the complex types and `num_complex::Complex` |

## configulator-cli

```sh
cargo install configulator-cli --locked
```

It reads your crate's source, so it doesn't need to build your app. Install
the version that matches your configulator-rs.

| Flag | Meaning |
| --- | --- |
| `--type` | Config type (required) |
| `--dir` | Directory to scan for the type, default `.` |
| `--schema` | Print a JSON Schema |
| `--sample` | Print a sample config. `--format` picks `yaml` (default), `json` or `toml` |
| `--markdown` | Print a Markdown table of every option. `--env-prefix`, `--env-separator` and `--flag-separator` set how names are shown |
| `--sample-file` | With `--sample`, write the sample to a file, such as `config.example.yaml` |
| `--markdown-file` | With `--markdown`, write the table into a file between the configulator markers |
| `--check` | With `--sample-file` or `--markdown-file`, exit 1 if the file is out of date instead of writing it |
| `--completions` | Print a completion script for a shell (`bash`, `zsh`, `fish`, `elvish`, `powershell`) |

## Generated config docs

Keep a `config.example.yaml` and a table of every option in your README up
to date with the CLI.

For the table, put the markers where it should go:

```markdown
## Configuration

<!-- configulator:begin -->
<!-- configulator:end -->
```

Then run the CLI by hand, from pre-commit, or from CI:

```sh
configulator --type AppConfig --sample --sample-file config.example.yaml
configulator --type AppConfig --markdown --markdown-file README.md --env-prefix MYAPP_
```

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/USA-RedDragon/configulator-rs
    rev: v0.3.0
    hooks:
      - id: configulator-sample
        args: [--type, AppConfig, --sample-file, config.example.yaml]
      - id: configulator-markdown
        args: [--type, AppConfig, --markdown-file, README.md, --env-prefix, MYAPP_]
```

```yaml
# GitHub Actions: fails pull requests when either file is stale and commits
# the update on pushes to the default branch. Needs contents: write.
- uses: USA-RedDragon/reusable-actions/configulator-rs-docs@v2
  with:
    type: AppConfig
    env-prefix: MYAPP_
```

The pre-commit hooks and the action both run the configulator-cli version
that matches configulator-rs in your `Cargo.lock`. The hooks install it into
`~/.cache/configulator-cli` the first time.
