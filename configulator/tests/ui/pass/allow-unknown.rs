use configulator::Config;

#[derive(Config)]
#[configulator(allow_unknown_fields)]
struct AllowUnknown {
    #[configulator(name = "x", default = "1")]
    x: u16,
}

fn main() {}
