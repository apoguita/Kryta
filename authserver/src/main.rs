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
#[path = "Protocol/mod.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "Server/mod.rs"]
mod server;

use crate::server::{AuthSession, FileCheck};
use common::configuration::{Complaint, Config, Policy, PolicyOverride, Severity};
use common::logging::{Log, LogLevel};
use common::utilities::create_pid_file;
use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
use database::updater::{UpdateState, collect_files};
use shared::network::socket::{Session, SessionFactory};
use shared::network::socket_mgr::SocketMgr;
use shared::worlds::WorldList;
use std::path::PathBuf;
use std::process::ExitCode;

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
        Config::load_json(&options.config, false, policy).map_err(|error| error.to_string())?;
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

    // How many threads serve the accepted sockets, for every listener this program opens. Reference:
    // `Network.Threads` (1), which its socket manager takes as its thread count.
    let (network_threads, notes) = config
        .value::<usize>("Network.Threads", 1)
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
    //
    // The connection string is critical: a setting is read from the file, so a program is not
    // allowed to fall back to a compiled-in host and open a database nobody named.
    config.mark_critical("LoginDatabaseInfo");
    let settings = read_settings(&config, DatabaseKind::Login)?;

    // The database's own trace: every statement the pool runs, on any of its connections.
    let (sql_trace, notes) = config
        .value_bool("Database.Trace", false)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    // Optional and off by default: with it on, every byte a client sends and receives is one line in
    // the log. It is the difference between "the client said nothing useful" and knowing exactly what
    // it said. Read here because both listeners use it.
    let (trace, notes) = config
        .value_bool("Network.Trace", false)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }

    let mut login =
        DatabaseWorkerPool::open_with_trace(DatabaseKind::Login, &settings, &log, sql_trace)?;
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
    let files = collect_files(schema, UpdateState::Released).map_err(|error| {
        format!(
            "the login schema at {} is not readable: {error}",
            schema.display()
        )
    })?;
    // The first administrator: a file, not a SQL seed (`config/Server/admin.json`,
    // `docs/decisions/0013-the-admin-identity-is-a-file.md`). It is read at every start and writes an
    // account only when the table is empty, so editing it later changes nothing about existing accounts.
    {
        let (admin_file, notes) = config
            .value_string("AdminAccountFile", "config/Server/admin.json")
            .map_err(|error| error.to_string())?;
        for note in notes {
            report(&note);
        }
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
                format!(
                    "No administrator file at {admin_file}: no account is created from a file."
                ),
            );
        }
    }
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

    // The same rows as settings: a world's address is the operator's choice, so it can be written in
    // a file as well as in the table, and a file row overrides a table row of the same id. A file
    // that is not there merges nothing.
    let (worlds_file, notes) = config
        .value_string("WorldsFile", "data/realms.json")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    if !worlds_file.is_empty() {
        worlds.merge_file(&worlds_file, &log)?;
    }

    // From here the pool is shared: the portal asks it who a login name is, from a network thread,
    // on every login. Its synchronous connection is one connection, so the pool goes behind a lock -
    // which is what the reference's own `DirectQuery` callers hold too.
    let login = std::sync::Arc::new(std::sync::Mutex::new(login));

    // The travel table in the login database: a `0x8029` answer records the hand-off here, and the
    // world claims the same row once when the connection presents its token pair
    // (`docs/port/06-world-plan.md`, "The three seams"). It is state, so it is a row.
    let travels = std::sync::Arc::new(game::maps::DatabaseTravels::new(
        std::sync::Arc::clone(&login),
        log.clone(),
    ));

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
    // The session is the reference's `AuthSession`: it reads the client's version and its seed and
    // answers with the server's seed. What it does not do yet is anything after that point, because
    // everything there is scrambled with a key both sides derive and that derivation is not written.
    // The file check is a protocol payload, not a configuration option: the client compares these
    // bytes and nobody chooses them, so they are constants (`docs/port/04-configuration.md`).
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

    // The accounts are rows in the login database, not a file: they are the server's state, and an
    // operator configures the server rather than its people (`docs/decisions/0006`). This store is
    // the game library's: a login session records the EULA version through it, and the
    // administration tool reads the same table through the same code.
    let game_accounts = std::sync::Arc::new(game::accounts::DatabaseAccounts::new(
        std::sync::Arc::clone(&login),
        log.clone(),
    ));

    // The two numbers a travel request is placed by, both settings: which map id is the creation
    // instance, and where a character with no map anywhere enters. Reference: the same decision is
    // `AuthSrv_HandleRequestGameInstance`'s, and the working implementation keeps the pair in its own
    // defaults file (`ASURA/crates/asura-rules/src/lib.rs`, `placement`).
    let (creation_map, notes) = config
        .value::<u16>("CreationMap", 0)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (fallback_outpost, notes) = config
        .value::<u16>("FallbackOutpost", 148)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    log.out(
        LogLevel::Info,
        "server.authserver",
        format!(
            "Travel placement: the creation instance is map {creation_map}, and a character with no map anywhere enters map {fallback_outpost}."
        ),
    );

    // The portal, when the configuration asks for it. It is served by this program rather than a
    // third one: it hands out the identity this program then resolves, and a third service is a third
    // thing to start before a client can log in - `docs/decisions/0003-portal-host.md`.
    let (portal_enabled, notes) = config
        .value_bool("Portal.Enable", false)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let mut portal_sockets = None;
    if portal_enabled {
        let (portal_bind, notes) = config
            .value_string("Portal.Bind", "127.0.0.1:6601")
            .map_err(|error| error.to_string())?;
        for note in notes {
            report(&note);
        }

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

        let mut portal = game::portal::Portal::new(crate::server::DatabaseAccounts::new(
            std::sync::Arc::clone(&login),
            log.clone(),
        ));
        let (title_world_config, notes) = config
            .value_string("Portal.TitleWorldConfig", "worldserver/worldserver.json")
            .map_err(|error| error.to_string())?;
        for note in notes {
            report(&note);
        }
        if !title_world_config.is_empty() {
            let world = Config::load_json(&title_world_config, false, Policy::default())
                .map_err(|error| error.to_string())?;
            let (pack, _) = world
                .value_string("WorldPack", "config/World/Vanilla")
                .map_err(|error| error.to_string())?;
            let path = std::path::Path::new(&pack).join("titles.json");
            let titles = game::characters::titles::VanillaTitles::load(&path)
                .inspect_err(|error| log.out(LogLevel::Error, "server.authserver", error))?;
            portal = portal.with_custom_titles(titles.custom_catalog());
            log.out(
                LogLevel::Info,
                "server.authserver",
                format!("Portal: custom title catalog from {}.", path.display()),
            );
        }
        let portal = std::sync::Arc::new(portal);
        let factory: SessionFactory = {
            let portal = std::sync::Arc::clone(&portal);
            let log = Some(log.clone());
            std::sync::Arc::new(move || {
                Box::new(game::portal::serve::PortalSession::new(
                    std::sync::Arc::clone(&portal),
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

    // The 591-byte block every login is answered with: a protocol payload, so a constant.
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

    // The front door: the worlds a travel is answered from, and where a gameplay connection that
    // arrives on this port is copied to. It is the same world list in both roles, so the address a
    // client is told and the address this program dials for it cannot disagree
    // (`docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`). The list is behind a
    // lock because it is refreshed on `RealmsStateUpdateDelay` (below).
    let world_list = std::sync::Arc::new(std::sync::Mutex::new(worlds));
    let front_door = std::sync::Arc::new(crate::server::FrontDoor::new(
        std::sync::Arc::clone(&world_list),
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

    // The world list is refreshed on the operator's interval, the same way the reference's
    // `sRealmList->Initialize(*ioContext, RealmsStateUpdateDelay)` keeps its realms current: the
    // table says who is online, a world that comes back must be handed out again, and one marked
    // offline must stop being offered - both without a restart. A change is logged; an unchanged
    // refresh says nothing.
    let stop_realms = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let refresh_writer = {
        let log = log.clone();
        let login = std::sync::Arc::clone(&login);
        let world_list = std::sync::Arc::clone(&world_list);
        let worlds_file = worlds_file.clone();
        let stop_realms = std::sync::Arc::clone(&stop_realms);
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

    // The characters of the account a login resolves: read from the character database, which is the
    // second database this program opens now that a login's answer lists them
    // (`docs/decisions/0007-character-schema.md`). Its connection string is critical for the same
    // reason as the login's: the answer a client is given is read from a database an operator named.
    config.mark_critical("CharacterDatabaseInfo");
    let character_settings = read_settings(&config, DatabaseKind::Character)?;
    let mut character_pool = DatabaseWorkerPool::open_with_trace(
        DatabaseKind::Character,
        &character_settings,
        &log,
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
    let characters_store = std::sync::Arc::new(crate::server::DatabaseCharacters::new(
        std::sync::Arc::new(std::sync::Mutex::new(character_pool)),
        log.clone(),
    ));

    let factory: SessionFactory = {
        let log = log.clone();
        let file_check = file_check.clone();
        let account_settings = account_settings.clone();
        let front_door = std::sync::Arc::clone(&front_door);
        let characters = std::sync::Arc::clone(&characters_store);
        let travels = std::sync::Arc::clone(&travels);
        let accounts = std::sync::Arc::clone(&game_accounts);
        std::sync::Arc::new(move || {
            Box::new(
                AuthSession::with_random_master(
                    log.clone(),
                    file_check.clone(),
                    account_settings.clone(),
                    Some(std::sync::Arc::clone(&front_door)),
                    Some(std::sync::Arc::clone(&characters)),
                )
                .with_travels(std::sync::Arc::clone(&travels))
                .with_accounts(std::sync::Arc::clone(&accounts))
                .with_placement(creation_map, fallback_outpost),
            ) as Box<dyn Session>
        })
    };

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
        .start_network(&bind, port, network_threads)
        .map_err(|error| format!("Failed to initialize network: {error}"))?;

    // The line that says the program is up, what it is up on, and how often it will say so again.
    // A program that cannot say this cannot be tested by anyone, including whoever runs it.
    let (status_interval, notes) = config
        .value::<u64>("StatusInterval", 30)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let status_interval = status_interval.max(1);

    // The stack's handshake: printed only when a supervisor asked for it (`KRYTA_STACK`), because
    // nothing this program says belongs on a screen the operator did not ask to fill. The record is
    // the appenders (`Auth.log`), and these same words go there.
    let ready =
        format!("READY: listening on {bind}:{bound}; status reported every {status_interval} s.");
    if std::env::var_os("KRYTA_STACK").is_some() {
        // Only ever read by the supervisor that set the variable, and never shown to anybody.
        println!("{ready}");
    }
    log.out(LogLevel::Info, "server.authserver", ready);

    // A status line on a timer, so "is it alive and has anything reached it" is answerable from the
    // log alone. Without this the only way to learn whether a client connected was to look at the
    // machine by hand.
    let stop_status = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let accepted = sockets.accepted_handle();
    let live = sockets.live_handle();
    let status_writer = {
        let log = log.clone();
        let stop_status = std::sync::Arc::clone(&stop_status);
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

    // Reference: the signal handlers stop the acceptor and the threads; `ioContext->run()` returns
    // and the program closes what it opened, in reverse order.
    wait_for_shutdown();
    log.out(
        LogLevel::Info,
        "server.authserver",
        "Stopping: the listener is closing.",
    );

    stop_status.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = status_writer.join();
    stop_realms.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = refresh_writer.join();

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

/// A one-line summary of a world list, for deciding whether a refresh changed anything.
fn world_list_summary(worlds: &std::sync::Arc<std::sync::Mutex<WorldList>>) -> String {
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
                if world.is_offline() {
                    "offline"
                } else {
                    "online"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
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
                    options.config = PathBuf::from(rest.next().ok_or("--config needs a path")?);
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
    println!(
        "  -c [ --config ] arg       use <arg> as configuration file (default: {DEFAULT_CONFIG})"
    );
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
