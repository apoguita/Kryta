//! Start-up: read `manage.json`, open what it names, and serve.
//!
//! The settings are read once, here, and the service stops if the login database does not answer
//! (`AGENTS.md`, rule 7). Every setting and its meaning is in `manage.json.dist`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::configuration::{Config, Policy};
use common::logging::{Log, LogLevel};
use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
use shared::network::http::{HttpError, Response, read_request, write_response};
use shared::network::socket::SessionIo;
use tokio::net::TcpListener;

use crate::api::Guard;
use crate::clients::{Control, Supervisor, supervisor_port};
use crate::state::{DatabaseSchema, Manage};

type Pool = Arc<Mutex<DatabaseWorkerPool>>;

/// Run the management service or its compatible legacy executable.
pub fn main_entry() -> std::process::ExitCode {
    let mut config_path = std::path::PathBuf::from("tools/manage/manage.json");
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-c" | "--config" => match arguments.next() {
                Some(path) => config_path = std::path::PathBuf::from(path),
                None => {
                    eprintln!("-c needs a path");
                    return std::process::ExitCode::FAILURE;
                }
            },
            // The panel has one address, the `Port` of `manage.json`, however it is started: on its
            // own or by the stack. A port given on the command line used to move it for one run,
            // and a panel that answers on a different port depending on how it was started is
            // confusing (owner, 2026-11-24).
            other => {
                eprintln!("unknown argument: {other} (the port is `Port` in the configuration file)");
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    match run(&config_path) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kryta-manage: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// A text setting.
fn text(config: &Config, key: &str, default: &str) -> Result<String, String> {
    config
        .value_string(key, default)
        .map(|(value, _)| value)
        .map_err(|error| error.to_string())
}

/// A text setting that is `None` when left empty.
fn optional(config: &Config, key: &str, default: &str) -> Result<Option<String>, String> {
    Ok(Some(text(config, key, default)?).filter(|value| !value.is_empty()))
}

fn run(config_path: &std::path::Path) -> Result<(), String> {
    let mut config = Config::load_json(config_path, false, Policy::default())
        .map_err(|error| error.to_string())?;
    let log = Log::new();
    log.initialize(&config).map_err(|error| error.to_string())?;
    log.out(
        LogLevel::Info,
        "manage",
        format!("kryta-manage reading {}", config_path.display()),
    );

    let bind = text(&config, "Bind", "127.0.0.1")?;
    let (port, _) = config
        .value::<u16>("Port", 6602)
        .map_err(|error| error.to_string())?;
    let guard = Guard {
        token: text(&config, "Token", "")?,
        port,
        loopback: Guard::is_loopback(&bind),
    };
    if !guard.loopback && guard.token.is_empty() {
        log.out(
            LogLevel::Warn,
            "manage",
            format!("Bind is {bind}, beyond this machine, and Token is empty: anyone who reaches the port can use the panel."),
        );
    }

    let manage = Arc::new(open(&mut config, guard, &log)?);
    serve(manage, &bind, port, &log)
}

/// Open everything `manage.json` names.
fn open(config: &mut Config, guard: Guard, log: &Log) -> Result<Manage, String> {
    config.mark_critical("LoginDatabaseInfo");
    let settings = read_settings(config, DatabaseKind::Login)?;
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, log)?;
    pool.ping()
        .map_err(|error| format!("the login database does not answer: {error}"))?;

    // The world list is the login's own reader, merged with the operator's file: the same rows the
    // auth program sends a client to, not a second reading of them.
    let mut worlds = shared::worlds::WorldList::new();
    let loaded = worlds.load(pool.connection_mut(), log)?;
    log.out(
        LogLevel::Info,
        "manage",
        format!("World list: {loaded} row(s) in the realmlist table."),
    );
    let worlds_file = text(config, "WorldsFile", "data/realms.json")?;
    if !worlds_file.is_empty() {
        worlds.merge_file(&worlds_file, log)?;
    }
    let login = Arc::new(Mutex::new(pool));
    let character_pool = optional_pool(config, DatabaseKind::Character, "CharacterDatabaseInfo", log)?;
    let world_pool = optional_pool(config, DatabaseKind::World, "WorldDatabaseInfo", log)?;

    Ok(Manage {
        accounts: Arc::new(game::accounts::DatabaseAccounts::new(
            Arc::clone(&login),
            log.clone(),
        )),
        characters: character_pool.as_ref().map(|pool| {
            Arc::new(game::characters::DatabaseCharacters::new(
                Arc::clone(pool),
                log.clone(),
            ))
        }),
        instances: world_pool.as_ref().map(|pool| {
            Arc::new(game::districts::DatabaseInstances::new(
                Arc::clone(pool),
                log.clone(),
            ))
        }),
        worlds: Arc::new(worlds),
        schemas: schemas(login, character_pool, world_pool),
        // Relative to where the tool is started, which is the repository root.
        world_config: optional(config, "WorldConfigFile", "worldserver/worldserver.json")?,
        auth_config: optional(config, "AuthConfigFile", "authserver/authserver.json")?,
        path_graphs_file: text(config, "PathGraphsFile", "data/path-graphs.bin")?,
        map_archive_file: text(config, "MapArchiveFile", "")?,
        control: control(config)?,
        supervisor: supervisor(config, log)?,
        guard,
        log: log.clone(),
    })
}

/// A database that may be left out: empty info leaves its areas switched off rather than failing
/// every request, and one that is named but does not open is reported and left off.
fn optional_pool(
    config: &Config,
    kind: DatabaseKind,
    info_key: &str,
    log: &Log,
) -> Result<Option<Pool>, String> {
    if text(config, info_key, "")?.is_empty() {
        return Ok(None);
    }
    let settings = read_settings(config, kind)?;
    match DatabaseWorkerPool::open(kind, &settings, log) {
        Ok(mut pool) => {
            pool.ping()
                .map_err(|error| format!("{info_key}: the database does not answer: {error}"))?;
            Ok(Some(Arc::new(Mutex::new(pool))))
        }
        Err(error) => {
            log.out(
                LogLevel::Warn,
                "manage",
                format!("{info_key}: the database is off: {error}"),
            );
            Ok(None)
        }
    }
}

/// Each database's own record of what it has applied is compared with these folders.
fn schemas(login: Pool, characters: Option<Pool>, world: Option<Pool>) -> Vec<DatabaseSchema> {
    let mut schemas = vec![DatabaseSchema {
        label: "login",
        pool: login,
        dir: "data/sql/base/db_auth",
    }];
    if let Some(pool) = characters {
        schemas.push(DatabaseSchema {
            label: "characters",
            pool,
            dir: "data/sql/base/db_characters",
        });
    }
    if let Some(pool) = world {
        schemas.push(DatabaseSchema {
            label: "world",
            pool,
            dir: "data/sql/base/db_world",
        });
    }
    schemas
}

/// The world's control endpoint, when a port and a credential file are both named.
fn control(config: &Config) -> Result<Option<Control>, String> {
    let (port, _) = config
        .value::<u16>("ControlPort", 0)
        .map_err(|error| error.to_string())?;
    let credential_file = text(config, "ControlCredentialFile", "")?;
    let (timeout, _) = config
        .value::<u64>("ControlTimeoutSeconds", 5)
        .map_err(|error| error.to_string())?;
    if port == 0 || credential_file.is_empty() {
        return Ok(None);
    }
    let credential = std::fs::read_to_string(&credential_file)
        .map_err(|error| format!("cannot read ControlCredentialFile {credential_file}: {error}"))?
        .trim()
        .to_owned();
    if credential.len() < 32 {
        return Err(format!(
            "ControlCredentialFile {credential_file} must hold at least 32 characters"
        ));
    }
    Ok(Some(Control {
        port,
        credential,
        timeout: Duration::from_secs(timeout.max(1)),
    }))
}

/// The supervisor owns the port and the process list in `stack.json`. The manager reads only the
/// existing `control_port`, so a browser action stays a client request.
fn supervisor(config: &Config, log: &Log) -> Result<Option<Supervisor>, String> {
    let stack_file = text(config, "SupervisorStackFile", "tools/supervisor/stack.json")?;
    let (timeout, _) = config
        .value::<u64>("SupervisorTimeoutSeconds", 20)
        .map_err(|error| error.to_string())?;
    if stack_file.is_empty() {
        return Ok(None);
    }
    match supervisor_port(&stack_file) {
        Ok(port) => Ok(Some(Supervisor {
            port,
            timeout: Duration::from_secs(timeout.max(1)),
        })),
        Err(error) => {
            log.out(LogLevel::Warn, "manage", error);
            Ok(None)
        }
    }
}

/// Listen, and answer each connection on its own task.
fn serve(manage: Arc<Manage>, bind: &str, port: u16, log: &Log) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    // The last reference to the manager must not be dropped on the runtime's thread: the database
    // pool's connections are closed by `postgres`'s blocking client, whose `Drop` starts a tokio
    // runtime of its own, and a runtime started inside a runtime aborts the process. The serving
    // future takes only a clone and the original stays here, so a failed bind answers an error
    // instead of a crash (proved by starting the panel while another holds the port).
    let serving = Arc::clone(&manage);
    let log = log.clone();
    let served = runtime.block_on(async move {
        let listener = TcpListener::bind(format!("{bind}:{port}"))
            .await
            .map_err(|error| format!("cannot listen on {bind}:{port}: {error}"))?;
        // The supervisor reads this line, and an independent launch displays the panel URL.
        let ready = format!("READY: listening on http://{bind}:{port}/ - the administration tool");
        println!("{ready}");
        log.out(LogLevel::Info, "manage", ready);

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    log.out(
                        LogLevel::Warn,
                        "manage",
                        format!("a connection could not be accepted: {error}"),
                    );
                    continue;
                }
            };
            let manage = Arc::clone(&serving);
            let log = log.clone();
            // Browsers open idle connections before sending requests. Each connection runs on its
            // own task, and the blocking database and control calls run on the blocking pool.
            tokio::spawn(async move {
                answer_connection(manage, SessionIo::new(stream)).await;
                log.out(LogLevel::Debug, "manage", format!("{peer} closed"));
            });
        }
    });
    // `manage` (and the pools inside it) and the runtime are dropped here, on the program's own
    // thread, never inside the runtime: the pools' close is allowed to start its own runtime.
    drop(runtime);
    drop(manage);
    served
}

/// Answer one connection: read a request, answer it on the blocking pool, close.
async fn answer_connection(manage: Arc<Manage>, mut io: SessionIo) {
    let request = match read_request(&mut io).await {
        Ok(request) => request,
        Err(HttpError::Incomplete) => {
            let _ = io.close().await;
            return;
        }
        Err(error) => {
            let refusal = Response::text(400, format!("a request was expected: {error}\n"));
            let _ = write_response(&mut io, &refusal).await;
            let _ = io.close().await;
            return;
        }
    };
    let line = format!("{} {}", request.method, request.path);
    let state = Arc::clone(&manage);
    let response = tokio::task::spawn_blocking(move || crate::answer(&state, &request))
        .await
        .unwrap_or_else(|_| {
            crate::api::answer(Err(crate::api::broken(
                "The management request could not be completed.",
            )))
        });
    if response.status >= 400 || line.starts_with("POST") {
        manage.say(format!("{line} -> {}", response.status));
    }
    let _ = write_response(&mut io, &response).await;
    let _ = io.close().await;
}
