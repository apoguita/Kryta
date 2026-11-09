//! authserver - the authentication program.
//!
//! Reference: `src/server/apps/authserver/Main.cpp` at 30a36ec4d. The stages below are that
//! file's order, and each one names the reference call it translates.
//!
//! Only the first stage exists: the command line. The program stops at the second with a
//! reason and exit code 1 rather than printing that it ran a stage it has not ported.

// The stage that uses these is not ported yet. The module exists so the boundary exists.
#[allow(dead_code)]
#[path = "Authentication/mod.rs"]
mod authentication;
#[allow(dead_code)]
#[path = "Server/mod.rs"]
mod server;

use common::configuration::{Complaint, Config, Policy, PolicyOverride, Severity};
use common::logging::{Log, LogLevel};
use common::utilities::create_pid_file;
use database::database::{read_settings, DatabaseKind, DatabaseWorkerPool};
use std::path::PathBuf;
use std::process::ExitCode;

/// The config file read when `-c` names none.
/// Reference: `_ACORE_REALM_CONFIG  "authserver.conf"`.
const DEFAULT_CONFIG: &str = "authserver.conf";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("authserver: {reason}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    // Stage 1. Reference: `GetConsoleArguments(argc, argv, configFile)`.
    let options = Options::parse(args)?;

    // Reference: `if (vm.count("help") || vm.count("version")) return 0;`
    if options.help {
        print_usage();
        return Ok(());
    }
    if options.version {
        println!("Kryta authserver {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Stage 2. Reference: `sConfigMgr->Configure(...)` then `LoadAppConfigs()`.
    //
    // The severity policy can be overridden on the command line, and a key it does not know is
    // refused rather than ignored - the reference reports it the same way.
    let mut policy = Policy::default();
    if let Some(text) = &options.config_policy {
        let control = PolicyOverride::parse(text).map_err(|error| error.to_string())?;
        if !control.apply(&mut policy) {
            return Err(format!(
                "--config-policy names no value this program has: '{}'",
                control.key
            ));
        }
    }

    let mut config =
        Config::load(&options.config, false, policy).map_err(|error| error.to_string())?;
    config.set_dry_run(options.dry_run);

    for complaint in config.complaints() {
        report(complaint);
    }

    // What the file answers, read in the reference's own order and through its own lookup:
    // `GetOption<int32>("RealmServerPort", 3724)`, then `GetOption<std::string>("BindIP", ...)`.
    let (port, notes) = config
        .value::<u16>("RealmServerPort", 3724)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (bind, notes) = config
        .value_string("BindIP", "0.0.0.0")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }

    println!(
        "authserver: config {} ({} keys), would bind {bind}:{port}",
        config.filename().display(),
        config.keys().count()
    );

    // Stage 3. Reference: `sLog->RegisterAppender<AppenderDB>()` then `sLog->Initialize(nullptr)`.
    //
    // The database appender belongs to `database/Logging/` and arrives with that crate; until then
    // a configuration that names one is reported and the rest of the log still works.
    let log = Log::new();
    let notes = log.initialize(&config).map_err(|error| error.to_string())?;
    for note in notes {
        println!("warning: {note}");
    }

    // Reference: `Acore::Banner::Show("authserver", ...)`, whose first line names the configuration
    // file it is using.
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!(
            "> Using configuration file       {}",
            config.filename().display()
        ),
    );
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!("Kryta authserver {}", env!("CARGO_PKG_VERSION")),
    );

    // Stage 4. Reference: `CreatePIDFile(pidFile)`, when `PidFile` is set. A file that cannot be
    // written stops the program, with the reference's own two lines.
    let (pid_file, notes) = config
        .value_string("PidFile", "")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }

    if !pid_file.is_empty() {
        match create_pid_file(&pid_file) {
            Some(pid) => log.out(
                LogLevel::Info,
                "server.authserver",
                format!("Daemon PID: {pid}\n"),
            ),
            None => {
                log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!("Cannot create PID file {pid_file} (possible error: permission)\n"),
                );
                log.shutdown();
                return Err(format!(
                    "cannot create PID file {pid_file} (possible error: permission)"
                ));
            }
        }
    }

    // Stage 5. Reference: `StartDB()` - a `DatabaseLoader` on the Login database, then the pool's
    // own line, `Started auth database connection pool.`
    //
    // Reference: `DatabaseLoader loader("server.authserver"); loader.AddDatabase(LoginDatabase,
    // "Login");` - the loader opens the pool, and a pool that cannot open stops the program.
    let settings = read_settings(&config, DatabaseKind::Login)?;
    let mut login = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)?;

    log.out(
        LogLevel::Info,
        "server.authserver",
        "Started auth database connection pool.",
    );

    // A pool that opened but cannot answer is not a pool: the reference pings it on `MaxPingTime`,
    // and a startup that will not survive its first query is better refused here.
    login
        .ping()
        .map_err(|error| format!("the login database does not answer: {error}"))?;
    login.close();

    log.shutdown();

    // Stage 6. Reference: `sSecretMgr->Initialize()`.
    //
    // Stages after it, in the reference's order, and none exists yet:
    //   sIPLocation->Load           the address lookup
    //   sRealmList->Initialize      the world list, refreshed on `RealmsStateUpdateDelay`
    //   sAuthSocketMgr.StartNetwork the listener, on `RealmServerPort` (3724)
    //   the signal handlers         SIGINT, SIGTERM
    //   SetProcessPriority          `ProcessorAffinity`, `HighPriority`
    //   ioContext->run              the io loop
    Err(format!(
        "stage 6 'secrets' is not ported (reference: sSecretMgr->Initialize); dry run: {}",
        config.is_dry_run()
    ))
}

/// Print a complaint at the severity the policy gave it.
///
/// Reference: `LogWithSeverity`, whose first act is to say nothing at `Skip`.
fn report(complaint: &Complaint) {
    match complaint.severity {
        Severity::Skip => {}
        Severity::Warn => println!("warning: {}", complaint.message),
        Severity::Error => eprintln!("error: {}", complaint.message),
        Severity::Fatal => eprintln!("fatal: {}", complaint.message),
    }
}

/// What the command line said.
///
/// Reference: the `options_description` in `GetConsoleArguments`.
#[derive(Debug, Default)]
struct Options {
    /// `-h`, `--help`. Print usage and exit 0.
    help: bool,
    /// `-v`, `--version`. Print the build and exit 0.
    version: bool,
    /// `-d`, `--dry-run`. Apply the database updates and stop.
    dry_run: bool,
    /// `-c`, `--config`. The file to read.
    config: PathBuf,
    /// `--config-policy`. Overrides the severity policy for unknown config keys.
    config_policy: Option<String>,
}

impl Options {
    /// Read the arguments.
    ///
    /// Reference: `command_line_parser(...).allow_unregistered()`. An argument this program
    /// does not know is kept rather than refused, so it does not return an error for one.
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            config: PathBuf::from(DEFAULT_CONFIG),
            ..Self::default()
        };

        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "-h" | "--help" => options.help = true,
                "-v" | "--version" => options.version = true,
                "-d" | "--dry-run" => options.dry_run = true,
                "-c" | "--config" => {
                    options.config = PathBuf::from(
                        rest.next().ok_or("--config needs a path")?,
                    );
                }
                "--config-policy" => {
                    options.config_policy =
                        Some(rest.next().ok_or("--config-policy needs a value")?.clone());
                }
                other => {
                    if let Some(value) = other.strip_prefix("--config=") {
                        options.config = PathBuf::from(value);
                    } else if let Some(value) = other.strip_prefix("--config-policy=") {
                        options.config_policy = Some(value.to_string());
                    }
                }
            }
        }

        Ok(options)
    }
}

fn print_usage() {
    println!("Allowed options:");
    println!("  -h [ --help ]             print usage message");
    println!("  -v [ --version ]          print version build info");
    println!("  -d [ --dry-run ]          Dry run");
    println!("  -c [ --config ] arg       use <arg> as configuration file (default: {DEFAULT_CONFIG})");
    println!("  --config-policy policy    override config severity policy");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        Options::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn the_config_file_defaults_to_the_reference_name() {
        assert_eq!(parse(&[]).config, PathBuf::from("authserver.conf"));
    }

    #[test]
    fn a_config_path_is_read_in_both_forms() {
        assert_eq!(parse(&["-c", "a.conf"]).config, PathBuf::from("a.conf"));
        assert_eq!(parse(&["--config=b.conf"]).config, PathBuf::from("b.conf"));
    }

    #[test]
    fn help_and_version_are_separate_flags() {
        let options = parse(&["-h"]);
        assert!(options.help && !options.version);
    }

    #[test]
    fn an_unknown_argument_is_kept_not_refused() {
        assert!(Options::parse(&["--something-new".to_string()]).is_ok());
    }

    #[test]
    fn a_value_that_is_missing_is_an_error() {
        assert!(Options::parse(&["--config".to_string()]).is_err());
    }
}
