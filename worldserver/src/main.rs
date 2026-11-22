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
use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
use database::updater::{UpdateState, collect_files};
use std::path::PathBuf;
use std::process::ExitCode;

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
        print_usage();
        return Ok(game::world::ExitCode::Normal);
    }
    if options.version {
        println!("Kryta worldserver {}", env!("CARGO_PKG_VERSION"));
        return Ok(game::world::ExitCode::Normal);
    }

    // Stage 2. Reference: `sConfigMgr->Configure(...)` then `LoadAppConfigs()`.
    //
    // The severity policy can be overridden on the command line, and a key it does not know is
    // refused rather than ignored - the reference reports it the same way.
    // A repair an operator asks for in one flag, rather than a dead stack: the schema is dropped and
    // rebuilt from the files (`docs/port/04-configuration.md`).
    let reset_schema = args.iter().any(|argument| argument == "--reset-schema");
    // `--recheck-schema`: a schema file changed after this database recorded it. The operator says so in
    // one command, and the recorded hash is re-taken from the file instead of the stack staying down.
    let recheck_schema = args.iter().any(|argument| argument == "--recheck-schema");

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

    // Stage 7. Reference: `sScriptMgr->SetScriptLoader(AddScripts)`, `SetModulesLoader`, then
    // `sScriptMgr->Initialize()` with `Initializing Scripts...` printed before it. The reference
    // starts its io-context threads (its `ThreadPool`) before this; that pool is this tree's socket
    // layer, which owns its runtimes and is created with the listener, so the thread stage lands
    // there and not as a stage of its own (`docs/port/01-structure.md`).
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

    // Stage 5. Reference: `StartDB()`, which opens the World and Character pools and runs the schema
    // updater on each - the stage the auth program has, twice.
    //
    // The world program opens three databases: the world's content, a character's sheets, and the
    // login database's `travel` record - the hand-off a login writes and this program claims, which
    // is why the two programs need not talk to each other (`docs/port/06-world-plan.md`, "The three
    // seams"). The login schema is applied here too, so a world started first still finds its table.
    let (sql_trace, notes) = config
        .value_bool("Database.Trace", false)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }

    config.mark_critical("LoginDatabaseInfo");
    let mut opened = Vec::new();
    let mut character_pool: Option<std::sync::Arc<std::sync::Mutex<DatabaseWorkerPool>>> = None;
    let mut world_pool: Option<std::sync::Arc<std::sync::Mutex<DatabaseWorkerPool>>> = None;
    let mut login_pool: Option<std::sync::Arc<std::sync::Mutex<DatabaseWorkerPool>>> = None;
    for (kind, label, schema) in [
        (DatabaseKind::Login, "Login", "data/sql/base/db_auth"),
        (DatabaseKind::World, "World", "data/sql/base/db_world"),
        (
            DatabaseKind::Character,
            "Character",
            "data/sql/base/db_characters",
        ),
    ] {
        let settings = read_settings(&config, kind)?;
        let mut pool = DatabaseWorkerPool::open_with_trace(kind, &settings, &log, sql_trace)?;

        // `--reset-schema`: the schema files changed after this database recorded them as applied, or an
        // operator wants the tables rebuilt from the files as they are. The world and login databases
        // hold state and no content, so dropping the schema there costs nothing that is not recreated;
        // the character database is refused, because it holds what players earned.
        // Only the world database is reset: it holds state and no content, and the files rebuild it.
        // The login database holds accounts and the character database holds what players earned, so
        // neither is dropped by a flag - a changed file there is `--recheck-schema`, which deletes
        // nothing (`docs/port/04-configuration.md`).
        if reset_schema && kind == DatabaseKind::World {
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
        if recheck_schema {
            for file in &files {
                let text = database::updater::read_sql_update(&file.path)
                    .map_err(|error| error.to_string())?;
                let hash = database::updater::apply::sha1_hex(&text);
                let name = file.name();
                pool.query_with(
                    "UPDATE \"updates\" SET \"hash\" = $1 WHERE \"name\" = $2",
                    &[&hash, &name],
                )
                .map_err(|error| {
                    format!("the {label} update record cannot be re-taken: {error}")
                })?;
                log.out(
                    LogLevel::Info,
                    "server.worldserver",
                    format!("{label} schema: re-recorded {name} as {hash}."),
                );
            }
        }
        let summary = pool.apply_updates(&files, &log)?;
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

        if kind == DatabaseKind::Character {
            // A creation confirm writes a character from a session thread, so the character pool is
            // shared with the listener the way the login program shares its own with the portal.
            character_pool = Some(std::sync::Arc::new(std::sync::Mutex::new(pool)));
        } else if kind == DatabaseKind::World {
            // The maps are rows, so the world pool is kept: the map catalogue is read from it below.
            world_pool = Some(std::sync::Arc::new(std::sync::Mutex::new(pool)));
        } else if kind == DatabaseKind::Login {
            // A connection claims its travel record from a session thread, so this pool is shared
            // with the listener as well.
            login_pool = Some(std::sync::Arc::new(std::sync::Mutex::new(pool)));
        } else {
            opened.push(pool);
        }
    }

    // Stage 8. Reference: `sWorld->SetInitialWorldSettings()` - everything the game is made of,
    // loaded before the listener opens. Here that is the map catalogue, and the log says how many
    // maps it holds; zero would be a statement, and the block below only serves what is listed.
    let (realm, notes) = config
        .value::<u32>("RealmID", 1)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }

    let mut world = game::world::World::new(realm);
    // The scripts the program registered at stage 7 are the world's: the manager is a game symbol in
    // the reference too (`sScriptMgr`, `src/server/game/Scripting/`), and the world is the object
    // that raises its moments.
    world.set_scripts(scripts);
    // The maps are the pack's (`config/World/Vanilla/maps/...`): what a world *is* is a file, and a row
    // is what a server writes (`docs/decisions/0014-the-world-content-is-a-pack.md`). Read once, held for
    // the life of the program, and the log says which source answered. The database is still opened for
    // the state it holds - instances, characters, items, travels - and no longer for content.
    let (world_pack, notes) = config
        .value_string("WorldPack", "config/World/Vanilla")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let pack = game::maps::pack::Pack::load(&world_pack, &log)?;
    let maps = world.load_maps(&pack, &format!("the map pack at {world_pack}"), &log)?;
    world.load_titles(std::path::Path::new(&world_pack).join("titles.json"), &log)?;
    // The experience curve: the pack's own file, read the way the titles are
    // (`game/Characters/Experience.rs`, ported from ASURA's `experience.json`).
    world.load_experience(
        std::path::Path::new(&world_pack).join(game::characters::experience::EXPERIENCE_FILE),
        &log,
    )?;
    // How many players a party may hold, per map: the pack's own file, read the way the titles are
    // (`game/Parties/mod.rs`, ported from ASURA's `party_sizes.json`).
    world.load_party_sizes(
        std::path::Path::new(&world_pack).join("party_sizes.json"),
        &log,
    )?;
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
    // The creation screen's own content: the bags and items a client is given, and what a new
    // character wears per campaign and profession. Reference: the same `SetInitialWorldSettings`
    // pass that loads a world's maps; a creation screen's equipment is content like any other.
    let (inventory_file, notes) = config
        .value_string(
            "InventoryFile",
            "config/World/Vanilla/creation/inventory.json",
        )
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (equipment_file, notes) = config
        .value_string(
            "EquipmentFile",
            "config/World/Vanilla/creation/equipment.json",
        )
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    // Which slot each kind of item is worn in. Content, like the two above: the number is the
    // client's own, and a wrong one draws a character without the piece it holds.
    let (worn_slots_file, notes) = config
        .value_string(
            "EquippedSlotsFile",
            "config/World/Vanilla/creation/worn_slots.json",
        )
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    // The guild settings: what a new guild starts with, the `0x0133` counter's start and step, and the
    // two placeholder prices (`game/Guilds/mod.rs`, ported from ASURA's `config/Server/guilds.json`).
    // `GuildsFile` says where the file is, as `CommandsFile` does for the command surface.
    let (guilds_file, notes) = config
        .value_string("GuildsFile", "config/Server/guilds.json")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    world.load_guilds(&guilds_file, &log)?;
    // The chat command surface: the names a player may type and the operation each one runs. Content,
    // not a setting - the file is where the surface lives, and `CommandsFile` says where that file is
    // (`docs/decisions/0011-the-command-surface-is-content.md`).
    let (commands_file, notes) = config
        .value_string("CommandsFile", "config/Server/commands.json")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    // The operations this world answers, before its rows are read: a row names one of them, and a name
    // the registry does not have is refused at start-up (`docs/port/14-operator-surfaces.md`). The chat
    // operations live in the game library and are registered here beside the world's own.
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
    let operations = std::sync::Arc::new(operations);
    world.set_operations(std::sync::Arc::clone(&operations));
    world.load_commands(&commands_file, &log)?;
    // The server-side wire capture (`game/Server/Capture.rs`): every game message, both directions, as
    // plaintext JSONL. Empty leaves it off, which is the default; a path is a setting, so a diagnostic
    // run names its own file (`docs/port/04-configuration.md`).
    let (capture_file, notes) = config
        .value_string("CaptureFile", "")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    if !capture_file.is_empty() {
        game::server::capture::install(&capture_file)?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!("Capturing the wire to {capture_file} (layer wire, two-byte headers)."),
        );
    }
    // The world's own numbers: every threshold an instance, a district or a spawn uses, read from this
    // program's file with the game's value as the default (R53, and docs/game-rules.md for each row).
    let (world_settings, notes) = game::world::world_config::WorldConfig::read(&config)?;
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

    // The path graphs are extracted data. ASURA's standard startup at
    // `crates/asura-worldserver/src/main.rs:366-391` may fall back to the client archive; Kryta's
    // serving process is detached from that archive and reads only the shipped cache. A cache may be
    // partial: maps it does not contain remain `NoGraph`, including future custom maps.
    let (graph_dump, notes) = config
        .value_string("PathGraphs", "data/path-graphs.bin")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let graphs = load_path_graphs(&graph_dump, &hostable, &log)?;
    world.set_graphs(graphs);

    let (bags, items, sets) = world.load_creation(
        &inventory_file,
        &equipment_file,
        Some(std::path::Path::new(&worn_slots_file)),
        &log,
    )?;
    log.out(
        LogLevel::Info,
        "server.worldserver",
        format!(
            "Creation screen: {bags} bag(s) and {items} item(s) from {inventory_file}, {sets} profession set(s) from {equipment_file}, worn slots from {worn_slots_file}."
        ),
    );

    // A map's doorways, which is what a step is judged against
    // (`game/Maps/Portals.rs`, `ASURA/crates/asura-world/src/portals.rs`). They come from the pack's
    // `portals.json` (`config/World/Vanilla/maps/**`), read once like the maps themselves; a map with no
    // `portals.json` has no doorway, and every step in it crosses nothing.
    {
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

    // The instance table (`data/sql/base/db_world/0003-instances.sql`): the copies a session opens
    // are rows, and a fresh world disposes what an earlier run left - nobody is inside it.
    if let Some(pool) = world_pool.as_ref() {
        let store = std::sync::Arc::new(game::districts::DatabaseInstances::new(
            std::sync::Arc::clone(pool),
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
    } else {
        log.out(
            LogLevel::Warn,
            "server.worldserver",
            "No world database is open, so instances are kept in memory only and no copy is recorded.",
        );
    }

    // The item definitions are the creation content's own: the pack says what an item is, and a load
    // resolves a character's held items against these. No table, no seed, no read-back - a start-up
    // writes nothing (AGENTS.md rule 7).
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
    let world = std::sync::Arc::new(world);

    // Stage 9. Reference: `sWorldSocketMgr.StartWorldNetwork(*ioContext, worldListener, worldPort,
    // networkThreads)` - the listener, on the address and port its own configuration names, with the
    // number of network threads `Network.Threads` asks for.
    let (bind, notes) = config
        .value_string("BindIP", "0.0.0.0")
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (port, notes) = config
        .value::<u16>("WorldServerPort", 8085)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (network_threads, notes) = config
        .value::<usize>("Network.Threads", 1)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (trace, notes) = config
        .value_bool("Network.Trace", false)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (status_interval, notes) = config
        .value::<u64>("StatusInterval", 30)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let status_interval = status_interval.max(1);
    // The world loop's floor: how long may pass before the loop does not bother to update. The
    // reference's own `MinWorldUpdateTime` (`worldserver.conf`, 1 ms) - `WorldUpdateLoop` sleeps the
    // remainder rather than updating twice inside it (`Main.cpp:586-616`).
    let (min_update_ms, notes) = config
        .value::<u64>("MinWorldUpdateTime", 1)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let min_update = std::time::Duration::from_millis(min_update_ms.max(1));

    let sockets = game::server::WorldSocketMgr::start(
        std::sync::Arc::clone(&world),
        character_pool.clone(),
        login_pool.clone(),
        log.clone(),
        &bind,
        port,
        network_threads,
        trace,
    )
    .map_err(|error| format!("the world listener could not start: {error}"))?;
    let bound = sockets.port().unwrap_or(port);

    // The stack's handshake: printed only when a supervisor asked for it (`KRYTA_STACK`), because
    // nothing this program says belongs on a screen the operator did not ask to fill. The record is
    // what the appenders are configured to take, and these same words go there
    // (`docs/port/03-tracing.md`).
    let ready = format!(
        "READY: listening on {bind}:{bound} as world {realm}; a client's gameplay connection is copied here by the login program; status reported every {status_interval} s."
    );
    if std::env::var_os("KRYTA_STACK").is_some() {
        // Only ever read by the supervisor that set the variable, and never shown to anybody.
        println!("{ready}");
    }
    log.out(LogLevel::Info, "server.worldserver", ready);
    // The world is up, so its scripts are told (`sScriptMgr->OnStartup()`, `Main.cpp:398`).
    world.on_startup();

    // Reference: Main.cpp:400-408 and CliRunnable.cpp:231: submit to the world update.
    let (console_enabled, notes) = config
        .value_bool("Console.Enable", true)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    let (queue_capacity, notes) = config
        .value::<usize>("Console.QueueCapacity", 64)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    if queue_capacity == 0 {
        return Err("Console.QueueCapacity must be greater than zero".into());
    }
    let (command_sender, command_receiver) = std::sync::mpsc::sync_channel(queue_capacity);
    let console_table = std::sync::Arc::new(world.commands().clone());
    let console = console_enabled
        && command_line::start(
            command_sender.clone(),
            std::sync::Arc::clone(&console_table),
            std::sync::Arc::clone(&operations),
        );
    let (control_enabled, _) = config
        .value_bool("Control.Enable", false)
        .map_err(|error| error.to_string())?;
    let control = if control_enabled {
        let (port, _) = config
            .value::<u16>("Control.Port", 7878)
            .map_err(|error| error.to_string())?;
        let (path, _) = config
            .value_string("Control.CredentialFile", "")
            .map_err(|error| error.to_string())?;
        let (timeout, _) = config
            .value::<u64>("Control.TimeoutSeconds", 5)
            .map_err(|error| error.to_string())?;
        if path.is_empty() || timeout == 0 {
            return Err("Control requires a credential file and positive timeout".into());
        }
        let server = remote_access::Server::start(
            port,
            std::path::Path::new(&path),
            std::time::Duration::from_secs(timeout),
            command_sender.clone(),
            std::sync::Arc::clone(&operations),
        )?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!("Local world control listening on 127.0.0.1:{port}."),
        );
        Some(server)
    } else {
        None
    };
    drop(command_sender);
    log.out(
        LogLevel::Info,
        "server.worldserver",
        if console {
            "World console enabled."
        } else {
            "World console unavailable or disabled."
        },
    );

    // ACSoapThread, the reference's remote endpoint, is not yet ported.
    // On shutdown, in the reference's order: `sWorldSessionMgr->KickAll` and the `UpdateSessions(1)`
    // pass that unloads what it kicked, then `sWorldSocketMgr::StopNetwork` (`Main.cpp:370-374`) -
    // `sockets.stop` does all three here - then `sScriptMgr->OnShutdown` once the network is gone
    // (`:426`). `sMapMgr->UnloadAll` and `OnAfterUnloadAllMaps` have no work to do in this tree yet:
    // the copies' rows are disposed by a fresh world at its start (`dispose_all`), which is what an
    // unload at the end would have achieved.

    // The world's loop: `WorldUpdateLoop` (`Main.cpp:584-620`), which is one thread in the reference
    // and is one here. It measures how long it has been, sleeps the remainder of
    // `MinWorldUpdateTime` when there is one, and calls `sWorld->Update(diff)` - the world decides
    // inside which of its own slower things are due (the copies' sweep) and tells the scripts every
    // tick. What is left that is the program's own and not the world's is the status line: it is this
    // tree's, and it rides the same loop on `StatusInterval` seconds.
    let stop_update = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let accepted = sockets.accepted_handle();
    let live = sockets.live_handle();
    let world_loop = {
        let log = log.clone();
        let update_world = std::sync::Arc::clone(&world);
        let stop_update = std::sync::Arc::clone(&stop_update);
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

    wait_for_shutdown(&world);
    let exit = world.exit_code();
    if let Some(control) = control {
        control.stop();
    }
    stop_update.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = world_loop.join();
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
    for pool in &mut opened {
        pool.close();
    }
    if let Some(pool) = character_pool {
        match pool.lock() {
            Ok(mut pool) => pool.close(),
            Err(_) => log.out(
                LogLevel::Error,
                "server.worldserver",
                "the character database pool was poisoned; it closes when the process ends",
            ),
        }
    }
    if let Some(pool) = login_pool {
        match pool.lock() {
            Ok(mut pool) => pool.close(),
            Err(_) => log.out(
                LogLevel::Error,
                "server.worldserver",
                "the login database pool was poisoned; it closes when the process ends",
            ),
        }
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
    Ok(exit)
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
