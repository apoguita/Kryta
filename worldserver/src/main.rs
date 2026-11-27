//! worldserver - the game program.
//!
//! Reference: `src/server/apps/worldserver/Main.cpp` at 30a36ec4d. The stages below are that
//! file's order, and each one names the reference call it translates.
//!
//! The program runs. It reads its configuration, logs, writes its PID file, registers its scripts,
//! opens the login, world and character databases and brings each schema up to date, loads the world
//! from its pack, listens, and runs the world's update loop until it is asked to stop. It exits with
//! the code the world stopped under.
//!
//! `run` is the list of stages, in the order they happen; each stage is one function below it.

// The console and the local control endpoint. `dead_code` is allowed on both because each holds
// items that only its own tests call.
#[allow(dead_code)]
#[path = "CommandLine/mod.rs"]
mod command_line;
#[allow(dead_code)]
#[path = "RemoteAccess/mod.rs"]
mod remote_access;

use common::configuration::startup::{policy, print_usage, read, read_bool, read_string, report};
use common::configuration::Config;
use common::logging::{Log, LogLevel};
use common::utilities::create_pid_file;
use database::database::{read_settings, DatabaseKind, DatabaseWorkerPool};
use database::updater::{collect_files, UpdateState};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// The config file read when `-c` names none.
/// Reference: `_ACORE_CORE_CONFIG  "worldserver.json"`, with this project's own file format: JSON.
const DEFAULT_CONFIG: &str = "worldserver.json";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        // The reference's own exit-code contract (`Main.cpp:434-439`): the world answers with the
        // code it stopped under, so whoever supervises it can tell a restart from a shutdown.
        Ok(code) => ExitCode::from(code as u8),
        Err(reason) => {
            eprintln!("worldserver: {reason}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<game::world::ExitCode, String> {
    // Stage 1. Reference: `GetConsoleArguments(argc, argv, configFile)`.
    let options = Options::parse(args)?;

    // Reference: `if (vm.count("help") || vm.count("version")) return 0;`
    if options.help {
        print_usage(DEFAULT_CONFIG);
        return Ok(game::world::ExitCode::Normal);
    }
    if options.version {
        println!("Kryta worldserver {}", env!("CARGO_PKG_VERSION"));
        return Ok(game::world::ExitCode::Normal);
    }

    // Stage 2. Reference: `sConfigMgr->Configure(...)` then `LoadAppConfigs()`.
    let repair = SchemaRepair::from_arguments(args);
    let mut config = read_configuration(&options)?;

    // Stage 3. Reference: `sLog->Initialize(sConfigMgr->GetOption<bool>("Log.Async.Enable", ...))`.
    let log = start_logging(&config)?;

    // Stage 4. Reference: `CreatePIDFile(pidFile)`, when `PidFile` is set.
    write_pid_file(&config, &log)?;

    // Stage 5. Reference: `sScriptMgr->SetScriptLoader(AddScripts)`, `SetModulesLoader`, then
    // `sScriptMgr->Initialize()`.
    let scripts = start_scripts(&log)?;

    // Stage 6. Reference: `StartDB()`.
    let databases = open_databases(&mut config, &log, repair)?;

    // Stage 7. Reference: `sWorld->SetInitialWorldSettings()`.
    let LoadedWorld {
        world,
        operations,
        realm,
    } = load_world(&config, &log, scripts, &databases)?;

    // Stage 8. Reference: `sWorldSocketMgr.StartWorldNetwork(*ioContext, worldListener, worldPort,
    // networkThreads)`.
    let listener = read_listener_settings(&config)?;
    let (sockets, bound) = start_listener(&world, &databases, &log, &listener)?;
    announce_ready(&log, &listener, bound, realm);
    // The world is up, so its scripts are told (`sScriptMgr->OnStartup()`, `Main.cpp:398`).
    world.on_startup();

    // Reference: Main.cpp:400-408 and CliRunnable.cpp:231: submit to the world update.
    let (control, command_receiver) = start_console_and_control(&config, &log, &world, &operations)?;

    // The world's loop: `WorldUpdateLoop` (`Main.cpp:584-620`).
    let world_loop = start_world_loop(
        &log,
        &world,
        &sockets,
        operations,
        command_receiver,
        &listener,
        bound,
        realm,
    )?;

    wait_for_shutdown(&world);
    let exit = world.exit_code();
    shut_down(&log, &world, control, world_loop, sockets, &databases);
    Ok(exit)
}

/// The two repairs an operator may ask for on the command line.
#[derive(Clone, Copy)]
struct SchemaRepair {
    /// `--reset-schema`: a repair an operator asks for in one flag, rather than a dead stack: the
    /// world database's schema is dropped and rebuilt from the files
    /// (`docs/port/04-configuration.md`).
    reset: bool,
    /// `--recheck-schema`: a schema file changed after this database recorded it. The operator says
    /// so in one command, and the recorded hash is re-taken from the file instead of the stack
    /// staying down.
    recheck: bool,
}

impl SchemaRepair {
    /// Read the two flags from the command line.
    fn from_arguments(args: &[String]) -> Self {
        let reset = args.iter().any(|argument| argument == "--reset-schema");
        let recheck = args.iter().any(|argument| argument == "--recheck-schema");
        Self { reset, recheck }
    }
}

/// The three databases this program opens, each behind the lock its users share it through.
struct Databases {
    /// The login database: a connection claims its travel record from a session thread, so this
    /// pool is shared with the listener.
    login: Arc<Mutex<DatabaseWorkerPool>>,
    /// The world database: the instances are rows, and the instance store is built on this pool.
    world: Arc<Mutex<DatabaseWorkerPool>>,
    /// The character database: a creation confirm writes a character from a session thread, so the
    /// pool is shared with the listener the way the login program shares its own with the portal.
    character: Arc<Mutex<DatabaseWorkerPool>>,
}

/// The world once its content is loaded, and what the later stages need beside it.
struct LoadedWorld {
    /// The world, shared from here on.
    world: Arc<game::world::World>,
    /// The operations this world answers.
    operations: Arc<game::chat::operations::Registry>,
    /// `RealmID`: which world this is.
    realm: u32,
}

/// The files the creation screen and the command surface are read from.
struct ContentFiles {
    /// `InventoryFile`.
    inventory: String,
    /// `EquipmentFile`.
    equipment: String,
    /// `EquippedSlotsFile`.
    worn_slots: String,
    /// `CommandsFile`.
    commands: String,
}

/// What the listener and the world loop run by, as the configuration states it.
struct ListenerSettings {
    /// `BindIP`.
    bind: String,
    /// `WorldServerPort`.
    port: u16,
    /// `Network.Threads`.
    network_threads: usize,
    /// `Network.Trace`.
    trace: bool,
    /// `StatusInterval`, in seconds, never less than one.
    status_interval: u64,
    /// `MinWorldUpdateTime`, never less than one millisecond.
    min_update: std::time::Duration,
}

/// A thread this program started, and the flag that asks it to stop.
struct Background {
    /// Set to ask the thread to end.
    stop: Arc<AtomicBool>,
    /// The thread, joined at shutdown.
    thread: JoinHandle<()>,
}

/// Stage 2: load the configuration file.
///
/// Reference: `sConfigMgr->Configure(...)` then `LoadAppConfigs()`. The severity policy can be
/// overridden on the command line, and a key it does not know is refused rather than ignored - the
/// reference reports it the same way.
fn read_configuration(options: &Options) -> Result<Config, String> {
    let policy = policy(options.config_policy.as_deref())?;

    let mut config =
        Config::load_json(&options.config, false, policy).map_err(|error| error.to_string())?;
    config.set_dry_run(options.dry_run);

    for complaint in config.complaints() {
        report(complaint);
    }
    Ok(config)
}

/// Stage 3: start the log, print the banner, and print the effective configuration when asked.
///
/// Reference: `sLog->Initialize(sConfigMgr->GetOption<bool>("Log.Async.Enable", ...))`.
fn start_logging(config: &Config) -> Result<Log, String> {
    let log = Log::new();
    let notes = log.initialize(config).map_err(|error| error.to_string())?;
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
/// Reference: `CreatePIDFile(pidFile)` - the same stage the auth program has, with the same two
/// lines.
fn write_pid_file(config: &Config, log: &Log) -> Result<(), String> {
    let pid_file = read_string(config, "PidFile", "")?;
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
    Ok(())
}

/// Stage 5: hand the script manager its two loaders and initialize it.
///
/// Reference: `sScriptMgr->SetScriptLoader(AddScripts)`, `SetModulesLoader`, then
/// `sScriptMgr->Initialize()` with `Initializing Scripts...` printed before it. The reference
/// starts its io-context threads (its `ThreadPool`) before this; that pool is this tree's socket
/// layer, which owns its runtimes and is created with the listener, so the thread stage lands
/// there and not as a stage of its own (`docs/port/01-structure.md`).
fn start_scripts(log: &Log) -> Result<game::scripting::ScriptMgr, String> {
    let mut scripts = game::scripting::ScriptMgr::new();
    scripts.set_script_loader(scripts::add_scripts);
    scripts.set_modules_loader(scripts::add_modules_scripts);
    log.out(LogLevel::Info, "server.loading", "Initializing Scripts...");
    let registered = scripts.initialize().map_err(|error| error.to_string())?;
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "Scripts: {} script(s), {} hook(s) registered.",
            registered.scripts, registered.hooks
        ),
    );
    Ok(scripts)
}

/// Stage 6: open the login, world and character databases, in that order, and bring each schema up
/// to date.
///
/// Reference: `StartDB()`, which opens the World and Character pools and runs the schema updater on
/// each - the stage the auth program has, twice.
///
/// The world program opens three databases: the world's state, a character's sheets, and the
/// login database's `travel` record - the hand-off a login writes and this program claims, which
/// is why the two programs need not talk to each other (`docs/port/06-world-plan.md`, "The three
/// seams"). The login schema is applied here too, so a world started first still finds its table.
fn open_databases(
    config: &mut Config,
    log: &Log,
    repair: SchemaRepair,
) -> Result<Databases, String> {
    let sql_trace = read_bool(config, "Database.Trace", false)?;

    config.mark_critical("LoginDatabaseInfo");
    let login = open_database(
        config,
        log,
        DatabaseKind::Login,
        "Login",
        "data/sql/base/db_auth",
        sql_trace,
        repair,
    )?;
    let world = open_database(
        config,
        log,
        DatabaseKind::World,
        "World",
        "data/sql/base/db_world",
        sql_trace,
        repair,
    )?;
    let character = open_database(
        config,
        log,
        DatabaseKind::Character,
        "Character",
        "data/sql/base/db_characters",
        sql_trace,
        repair,
    )?;
    Ok(Databases {
        login: Arc::new(Mutex::new(login)),
        world: Arc::new(Mutex::new(world)),
        character: Arc::new(Mutex::new(character)),
    })
}

/// Open one database, apply the repair the operator asked for, apply its schema files, and say so.
fn open_database(
    config: &Config,
    log: &Log,
    kind: DatabaseKind,
    label: &str,
    schema: &str,
    sql_trace: bool,
    repair: SchemaRepair,
) -> Result<DatabaseWorkerPool, String> {
    let settings = read_settings(config, kind)?;
    let mut pool = DatabaseWorkerPool::open_with_trace(kind, &settings, log, sql_trace)?;

    // `--reset-schema`: the schema files changed after this database recorded them as applied, or an
    // operator wants the tables rebuilt from the files as they are.
    // Only the world database is reset: it holds state and no content, and the files rebuild it.
    // The login database holds accounts and the character database holds what players earned, so
    // neither is dropped by a flag - a changed file there is `--recheck-schema`, which deletes
    // nothing (`docs/port/04-configuration.md`).
    if repair.reset && kind == DatabaseKind::World {
        pool.query_with("DROP SCHEMA IF EXISTS public CASCADE", &[])
            .map_err(|error| format!("the {label} schema cannot be dropped: {error}"))?;
        pool.query_with("CREATE SCHEMA public", &[])
            .map_err(|error| format!("the {label} schema cannot be recreated: {error}"))?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!("Reset the {label} schema: dropped and recreated, so the files apply from scratch."),
        );
    }

    let schema = std::path::Path::new(schema);
    let files = collect_files(schema, UpdateState::Released).map_err(|error| {
        format!(
            "the {label} schema at {} is not readable: {error}",
            schema.display()
        )
    })?;
    if repair.recheck {
        for file in &files {
            let text = database::updater::read_sql_update(&file.path)
                .map_err(|error| error.to_string())?;
            let hash = database::updater::apply::sha1_hex(&text);
            let name = file.name();
            pool.query_with(
                "UPDATE \"updates\" SET \"hash\" = $1 WHERE \"name\" = $2",
                &[&hash, &name],
            )
            .map_err(|error| format!("the {label} update record cannot be re-taken: {error}"))?;
            log.out(
                LogLevel::Info,
                "server.worldserver",
                format!("{label} schema: re-recorded {name} as {hash}."),
            );
        }
    }
    let summary = pool.apply_updates(&files, log)?;
    pool.ping()
        .map_err(|error| format!("the {label} database does not answer: {error}"))?;

    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "Started {label} database connection pool: {} file(s) applied, {} already current.",
            summary.updated, summary.skipped
        ),
    );
    Ok(pool)
}

/// Stage 7: build the world and load everything the game is made of, before the listener opens.
///
/// Reference: `sWorld->SetInitialWorldSettings()`. Each step below is one function, in the order
/// the content is read: the maps and titles, the operations and the command surface, the wire
/// capture, the world's numbers, the path graphs, the creation screen, the doorways, the instance
/// table and the item definitions.
fn load_world(
    config: &Config,
    log: &Log,
    scripts: game::scripting::ScriptMgr,
    databases: &Databases,
) -> Result<LoadedWorld, String> {
    let realm = read::<u32>(config, "RealmID", 1)?;

    let mut world = game::world::World::new(realm);
    // The scripts the program registered at stage 5 are the world's: the manager is a game symbol in
    // the reference too (`sScriptMgr`, `src/server/game/Scripting/`), and the world is the object
    // that raises its moments.
    world.set_scripts(scripts);

    let (pack, hostable) = load_maps(config, log, &mut world)?;
    let files = read_content_files(config)?;
    let operations = load_operations_and_commands(&files, log, &mut world)?;
    start_wire_capture(config, log)?;
    load_world_settings(config, log, &mut world)?;
    load_graphs(config, log, &hostable, &pack, &mut world)?;
    load_creation(&files, log, &mut world)?;
    load_doorways(&pack, log, &mut world);
    open_instances(&databases.world, log, &mut world);
    load_item_definitions(log, &mut world);

    Ok(LoadedWorld {
        world: Arc::new(world),
        operations,
        realm,
    })
}

/// Read the map pack into the world, then the titles, and answer the pack and the maps this world
/// can put a client in.
///
/// The maps are the pack's (`config/World/Vanilla/maps/...`): what a world *is* is a file, and a row
/// is what a server writes (`docs/decisions/0014-the-world-content-is-a-pack.md`). Read once, held for
/// the life of the program, and the log says which source answered. The databases are opened for
/// the state they hold - instances, characters, items, travels - and not for content.
fn load_maps(
    config: &Config,
    log: &Log,
    world: &mut game::world::World,
) -> Result<(game::maps::pack::Pack, Vec<game::world::MapEntry>), String> {
    let world_pack = read_string(config, "WorldPack", "config/World/Vanilla")?;
    let pack = game::maps::pack::Pack::load(&world_pack, log)?;
    let maps = world.load_maps(&pack, &format!("the map pack at {world_pack}"), log)?;
    world.load_titles(std::path::Path::new(&world_pack).join("titles.json"), log)?;
    // The maps this world can put a client in: a record the pack states no type or no blob for is a
    // destination the client can resolve and this world cannot load, so it is not asked for a graph
    // (`ASURA/crates/asura-world/src/catalogue.rs:54-63`).
    let hostable: Vec<game::world::MapEntry> = world
        .maps()
        .iter()
        .filter(|map| map.is_hostable())
        .cloned()
        .collect();
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "World of {maps} map(s), {} of them hosted{}.{}",
            hostable.len(),
            if maps <= 8 {
                format!(
                    ": {}",
                    world
                        .maps()
                        .iter()
                        .map(|map| format!("{} '{}' (type {})", map.id, map.name, map.kind_name()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                String::new()
            },
            if world.maps().len() > hostable.len() {
                format!(
                    " {} record(s) are destinations the client can resolve and this world cannot load.",
                    world.maps().len() - hostable.len()
                )
            } else {
                String::new()
            }
        ),
    );
    Ok((pack, hostable))
}

/// Read where the creation screen's content and the command surface are.
fn read_content_files(config: &Config) -> Result<ContentFiles, String> {
    // The creation screen's own content: the bags and items a client is given, and what a new
    // character wears per campaign and profession. Reference: the same `SetInitialWorldSettings`
    // pass that loads a world's maps; a creation screen's equipment is content like any other.
    let inventory = read_string(
        config,
        "InventoryFile",
        "config/World/Vanilla/creation/inventory.json",
    )?;
    let equipment = read_string(
        config,
        "EquipmentFile",
        "config/World/Vanilla/creation/equipment.json",
    )?;
    // Which slot each kind of item is worn in. Content, like the two above: the number is the
    // client's own, and a wrong one draws a character without the piece it holds.
    let worn_slots = read_string(
        config,
        "EquippedSlotsFile",
        "config/World/Vanilla/creation/worn_slots.json",
    )?;
    // The chat command surface: the names a player may type and the operation each one runs. Content,
    // not a setting - the file is where the surface lives, and `CommandsFile` says where that file is
    // (`docs/decisions/0011-the-command-surface-is-content.md`).
    let commands = read_string(config, "CommandsFile", "config/Server/commands.json")?;
    Ok(ContentFiles {
        inventory,
        equipment,
        worn_slots,
        commands,
    })
}

/// Register the operations this world answers, then read the command rows that name them.
///
/// The operations come before the rows: a row names one of them, and a name the registry does not
/// have is refused at start-up (`docs/port/14-operator-surfaces.md`). The chat operations live in
/// the game library and are registered here beside the world's own.
fn load_operations_and_commands(
    files: &ContentFiles,
    log: &Log,
    world: &mut game::world::World,
) -> Result<Arc<game::chat::operations::Registry>, String> {
    let mut operations = game::chat::operations::Registry::new();
    game::chat::operations::register_chat(&mut operations)?;
    operations.register(
        game::chat::operations::Registration {
            name: "world.status",
            help: "what this world is running",
            arguments: &[],
        },
        game::chat::operations::world_status,
    )?;
    operations.register(
        game::chat::operations::Registration {
            name: "ListTargets",
            help: "the alias table this world answers",
            arguments: &[],
        },
        game::chat::operations::list_targets,
    )?;
    operations.register(
        game::chat::operations::Registration {
            name: "world.operations",
            help: "the operations this world registers, one a line",
            arguments: &[],
        },
        command_line::list_operations,
    )?;
    operations.register(
        game::chat::operations::Registration {
            name: "world.exit",
            help: "stop this world; the process exits 0",
            arguments: &[],
        },
        command_line::world_exit,
    )?;
    operations.register(
        game::chat::operations::Registration {
            name: "world.restart",
            help: "stop this world and exit 2, so a supervisor starts it again",
            arguments: &[],
        },
        command_line::world_restart,
    )?;
    let operations = Arc::new(operations);
    world.set_operations(Arc::clone(&operations));
    world.load_commands(&files.commands, log)?;
    Ok(operations)
}

/// Start the server-side wire capture, when `CaptureFile` names a file.
///
/// The capture (`game/Server/Capture.rs`) is every game message, both directions, as plaintext
/// JSONL. Empty leaves it off, which is the default; a path is a setting, so a diagnostic run names
/// its own file (`docs/port/04-configuration.md`).
fn start_wire_capture(config: &Config, log: &Log) -> Result<(), String> {
    let capture_file = read_string(config, "CaptureFile", "")?;
    if !capture_file.is_empty() {
        game::server::capture::install(&capture_file)?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!("Capturing the wire to {capture_file} (layer wire, two-byte headers)."),
        );
    }
    Ok(())
}

/// Read the world's own numbers and give them to the world.
///
/// Every threshold an instance, a district or a spawn uses, read from this program's file with the
/// game's value as the default (R53, and docs/game-rules.md for each row).
fn load_world_settings(
    config: &Config,
    log: &Log,
    world: &mut game::world::World,
) -> Result<(), String> {
    let (world_settings, notes) = game::world::world_config::WorldConfig::read(config)?;
    for note in notes {
        log.out(LogLevel::Info, "server.worldserver", note);
    }
    world.set_settings(world_settings);
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "World settings: a district holds {} player(s), a party is capped at {}.",
            world.settings().district_allotment,
            world.settings().party_size_cap
        ),
    );
    Ok(())
}

/// Read the path-graph cache `PathGraphs` names and give it to the world.
///
/// The path graphs are extracted data. ASURA's standard startup at
/// `crates/asura-worldserver/src/main.rs:366-391` may fall back to the client archive; Kryta's
/// serving process is detached from that archive and reads only the shipped cache. A cache may be
/// partial: maps it does not contain remain `NoGraph`.
///
/// A map made outside the game is not in the cache: its graph is read from the finished map file
/// its pack folder carries (`docs/port/24-custom-maps.md`).
fn load_graphs(
    config: &Config,
    log: &Log,
    hostable: &[game::world::MapEntry],
    pack: &game::maps::pack::Pack,
    world: &mut game::world::World,
) -> Result<(), String> {
    let graph_dump = read_string(config, "PathGraphs", "data/path-graphs.bin")?;
    let graphs = load_path_graphs(&graph_dump, hostable, log)?;
    load_map_files(pack.map_files(), hostable, &graphs, log)?;
    world.set_graphs(graphs);
    Ok(())
}

/// Read the map files the pack carries: check each, and take the walk graph from each finished one.
///
/// A file that is not a map stops the program, as any broken file of the pack does: a client sent
/// to a map whose file it cannot read ends by its own assertion
/// (`docs/port/24-custom-maps.md`, "What phase 3 has"). A map with a stripped file and no finished
/// one is served and has no graph until a client has compiled it and the result is put beside it.
fn load_map_files(
    files: &[game::maps::pack::MapFiles],
    hostable: &[game::world::MapEntry],
    graphs: &game::terrain::pathfind::PathGraphs,
    log: &Log,
) -> Result<(), String> {
    for files in files {
        if let Some(path) = &files.stripped {
            game::maps::pack::read_map_file(path, files.map_id)?;
        }
        match &files.finished {
            Some(path) => {
                let bytes = std::fs::read(path)
                    .map_err(|error| format!("cannot read the map file {}: {error}", path.display()))?;
                graphs.insert_file(files.file_id, &bytes).map_err(|error| {
                    format!(
                        "{} holds no walk graph for map {}: {error}",
                        path.display(),
                        files.map_id
                    )
                })?;
                log.out(
                    LogLevel::Info,
                    "server.worldserver",
                    format!(
                        "Map {}: walk graph read from its map file {}.",
                        files.map_id,
                        path.display()
                    ),
                );
            }
            None => log.out(
                LogLevel::Info,
                "server.worldserver",
                format!(
                    "Map {} has a stripped map file and no finished one ({}): it has no walk graph until a client has compiled it and that file is in its folder.",
                    files.map_id,
                    game::maps::pack::finished_file_name(files.file_id)
                ),
            ),
        }
    }
    graphs.bind(hostable);
    Ok(())
}

/// Read the creation screen's content into the world, and say what was read.
fn load_creation(
    files: &ContentFiles,
    log: &Log,
    world: &mut game::world::World,
) -> Result<(), String> {
    let inventory_file = &files.inventory;
    let equipment_file = &files.equipment;
    let worn_slots_file = &files.worn_slots;
    let (bags, items, sets) =
        world.load_creation(
            inventory_file,
            equipment_file,
            Some(std::path::Path::new(worn_slots_file)),
            log,
        )?;
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "Creation screen: {bags} bag(s) and {items} item(s) from {inventory_file}, {sets} profession set(s) from {equipment_file}, worn slots from {worn_slots_file}."
        ),
    );
    Ok(())
}

/// Give the world its maps' doorways, which is what a step is judged against.
///
/// `game/Maps/Portals.rs`, `ASURA/crates/asura-world/src/portals.rs`. They come from the pack's
/// `portals.json` (`config/World/Vanilla/maps/**`), read once like the maps themselves; a map with no
/// `portals.json` has no doorway, and every step in it crosses nothing.
fn load_doorways(pack: &game::maps::pack::Pack, log: &Log, world: &mut game::world::World) {
    match pack.arches() {
        Ok(arches) => {
            let portals = game::maps::Portals::new(
                arches,
                world.settings().point_box_half_extent,
            );
            log.out(
                LogLevel::Info,
                "server.worldserver",
                format!(
                    "Doorways: {} arch(es) on {} map(s); a point doorway's box has a half side of {:.0} (World.PointBoxHalfExtent).",
                    portals.arches(),
                    portals.maps_with_doorways(),
                    portals.extent()
                ),
            );
            world.set_portals(portals);
        }
        Err(error) => log.out(
            LogLevel::Warn,
            "server.worldserver",
            format!(
                "No doorway rows: {error}. Every step crosses nothing until the map_arch table is readable."
            ),
        ),
    }
}

/// Build the instance store on the world database and dispose what an earlier run left.
///
/// The instance table (`data/sql/base/db_world/0003-instances.sql`): the copies a session opens
/// are rows, and a fresh world disposes what an earlier run left - nobody is inside it.
fn open_instances(
    world_pool: &Arc<Mutex<DatabaseWorkerPool>>,
    log: &Log,
    world: &mut game::world::World,
) {
    let store = Arc::new(game::districts::DatabaseInstances::new(
        Arc::clone(world_pool),
        log.clone(),
    ));
    match store.dispose_all() {
        Ok(count) if count > 0 => log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Disposed {count} instance row(s) left by an earlier run; a fresh world has nobody inside it."
            ),
        ),
        Ok(_) => {}
        Err(error) => log.out(
            LogLevel::Error,
            "server.worldserver",
            format!("Cannot read the instance table: {error}"),
        ),
    }
    world.set_instances(store, log.clone());
}

/// Give the world its item definitions, from the creation content already read.
///
/// The item definitions are the creation content's own: the pack says what an item is, and a load
/// resolves a character's held items against these. No table, no seed, no read-back - a start-up
/// writes nothing (AGENTS.md rule 7).
fn load_item_definitions(log: &Log, world: &mut game::world::World) {
    let templates = game::data_stores::definitions_as_templates(world.creation().definitions());
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "Item definitions: {} read from the world pack; nothing is seeded.",
            templates.len()
        ),
    );
    world.set_templates(templates);
}

/// Read what the listener and the world loop run by.
///
/// The listener is on the address and port its own configuration names, with the number of network
/// threads `Network.Threads` asks for.
fn read_listener_settings(config: &Config) -> Result<ListenerSettings, String> {
    let bind = read_string(config, "BindIP", "0.0.0.0")?;
    let port = read::<u16>(config, "WorldServerPort", 8085)?;
    let network_threads = read::<usize>(config, "Network.Threads", 1)?;
    let trace = read_bool(config, "Network.Trace", false)?;
    let status_interval = read::<u64>(config, "StatusInterval", 30)?;
    let status_interval = status_interval.max(1);
    // The world loop's floor: how long may pass before the loop does not bother to update. The
    // reference's own `MinWorldUpdateTime` (`worldserver.conf`, 1 ms) - `WorldUpdateLoop` sleeps the
    // remainder rather than updating twice inside it (`Main.cpp:586-616`).
    let min_update_ms = read::<u64>(config, "MinWorldUpdateTime", 1)?;
    let min_update = std::time::Duration::from_millis(min_update_ms.max(1));
    Ok(ListenerSettings {
        bind,
        port,
        network_threads,
        trace,
        status_interval,
        min_update,
    })
}

/// Stage 8: start the world listener, answering the port it bound.
///
/// Reference: `sWorldSocketMgr.StartWorldNetwork(*ioContext, worldListener, worldPort,
/// networkThreads)`.
fn start_listener(
    world: &Arc<game::world::World>,
    databases: &Databases,
    log: &Log,
    listener: &ListenerSettings,
) -> Result<(game::server::WorldSocketMgr, u16), String> {
    let sockets =
        game::server::WorldSocketMgr::start(
            Arc::clone(world),
            Some(Arc::clone(&databases.character)),
            Some(Arc::clone(&databases.login)),
            log.clone(),
            &listener.bind,
            listener.port,
            listener.network_threads,
            listener.trace,
        )
            .map_err(|error| format!("the world listener could not start: {error}"))?;
    let bound = sockets.port().unwrap_or(listener.port);
    Ok((sockets, bound))
}

/// Say the program is up and what it is up on.
///
/// The stack's handshake: printed only when a supervisor asked for it (`KRYTA_STACK`), because
/// nothing this program says belongs on a screen the operator did not ask to fill. The record is
/// what the appenders are configured to take, and these same words go there
/// (`docs/port/03-tracing.md`).
fn announce_ready(log: &Log, listener: &ListenerSettings, bound: u16, realm: u32) {
    let bind = &listener.bind;
    let status_interval = listener.status_interval;
    let ready = format!(
        "READY: listening on {bind}:{bound} as world {realm}; a client's gameplay connection is copied here by the login program; status reported every {status_interval} s."
    );
    if std::env::var_os("KRYTA_STACK").is_some() {
        // Only ever read by the supervisor that set the variable, and never shown to anybody.
        println!("{ready}");
    }
    log.out(LogLevel::Info, "server.worldserver", ready);
}

/// Start the console and, when `Control.Enable` asks for it, the local control endpoint; answer the
/// endpoint and the queue the world loop drains.
///
/// Reference: Main.cpp:400-408 and CliRunnable.cpp:231: submit to the world update. The control
/// endpoint stands where the reference's `ACSoapThread` does: JSON on loopback instead of SOAP
/// (`RemoteAccess/mod.rs`, `ACSoap.cpp:81-140`).
fn start_console_and_control(
    config: &Config,
    log: &Log,
    world: &game::world::World,
    operations: &Arc<game::chat::operations::Registry>,
) -> Result<(Option<remote_access::Server>, Receiver<command_line::Request>), String> {
    let console_enabled = read_bool(config, "Console.Enable", true)?;
    let queue_capacity = read::<usize>(config, "Console.QueueCapacity", 64)?;
    if queue_capacity == 0 { return Err("Console.QueueCapacity must be greater than zero".into()); }
    let (command_sender, command_receiver) = std::sync::mpsc::sync_channel(queue_capacity);
    let console_table = Arc::new(world.commands().clone());
    let console = console_enabled
        && command_line::start(
            command_sender.clone(),
            Arc::clone(&console_table),
            Arc::clone(operations),
        );
    let (control_enabled, _) = config.value_bool("Control.Enable", false).map_err(|error| error.to_string())?;
    let control = if control_enabled {
        let (port, _) = config.value::<u16>("Control.Port", 7878).map_err(|error| error.to_string())?;
        let (path, _) = config.value_string("Control.CredentialFile", "").map_err(|error| error.to_string())?;
        let (timeout, _) = config.value::<u64>("Control.TimeoutSeconds", 5).map_err(|error| error.to_string())?;
        if path.is_empty() || timeout == 0 { return Err("Control requires a credential file and positive timeout".into()); }
        let server = remote_access::Server::start(port, std::path::Path::new(&path), std::time::Duration::from_secs(timeout), command_sender.clone(), Arc::clone(operations))?;
        log.out(LogLevel::Info, "server.worldserver", format!("Local world control listening on 127.0.0.1:{port}."));
        Some(server)
    } else { None };
    drop(command_sender);
    log.out(LogLevel::Info, "server.worldserver", if console {
        "World console enabled."
    } else {
        "World console unavailable or disabled."
    });
    Ok((control, command_receiver))
}

/// Start the world's update loop on its own thread.
///
/// The world's loop: `WorldUpdateLoop` (`Main.cpp:584-620`), which is one thread in the reference
/// and is one here. It measures how long it has been, sleeps the remainder of
/// `MinWorldUpdateTime` when there is one, and calls `sWorld->Update(diff)` - the world decides
/// inside which of its own slower things are due (the copies' sweep) and tells the scripts every
/// tick. What is left that is the program's own and not the world's is the status line: it is this
/// tree's, and it rides the same loop on `StatusInterval` seconds.
#[allow(clippy::too_many_arguments)]
fn start_world_loop(
    log: &Log,
    world: &Arc<game::world::World>,
    sockets: &game::server::WorldSocketMgr,
    operations: Arc<game::chat::operations::Registry>,
    command_receiver: Receiver<command_line::Request>,
    listener: &ListenerSettings,
    bound: u16,
    realm: u32,
) -> Result<Background, String> {
    let bind = listener.bind.clone();
    let status_interval = listener.status_interval;
    let min_update = listener.min_update;
    let stop_update = Arc::new(AtomicBool::new(false));
    let accepted = sockets.accepted_handle();
    let live = sockets.live_handle();
    let world_loop = {
        let log = log.clone();
        let update_world = Arc::clone(world);
        let stop_update = Arc::clone(&stop_update);
        let started = std::time::Instant::now();
        let status_period = std::time::Duration::from_secs(status_interval);
        std::thread::Builder::new()
            .name("world-update".to_string())
            .spawn(move || {
                let mut previous = std::time::Instant::now();
                let mut next_status = previous + status_period;
                while !stop_update.load(std::sync::atomic::Ordering::Relaxed) {
                    let now = std::time::Instant::now();
                    let diff = now.duration_since(previous);
                    if diff < min_update {
                        std::thread::sleep(min_update - diff);
                        continue;
                    }
                    previous = now;
                    command_line::process(&command_receiver, &operations, &update_world);
                    // A console operation may have asked the world to stop (`World::StopNow`); the
                    // program's own loop is what notices, and the exit code it answers with is the
                    // world's (`Main.cpp:434-439`).
                    if update_world.stop_requested() {
                        stop_update.store(true, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    for (map_id, district) in update_world.update(diff) {
                        let hold = update_world.settings().explorable_disconnect_hold;
                        log.out(
                            LogLevel::Info,
                            "server.worldserver",
                            format!(
                                "Instance on map {map_id}, district {district}, was empty for {hold} s and is disposed (R37, R40)."
                            ),
                        );
                    }
                    if now >= next_status {
                        next_status = now + status_period;
                        let instances = match update_world.districts().lock() {
                            Ok(districts) => districts.open(),
                            Err(_) => Vec::new(),
                        };
                        let open = if instances.is_empty() {
                            "none".to_string()
                        } else {
                            instances
                                .iter()
                                .map(|(map_id, district, members)| {
                                    format!("map {map_id} district {district} ({members} inside)")
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        log.out(
                            LogLevel::Info,
                            "server.worldserver",
                            format!(
                                "status: up {} s, listening on {bind}:{bound} as world {realm}, {} connection(s) accepted, {} live, {} map(s) hosted, {} instance(s) open: {open}.",
                                started.elapsed().as_secs(),
                                accepted.load(std::sync::atomic::Ordering::Relaxed),
                                live.load(std::sync::atomic::Ordering::Relaxed),
                                update_world.maps().len(),
                                instances.len()
                            ),
                        );
                    }
                }
            })
            .map_err(|error| format!("the world loop could not start: {error}"))?
    };
    Ok(Background {
        stop: stop_update,
        thread: world_loop,
    })
}

/// Close what the program opened, in the reference's order.
///
/// `sWorldSessionMgr->KickAll` and the `UpdateSessions(1)` pass that unloads what it kicked, then
/// `sWorldSocketMgr::StopNetwork` (`Main.cpp:370-374`) - `sockets.stop` does all three here - then
/// `sScriptMgr->OnShutdown` once the network is gone (`:426`). `sMapMgr->UnloadAll` and
/// `OnAfterUnloadAllMaps` have no work to do in this tree yet: the copies' rows are disposed by a
/// fresh world at its start (`dispose_all`), which is what an unload at the end would have achieved.
fn shut_down(
    log: &Log,
    world: &game::world::World,
    control: Option<remote_access::Server>,
    world_loop: Background,
    sockets: game::server::WorldSocketMgr,
    databases: &Databases,
) {
    if let Some(control) = control { control.stop(); }
    world_loop.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = world_loop.thread.join();
    log.out(
        LogLevel::Info,
        "server.worldserver",
        "Stopping: the listener is closing.",
    );
    let accepted = sockets.accepted_count();
    sockets.stop();
    // And the world's own shutdown, raised once the network is gone, which is where the
    // reference raises it (`sScriptMgr->OnShutdown()`, `Main.cpp:426`, after the pool is gone).
    world.on_shutdown();
    match databases.character.lock() {
        Ok(mut pool) => pool.close(),
        Err(_) => log.out(
            LogLevel::Error,
            "server.worldserver",
            "the character database pool was poisoned; it closes when the process ends",
        ),
    }
    match databases.login.lock() {
        Ok(mut pool) => pool.close(),
        Err(_) => log.out(
            LogLevel::Error,
            "server.worldserver",
            "the login database pool was poisoned; it closes when the process ends",
        ),
    }
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!("Stopped. {accepted} connection(s) were accepted while it ran."),
    );
    // The capture's last records must reach the disk before the process ends: it is evidence, and a
    // diagnostic session usually ends with a kill (`game/Server/Capture.rs`, FLUSH_EVERY).
    game::server::capture::stop();
    log.shutdown();
}

/// Block until the operator stops the program, or the world is asked to stop.
///
/// Reference: `ioContext->run()` returning when the signal handler stopped it. A Ctrl-C is that
/// signal here, the same as the auth program's; a console operation asks through the world instead
/// (`World::StopNow`, and `Main.cpp:434-439` for the exit code that follows).
fn wait_for_shutdown(world: &game::world::World) {
    let (sender, receiver) = std::sync::mpsc::channel::<()>();
    if ctrlc::set_handler(move || {
        let _ = sender.send(());
    })
    .is_err()
    {
        return;
    }
    while !world.stop_requested() {
        match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
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

/// Load the extracted cache before opening the listener.
///
/// The cache is not required to cover every map. The archive is intentionally absent from this call
/// path: a missing graph belongs to that map, not to server startup.
fn load_path_graphs(
    dump: &str,
    maps: &[game::world::MapEntry],
    log: &Log,
) -> Result<game::terrain::pathfind::PathGraphs, String> {
    if dump.trim().is_empty() {
        return Err("PathGraphs must name the extracted path-graph cache".to_string());
    }
    let graphs = game::terrain::pathfind::PathGraphs::from_dump(dump)
        .map_err(|error| format!("cannot read PathGraphs {dump}: {error}"))?;
    let bound = graphs.bind(maps);
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "Path graphs from {dump}: {bound} of {} hosted map(s) bound; other maps have no graph.",
            maps.len()
        ),
    );
    Ok(graphs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        Options::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn vale(file_id: u32) -> game::world::MapEntry {
        game::world::MapEntry {
            id: 148,
            kind: Some(0),
            file_id,
            name: "a vale".to_string(),
            instance_value: None,
        }
    }

    /// A pack's map files at start-up: a stripped file alone is served and has no graph, and a file
    /// that is not a map stops the program, whichever of the two it is.
    #[test]
    fn a_map_file_that_is_not_a_map_stops_the_start() {
        use game::maps::pack::MapFiles;
        let folder = std::env::temp_dir().join("kryta-worldserver-test-map-files");
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(&folder).expect("the folder");
        let good = folder.join("0005F100.stripped.ffna");
        std::fs::write(&good, b"ffna\x03").expect("a map container with no record");
        let bad = folder.join("0005F100.ffna");
        std::fs::write(&bad, b"not a map").expect("a file that is not a map");
        let maps = [vale(0x5F100)];
        let log = Log::new();
        let files = |stripped: Option<&std::path::PathBuf>, finished: Option<&std::path::PathBuf>| {
            [MapFiles {
                map_id: 148,
                file_id: 0x5F100,
                stripped: stripped.cloned(),
                finished: finished.cloned(),
            }]
        };

        let graphs = game::terrain::pathfind::PathGraphs::empty();
        load_map_files(&files(Some(&good), None), &maps, &graphs, &log).expect("a stripped file alone");
        assert!(graphs.maps().is_empty(), "a map nobody compiled has no graph");

        let error = load_map_files(&files(Some(&bad), None), &maps, &graphs, &log)
            .expect_err("a stripped file that is not a map");
        assert!(error.contains("is not a map file"), "{error}");
        let error = load_map_files(&files(Some(&good), Some(&bad)), &maps, &graphs, &log)
            .expect_err("a finished file that is not a map");
        assert!(error.contains("holds no walk graph"), "{error}");
        // A stripped file holds no path record of stage 2, so it is not a finished file either.
        let error = load_map_files(&files(None, Some(&good)), &maps, &graphs, &log)
            .expect_err("a stripped file under the finished name");
        assert!(error.contains("holds no walk graph"), "{error}");
        std::fs::remove_dir_all(&folder).ok();
    }

    /// With a finished file a client compiled (`KRYTA_FINISHED_MAP`, and its spawn point in
    /// `KRYTA_FINISHED_MAP_SPAWN` as `x,y`): the graph is bound and a walk from the spawn point is
    /// answered from it. The run of 2026-11-27 is `docs/port/24-custom-maps.md`.
    #[test]
    fn a_finished_map_file_gives_the_map_its_walk_graph() {
        let (Ok(path), Ok(spawn)) =
            (std::env::var("KRYTA_FINISHED_MAP"), std::env::var("KRYTA_FINISHED_MAP_SPAWN"))
        else {
            eprintln!("skipped: no KRYTA_FINISHED_MAP and KRYTA_FINISHED_MAP_SPAWN");
            return;
        };
        let (x, y) = spawn.split_once(',').expect("x,y");
        let (x, y): (f32, f32) = (x.trim().parse().expect("x"), y.trim().parse().expect("y"));
        let files = [game::maps::pack::MapFiles {
            map_id: 148,
            file_id: 0x5F100,
            stripped: None,
            finished: Some(path.into()),
        }];
        let graphs = game::terrain::pathfind::PathGraphs::empty();
        load_map_files(&files, &[vale(0x5F100)], &graphs, &Log::new()).expect("the file imports");
        assert_eq!(graphs.maps(), vec![148]);
        let from = game::terrain::Pos { x, y, plane: 0 };
        let to = game::terrain::Pos { x: x + 100.0, y, plane: 0 };
        let walk = graphs.walk(148, from, to);
        assert!(
            !matches!(walk, game::terrain::Walk::NoGraph),
            "the map has a graph: {walk:?}"
        );
        eprintln!("walk from the spawn point: {walk:?}");
    }

    #[test]
    fn the_config_file_defaults_to_the_reference_name() {
        assert_eq!(parse(&[]).config, PathBuf::from("worldserver.json"));
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
