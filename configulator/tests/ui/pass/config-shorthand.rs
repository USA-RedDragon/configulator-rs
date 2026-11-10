use configulator::Config;

#[derive(Config)]
#[configulator(config_shorthand = "f")]
struct Renamed {
    #[configulator(name = "concurrency", short = 'c', default = "1")]
    concurrency: u32,
}

#[derive(Config)]
#[configulator(config_shorthand = "")]
struct NoConfigFile {
    #[configulator(name = "concurrency", short = 'c', default = "1")]
    concurrency: u32,
}

fn main() {}
