# configulator-cli

Prints a JSON Schema, sample config, or Markdown table of options for a [`configulator-rs`](https://crates.io/crates/configulator-rs) `#[derive(Config)]` struct.
It reads your source code instead of compiling it, so you don't need to add anything to your app.

```sh
cargo install configulator-cli
configulator --type AppConfig --schema > appconfig.schema.json
configulator --type AppConfig --sample > config.sample.yaml
configulator --type AppConfig --sample --format toml > config.sample.toml
configulator --type AppConfig --markdown >> README.md
```

Run it from your crate's directory, or pass `--dir`.
