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
    // Stages after it, in the reference's order, and none exists yet:
    //   sLog->Initialize              logging
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
    Err(format!(
        "stage 2 'config' is not ported (reference: sConfigMgr->Configure + LoadAppConfigs); \
         it would read {} (dry run: {}, config policy: {})",
        options.config.display(),
        options.dry_run,
        options.config_policy.as_deref().unwrap_or("none"),
    ))
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
