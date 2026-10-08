use configulator::Config;

#[derive(Config)]
struct ShortMulti {
    #[configulator(name = "x", short = "xy")]
    x: String,
}

fn main() {}
