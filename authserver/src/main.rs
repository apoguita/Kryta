//! authserver - the authentication program.
//!
//! Reference: `src/server/apps/authserver/Main.cpp` at 30a36ec4d. The stages below are that
//! file's order, and each one names the reference call it translates.
//!
//! The program runs. It reads its configuration, logs, writes its PID file, opens the login
//! database and brings its schema up to date, loads the world list, and listens. The conversation
//! on that listener is the next subject; what it answers today is the four bytes that say what kind
//! of connection arrived.

#[path = "Authentication/mod.rs"]
mod authentication;
#[allow(dead_code)]
#[path = "Server/mod.rs"]
mod server;

use crate::authentication::ConnectionKind;
use common::configuration::{Complaint, Config, Policy, PolicyOverride, Severity};
use common::logging::{Log, LogLevel};
use common::utilities::create_pid_file;
use database::database::{read_settings, DatabaseKind, DatabaseWorkerPool};
use database::updater::{collect_files, UpdateState};
use shared::network::socket::{Session, SessionFactory, SessionFuture, SessionIo};
use shared::network::socket_mgr::SocketMgr;
use shared::worlds::WorldList;
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

    // Reference: `DatabaseLoader::Load` opens the pool and then runs the updater on it, and only
    // after both does it report the pool as started.
    let schema = std::path::Path::new("data/sql/base/db_auth");
    let files = collect_files(schema, UpdateState::Released)
        .map_err(|error| format!("the login schema at {} is not readable: {error}", schema.display()))?;
    let summary = login.apply_updates(&files, &log)?;
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!(
            "Login database: {} file(s) applied, {} already current.",
            summary.updated, summary.skipped
        ),
    );

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

    // Stage 6. Reference: `sSecretMgr->Initialize()`.
    //
    // Not ported, and it is not a decision this program can take: the reference's only secret is a
    // two-factor master key for Azeroth's accounts, and this game's login carries a portal-issued
    // identity instead. `shared/Secrets/` stays empty until that is settled, and `../AGENTS.md`
    // records it.
    //
    // Stage 7. Reference: `sRealmList->Initialize(*ioContext, RealmsStateUpdateDelay)`.
    let (update_delay, notes) = config
        .value::<u32>("RealmsStateUpdateDelay", 20)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }

    let mut worlds = WorldList::new();
    let loaded = worlds.load(login.connection_mut(), &log)?;
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!("World list: {loaded} world(s), refreshed every {update_delay} s."),
    );

    if worlds.is_empty() {
        // Reference: the three reasons the reference prints before it exits 1.
        log.out(
            LogLevel::Error,
            "server.authserver",
            "No valid realms specified. Possible reasons:",
        );
        log.out(
            LogLevel::Error,
            "server.authserver",
            "- the realmlist table of the auth database is empty",
        );
        log.out(
            LogLevel::Error,
            "server.authserver",
            "- every realm has flag 3 (REALM_FLAG_VERSION_MISMATCH | REALM_FLAG_OFFLINE), which the realm list query excludes. Reset it with: UPDATE realmlist SET flag = 0 WHERE id = <realm id>;",
        );
        log.out(
            LogLevel::Error,
            "server.authserver",
            "- no realm address could be resolved, see the resolver errors logged above",
        );
        log.shutdown();
        return Err("no valid realms specified".to_string());
    }

    // Stage 8. Reference: `sAuthSocketMgr.StartNetwork(*ioContext, bindIp, port)`, then the signal
    // handlers, `SetProcessPriority`, and `ioContext->run()`.
    //
    // The session is where stage 9 lands: the reference's `AuthSession` starts by reading the
    // client's version message and answering with the key exchange, and Tyria's form of that is
    // measured material, not the reference's. Until it is written, a connection is logged and
    // closed, which is honest about what this program can do rather than leaving a socket open with
    // nothing on the other end.
    let factory: SessionFactory = {
        let log = log.clone();
        std::sync::Arc::new(move || {
            let log = log.clone();
            Box::new(Unwritten(log)) as Box<dyn Session>
        })
    };

    let mut sockets = SocketMgr::new(factory);
    let bound = sockets
        .start_network(&bind, port, 1)
        .map_err(|error| format!("Failed to initialize network: {error}"))?;

    log.out(
        LogLevel::Info,
        "server.authserver",
        format!("Listening on {bind}:{bound}."),
    );

    // Reference: the signal handlers stop the acceptor and the threads; `ioContext->run()` returns
    // and the program closes what it opened, in reverse order.
    wait_for_shutdown();
    log.out(
        LogLevel::Info,
        "server.authserver",
        "Stopping: the listener is closing.",
    );

    sockets.stop_network();
    login.close();
    log.shutdown();

    Ok(())
}

/// The session a connection gets until the auth conversation is written.
///
/// It reads the four bytes that decide what the connection is, says what it found, and closes.
/// Named for what it is, so nothing reads this as a working login: the conversation that follows
/// those four bytes is the next subject.
struct Unwritten(Log);

impl Session for Unwritten {
    fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
        Box::pin(async move {
            let peer = io
                .peer()
                .map(|address| address.to_string())
                .unwrap_or_else(|| "an unknown address".to_string());

            let mut first_four = [0u8; 4];
            if let Err(error) = read_exactly(&mut io, &mut first_four).await {
                self.0.out(
                    LogLevel::Warn,
                    "server.authserver",
                    format!("Connection from {peer} closed before it said what it was: {error}"),
                );
                let _ = io.close().await;
                return;
            }

            match ConnectionKind::classify(first_four) {
                Some(kind) => self.0.out(
                    LogLevel::Info,
                    "server.authserver",
                    format!(
                        "Connection from {peer}: {} connection, first message {} bytes. The conversation is not written yet.",
                        kind.as_str(),
                        kind.first_message_size()
                    ),
                ),
                None => self.0.out(
                    LogLevel::Warn,
                    "server.authserver",
                    format!(
                        "Connection from {peer} opened with {first_four:02X?}, which is not a connection this server recognises."
                    ),
                ),
            }

            let _ = io.close().await;
        })
    }
}

/// Read exactly as many bytes as the slice holds.
async fn read_exactly(io: &mut SessionIo, out: &mut [u8]) -> Result<(), String> {
    let mut filled = 0;
    while filled < out.len() {
        match io.read(&mut out[filled..]).await {
            Ok(0) => return Err("the peer closed".to_string()),
            Ok(count) => filled += count,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

/// Block until the operator stops the program.
///
/// Reference: `ioContext->run()` returning when the signal handler stopped the io context. A
/// Ctrl-C is that signal here.
fn wait_for_shutdown() {
    let (sender, receiver) = std::sync::mpsc::channel::<()>();
    if ctrlc::set_handler(move || {
        let _ = sender.send(());
    })
    .is_err()
    {
        // Without a handler the program cannot be stopped politely; it still listens, and the
        // process can be ended from outside.
        return;
    }

    let _ = receiver.recv();
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
