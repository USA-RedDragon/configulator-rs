use configulator::Config;

#[derive(Config)]
struct Sub {
    #[configulator(name = "x")]
    x: String,
}

#[derive(Config)]
struct DefaultOnNested {
    #[configulator(name = "sub", nested, default = "nope")]
    sub: Sub,
}

fn main() {}
