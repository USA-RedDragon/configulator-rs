use configulator::Config;

#[derive(Config)]
struct EnvOptIn {
    #[configulator(name = "tags", env = "TAGS")]
    tags: Vec<String>,
}

fn main() {}
