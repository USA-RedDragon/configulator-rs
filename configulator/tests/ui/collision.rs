use configulator::Config;

#[derive(Config)]
struct Colliding {
    #[configulator(name = "my-name")]
    a: String,
    #[configulator(name = "my_name")]
    b: String,
}

fn main() {}
