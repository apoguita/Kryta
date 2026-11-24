//! authserver - the authentication program.
//!
//! Reference: `src/server/apps/authserver/Main.cpp` at 30a36ec4d. The stages below are that
//! file's order, and each one names the reference call it translates.
//!
//! The program runs. It reads its configuration, logs, writes its PID file, opens the login
//! database and brings its schema up to date, loads the world list, opens the character database,
//! and listens. On that listener it answers the file check and the login conversation
//! (`Server/session.rs`), copies a gameplay connection to the world it names
//! (`Server/front_door.rs`), and serves the portal when the configuration asks for it.
//!
//! `run` is the list of stages, in the order they happen; each stage is one function below it.

#[path = "Authentication/mod.rs"]
mod authentication;
#[path = "Protocol/mod.rs"]
mod protocol;
#[path = "Server/mod.rs"]
mod server;

use crate::server::{AuthSession, FileCheck};
use common::configuration::startup::{policy, print_usage, read, read_bool, read_string, report};
use common::configuration::{Config, Policy};
use common::logging::{Log, LogLevel};
use common::utilities::create_pid_file;
use database::database::{read_settings, DatabaseKind, DatabaseWorkerPool};
use database::updater::{collect_files, UpdateState};
use shared::network::socket::{Session, SessionFactory};
use shared::network::socket_mgr::SocketMgr;
use shared::worlds::WorldList;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// The config file read when `-c` names none.
/// Reference: `_ACORE_REALM_CONFIG  "authserver.conf"`, with this project's own file format: JSON.
const DEFAULT_CONFIG: &str = "authserver.json";

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
        print_usage(DEFAULT_CONFIG);
        return Ok(());
    }
    if options.version {
        println!("Kryta authserver {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Stage 2. Reference: `sConfigMgr->Configure(...)` then `LoadAppConfigs()`.
    let (mut config, listener) = read_configuration(&options)?;

    // Stage 3. Reference: `sLog->RegisterAppender<AppenderDB>()` then `sLog->Initialize(nullptr)`.
    let log = start_logging(&config)?;

    // Stage 4. Reference: `CreatePIDFile(pidFile)`, when `PidFile` is set.
    write_pid_file(&config, &log)?;

    // Stage 5. Reference: `StartDB()`.
    let LoginDatabase {
        pool: mut login,
        sql_trace,
        network_trace,
    } = open_login_database(&mut config, &log)?;

    // Stage 6. Reference: `sSecretMgr->Initialize()`.
    //
    // Not ported, and it is not a decision this program can take: the reference's only secret is a
    // two-factor master key for Azeroth's accounts, and this game's login carries a portal-issued
    // identity instead. `shared/Secrets/` stays empty until that is settled, and `../AGENTS.md`
    // records it.
    //
    // Stage 7. Reference: `sRealmList->Initialize(*ioContext, RealmsStateUpdateDelay)`.
    let WorldListSource {
        worlds,
        update_delay,
        worlds_file,
    } = read_world_list(&config, &log, &mut login)?;

    // From here the pool is shared: the portal asks it who a login name is, from a network thread,
    // on every login. Its synchronous connection is one connection, so the pool goes behind a lock -
    // which is what the reference's own `DirectQuery` callers hold too.
    let login = Arc::new(Mutex::new(login));

    // The travel table in the login database: a `0x8029` answer records the hand-off here, and the
    // world claims the same row once when the connection presents its token pair
    // (`docs/port/06-world-plan.md`, "The three seams"). It is state, so it is a row.
    let travels = Arc::new(game::maps::DatabaseTravels::new(
        Arc::clone(&login),
        log.clone(),
    ));

    refuse_an_empty_world_list(&worlds, &log)?;

    // Stage 8. Reference: `sAuthSocketMgr.StartNetwork(*ioContext, bindIp, port)`, then the signal
    // handlers, `SetProcessPriority`, and `ioContext->run()`.
    //
    // The session is the reference's `AuthSession`: it reads the client's version and its seed,
    // answers with the server's seed, and then answers the client's messages through the cipher both
    // sides derive from the exchange (`Server/session.rs`). What follows is what a session is built
    // from, then the listeners, in the order each one reports itself.
    let file_check = read_file_check(&log)?;

    // The accounts are rows in the login database, not a file: they are the server's state, and an
    // operator configures the server rather than its people (`docs/decisions/0006`). This store is
    // the game library's: a login session records the EULA version through it, and the
    // administration tool reads the same table through the same code.
    let game_accounts = Arc::new(game::accounts::DatabaseAccounts::new(
        Arc::clone(&login),
        log.clone(),
    ));

    let placement = read_placement(&config, &log)?;
    let portal_sockets = start_portal(
        &config,
        &log,
        &login,
        &game_accounts,
        network_trace,
        listener.network_threads,
    )?;
    let account_settings = read_account_settings(&log)?;
    let (world_list, front_door) = open_front_door(worlds, &log);
    let refresh = start_world_list_refresh(&log, &login, &world_list, &worlds_file, update_delay)?;
    let characters_store = open_character_database(&mut config, &log, sql_trace)?;

    let factory = session_factory(
        &log,
        &file_check,
        &account_settings,
        &front_door,
        &characters_store,
        &travels,
        &game_accounts,
        placement,
    );
    let (sockets, bound) = start_listener(factory, &log, network_trace, &listener)?;
    let status_interval = announce_ready(&config, &log, &listener.bind, bound)?;
    let status = start_status_reporter(&log, &sockets, listener.bind, bound, status_interval)?;

    // Reference: the signal handlers stop the acceptor and the threads; `ioContext->run()` returns
    // and the program closes what it opened, in reverse order.
    wait_for_shutdown();
    shut_down(&log, status, refresh, sockets, portal_sockets, &login);

    Ok(())
}

/// Where the login listener binds and how many threads serve it, as the configuration states it.
struct ListenerSettings {
    /// `RealmServerPort`.
    port: u16,
    /// `BindIP`.
    bind: String,
    /// `Network.Threads`: the thread count of every listener this program opens.
    network_threads: usize,
}

/// The login database once its schema is current, and the two traces read beside it.
struct LoginDatabase {
    /// The login pool, not yet shared.
    pool: DatabaseWorkerPool,
    /// `Database.Trace`: every statement a pool runs is logged.
    sql_trace: bool,
    /// `Network.Trace`: every byte a listener reads and writes is logged.
    network_trace: bool,
}

/// The world list as it was first read, and the two settings its refresh runs by.
struct WorldListSource {
    /// The worlds the table and the file state.
    worlds: WorldList,
    /// `RealmsStateUpdateDelay`, in seconds.
    update_delay: u32,
    /// `WorldsFile`: the file whose rows override the table's. Empty merges nothing.
    worlds_file: String,
}

/// The two numbers a travel request is placed by.
#[derive(Clone, Copy)]
struct Placement {
    /// `CreationMap`: the map id that means the creation instance.
    creation_map: u16,
    /// `FallbackOutpost`: where a character with no map anywhere enters.
    fallback_outpost: u16,
}

/// A thread this program started, and the flag that asks it to stop.
struct Background {
    /// Set to ask the thread to end.
    stop: Arc<AtomicBool>,
    /// The thread, joined at shutdown.
    thread: JoinHandle<()>,
}

/// Stage 2: load the configuration file and read where the listener binds.
///
/// Reference: `sConfigMgr->Configure(...)` then `LoadAppConfigs()`. The severity policy can be
/// overridden on the command line, and a key it does not know is refused rather than ignored - the
/// reference reports it the same way.
fn read_configuration(options: &Options) -> Result<(Config, ListenerSettings), String> {
    let policy = policy(options.config_policy.as_deref())?;

    let mut config =
        Config::load_json(&options.config, false, policy).map_err(|error| error.to_string())?;
    config.set_dry_run(options.dry_run);
    for complaint in config.complaints() {
        report(complaint);
    }

    // What the file answers, read in the reference's own order and through its own lookup:
    // `GetOption<int32>("RealmServerPort", 3724)`, then `GetOption<std::string>("BindIP", ...)`.
    let port = read::<u16>(&config, "RealmServerPort", 3724)?;
    let bind = read_string(&config, "BindIP", "0.0.0.0")?;

    // How many threads serve the accepted sockets, for every listener this program opens. Reference:
    // `Network.Threads` (1), which its socket manager takes as its thread count.
    let network_threads = read::<usize>(&config, "Network.Threads", 1)?;

    println!(
        "authserver: config {} ({} keys), would bind {bind}:{port}",
        config.filename().display(),
        config.keys().count()
    );

    Ok((
        config,
        ListenerSettings {
            port,
            bind,
            network_threads,
        },
    ))
}

/// Stage 3: start the log, print the banner, and print the effective configuration when asked.
///
/// Reference: `sLog->RegisterAppender<AppenderDB>()` then `sLog->Initialize(nullptr)`. The database
/// appender belongs to `database/Logging/` and arrives with that crate; until then a configuration
/// that names one is reported and the rest of the log still works.
fn start_logging(config: &Config) -> Result<Log, String> {
    let log = Log::new();
    let notes = log.initialize(config).map_err(|error| error.to_string())?;
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

    // What this process actually took from its configuration, when asked. Masked: a connection
    // string holds a password and a log line gets pasted into issue reports.
    let show_config = read_bool(config, "Log.EffectiveConfig", false)?;
    if show_config {
        for (key, value) in config.describe() {
            log.out(
                LogLevel::Debug,
                "server.loading",
                format!("> Config: {key} = {value}"),
            );
        }
    }
    Ok(log)
}

/// Stage 4: write the PID file, when `PidFile` names one.
///
/// Reference: `CreatePIDFile(pidFile)`. A file that cannot be written stops the program, with the
/// reference's own two lines.
fn write_pid_file(config: &Config, log: &Log) -> Result<(), String> {
    let pid_file = read_string(config, "PidFile", "")?;

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
    Ok(())
}

/// Stage 5: open the login database, write the first game account when the table is empty, and
/// bring the schema up to date.
///
/// Reference: `StartDB()` - a `DatabaseLoader` on the Login database, then the pool's own line,
/// `Started auth database connection pool.` `DatabaseLoader loader("server.authserver");
/// loader.AddDatabase(LoginDatabase, "Login");` - the loader opens the pool, and a pool that cannot
/// open stops the program.
fn open_login_database(config: &mut Config, log: &Log) -> Result<LoginDatabase, String> {
    // The connection string is critical: a setting is read from the file, so a program is not
    // allowed to fall back to a compiled-in host and open a database nobody named.
    config.mark_critical("LoginDatabaseInfo");
    let settings = read_settings(config, DatabaseKind::Login)?;

    // The database's own trace: every statement the pool runs, on any of its connections.
    let sql_trace = read_bool(config, "Database.Trace", false)?;
    // Optional and off by default: with it on, every byte a client sends and receives is one line in
    // the log. It is the difference between "the client said nothing useful" and knowing exactly what
    // it said. Read here because both listeners use it.
    let network_trace = read_bool(config, "Network.Trace", false)?;

    let mut login = DatabaseWorkerPool::open_with_trace(DatabaseKind::Login, &settings, log, sql_trace)?;
    if sql_trace {
        if log.enabled("sql.trace", LogLevel::Debug) {
            log.out(
                LogLevel::Info,
                "server.authserver",
                "Database.Trace is on: every statement is logged at debug.",
            );
        } else {
            log.out(
                LogLevel::Warn,
                "server.authserver",
                "Database.Trace is on, but no logger named 'sql.trace' is enabled, so nothing will be traced. Add this line to the configuration file: Logger.sql.trace = 6,Console Auth",
            );
        }
    }

    // Reference: `DatabaseLoader::Load` opens the pool and then runs the updater on it, and only
    // after both does it report the pool as started.
    let schema = std::path::Path::new("data/sql/base/db_auth");
    let files = collect_files(schema, UpdateState::Released)
        .map_err(|error| format!("the login schema at {} is not readable: {error}", schema.display()))?;
    bootstrap_first_account(config, log, &mut login)?;
    let summary = login.apply_updates(&files, log)?;
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

    Ok(LoginDatabase {
        pool: login,
        sql_trace,
        network_trace,
    })
}

/// Write the first game account from `AdminAccountFile` when the account table is empty.
///
/// The first game account: a file, not a SQL seed (`config/Server/admin.json`,
/// `docs/decisions/0006-accounts-are-rows.md`). It is a client's account and a row - not the
/// administrator identity of `docs/decisions/0013-the-admin-identity-is-a-file.md`, which is a
/// separate file-held identity and is not built. The file is read at every start and writes an
/// account only when the table is empty, so editing it later changes nothing about existing accounts.
fn bootstrap_first_account(
    config: &Config,
    log: &Log,
    login: &mut DatabaseWorkerPool,
) -> Result<(), String> {
    let admin_file = read_string(config, "AdminAccountFile", "config/Server/admin.json")?;
    let path = std::path::Path::new(&admin_file);
    if path.exists() {
        let document = common::configuration::json::Document::read(path)
            .map_err(|error| error.to_string())?;
        let record = document.record("")?;
        let username = record.required("username")?;
        let token = record.required("password")?;
        let user_id = record
            .text("user_id")?
            .unwrap_or_else(|| format!("00000000-0000-0000-0000-{:012X}", 0));
        let empty = login
            .query_with("SELECT count(*)::text FROM \"account\"", &[])
            .map_err(|error| format!("the account table cannot be read: {error}"))?
            .first()
            .map(|row| row.get::<usize, String>(0) == "0")
            .unwrap_or(false);
        if empty {
            login
                .query_with(
                    "INSERT INTO \"account\" (\"username\", \"user_id\", \"token\", \"eula\", \"active\") \
                     VALUES ($1, $2, $3, 1, true) ON CONFLICT (\"username\") DO NOTHING",
                    &[&username, &user_id, &token],
                )
                .map_err(|error| format!("the first administrator cannot be written: {error}"))?;
            log.out(
                LogLevel::Info,
                "server.authserver",
                format!("The first administrator '{username}' was created from {admin_file}."),
            );
        } else {
            log.out(
                LogLevel::Debug,
                "server.authserver",
                format!("Accounts exist; {admin_file} is not applied."),
            );
        }
    } else {
        log.out(
            LogLevel::Warn,
            "server.authserver",
            format!("No administrator file at {admin_file}: no account is created from a file."),
        );
    }
    Ok(())
}

/// Stage 7: read the world list from the login database, then merge the worlds file over it.
///
/// Reference: `sRealmList->Initialize(*ioContext, RealmsStateUpdateDelay)`.
fn read_world_list(
    config: &Config,
    log: &Log,
    login: &mut DatabaseWorkerPool,
) -> Result<WorldListSource, String> {
    let update_delay = read::<u32>(config, "RealmsStateUpdateDelay", 20)?;

    let mut worlds = WorldList::new();
    let loaded = worlds.load(login.connection_mut(), log)?;
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!("World list: {loaded} world(s), refreshed every {update_delay} s."),
    );

    // The same rows as settings: a world's address is the operator's choice, so it can be written in
    // a file as well as in the table, and a file row overrides a table row of the same id. A file
    // that is not there merges nothing.
    let worlds_file = read_string(config, "WorldsFile", "data/realms.json")?;
    if !worlds_file.is_empty() {
        worlds.merge_file(&worlds_file, log)?;
    }

    Ok(WorldListSource {
        worlds,
        update_delay,
        worlds_file,
    })
}

/// Stop the program when the world list holds no world.
///
/// Reference: the three reasons the reference prints before it exits 1.
fn refuse_an_empty_world_list(worlds: &WorldList, log: &Log) -> Result<(), String> {
    if worlds.is_empty() {
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
    Ok(())
}

/// Decode the file check a session answers with, and say how long it is.
///
/// The file check is a protocol payload, not a configuration option: the client compares these
/// bytes and nobody chooses them, so they are constants (`docs/port/04-configuration.md`).
fn read_file_check(log: &Log) -> Result<FileCheck, String> {
    let file_check = FileCheck::shipped()
        .map_err(|error| format!("the file check's own constants do not decode: {error}"))?;
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!(
            "File check: {} bytes of question, {} of answer.",
            file_check.request_len(),
            file_check.answer().len()
        ),
    );
    Ok(file_check)
}

/// Read the two numbers a travel request is placed by, and say them.
///
/// Both are settings: which map id is the creation instance, and where a character with no map
/// anywhere enters. GWPS: the same decision is `AuthSrv_HandleRequestGameInstance`'s, and the
/// working implementation keeps the pair in its own defaults file
/// (`ASURA/crates/asura-rules/src/lib.rs`, `placement`).
fn read_placement(config: &Config, log: &Log) -> Result<Placement, String> {
    let creation_map = read::<u16>(config, "CreationMap", 0)?;
    let fallback_outpost = read::<u16>(config, "FallbackOutpost", 148)?;
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!(
            "Travel placement: the creation instance is map {creation_map}, and a character with no map anywhere enters map {fallback_outpost}."
        ),
    );
    Ok(Placement {
        creation_map,
        fallback_outpost,
    })
}

/// Start the portal's listener, when `Portal.Enable` asks for it.
///
/// The portal is served by this program rather than a third one: it hands out the identity this
/// program then resolves, and a third service is a third thing to start before a client can log in -
/// `docs/decisions/0003-portal-host.md`.
fn start_portal(
    config: &Config,
    log: &Log,
    login: &Arc<Mutex<DatabaseWorkerPool>>,
    game_accounts: &game::accounts::DatabaseAccounts,
    trace: bool,
    network_threads: usize,
) -> Result<Option<SocketMgr>, String> {
    let portal_enabled = read_bool(config, "Portal.Enable", false)?;
    let mut portal_sockets = None;
    if portal_enabled {
        let portal_bind = read_string(config, "Portal.Bind", "127.0.0.1:6601")?;

        let count = game_accounts
            .list()
            .map(|accounts| accounts.len())
            .unwrap_or_else(|error| {
                log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!("Cannot count the accounts: {error}"),
                );
                0
            });
        log.out(
            LogLevel::Info,
            "server.authserver",
            format!("Portal: {count} account(s) in the login database, binding {portal_bind}."),
        );

        let mut portal = game::portal::Portal::new(
            crate::server::DatabaseAccounts::new(Arc::clone(login), log.clone()),
        );
        let title_world_config =
            read_string(config, "Portal.TitleWorldConfig", "worldserver/worldserver.json")?;
        if !title_world_config.is_empty() {
            let world = Config::load_json(&title_world_config, false, Policy::default())
                .map_err(|error| error.to_string())?;
            let (pack, _) = world.value_string("WorldPack", "config/World/Vanilla")
                .map_err(|error| error.to_string())?;
            let path = std::path::Path::new(&pack).join("titles.json");
            let titles = game::characters::titles::VanillaTitles::load(&path)
                .inspect_err(|error| log.out(LogLevel::Error, "server.authserver", error))?;
            portal = portal.with_custom_titles(titles.custom_catalog());
            log.out(LogLevel::Info, "server.authserver", format!("Portal: custom title catalog from {}.", path.display()));
        }
        let portal = Arc::new(portal);
        let factory: SessionFactory = {
            let portal = Arc::clone(&portal);
            let log = Some(log.clone());
            Arc::new(move || {
                Box::new(game::portal::serve::PortalSession::new(
                    Arc::clone(&portal),
                    log.clone(),
                )) as Box<dyn Session>
            })
        };

        let (host, port_text) = portal_bind
            .rsplit_once(':')
            .ok_or_else(|| format!("Portal.Bind is not 'address:port': {portal_bind}"))?;
        let portal_port: u16 = port_text
            .parse()
            .map_err(|_| format!("Portal.Bind has no port: {portal_bind}"))?;

        let mut sockets = SocketMgr::new(factory);
        sockets.set_log(log.clone());
        if trace {
            sockets.set_trace(log.clone());
        }
        let bound = sockets
            .start_network(host, portal_port, network_threads)
            .map_err(|error| format!("the portal could not start: {error}"))?;
        log.out(
            LogLevel::Info,
            "server.authserver",
            format!("Portal: listening on {host}:{bound}."),
        );
        portal_sockets = Some(sockets);
    }
    Ok(portal_sockets)
}

/// Decode the 591-byte block every login is answered with: a protocol payload, so a constant.
fn read_account_settings(log: &Log) -> Result<Vec<u8>, String> {
    let account_settings = crate::server::account_settings::block();
    if account_settings.len() != crate::server::account_settings::LEN {
        return Err(format!(
            "the account settings block is {} bytes and the client needs {}, so it would desynchronise the connection",
            account_settings.len(),
            crate::server::account_settings::LEN
        ));
    }
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!("Account settings: {} bytes.", account_settings.len()),
    );
    Ok(account_settings)
}

/// Put the world list behind its lock and build the front door on it.
///
/// The front door: the worlds a travel is answered from, and where a gameplay connection that
/// arrives on this port is copied to. It is the same world list in both roles, so the address a
/// client is told and the address this program dials for it cannot disagree
/// (`docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`). The list is behind a
/// lock because it is refreshed on `RealmsStateUpdateDelay` (`start_world_list_refresh`).
fn open_front_door(
    worlds: WorldList,
    log: &Log,
) -> (Arc<Mutex<WorldList>>, Arc<crate::server::FrontDoor>) {
    let world_list = Arc::new(Mutex::new(worlds));
    let front_door = Arc::new(crate::server::FrontDoor::new(
        Arc::clone(&world_list),
        log.clone(),
    ));
    match front_door.pick() {
        Some(server) => log.out(
            LogLevel::Info,
            "server.authserver",
            format!(
                "Front door: a client asking for a map is told to dial {} as world {}, and its gameplay connection is copied there.",
                server.address, server.token
            ),
        ),
        None => log.out(
            LogLevel::Warn,
            "server.authserver",
            "Front door: every world in the list is offline, so a client asking for a map will be refused.",
        ),
    }
    (world_list, front_door)
}

/// Start the thread that re-reads the world list on the operator's interval.
///
/// The same way the reference's `sRealmList->Initialize(*ioContext, RealmsStateUpdateDelay)` keeps
/// its realms current: the table says who is online, a world that comes back must be handed out
/// again, and one marked offline must stop being offered - both without a restart. A change is
/// logged; an unchanged refresh says nothing.
fn start_world_list_refresh(
    log: &Log,
    login: &Arc<Mutex<DatabaseWorkerPool>>,
    world_list: &Arc<Mutex<WorldList>>,
    worlds_file: &str,
    update_delay: u32,
) -> Result<Background, String> {
    let stop_realms = Arc::new(AtomicBool::new(false));
    let refresh_writer = {
        let log = log.clone();
        let login = Arc::clone(login);
        let world_list = Arc::clone(world_list);
        let worlds_file = worlds_file.to_string();
        let stop_realms = Arc::clone(&stop_realms);
        let delay = update_delay.max(1);
        std::thread::Builder::new()
            .name("realms".to_string())
            .spawn(move || {
                let mut last = world_list_summary(&world_list);
                while !stop_realms.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_secs(u64::from(delay)));
                    if stop_realms.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let mut refreshed = WorldList::new();
                    let loaded = match login.lock() {
                        Ok(mut pool) => refreshed.load(pool.connection_mut(), &log),
                        Err(_) => continue,
                    };
                    match loaded {
                        Ok(_) => {}
                        Err(error) => {
                            log.out(
                                LogLevel::Warn,
                                "server.authserver",
                                format!("The world list could not be refreshed: {error}"),
                            );
                            continue;
                        }
                    }
                    if !worlds_file.is_empty()
                        && let Err(error) = refreshed.merge_file(&worlds_file, &log)
                    {
                        log.out(
                            LogLevel::Warn,
                            "server.authserver",
                            format!("The world list file could not be read on a refresh: {error}"),
                        );
                    }
                    if let Ok(mut current) = world_list.lock() {
                        *current = refreshed;
                    }
                    let now = world_list_summary(&world_list);
                    if now != last {
                        log.out(
                            LogLevel::Info,
                            "server.authserver",
                            format!("World list refreshed: {now}"),
                        );
                        last = now;
                    }
                }
            })
            .map_err(|error| format!("cannot start the world list refresh: {error}"))?
    };
    Ok(Background {
        stop: stop_realms,
        thread: refresh_writer,
    })
}

/// Open the character database and build the store a login lists an account's characters from.
///
/// The character database is the second one this program opens, because a login's answer lists the
/// account's characters (`docs/decisions/0007-character-schema.md`). Its connection string is
/// critical for the same reason as the login's: the answer a client is given is read from a database
/// an operator named.
fn open_character_database(
    config: &mut Config,
    log: &Log,
    sql_trace: bool,
) -> Result<Arc<crate::server::DatabaseCharacters>, String> {
    config.mark_critical("CharacterDatabaseInfo");
    let character_settings = read_settings(config, DatabaseKind::Character)?;
    let mut character_pool = DatabaseWorkerPool::open_with_trace(
        DatabaseKind::Character,
        &character_settings,
        log,
        sql_trace,
    )?;
    character_pool
        .ping()
        .map_err(|error| format!("the character database does not answer: {error}"))?;
    log.out(
        LogLevel::Info,
        "server.authserver",
        "Started character database connection pool.",
    );
    Ok(Arc::new(crate::server::DatabaseCharacters::new(
        Arc::new(Mutex::new(character_pool)),
        log.clone(),
    )))
}

/// The factory the login listener makes one session per connection with.
#[allow(clippy::too_many_arguments)]
fn session_factory(
    log: &Log,
    file_check: &FileCheck,
    account_settings: &[u8],
    front_door: &Arc<crate::server::FrontDoor>,
    characters_store: &Arc<crate::server::DatabaseCharacters>,
    travels: &Arc<game::maps::DatabaseTravels>,
    game_accounts: &Arc<game::accounts::DatabaseAccounts>,
    placement: Placement,
) -> SessionFactory {
    let log = log.clone();
    let file_check = file_check.clone();
    let account_settings = account_settings.to_vec();
    let front_door = Arc::clone(front_door);
    let characters = Arc::clone(characters_store);
    let travels = Arc::clone(travels);
    let accounts = Arc::clone(game_accounts);
    let Placement {
        creation_map,
        fallback_outpost,
    } = placement;
    Arc::new(move || {
        Box::new(
            AuthSession::with_random_master(
                log.clone(),
                file_check.clone(),
                account_settings.clone(),
                Some(Arc::clone(&front_door)),
                Some(Arc::clone(&characters)),
            )
            .with_travels(Arc::clone(&travels))
            .with_accounts(Arc::clone(&accounts))
            .with_placement(creation_map, fallback_outpost),
        ) as Box<dyn Session>
    })
}

/// Stage 8: bind the login listener and start its threads, answering the port it bound.
///
/// Reference: `sAuthSocketMgr.StartNetwork(*ioContext, bindIp, port)`.
fn start_listener(
    factory: SessionFactory,
    log: &Log,
    trace: bool,
    listener: &ListenerSettings,
) -> Result<(SocketMgr, u16), String> {
    let mut sockets = SocketMgr::new(factory);
    sockets.set_log(log.clone());

    if trace {
        sockets.set_trace(log.clone());
        // The bytes are logged at debug, which the shipped configuration filters out. A switch that
        // silently does nothing is worse than no switch, so the program says which line to add.
        if log.enabled("network.trace", LogLevel::Debug) {
            log.out(
                LogLevel::Info,
                "server.authserver",
                "Network.Trace is on: every byte in and out is logged at debug.",
            );
        } else {
            log.out(
                LogLevel::Warn,
                "server.authserver",
                "Network.Trace is on, but no logger named 'network.trace' is enabled, so nothing will be traced. Add this line to the configuration file: Logger.network.trace = 6,Console Auth",
            );
        }
    }

    let bound = sockets
        .start_network(&listener.bind, listener.port, listener.network_threads)
        .map_err(|error| format!("Failed to initialize network: {error}"))?;
    Ok((sockets, bound))
}

/// Say the program is up, what it is up on, and how often it will say so again; answer that
/// interval, in seconds.
///
/// A program that cannot say this cannot be tested by anyone, including whoever runs it.
fn announce_ready(config: &Config, log: &Log, bind: &str, bound: u16) -> Result<u64, String> {
    let status_interval = read::<u64>(config, "StatusInterval", 30)?;
    let status_interval = status_interval.max(1);

    // The stack's handshake: printed only when a supervisor asked for it (`KRYTA_STACK`), because
    // nothing this program says belongs on a screen the operator did not ask to fill. The record is
    // the appenders (`Auth.log`), and these same words go there.
    let ready = format!(
        "READY: listening on {bind}:{bound}; status reported every {status_interval} s."
    );
    if std::env::var_os("KRYTA_STACK").is_some() {
        // Only ever read by the supervisor that set the variable, and never shown to anybody.
        println!("{ready}");
    }
    log.out(LogLevel::Info, "server.authserver", ready);
    Ok(status_interval)
}

/// Start the thread that writes a status line on a timer.
///
/// With it, "is it alive and has anything reached it" is answerable from the log alone, without
/// looking at the machine by hand.
fn start_status_reporter(
    log: &Log,
    sockets: &SocketMgr,
    bind: String,
    bound: u16,
    status_interval: u64,
) -> Result<Background, String> {
    let stop_status = Arc::new(AtomicBool::new(false));
    let accepted = sockets.accepted_handle();
    let live = sockets.live_handle();
    let status_writer = {
        let log = log.clone();
        let stop_status = Arc::clone(&stop_status);
        let started = std::time::Instant::now();
        std::thread::Builder::new()
            .name("status".to_string())
            .spawn(move || {
                while !stop_status.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_secs(status_interval));
                    if stop_status.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    log.out(
                        LogLevel::Info,
                        "server.authserver",
                        format!(
                            "status: up {} s, listening on {bind}:{bound}, {} connection(s) accepted, {} live.",
                            started.elapsed().as_secs(),
                            accepted.load(std::sync::atomic::Ordering::Relaxed),
                            live.load(std::sync::atomic::Ordering::Relaxed)
                        ),
                    );
                }
            })
            .map_err(|error| format!("cannot start the status writer: {error}"))?
    };
    Ok(Background {
        stop: stop_status,
        thread: status_writer,
    })
}

/// Close what the program opened: the two threads, the listeners, the login pool, the log.
fn shut_down(
    log: &Log,
    status: Background,
    refresh: Background,
    mut sockets: SocketMgr,
    portal_sockets: Option<SocketMgr>,
    login: &Arc<Mutex<DatabaseWorkerPool>>,
) {
    log.out(
        LogLevel::Info,
        "server.authserver",
        "Stopping: the listener is closing.",
    );

    status.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = status.thread.join();
    refresh.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = refresh.thread.join();

    sockets.stop_network();
    if let Some(mut portal_sockets) = portal_sockets {
        portal_sockets.stop_network();
    }
    match login.lock() {
        Ok(mut pool) => pool.close(),
        Err(_) => log.out(
            LogLevel::Error,
            "server.authserver",
            "the login database pool was poisoned; it closes when the process ends",
        ),
    }
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!(
            "Stopped. {} connection(s) were accepted while it ran.",
            sockets.accepted_count()
        ),
    );
    log.shutdown();
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

/// A one-line summary of a world list, for deciding whether a refresh changed anything.
fn world_list_summary(worlds: &Arc<Mutex<WorldList>>) -> String {
    let Ok(worlds) = worlds.lock() else {
        return String::new();
    };
    worlds
        .worlds()
        .values()
        .map(|world| {
            format!(
                "world {} at {}:{} {}",
                world.id,
                world.address_text,
                world.port,
                if world.is_offline() { "offline" } else { "online" }
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        Options::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn the_config_file_defaults_to_the_reference_name() {
        assert_eq!(parse(&[]).config, PathBuf::from("authserver.json"));
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
