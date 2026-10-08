use configulator::Config;

#[derive(Config)]
struct EnvOptIn {
    #[configulator(name = "labels", env = "LABELS")]
    labels: std::collections::HashMap<String, String>,
}

fn main() {}
