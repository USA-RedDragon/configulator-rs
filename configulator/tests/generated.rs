//! The generated schema and sample match the Go generator byte for byte.
//! The golden files are the Go generator's output for the same config,
//! written with `configulator -type Config -schema` and `-sample`. The
//! `go_edge_*` files come from a Go `Config` with the fields of `Edge`.

#![allow(dead_code)]

use std::collections::HashMap;

use configulator::{Config, Configulator, Duration, Validate};

#[derive(Config, Debug)]
struct Config {
    #[configulator(name = "name", default = "say \"hi\"", description = "a | b")]
    name: String,
    #[configulator(name = "big", default = "18446744073709551615")]
    big: u64,
    #[configulator(name = "whole", default = "1000")]
    whole: f64,
    #[configulator(name = "huge", default = "1e21")]
    huge: f64,
    #[configulator(name = "tiny", default = "0.0000001")]
    tiny: f64,
    #[configulator(name = "wait")]
    wait: Duration,
    #[configulator(name = "tags", default = "a,b")]
    tags: Vec<String>,
    #[configulator(name = "labels")]
    labels: HashMap<String, String>,
    #[configulator(name = "token", secret)]
    token: String,
    #[configulator(name = "http", nested)]
    http: Http,
}

#[derive(Config, Debug)]
struct Http {
    #[configulator(name = "host", default = "localhost")]
    host: String,
    #[configulator(name = "tls", nested)]
    tls: Tls,
}

#[derive(Config, Debug)]
struct Tls {
    #[configulator(name = "on", default = "true")]
    on: bool,
}

impl Validate for Config {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Defaults the Go generator spells specially: NaN and infinities, bool
/// spellings like `1`, a list that doesn't parse, control characters, and
/// keys that need quoting.
#[derive(Config, Debug)]
struct Edge {
    #[configulator(name = "nan", default = "NaN")]
    nan: f64,
    #[configulator(name = "infs", default = "1,-inf")]
    infs: Vec<f64>,
    #[configulator(name = "one", default = "1")]
    one: bool,
    #[configulator(name = "eff", default = "f")]
    eff: Option<bool>,
    #[configulator(name = "bad", default = "1,x")]
    bad: Vec<i64>,
    #[configulator(name = "ctl", default = "a\x01\x7f\"\\\t")]
    ctl: String,
    #[configulator(name = "bigs", default = "1,18446744073709551615")]
    bigs: Vec<u64>,
    #[configulator(name = "odd.key", nested)]
    odd: Odd,
}

#[derive(Config, Debug)]
struct Odd {
    #[configulator(name = "in ner", default = "18446744073709551615")]
    val: u64,
}

impl Validate for Edge {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[test]
fn edge_json_schema_matches_go() {
    assert_eq!(
        Configulator::<Edge>::json_schema(),
        include_str!("golden/go_edge_schema.json")
    );
}

#[test]
fn edge_yaml_sample_matches_go() {
    assert_eq!(
        Configulator::<Edge>::sample_config(),
        include_str!("golden/go_edge_sample.yaml")
    );
}

#[test]
fn json_schema_matches_go() {
    assert_eq!(
        Configulator::<Config>::json_schema(),
        include_str!("golden/go_schema.json")
    );
}

#[test]
fn yaml_sample_matches_go() {
    assert_eq!(
        Configulator::<Config>::sample_config(),
        include_str!("golden/go_sample.yaml")
    );
}
