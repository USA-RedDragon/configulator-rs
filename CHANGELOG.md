# Changelog

## 0.3.1

Matches Go configulator v2.4's generated output:

- YAML sample: bool list defaults are normalized (`1,f,True` shows as
  `[true, false, true]`), and a list default with an element that doesn't
  parse is written as `[]`, like the JSON and TOML samples.
- TOML sample: the "too large for a TOML integer" comment spells the key
  the way the TOML line does, quoting it when needed.
- Markdown table: control characters in any cell are escaped (`\n`, `\t`,
  `\x1b`) so a row can't break the table.

## 0.3.0

Go parity release, checked against the shared spec 0.2.0 (58 cases).

### Added

- `configulator::Complex64` and `Complex128`, parsed and printed like Go
  (`1+2i`, `(1-2i)`, `3`), with an optional `num-complex` feature.
- `defaults()`, the config with only defaults applied, using the builder's
  array separator and skipping `required` checks, like Go's `Default()`.
- Flag help shows each field's default (never a secret's), and `--config` is
  labelled "config file" with the first search path as its default.
- `env = "NAME"` and `flag = "name"` work on lists of scalars.
- Shell completion: `Configulator::command()` returns the clap `Command`
  for `clap_complete`, `--config` and `PathBuf` fields carry path hints, and
  `configulator --completions <shell>` prints the CLI's own script.
- Error variants `DecodeError { path, message }`,
  `SearchPathUnreadable { path, message }`, and `searched` on `FileNotFound`.
  `ParseError` has `path` (the config path) and `source` (the env var,
  `--flag` or `default tag`).
- configulator-cli: Go-compatible output byte for byte (JSON Schema, YAML,
  JSON and TOML samples, Markdown table), a Duration sample of `"0s"`, `|`
  escaped in Markdown cells, bool defaults normalized (`"1"` shows as
  `true`), NaN and infinity spelled per format, TOML strings and keys
  escaped for TOML, and a `--check` hint that repeats the run's own
  arguments.

### Changed (breaking)

- Config files are strict about scalar types: `port: "80"`, `name: 5`,
  `verbose: "true"` and `count: 1.5` are errors. An integer for a float field
  is fine.
- An empty env separator means `_` instead of `__`. Set `separator: "__"`
  to keep the old names.
- configulator-cli's `--env-separator` defaults to `_`.
- Error messages use Go's wording, for example
  `port: cannot parse "x" from APP_PORT: ...` and
  `tls.cert: required but not set by any layer`. `FileError` is replaced by
  `DecodeError` and `SearchPathUnreadable`, and `BadEnvOptions` and
  `FlagConflict` are struct variants.
- `print_config()` uses Go's `PrintConfig` format: strings unquoted, lists as
  `[a b]`, maps as `map[k:v]`, `<unset>` for unset optionals, and a
  collection holding a secret printed as `(redacted)`.
- Default origins are reported as `default tag` instead of `default`.
- `default = ""` is a compile error, like Go. Remove it; the zero value is
  used anyway.
- Generated titles no longer include the type name: the JSON Schema title is
  `Configuration`, the sample starts `# Sample configuration` and the
  Markdown table `# Configuration`.
- configulator-cli usage errors exit 2, and all errors start with
  `configulator: `.

### Fixed

- Config file errors name the full path of the bad value or unknown key,
  such as `http.port`, `servers[1].weight` or `db.pool.zz`.
- An empty config file loads as no keys instead of failing to decode, and
  `Report::file()` names the loaded file even when nothing from it survived.
- No error message repeats a secret's value, including the parser's own text.
- Durations above Go's maximum (`2562047h47m16.854775807s`) are an error.
- `required` is checked inside list and map elements.

## 0.2.4

- `required` is checked inside list and map elements. A missing field fails
  with its element path, such as `servers[1].addr`.
- Secret fields stay out of generated docs: no default in the JSON Schema or
  the Markdown table, commented out in the YAML sample, and left out of the
  JSON and TOML samples. Copying a sample no longer sets a secret to
  `(secret)`.
- List defaults in the YAML sample are quoted per element (`["*", "a"]`,
  `[80, 443]`), so the sample is valid YAML. List defaults in the JSON Schema
  are typed.
- The Markdown table shows flag shorthands, as in `` `-p`, `--port` ``.

## 0.2.3

- The sample config shows a full commented example for every list, map and
  optional struct, at any depth, instead of only the first field or `{}`.
  An optional struct is commented out, so copying the sample doesn't turn it
  on. An optional value is commented out unless it has a default. Map
  examples use the key `example`.
- The pre-commit hooks install and run the configulator-cli version that
  matches configulator-rs in your `Cargo.lock`, instead of whatever
  `configulator` is on `PATH`.

## 0.2.2

- `--version` works when the app passes a `clap::Command` with `.version(...)`.
  It comes back as `CLIError`, like `--help`. A config field that uses
  `--version` or `-V` on such a command is a `FlagConflict`.
- configulator-cli: `--sample-file` writes the sample config to a file, such as
  `config.example.yaml`. `--markdown-file` writes the Markdown table into a
  file between `<!-- configulator:begin -->` and `<!-- configulator:end -->`.
  `--check` exits 1 instead of writing if either file is out of date.
- pre-commit hooks `configulator-sample` and `configulator-markdown`.
- An empty `default` shows as a blank cell in the Markdown table, as in Go.

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
