use configulator::Config;

#[derive(Config)]
#[configulator(crate = "::configulator")]
struct CrateAttr {
    #[configulator(name = "x", default = "1")]
    x: u16,
}

fn main() {
    let _ = configulator::Configulator::<CrateAttr>::defaults_only().unwrap();
}
