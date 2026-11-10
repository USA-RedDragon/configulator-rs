use configulator::Config;

#[derive(Config)]
struct ShortConfig {
    #[configulator(name = "concurrency", short = 'c', default = "1")]
    concurrency: u32,
}

fn main() {}
