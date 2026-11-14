#![cfg(all(feature = "file", feature = "cli", feature = "env"))]

use std::io::Write;

use clap::{error::ErrorKind, Arg, ArgAction, Command};
use configulator::{
    serde_loader, CLIFlagOptions, Config, Configulator, ConfigulatorError, FileOptions, Layer,
    Validate,
};

#[derive(Config, Debug, PartialEq)]
struct AppConfig {
    #[configulator(
        name = "port",
        short = 'p',
        default = "8080",
        description = "listen port"
    )]
    port: u16,

    #[configulator(name = "db", nested)]
    db: Db,
}

#[derive(Config, Debug, PartialEq)]
struct Db {
    #[configulator(name = "url", default = "sqlite://", description = "database URL")]
    url: String,
}

impl Validate for AppConfig {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

fn app() -> Command {
    Command::new("app")
        .arg(Arg::new("target").num_args(0..))
        .subcommand(
            Command::new("serve")
                .arg(Arg::new("workers").long("workers").num_args(1))
                .arg(Arg::new("name")),
        )
        .subcommand(
            Command::new("migrate").arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .action(ArgAction::SetTrue),
            ),
        )
}

fn builder(args: &[&str]) -> Configulator<AppConfig> {
    Configulator::<AppConfig>::new()
        .with_cli_command(app())
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(args.iter().map(|s| s.to_string()).collect())
}

fn yaml_file(contents: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(".yaml").tempfile().unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    f
}

#[test]
fn subcommand_with_its_own_args() {
    let (cfg, m) = builder(&["serve", "--workers", "4", "web"])
        .load_with_matches()
        .unwrap();
    assert_eq!(cfg.port, 8080);
    let (name, sub) = m.subcommand().unwrap();
    assert_eq!(name, "serve");
    assert_eq!(sub.get_one::<String>("workers").unwrap(), "4");
    assert_eq!(sub.get_one::<String>("name").unwrap(), "web");

    let (_, m) = builder(&["migrate", "--dry-run"])
        .load_with_matches()
        .unwrap();
    let (name, sub) = m.subcommand().unwrap();
    assert_eq!(name, "migrate");
    assert!(sub.get_flag("dry-run"));
}

#[test]
fn positional_args() {
    let (cfg, m) = builder(&["a", "--port", "9", "b"])
        .load_with_matches()
        .unwrap();
    assert_eq!(cfg.port, 9);
    let targets: Vec<&String> = m.get_many("target").unwrap().collect();
    assert_eq!(targets, ["a", "b"]);
    assert!(m.subcommand().is_none());
}

#[test]
fn config_flags_before_or_after_the_subcommand() {
    for args in [
        &["--port", "9", "--db.url", "pg://x", "serve"][..],
        &["serve", "--port", "9", "--db.url", "pg://x"][..],
        &["-p", "9", "serve", "--db.url", "pg://x"][..],
        &["serve", "-p", "9", "--db.url=pg://x"][..],
    ] {
        let (cfg, report) = builder(args).load_with_report().unwrap();
        assert_eq!(cfg.port, 9, "{args:?}");
        assert_eq!(cfg.db.url, "pg://x", "{args:?}");
        assert_eq!(report.origin("port").unwrap().layer, Layer::Cli, "{args:?}");
        assert_eq!(report.origin("port").unwrap().detail, "--port", "{args:?}");
    }
}

#[test]
fn defaults_on_global_flags_do_not_count_as_set() {
    let (_, report) = builder(&["serve"]).load_with_report().unwrap();
    assert_eq!(report.origin("port").unwrap().layer, Layer::Default);
}

#[test]
fn config_file_flag_after_the_subcommand() {
    let f = yaml_file("port: 7\n");
    let path = f.path().to_str().unwrap();
    let (cfg, m) = Configulator::<AppConfig>::new()
        .with_cli_command(app())
        .with_file(FileOptions {
            paths: vec!["missing.yaml".into()],
            ..FileOptions::new(serde_loader(|s| serde_yaml_ng::from_str(s)))
        })
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(vec!["serve".into(), "-c".into(), path.into()])
        .load_with_matches()
        .unwrap();
    assert_eq!(cfg.port, 7);
    assert_eq!(m.subcommand_name(), Some("serve"));
}

#[test]
fn caller_parses_and_passes_matches() {
    let b = Configulator::<AppConfig>::new()
        .with_cli_command(app())
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        });
    let m = b
        .command()
        .unwrap()
        .try_get_matches_from(["serve", "--workers", "2", "--port", "5"])
        .unwrap();
    let (name, sub) = m.subcommand().unwrap();
    assert_eq!(name, "serve");
    assert_eq!(sub.get_one::<String>("workers").unwrap(), "2");

    let from_root = b.with_matches(m.clone()).load().unwrap();
    assert_eq!(from_root.port, 5);

    let from_sub = Configulator::<AppConfig>::new()
        .with_matches(sub.clone())
        .load()
        .unwrap();
    assert_eq!(from_sub.port, 5);
}

#[test]
fn with_matches_does_not_read_process_args() {
    let m = Configulator::<AppConfig>::new()
        .command()
        .unwrap()
        .try_get_matches_from(["--db.url", "pg://y"])
        .unwrap();
    let cfg = Configulator::<AppConfig>::new()
        .with_matches(m)
        .load()
        .unwrap();
    assert_eq!(cfg.db.url, "pg://y");
}

#[test]
fn help_on_a_subcommand() {
    let err = builder(&["serve", "--help"])
        .load_with_matches()
        .unwrap_err();
    let ConfigulatorError::CLIError(e) = err else {
        panic!("expected CLIError, got {err:?}");
    };
    assert_eq!(e.kind(), ErrorKind::DisplayHelp);
    let help = e.to_string();
    assert!(help.contains("Usage: app serve "), "{help}");
    assert!(help.contains("--workers"), "{help}");
    assert!(help.contains("--port"), "{help}");
    assert!(help.contains("listen port"), "{help}");
}

#[test]
fn bad_flag_on_a_subcommand() {
    let err = builder(&["migrate", "--nope"])
        .load_with_matches()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::CLIError(ref e) if e.kind() == ErrorKind::UnknownArgument),
        "{err:?}"
    );
}

#[test]
fn flag_conflict_inside_a_subcommand() {
    let cmd =
        Command::new("app").subcommand(Command::new("serve").arg(Arg::new("port").long("port")));
    let err = Configulator::<AppConfig>::new()
        .with_cli_command(cmd)
        .with_cli_flags(CLIFlagOptions {
            separator: ".".into(),
        })
        .with_cli_args(vec![])
        .load()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict { ref flag, shorthand: None, ref existing } if flag == "port" && existing == "port"),
        "{err:?}"
    );

    let cmd = Command::new("app")
        .subcommand(Command::new("serve").arg(Arg::new("pid").short('p').long("pid")));
    let err = Configulator::<AppConfig>::new()
        .with_cli_command(cmd)
        .command()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict { shorthand: Some('p'), ref existing, .. } if existing == "pid"),
        "{err:?}"
    );

    let cmd = Command::new("app")
        .subcommand(Command::new("serve").arg(Arg::new("cfg").short('c').long("cfg")));
    let err = Configulator::<AppConfig>::new()
        .with_cli_command(cmd)
        .with_file(FileOptions::new(serde_loader(|s| {
            serde_yaml_ng::from_str(s)
        })))
        .command()
        .unwrap_err();
    assert!(
        matches!(err, ConfigulatorError::FlagConflict { shorthand: Some('c'), ref existing, .. } if existing == "cfg"),
        "{err:?}"
    );
}

#[test]
fn load_with_matches_without_cli_flags_uses_dot() {
    let (cfg, _) = Configulator::<AppConfig>::new()
        .with_cli_command(app())
        .with_cli_args(vec!["serve".into(), "--db.url".into(), "pg://z".into()])
        .load_with_matches()
        .unwrap();
    assert_eq!(cfg.db.url, "pg://z");
}

#[derive(Config, Debug, PartialEq)]
struct Flags {
    #[configulator(name = "quiet", short = 'q', default = "false")]
    quiet: bool,

    #[configulator(name = "verbose", short = 'v', default = "false")]
    verbose: bool,
}

impl Validate for Flags {
    fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

fn flags(args: &[&str]) -> Result<(Flags, clap::ArgMatches), ConfigulatorError> {
    Configulator::<Flags>::new()
        .with_cli_command(app())
        .with_cli_args(args.iter().map(|s| s.to_string()).collect())
        .load_with_matches()
}

#[test]
fn bool_flag_does_not_take_the_next_arg() {
    for args in [
        &["-q", "serve"][..],
        &["--quiet", "serve"][..],
        &["serve", "-q"][..],
        &["-qv", "serve"][..],
    ] {
        let (cfg, m) = flags(args).unwrap();
        assert!(cfg.quiet, "{args:?}");
        assert_eq!(m.subcommand_name(), Some("serve"), "{args:?}");
    }
    let (cfg, m) = flags(&["-q", "a", "b"]).unwrap();
    assert!(cfg.quiet);
    let targets: Vec<&String> = m.get_many("target").unwrap().collect();
    assert_eq!(targets, ["a", "b"]);
}

#[test]
fn bool_flag_value_needs_equals() {
    for (args, quiet) in [
        (&["--quiet=false"][..], false),
        (&["--quiet=true"][..], true),
        (&["--quiet=0"][..], false),
        (&["-q=false"][..], false),
        (&["-q"][..], true),
        (&[][..], false),
    ] {
        let (cfg, _) = flags(args).unwrap();
        assert_eq!(cfg.quiet, quiet, "{args:?}");
    }
    let (cfg, m) = flags(&["--quiet", "false"]).unwrap();
    assert!(cfg.quiet);
    let targets: Vec<&String> = m.get_many("target").unwrap().collect();
    assert_eq!(targets, ["false"]);
}
