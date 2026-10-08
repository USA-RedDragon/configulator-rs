//! Conformance corpus runner: executes every case under the shared
//! `spec/cases/` directory (from the configulator Go repo, which owns the
//! SPEC) against the corpus shapes in `spec/shapes.md`.
//!
//! The spec directory is located via `CONFIGULATOR_SPEC_DIR`, falling back
//! to a sibling checkout at `../configulator/spec`. Cases listed in
//! `skip-rust.txt` are skipped. If `CONFIGULATOR_SPEC_DIR` is set, a
//! missing or empty cases directory fails the test.

#![cfg(all(feature = "file", feature = "cli", feature = "env"))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use configulator::{
    serde_loader, CLIFlagOptions, Config, Configulator, ConfigulatorError, Duration,
    EnvironmentVariableOptions, FileOptions, Layer, Report, Validate,
};
use serde_json::{json, Value};

#[derive(Config, Debug)]
struct Scalars {
    #[configulator(name = "name", default = "svc")]
    name: String,
    #[configulator(name = "count")]
    count: i64,
    #[configulator(name = "port", default = "8080")]
    port: u16,
    #[configulator(name = "ratio", default = "1.5")]
    ratio: f64,
    #[configulator(name = "verbose", default = "false")]
    verbose: bool,
}

#[derive(Config, Debug)]
struct Nested {
    #[configulator(name = "app-name", default = "myapp")]
    app_name: String,
    #[configulator(name = "http", nested)]
    http: Http,
    #[configulator(name = "db", nested)]
    db: Db,
}

#[derive(Config, Debug)]
struct Http {
    #[configulator(name = "host", default = "localhost")]
    host: String,
    #[configulator(name = "port", default = "8080")]
    port: u16,
}

#[derive(Config, Debug)]
struct Db {
    #[configulator(name = "url", default = "postgres://localhost/db")]
    url: String,
    #[configulator(name = "pool", nested)]
    pool: NPool,
}

#[derive(Config, Debug)]
struct NPool {
    #[configulator(name = "size", default = "10")]
    size: u16,
}

#[derive(Config, Debug)]
struct Collections {
    #[configulator(name = "tags", default = "a,b")]
    tags: Vec<String>,
    #[configulator(name = "labels")]
    labels: HashMap<String, String>,
    #[configulator(name = "servers", nested)]
    servers: Vec<Server>,
    #[configulator(name = "pools", nested)]
    pools: HashMap<String, CPool>,
    #[configulator(name = "log-level", default = "info")]
    log_level: String,
}

#[derive(Config, Debug)]
struct Server {
    #[configulator(name = "addr")]
    addr: String,
    #[configulator(name = "weight", default = "1")]
    weight: u16,
}

#[derive(Config, Debug)]
struct CPool {
    #[configulator(name = "size", default = "5")]
    size: u16,
}

#[derive(Config, Debug)]
struct Optionals {
    #[configulator(name = "port")]
    port: Option<u16>,
    #[configulator(name = "name", default = "opt-name")]
    name: Option<String>,
    #[configulator(name = "tls", nested)]
    tls: Option<Tls>,
}

#[derive(Config, Debug)]
struct Tls {
    #[configulator(name = "cert")]
    cert: String,
    #[configulator(name = "min-version", default = "12")]
    min_version: u16,
}

#[derive(Config, Debug)]
struct Durations {
    #[configulator(name = "timeout", default = "30s")]
    timeout: Duration,
    #[configulator(name = "label")]
    label: String,
}

#[derive(Config, Debug)]
struct NestedCollections {
    #[configulator(name = "peers", nested)]
    peers: Vec<NcPeer>,
    #[configulator(name = "by-name", nested)]
    by_name: HashMap<String, NcPeer>,
}

#[derive(Config, Debug)]
struct NcPeer {
    #[configulator(name = "name")]
    name: String,
    #[configulator(name = "slots", default = "3")]
    slots: i64,
    #[configulator(name = "rules", nested)]
    rules: Vec<NcRule>,
    #[configulator(name = "tags", nested)]
    tags: HashMap<String, NcRule>,
    #[configulator(name = "inner", nested)]
    inner: NcLevel,
    #[configulator(name = "opt", nested)]
    opt: Option<NcLevel>,
}

#[derive(Config, Debug)]
struct NcRule {
    #[configulator(name = "from")]
    from: i64,
    #[configulator(name = "range", default = "1")]
    range: i64,
    #[configulator(name = "on", default = "true")]
    on: bool,
}

#[derive(Config, Debug)]
struct NcLevel {
    #[configulator(name = "level", default = "7")]
    level: i64,
}

#[derive(Config, Debug)]
struct Complex {
    #[configulator(name = "z", default = "1+2i")]
    z: configulator::Complex128,
    #[configulator(name = "exponent")]
    exponent: configulator::Complex128,
    #[configulator(name = "w")]
    w: configulator::Complex64,
    #[configulator(name = "zs")]
    zs: Vec<configulator::Complex128>,
}

#[derive(Config, Debug)]
struct Required {
    #[configulator(name = "top", required)]
    top: String,
    #[configulator(name = "nested", nested)]
    nested: RNested,
    #[configulator(name = "opt", nested)]
    opt: Option<ROpt>,
    #[configulator(name = "items", nested)]
    items: Vec<ROpt>,
}

#[derive(Config, Debug)]
struct RNested {
    #[configulator(name = "leaf", required)]
    leaf: String,
}

#[derive(Config, Debug)]
struct ROpt {
    #[configulator(name = "leaf", required)]
    leaf: String,
    #[configulator(name = "other")]
    other: String,
}

impl Validate for Required {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.top == "invalid" {
            return Err("top is invalid".into());
        }
        Ok(())
    }
}

#[derive(Config, Debug)]
struct Attributes {
    #[configulator(name = "token", secret)]
    token: i64,
    #[configulator(name = "renamed", env = "RN", flag = "rn")]
    renamed: String,
    #[configulator(name = "no-env", env = "-")]
    no_env: String,
    #[configulator(name = "no-flag", flag = "-")]
    no_flag: String,
    #[configulator(name = "port", short = 'p')]
    port: u16,
}

macro_rules! ok_validate {
    ($($t:ty),*) => {
        $(impl Validate for $t {
            fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                Ok(())
            }
        })*
    };
}
ok_validate!(
    Scalars,
    Nested,
    Collections,
    Optionals,
    Durations,
    NestedCollections,
    Complex,
    Attributes
);

trait ToJson {
    fn to_json(&self) -> Value;
}

impl ToJson for Scalars {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name, "count": self.count, "port": self.port,
            "ratio": self.ratio, "verbose": self.verbose,
        })
    }
}

impl ToJson for Nested {
    fn to_json(&self) -> Value {
        json!({
            "app-name": self.app_name,
            "http": {"host": self.http.host, "port": self.http.port},
            "db": {"url": self.db.url, "pool": {"size": self.db.pool.size}},
        })
    }
}

impl ToJson for Collections {
    fn to_json(&self) -> Value {
        json!({
            "tags": self.tags,
            "labels": self.labels,
            "servers": self.servers.iter()
                .map(|s| json!({"addr": s.addr, "weight": s.weight}))
                .collect::<Vec<_>>(),
            "pools": self.pools.iter()
                .map(|(k, p)| (k.clone(), json!({"size": p.size})))
                .collect::<serde_json::Map<_, _>>(),
            "log-level": self.log_level,
        })
    }
}

impl ToJson for Optionals {
    fn to_json(&self) -> Value {
        json!({
            "port": self.port,
            "name": self.name,
            "tls": self.tls.as_ref().map(|t| json!({
                "cert": t.cert, "min-version": t.min_version,
            })),
        })
    }
}

impl ToJson for Durations {
    fn to_json(&self) -> Value {
        json!({"timeout": self.timeout.to_string(), "label": self.label})
    }
}

impl ToJson for NcRule {
    fn to_json(&self) -> Value {
        json!({"from": self.from, "range": self.range, "on": self.on})
    }
}

impl ToJson for NcPeer {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "slots": self.slots,
            "rules": self.rules.iter().map(ToJson::to_json).collect::<Vec<_>>(),
            "tags": self.tags.iter()
                .map(|(k, r)| (k.clone(), r.to_json()))
                .collect::<serde_json::Map<_, _>>(),
            "inner": {"level": self.inner.level},
            "opt": self.opt.as_ref().map(|l| json!({"level": l.level})),
        })
    }
}

impl ToJson for NestedCollections {
    fn to_json(&self) -> Value {
        json!({
            "peers": self.peers.iter().map(ToJson::to_json).collect::<Vec<_>>(),
            "by-name": self.by_name.iter()
                .map(|(k, p)| (k.clone(), p.to_json()))
                .collect::<serde_json::Map<_, _>>(),
        })
    }
}

impl ToJson for Required {
    fn to_json(&self) -> Value {
        let ropt = |r: &ROpt| json!({"leaf": r.leaf, "other": r.other});
        json!({
            "top": self.top,
            "nested": {"leaf": self.nested.leaf},
            "opt": self.opt.as_ref().map(ropt),
            "items": self.items.iter().map(ropt).collect::<Vec<_>>(),
        })
    }
}

impl ToJson for Attributes {
    fn to_json(&self) -> Value {
        json!({
            "token": self.token,
            "renamed": self.renamed,
            "no-env": self.no_env,
            "no-flag": self.no_flag,
            "port": self.port,
        })
    }
}

fn complex_json(re: f64, im: f64) -> Value {
    let num = |v: f64| {
        if v.fract() == 0.0 && v.abs() < 1e15 {
            json!(v as i64)
        } else {
            json!(v)
        }
    };
    json!([num(re), num(im)])
}

impl ToJson for Complex {
    fn to_json(&self) -> Value {
        json!({
            "z": complex_json(self.z.re, self.z.im),
            "exponent": complex_json(self.exponent.re, self.exponent.im),
            "w": complex_json(self.w.re as f64, self.w.im as f64),
            "zs": self.zs.iter().map(|c| complex_json(c.re, c.im)).collect::<Vec<_>>(),
        })
    }
}

fn spec_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CONFIGULATOR_SPEC_DIR") {
        return Some(PathBuf::from(dir));
    }
    let sibling = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configulator/spec");
    if sibling.is_dir() {
        return Some(sibling);
    }
    None
}

fn read_json(path: &Path) -> Option<Value> {
    let data = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&data).expect("invalid JSON fixture"))
}

struct CaseOptions {
    prefix: String,
    env_separator: String,
    flag_separator: String,
    array_separator: String,
}

struct Case {
    name: String,
    options: CaseOptions,
    dir: PathBuf,
    shape: String,
    env: HashMap<String, String>,
    argv: Vec<String>,
    expect: Option<Value>,
    expect_errors: Option<Value>,
    expect_origins: Option<Value>,
}

fn load_case(dir: &Path) -> Case {
    let shape = std::fs::read_to_string(dir.join("shape"))
        .unwrap_or_else(|_| panic!("{}: missing shape file", dir.display()))
        .trim()
        .to_string();
    let options = read_json(&dir.join("options.json")).unwrap_or_else(|| json!({}));
    let opt = |key: &str, default: &str| {
        options
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    };
    let options = CaseOptions {
        prefix: opt("prefix", "APP_"),
        env_separator: opt("env_separator", "_"),
        flag_separator: opt("flag_separator", "."),
        array_separator: opt("array_separator", ","),
    };
    let env = read_json(&dir.join("env.json"))
        .map(|v| {
            v.as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let argv = read_json(&dir.join("argv.json"))
        .map(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    Case {
        name: dir.file_name().unwrap().to_string_lossy().to_string(),
        options,
        dir: dir.to_path_buf(),
        shape,
        env,
        argv,
        expect: read_json(&dir.join("expect.json")),
        expect_errors: read_json(&dir.join("expect_errors.json")),
        expect_origins: read_json(&dir.join("expect_origins.json")),
    }
}

fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

fn error_kind(err: &ConfigulatorError) -> &'static str {
    match err {
        ConfigulatorError::ExplicitFileError { .. } => "ExplicitFileMissing",
        ConfigulatorError::ParseError { .. } => "ParseError",
        ConfigulatorError::BadEnvOptions { .. } => "BadEnvOptions",
        ConfigulatorError::ValidationError(_) => "ValidationError",
        ConfigulatorError::Required { .. } => "RequiredError",
        ConfigulatorError::SearchPathUnreadable { .. } => "SearchPathUnreadable",
        ConfigulatorError::FileNotFound { .. } => "NoFileFound",
        ConfigulatorError::CLIError(_) => "FlagError",
        ConfigulatorError::DecodeError { message, .. } => {
            if message.contains("unknown field") {
                "UnknownKey"
            } else {
                "DecodeError"
            }
        }
        _ => "Other",
    }
}

fn run_shape<C>(case: &Case) -> Result<(C, Report), ConfigulatorError>
where
    C: configulator::HasShadow + Validate,
    C::Shadow: for<'de> serde::Deserialize<'de>,
{
    Configulator::<C>::new()
        .with_file(FileOptions {
            paths: vec![case.dir.join("config.json")],
            ..FileOptions::new(serde_loader(|s| serde_json::from_str(s)))
        })
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: case.options.prefix.clone(),
            separator: case.options.env_separator.clone(),
        })
        .with_env_vars(case.env.clone())
        .with_cli_flags(CLIFlagOptions {
            separator: case.options.flag_separator.clone(),
        })
        .with_array_separator(case.options.array_separator.clone())
        .with_cli_args(case.argv.clone())
        .load_with_report()
}

fn check_case<C>(case: &Case) -> Result<(), String>
where
    C: configulator::HasShadow + Validate + ToJson,
    C::Shadow: for<'de> serde::Deserialize<'de>,
{
    match run_shape::<C>(case) {
        Ok((config, report)) => {
            if let Some(expect_errors) = &case.expect_errors {
                return Err(format!("expected error {expect_errors}, got success"));
            }
            let expect = case
                .expect
                .as_ref()
                .ok_or("case has neither expect.json nor expect_errors.json")?;
            let got = config.to_json();
            if !json_eq(&got, expect) {
                return Err(format!("value mismatch\n  want: {expect}\n  got:  {got}"));
            }
            if let Some(origins) = &case.expect_origins {
                for (path, want) in origins.as_object().unwrap() {
                    let origin = report
                        .origin(path)
                        .ok_or_else(|| format!("no origin recorded for {path}"))?;
                    let want_layer = want["layer"].as_str().unwrap();
                    let got_layer = match origin.layer {
                        Layer::Default => "default",
                        Layer::File => "file",
                        Layer::Env => "env",
                        Layer::Cli => "cli",
                    };
                    if got_layer != want_layer {
                        return Err(format!(
                            "origin layer mismatch for {path}: want {want_layer}, got {got_layer}"
                        ));
                    }
                    if let Some(detail) = want.get("detail").and_then(|d| d.as_str()) {
                        let matches = if origin.layer == Layer::File {
                            origin.detail.ends_with(detail)
                        } else {
                            origin.detail == detail
                        };
                        if !matches {
                            return Err(format!(
                                "origin detail mismatch for {path}: want {detail:?}, got {:?}",
                                origin.detail
                            ));
                        }
                    }
                }
            }
            Ok(())
        }
        Err(err) => {
            let expect_errors = case
                .expect_errors
                .as_ref()
                .ok_or_else(|| format!("unexpected error: {err}"))?;
            let want_kind = expect_errors["kind"].as_str().unwrap();
            let got_kind = error_kind(&err);
            if got_kind != want_kind {
                return Err(format!(
                    "error kind mismatch: want {want_kind}, got {got_kind} ({err})"
                ));
            }
            let msg = err.to_string();
            if let Some(contains) = expect_errors.get("contains").and_then(|c| c.as_array()) {
                for needle in contains {
                    let needle = needle.as_str().unwrap();
                    if !msg.contains(needle) {
                        return Err(format!("error {msg:?} does not contain {needle:?}"));
                    }
                }
            }
            if let Some(excludes) = expect_errors.get("excludes").and_then(|c| c.as_array()) {
                for needle in excludes {
                    let needle = needle.as_str().unwrap();
                    if msg.contains(needle) {
                        return Err(format!("error {msg:?} contains excluded {needle:?}"));
                    }
                }
            }
            Ok(())
        }
    }
}

#[test]
fn corpus() {
    let Some(spec) = spec_dir() else {
        eprintln!(
            "SKIP: spec dir not found; set CONFIGULATOR_SPEC_DIR or check out \
             the configulator repo as a sibling"
        );
        return;
    };
    let cases_dir = spec.join("cases");
    let skip: Vec<String> = std::fs::read_to_string(spec.join("skip-rust.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&cases_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", cases_dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(
        !dirs.is_empty(),
        "enumerate-or-fail: no cases in {}",
        cases_dir.display()
    );

    let mut failures = Vec::new();
    let mut ran = 0;
    for dir in &dirs {
        let case = load_case(dir);
        if skip.contains(&case.name) {
            eprintln!("SKIP (skip-rust.txt): {}", case.name);
            continue;
        }
        let result = match case.shape.as_str() {
            "scalars" => check_case::<Scalars>(&case),
            "nested" => check_case::<Nested>(&case),
            "collections" => check_case::<Collections>(&case),
            "optionals" => check_case::<Optionals>(&case),
            "durations" => check_case::<Durations>(&case),
            "nested-collections" => check_case::<NestedCollections>(&case),
            "complex" => check_case::<Complex>(&case),
            "required" => check_case::<Required>(&case),
            "attributes" => check_case::<Attributes>(&case),
            other => Err(format!("unknown shape {other:?}")),
        };
        ran += 1;
        if let Err(msg) = result {
            failures.push(format!("{}: {msg}", case.name));
        }
    }

    eprintln!(
        "corpus: {ran}/{} cases run, {} skipped",
        dirs.len(),
        dirs.len() - ran
    );
    assert!(
        failures.is_empty(),
        "{} corpus case(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn schema_smoke() {
    let schema = Configulator::<Collections>::json_schema();
    assert!(schema.contains("\"additionalProperties\""));
    let sample = Configulator::<Nested>::sample_config();
    assert!(sample.contains("app-name: \"myapp\""));
    eprintln!("{schema}\n{sample}");
}
