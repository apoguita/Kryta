//! worldserver - the game program.
//!
//! Reference: `src/server/apps/worldserver/Main.cpp` at 30a36ec4d. The stages below are that
//! file's order, and each one names the reference call it translates.
//!
//! Only the first stage exists: the command line. The program stops at the second with a
//! reason and exit code 1 rather than printing that it ran a stage it has not ported.

// The stages that use these are not ported yet. The module exists so the boundary exists.
#[allow(dead_code)]
#[path = "CommandLine/mod.rs"]
mod command_line;
#[allow(dead_code)]
#[path = "RemoteAccess/mod.rs"]
mod remote_access;

use common::configuration::{Complaint, Config, Policy, PolicyOverride, Severity};
use common::logging::{Log, LogLevel};
use common::utilities::create_pid_file;
use std::path::PathBuf;
use std::process::ExitCode;

/// The config file read when `-c` names none.
/// Reference: `_ACORE_CORE_CONFIG  "worldserver.conf"`.
const DEFAULT_CONFIG: &str = "worldserver.conf";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("worldserver: {reason}");
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
        println!("Kryta worldserver {}", env!("CARGO_PKG_VERSION"));
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

    // Stage 3. Reference: `sLog->Initialize(sConfigMgr->GetOption<bool>("Log.Async.Enable", ...))`.
    let log = Log::new();
    let notes = log.initialize(&config).map_err(|error| error.to_string())?;
    for note in notes {
        println!("warning: {note}");
    }

    // Reference: `Acore::Banner::Show("worldserver-daemon", ...)`, whose first line names the
    // configuration file.
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "> Using configuration file       {}",
            config.filename().display()
        ),
    );
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!("Kryta worldserver {}", env!("CARGO_PKG_VERSION")),
    );

    // What this process actually took from its configuration, when asked. Masked: a connection
    // string holds a password and a log line gets pasted into issue reports.
    let (show_config, notes) = config
        .value_bool("Log.EffectiveConfig", false)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    if show_config {
        for (key, value) in config.describe() {
            log.out(
                LogLevel::Debug,
                "server.loading",
                format!("> Config: {key} = {value}"),
            );
        }
    }
    // Stage 4. Reference: `CreatePIDFile(pidFile)`, when `PidFile` is set - the same stage the auth
    // program has, with the same two lines.
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
                "server.worldserver",
                format!("Daemon PID: {pid}\n"),
            ),
            None => {
                log.out(
                    LogLevel::Error,
                    "server.worldserver",
                    format!("Cannot create PID file {pid_file} (possible error: permission)\n"),
                );
                log.shutdown();
                return Err(format!(
                    "cannot create PID file {pid_file} (possible error: permission)"
                ));
            }
        }
    }

    log.shutdown();

    // Stage 5. Reference: `StartDB()`, which opens the World and Character pools.
    //
    // Stages after it, in the reference's order, and none exists yet:
    //   ThreadPool                    the worker threads, `ThreadPool` (2)
    //   SetScriptLoader(AddScripts)   the script hooks, and the modules loader beside them
    //   sScriptMgr->Initialize
    //   sWorld->SetInitialWorldSettings
    //   StartWorldNetwork             the listener, `BindIP` + `WorldServerPort` (8085),
    //                                 `Network.Threads` (1)
    //   CliRunnable                   the console, when `Console.Enable`
    //   ACSoapThread                  the SOAP endpoint, when `SOAP.Enabled`, port 7878
    //   sScriptMgr->OnStartup
    // On shutdown: `sWorldSessionMgr->KickAll`, `sMapMgr->UnloadAll`,
    // `sScriptMgr->OnAfterUnloadAllMaps`, `sScriptMgr->OnShutdown`.
    //
    // The world program opens two databases rather than one, and neither schema is here: the login
    // database's `realmlist` transfers from the reference because a world list is a world list, but
    // the world's content and a character's sheets are Tyria's and have no reference counterpart.
    Err(format!(
        "stage 5 'the world and character databases' is not ported (reference: StartDB); dry run: {}",
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
/// Reference: the `options_description` in `GetConsoleArguments`. It is the same list the
/// auth program has, because the reference gives both programs the same options.
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
        assert_eq!(parse(&[]).config, PathBuf::from("worldserver.conf"));
    }

    #[test]
    fn a_config_path_is_read_in_both_forms() {
        assert_eq!(parse(&["-c", "a.conf"]).config, PathBuf::from("a.conf"));
        assert_eq!(parse(&["--config=b.conf"]).config, PathBuf::from("b.conf"));
    }

    #[test]
    fn help_and_version_are_separate_flags() {
        let options = parse(&["-v"]);
        assert!(options.version && !options.help);
    }

    #[test]
    fn dry_run_is_its_own_flag() {
        assert!(parse(&["--dry-run"]).dry_run);
    }

    #[test]
    fn an_unknown_argument_is_kept_not_refused() {
        assert!(Options::parse(&["--something-new".to_string()]).is_ok());
    }
}
