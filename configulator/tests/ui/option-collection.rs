use configulator::Config;

#[derive(Config)]
struct OptVec {
    #[configulator(name = "x")]
    x: Option<Vec<u16>>,
}

fn main() {}
