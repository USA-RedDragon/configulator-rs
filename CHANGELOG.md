# Changelog

## 0.2.0

Rewrite of how sources are loaded. Each source (defaults, file, env, CLI) now
parses into a generated "shadow" copy of your struct where every field is
optional, the shadows are merged in precedence order, and the real struct is
built once at the end. This replaces the old `ValueMap`, which turned
everything into strings first.

### Added

- `Option<T>` fields for scalars and nested structs. They stay `None` unless a
  source sets them, and `null` in a file counts as unset.
- `Vec<Struct>` and map fields (`HashMap`/`BTreeMap`, scalar or struct values).
  These are file only, and each element gets the struct's defaults.
- `load_with_report()` returns a `Report` showing which source set each field,
  keyed by path (`db.pool.size`, `servers[0].addr`).
- Custom types no longer need `Deserialize`. File values are parsed with
  `FromStr` like env and CLI values, and error locations are kept.
- Derive attributes: `nested`, `required`, `secret`, `short = 'p'`,
  `env = "-"`/`env = "NAME"`, `flag = "-"`/`flag = "NAME"`.
- Struct attributes: `#[configulator(crate = "path")]` and
  `#[configulator(allow_unknown_fields)]`.
- Generated `print_config()`. `secret` fields are redacted there and in parse
  errors. Field types without `Debug` print as `(no Debug impl)`.
- `configulator::Duration` for Go-style durations (`30s`, `1h30m`).
- `with_array_separator()` sets the list separator for env values and
  `default` attributes.
- `FileOptions::explicit` for a file that must exist, plus `flag_name` and
  `shorthand` to rename the `--config` flag. `FileOptions::new(loader)` fills
  in the rest so you can use `..FileOptions::new(...)`.
- `with_env_vars()` (`testing` feature) for setting env vars in tests without
  touching the real environment.
- New errors: `BadEnvOptions`, `Required { path }`, and `FlagConflict` when
  two flags share a name or shorthand (including `--config`/`-c`, `-h`, and
  args on a custom `clap::Command`).
- `required` works on nested structs (set if any field under it is set). The
  `required` fields inside an `Option` struct are only checked when the struct
  is set.
- `CLIError` now holds the `clap::Error` instead of a string. `--help`,
  `--version`, and parse errors are all returned to the app, so call
  `e.exit()` on it to get clap's normal output and exit code (see the
  examples). Without a custom command, the usage line now shows the binary
  name instead of `app`.
- `tests/corpus.rs` runs the shared test cases from the
  [configulator](https://github.com/USA-RedDragon/configulator) (Go) repo.
- `configulator-cli` prints a JSON Schema, sample config
  (`--format yaml|json|toml`), or Markdown table for a `#[derive(Config)]`
  struct, like the Go generator's `-schema`/`-sample`/`-markdown` flags.
  `Configulator::<C>::json_schema()` and `sample_config()` do the same from
  code. Structs with `allow_unknown_fields` don't get
  `additionalProperties: false`.

### Breaking

- Nested struct fields must be marked `#[configulator(nested)]`, including
  `Vec`/map element structs. Without it you get a `FromStr` compile error.
- The env prefix is used as-is: write `prefix = "MYAPP__"` where you used to
  write `"MYAPP"` with separator `"__"`. The prefix must be uppercase and the
  separator can't contain `-`.
- `FileLoader` is now `FileLoader<C>`. `serde_loader(...)` calls don't change,
  but hand-written loaders need updating.
- Unknown keys in config files are an error unless the struct has
  `allow_unknown_fields`. This means YAML `<<` merge keys are rejected.
- An empty env var is now an empty string, not `T::default()`.
- Splitting lists no longer trims whitespace or drops empty items, matching Go.
- A single value in a file is no longer accepted for a list field.
- Config structs no longer need `derive(Default)`.
- Only `load()` and `load_with_report()` need `Validate`.
- Removed `ConfigValue`, `ValueMap`, `FromValueMap`, `ConfigFields`,
  `ConfiguratorScalar`, and `ConfigDetect`.
- MSRV is 1.85. clap 4.6 needs it, and the old 1.70 didn't actually build.

## 0.1.4

- A `--config` file that doesn't exist or can't be read is now an error
  (`ExplicitFileError`) instead of quietly using defaults. It doesn't fall back
  to the search paths.
- The config file path no longer goes through the value map under a special
  key, so a field named `__config_file__` works now.
