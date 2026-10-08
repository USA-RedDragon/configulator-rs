use configulator::Config;

#[derive(Config)]
struct EmptyDefault {
    #[configulator(name = "user", default = "")]
    user: String,
}

fn main() {}
