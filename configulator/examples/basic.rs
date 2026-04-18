use configulator::{
    CLIFlagOptions, Config, Configulator, EnvironmentVariableOptions,
    FileOptions, Validate, serde_loader,
};

#[derive(Config, Default, Debug)]
struct AppConfig {
    #[configulator(name = "host", default = "127.0.0.1", description = "Bind address")]
    host: String,

    #[configulator(name = "port", default = "8080", description = "Listen port")]
    port: u16,

    #[configulator(name = "debug", default = "false", description = "Enable debug mode")]
    debug: bool,

    #[configulator(name = "allowed-origins", default = "localhost,example.com")]
    allowed_origins: Vec<String>,

    #[configulator(name = "database")]
    database: DatabaseConfig,
}

#[derive(Config, Default, Debug)]
struct DatabaseConfig {
    #[configulator(name = "url", default = "postgres://localhost/mydb")]
    url: String,

    #[configulator(name = "max-connections", default = "10")]
    max_connections: u32,
}

impl Validate for AppConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.port == 0 {
            return Err("port must be non-zero".into());
        }
        if self.database.max_connections == 0 {
            return Err("max-connections must be non-zero".into());
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configulator::<AppConfig>::new()
        .with_file(FileOptions {
            paths: vec!["config.yaml".into(), "/etc/myapp/config.yaml".into()],
            error_if_not_found: false,
            loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
        })
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: "MYAPP".into(),
            separator: "__".into(),
        })
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .load()?;

    println!("Config loaded successfully!");
    println!("  Host:            {}", config.host);
    println!("  Port:            {}", config.port);
    println!("  Debug:           {}", config.debug);
    println!("  Allowed Origins: {:?}", config.allowed_origins);
    println!("  Database URL:    {}", config.database.url);
    println!("  Max Connections: {}", config.database.max_connections);

    Ok(())
}
