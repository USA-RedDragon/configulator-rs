use configulator::Config;

#[derive(Config)]
struct FlagOptIn {
    #[configulator(name = "labels", flag = "labels")]
    labels: std::collections::HashMap<String, String>,
}

fn main() {}
