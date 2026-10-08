//! The generated schema and sample match the Go generator byte for byte.
//! The golden files are the Go generator's output for the same config,
//! written with `configulator -type Config -schema` and `-sample`.

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
