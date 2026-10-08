use configulator::Config;

#[derive(Config)]
struct Generic<T: std::str::FromStr + Default> {
    #[configulator(name = "x")]
    x: T,
}

fn main() {}
