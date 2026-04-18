use configulator::{
    CLIFlagOptions, Config, Configulator, EnvironmentVariableOptions,
    FileOptions, Validate, serde_loader,
};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

// --- Custom type that implements FromStr ---

#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}


impl FromStr for LogLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" | "warning" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(format!("unknown log level: '{s}'")),
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trace => write!(f, "trace"),
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

// --- Config structs demonstrating custom and stdlib types ---

#[derive(Config, Default, Debug)]
struct ServerConfig {
    /// Bind address - string that gets validated as an IP address
    #[configulator(name = "bind-address", default = "127.0.0.1", description = "Address to bind to")]
    bind_address: String,

    #[configulator(name = "port", default = "8080", description = "Listen port")]
    port: u16,

    /// Custom enum type - just needs FromStr + Default
    #[configulator(name = "log-level", default = "info", description = "Log verbosity")]
    log_level: LogLevel,

    /// PathBuf - another stdlib type with FromStr + Default
    #[configulator(name = "data-dir", default = "/var/lib/myapp", description = "Data directory")]
    data_dir: PathBuf,

    /// Vec of strings - validated as IP addresses
    #[configulator(name = "allowed-ips", default = "127.0.0.1")]
    allowed_ips: Vec<String>,

    /// Nested struct - automatically detected because DbConfig derives Config
    #[configulator(name = "database")]
    database: DbConfig,

    #[configulator(name = "cache")]
    cache: CacheConfig,
}

#[derive(Config, Default, Debug)]
struct DbConfig {
    #[configulator(name = "host", default = "localhost")]
    host: String,

    #[configulator(name = "port", default = "5432")]
    port: u16,

    #[configulator(name = "name", default = "myapp")]
    name: String,

    #[configulator(name = "max-connections", default = "10", description = "Connection pool size")]
    max_connections: u32,
}

#[derive(Config, Default, Debug)]
struct CacheConfig {
    #[configulator(name = "enabled", default = "true", description = "Enable caching")]
    enabled: bool,

    #[configulator(name = "ttl-seconds", default = "300", description = "Cache TTL in seconds")]
    ttl_seconds: u64,
}

impl Validate for ServerConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.port == 0 {
            return Err("port must be non-zero".into());
        }
        if self.database.max_connections == 0 {
            return Err("database.max-connections must be non-zero".into());
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configulator::<ServerConfig>::new()
        .with_file(FileOptions {
            paths: vec!["server.yaml".into()],
            error_if_not_found: false,
            loader: serde_loader(|s| serde_yaml_ng::from_str(s)),
        })
        .with_environment_variables(EnvironmentVariableOptions {
            prefix: "SERVER".into(),
            separator: "__".into(),
        })
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .load()?;

    println!("Server config loaded:");
    println!("  Bind:        {}:{}", config.bind_address, config.port);
    println!("  Log level:   {}", config.log_level);
    println!("  Data dir:    {}", config.data_dir.display());
    println!("  Allowed IPs: {:?}", config.allowed_ips);
    println!(
        "  DB:          {}:{}/{} (pool: {})",
        config.database.host,
        config.database.port,
        config.database.name,
        config.database.max_connections
    );
    println!(
        "  Cache:       enabled={}, ttl={}s",
        config.cache.enabled, config.cache.ttl_seconds
    );

    Ok(())
}
