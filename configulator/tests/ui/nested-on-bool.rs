use configulator::Config;

#[derive(Config)]
struct NestedBool {
    #[configulator(name = "x", nested)]
    x: bool,
}

fn main() {}
