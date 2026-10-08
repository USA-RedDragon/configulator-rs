use configulator::Config;

#[derive(Config)]
struct OptOpt {
    #[configulator(name = "x")]
    x: Option<Option<u16>>,
}

fn main() {}
