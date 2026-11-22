//! `kryta-manage`: the administration tool - the operator's front door to the state the two server
//! programs keep.
//!
//! It is not a server. No client talks to it, it serves no game protocol, and neither server program
//! knows it exists. What it does is read and write the state the two programs already keep: rows in the
//! login, character and world databases, and the JSON content files. The data access is the nouns'
//! own code (`game::accounts`, `game::characters`, `game::maps`, ...), so what this page changes is
//! exactly what the server reads.
//!
//! The arrangement is layered, and each layer only knows the one below it:
//!
//! - `web` - the HTTP helpers, the shell every page is drawn into, the components a page is built
//!   from, and the panel's stylesheet. Nothing here knows what an account is.
//! - `view` - the game's own names for the numbers a row stores (campaigns, professions, map ids),
//!   and the row shapes a page lists.
//! - `pages` - one module per destination, a function from the tool's state to an HTML body.
//! - this file - the routing table, the data sources, and the two owners the manager asks (the
//!   supervisor for process lifecycle, the running world for operations).
//!
//! Why a page rather than SQL by hand: the state is spread over two databases and four content files,
//! and a wrong `UPDATE` against a live account is not something a person should be doing from memory.
//!
//! Settings are in `manage.json` (`docs/port/05-tooling.md`); the tool refuses to start without a
//! database to point at.

mod navigation;
mod pages;
mod view;
mod web;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::configuration::{Config, Policy};
use common::logging::{Log, LogLevel};
use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
use shared::network::http::{HttpError, Request, Response, read_request, write_response};
use shared::network::socket::{Session, SessionFuture, SessionIo};
use tokio::net::TcpListener;

/// One database the schema page reads: its label, the pool, and where its files live.
pub struct DatabaseSchema {
    /// What the page calls it.
    pub label: &'static str,
    /// The database itself.
    pub pool: Arc<Mutex<DatabaseWorkerPool>>,
    /// The directory its schema files live in.
    pub dir: &'static str,
}

/// One schema file, as the database's own record and the disk agree or not.
pub struct SchemaFile {
    /// The file's name, the one the updater records.
    pub name: String,
    /// `applied`, `changed`, `new`, or `recorded, not on disk`.
    pub state: &'static str,
}

/// What the tool serves from: the account table, and the token a request must carry when one is set.
pub struct Manage {
    accounts: Arc<game::accounts::DatabaseAccounts>,
    /// The character database, when the tool opened one: the characters page.
    characters: Option<Arc<game::characters::DatabaseCharacters>>,
    /// The world database's item templates, when the world database is open: what a creation
    /// confirmation points a written item row at.
    items: Option<Arc<game::data_stores::DatabaseItems>>,
    /// The instance table, when the world database is open: what the content page shows running.
    instances: Option<Arc<game::districts::DatabaseInstances>>,
    /// The world list, read from `realmlist` and merged with `data/realms.json`.
    worlds: Option<Arc<shared::worlds::WorldList>>,
    /// The databases whose updater records the schema page shows.
    schemas: Vec<DatabaseSchema>,
    /// The two server programs' own settings files, when the tool is told where they are: the
    /// settings page shows them, and the world's own `WorldPack` names the content the world serves.
    /// A file is shown and validated, never rewritten (`manage.json`).
    world_config: Option<String>,
    auth_config: Option<String>,
    /// The portable graph cache the manager inspects and selected-map exports update.
    path_graphs_file: String,
    /// An explicit client archive used only by the manager's offline dump action.
    map_archive_file: String,
    /// The running world's local control endpoint, when `manage.json` names one: the status page asks
    /// it for `world.status`. This is the manager's connection to the world, never a second world.
    control: Option<Control>,
    /// The running stack's own loopback channel, discovered from its `stack.json`. The manager is a
    /// client of this channel and never starts or watches a process itself.
    supervisor: Option<Supervisor>,
    token: Option<String>,
    log: Log,
}

/// Where the running world answers operator operations: `worldserver`'s loopback control endpoint.
pub struct Control {
    /// The loopback port the world program listens on (`Control.Port` in its own settings).
    port: u16,
    /// The service credential the world program requires, read from its restricted file.
    credential: String,
    /// How long the status page waits for the world before reporting it unavailable.
    timeout: Duration,
}

/// Where the running supervisor accepts the lifecycle verbs it owns.
pub struct Supervisor {
    port: u16,
    timeout: Duration,
}

impl Manage {
    /// One page, answered.
    ///
    /// The routing table is the one place that knows the panel's addresses. Every route names a
    /// page module and nothing else; a route that mutates is a `POST` and answers another page or a
    /// redirect, so a browser's back button never re-runs a write.
    fn answer(&self, request: &Request) -> Response {
        if !self.authorised(request) {
            return pages::unauthorised();
        }
        let segments: Vec<&str> = request
            .path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        match request.method.as_str() {
            "GET" => self.get(&segments, request),
            "POST" => self.post(&segments, request),
            _ => pages::method_not_allowed(&request.path),
        }
    }

    /// The pages a `GET` opens.
    ///
    /// The hierarchy is the routing: an account is a page with a row id, a player is a page with
    /// the id the client knows it by, and the two old addresses (`/accounts?account=`,
    /// `/characters?account=`) are redirects into it.
    fn get(&self, segments: &[&str], request: &Request) -> Response {
        match segments {
            [] => pages::overview::page(self),
            ["static", "style.css"] => {
                Response::new(200, web::layout::STYLE_TYPE, web::layout::STYLE.as_bytes())
            }
            ["health"] => Response::text(200, "ok\n"),
            ["accounts"] => {
                // The old address: a page that named an account is sent to its own page.
                if let Some(account) = request
                    .query("account")
                    .and_then(|value| value.parse::<i64>().ok())
                {
                    return web::http::redirect(&format!("/accounts/{account}"));
                }
                pages::accounts::index(self)
            }
            ["accounts", id] => by_id(id, |id| pages::accounts::account(self, id)),
            ["players"] => pages::players::list(self, request),
            // A numeric segment is the old account page, not a player: a player's id is a uuid.
            ["players", segment] => match segment.parse::<i64>() {
                Ok(id) => web::http::redirect(&format!("/accounts/{id}")),
                Err(_) => pages::players::detail(self, segment),
            },
            ["services"] => pages::services::page(self, None),
            ["services", "status"] => pages::services::status_fragment(self),
            ["console"] => pages::console::page(self, None),
            ["console", "operations"] => pages::console::operations_fragment(self),
            ["content"] => pages::content::page(self),
            ["worlds"] => pages::worlds::page(self),
            ["navigation"] => {
                pages::navigation::page(self, None, request.query("inspect") == Some("1"))
            }
            ["status"] => pages::status::page(self),
            ["status", "data"] => pages::status::fragment(self),
            ["settings"] => pages::settings::page(self),
            ["schema"] => pages::schema::page(self),
            ["api", "navigation", "maps"] => self.navigation_api(),
            // The addresses the panel had before the hierarchy was named: still bookmarks.
            ["players", account, "characters"] => match account.parse::<i64>() {
                Ok(id) => web::http::redirect(&format!("/players?account={id}")),
                Err(_) => web::http::redirect("/players"),
            },
            ["players", _account, "characters", uuid] => {
                web::http::redirect(&format!("/players/{uuid}"))
            }
            ["characters"] => {
                // This address used to name the account by its portal user id, not by its row id.
                let target = match (request.query("account"), request.query("character")) {
                    (_, Some(character)) => format!("/players/{character}"),
                    (Some(user_id), None) => match self.list_accounts() {
                        Ok(accounts) => match accounts
                            .into_iter()
                            .find(|account| account.user_id == user_id)
                        {
                            Some(account) => format!("/players?account={}", account.id),
                            None => "/players".to_string(),
                        },
                        Err(_) => "/players".to_string(),
                    },
                    (None, _) => "/players".to_string(),
                };
                web::http::redirect(&target)
            }
            _ => pages::missing(&request.path),
        }
    }

    /// The pages a form `POST`s to.
    fn post(&self, segments: &[&str], request: &Request) -> Response {
        match segments {
            ["accounts", "create"] => pages::accounts::create(self, request),
            ["accounts", id, "update"] => {
                by_id(id, |id| pages::accounts::update(self, request, id))
            }
            ["accounts", id, "active"] => {
                by_id(id, |id| pages::accounts::set_active(self, request, id))
            }
            ["accounts", id, "eula"] => {
                by_id(id, |id| pages::accounts::set_eula(self, request, id))
            }
            ["accounts", id, "delete"] => {
                by_id(id, |id| pages::accounts::delete(self, request, id))
            }
            ["players", "create"] => pages::players::create(self, request),
            ["players", uuid, "rename"] => pages::players::rename(self, request, uuid),
            ["players", uuid, "level"] => pages::players::set_level(self, request, uuid),
            ["players", uuid, "delete"] => pages::players::delete(self, request, uuid),
            ["services", "stop"] => pages::services::action(self, request, "stop"),
            ["services", "restart"] => pages::services::action(self, request, "restart"),
            ["console", "run"] => pages::console::run(self, request),
            ["navigation", "dump"] => self.dump_navigation_page(request),
            ["api", "navigation", "dump"] => self.dump_navigation_api(request),
            _ => pages::method_not_allowed(&request.path),
        }
    }

    /// Whether a request may be answered.
    ///
    /// With no token configured, every request from this machine is allowed and nothing else: the page
    /// binds loopback, and a tool an operator starts is meant to be usable. With a token, the request
    /// carries it as `?token=` or as the `auth` cookie.
    fn authorised(&self, request: &Request) -> bool {
        let Some(expected) = self.token.as_deref() else {
            return true;
        };
        if expected.is_empty() {
            return true;
        }
        if request.query("token") == Some(expected) {
            return true;
        }
        request
            .header("cookie")
            .map(|cookies| {
                cookies
                    .split(';')
                    .any(|cookie| cookie.trim() == format!("auth={expected}"))
            })
            .unwrap_or(false)
    }

    // The data sources a page asks. Each accessor answers the noun's own repository, so a page
    // never owns a second copy of the data or the SQL.

    /// The login database's account table.
    pub fn accounts(&self) -> &game::accounts::DatabaseAccounts {
        &self.accounts
    }

    /// The character database, when `manage.json` names one.
    pub fn characters(&self) -> Option<&game::characters::DatabaseCharacters> {
        self.characters.as_deref()
    }

    /// The world database's item templates.
    pub fn items(&self) -> Option<&game::data_stores::DatabaseItems> {
        self.items.as_deref()
    }

    /// The world database's instance table.
    pub fn instances(&self) -> Option<&game::districts::DatabaseInstances> {
        self.instances.as_deref()
    }

    /// The realm list the login program reads.
    pub fn worlds(&self) -> Option<&shared::worlds::WorldList> {
        self.worlds.as_deref()
    }

    /// The tool's log, for the views that load content (the creation record, the pack).
    pub fn log(&self) -> &Log {
        &self.log
    }

    /// Say one line, as every action this tool takes does.
    pub fn say(&self, text: impl Into<String>) {
        self.log.out(LogLevel::Info, "manage", text);
    }

    /// The world program's settings file, when one is named.
    pub fn world_config(&self) -> Option<&str> {
        self.world_config.as_deref()
    }

    /// The login program's settings file, when one is named.
    pub fn auth_config(&self) -> Option<&str> {
        self.auth_config.as_deref()
    }

    /// The accounts, for a page that lists them.
    pub fn list_accounts(&self) -> Result<Vec<game::accounts::Account>, String> {
        self.accounts.list()
    }

    /// The characters of one account, for the account workspace. A character remains owned by the
    /// account that created it; this is not a second character inventory kept by the manager.
    pub fn characters_of_account(&self, account: &str) -> Option<Vec<game::characters::Character>> {
        self.characters
            .as_ref()
            .map(|characters| characters.for_account(account))
    }

    /// What one character holds, read through the repository the load uses.
    pub fn list_items_of(&self, uuid: &str) -> Option<Vec<game::characters::HeldItem>> {
        self.characters
            .as_ref()
            .and_then(|characters| characters.items_of(uuid).ok())
    }

    /// The maps one character may travel to, read through the repository the load's gate uses (R12).
    pub fn list_unlocks(&self, uuid: &str) -> Option<Vec<u32>> {
        self.characters
            .as_ref()
            .and_then(|characters| characters.unlocked_maps(uuid).ok())
    }

    /// The world list, when the login database answered.
    pub fn list_worlds(&self) -> Option<Vec<shared::worlds::Realm>> {
        self.worlds
            .as_ref()
            .map(|worlds| worlds.worlds().values().cloned().collect())
    }

    /// The copies the instance table records, when a world database is configured.
    pub fn list_instances(&self) -> Option<Vec<game::districts::InstanceRow>> {
        self.instances.as_ref()?.rows().ok()
    }

    /// Where the two programs' settings files are, when the tool was told: the settings page reads
    /// them from these paths and never writes them.
    pub fn config_paths(&self) -> (Option<&str>, Option<&str>) {
        (self.world_config.as_deref(), self.auth_config.as_deref())
    }

    /// The world's own map pack, read through the pack reader the world uses.
    ///
    /// The content of a world is the pack (`docs/decisions/0014-the-world-content-is-a-pack.md`):
    /// the manager reads it with the same reader so a page shows the maps, entry points and
    /// arrivals the world will serve, not rows nothing reads.
    pub fn world_pack(&self) -> Result<game::maps::pack::Pack, String> {
        game::maps::pack::Pack::load(&self.world_pack_path()?, &self.log)
    }

    pub(crate) fn world_pack_path(&self) -> Result<String, String> {
        let world_config = self.world_config.as_deref().ok_or_else(|| {
            "the world program's settings file is not named in manage.json (WorldConfigFile)"
                .to_string()
        })?;
        let config = Config::load_json(world_config, false, Policy::default())
            .map_err(|error| error.to_string())?;
        let (pack_path, _) = config
            .value_string("WorldPack", "config/World/Vanilla")
            .map_err(|error| error.to_string())?;
        if pack_path.trim().is_empty() {
            return Err("the world program names no WorldPack".to_string());
        }
        Ok(pack_path)
    }

    /// Every map name the world's pack states, keyed by id: what the pages call an outpost.
    pub fn map_names(&self) -> std::collections::HashMap<u32, String> {
        self.world_pack()
            .ok()
            .and_then(|pack| pack.load_maps().ok())
            .map(|(maps, _)| maps.into_iter().map(|map| (map.id, map.name)).collect())
            .unwrap_or_default()
    }

    /// The creation record lists the world runs from, read through the noun's own loader.
    pub fn creation_tables(&self) -> Result<game::creation::Tables, String> {
        let world_config = self.world_config.as_deref().ok_or_else(|| {
            "the world program's settings file is not named in manage.json (WorldConfigFile)"
                .to_string()
        })?;
        let config = Config::load_json(world_config, false, Policy::default())
            .map_err(|error| error.to_string())?;
        let (inventory, _) = config
            .value_string(
                "InventoryFile",
                "config/World/Vanilla/creation/inventory.json",
            )
            .map_err(|error| error.to_string())?;
        let (equipment, _) = config
            .value_string(
                "EquipmentFile",
                "config/World/Vanilla/creation/equipment.json",
            )
            .map_err(|error| error.to_string())?;
        let (worn, _) = config
            .value_string(
                "EquippedSlotsFile",
                "config/World/Vanilla/creation/worn_slots.json",
            )
            .map_err(|error| error.to_string())?;
        game::creation::Tables::load(
            &inventory,
            &equipment,
            Some(std::path::Path::new(&worn)),
            &self.log,
        )
    }

    /// The running world's own status, asked of its control endpoint.
    ///
    /// One operation, carried by the world's request queue and executed on the world's update thread,
    /// so a page can read the world without becoming a second owner of its state
    /// (`docs/port/14-operator-surfaces.md`, "First slice: local manager reads world status").
    pub fn world_status(&self) -> Result<String, String> {
        self.world_operation("world.status", &BTreeMap::new())
    }

    /// Call one registered operation through the same endpoint as `kryta-cli`. The world retains
    /// argument validation, queue ownership and execution; this browser route is only an adapter.
    pub fn world_operation(
        &self,
        operation: &str,
        arguments: &BTreeMap<String, String>,
    ) -> Result<String, String> {
        let Some(control) = self.control.as_ref() else {
            return Err(
                "no world control endpoint is configured: set ControlPort and ControlCredentialFile \
                 in manage.json, and Control.Enable with a credential file in worldserver.json"
                    .into(),
            );
        };
        let arguments = serde_json::Value::Object(
            arguments
                .iter()
                .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
                .collect(),
        );
        let body = serde_json::json!({
            "request_id": "manage-console",
            "operation": operation,
            "arguments": arguments
        })
        .to_string();
        let request = format!(
            "POST /v1/operations HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            control.credential,
            body.len()
        );
        let address = SocketAddr::from(([127, 0, 0, 1], control.port));
        let mut stream =
            TcpStream::connect_timeout(&address, control.timeout).map_err(|error| {
                format!("the world control endpoint at {address} is unavailable: {error}")
            })?;
        stream
            .set_read_timeout(Some(control.timeout))
            .map_err(|error| error.to_string())?;
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("the request to the world failed: {error}"))?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|error| format!("the world's answer could not be read: {error}"))?;
        let (_, body) = response
            .split_once("\r\n\r\n")
            .ok_or("the world answered with something that is not an HTTP response")?;
        let value: serde_json::Value = serde_json::from_str(body)
            .map_err(|_| "the world answered with something that is not JSON")?;
        if value.get("ok").and_then(|ok| ok.as_bool()) == Some(true) {
            return Ok(value
                .get("result")
                .and_then(|result| result.as_str())
                .unwrap_or_default()
                .to_string());
        }
        Err(value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(|message| message.as_str())
            .unwrap_or("the world refused the operation")
            .to_string())
    }

    /// Ask the running supervisor for the state or one of its documented lifecycle verbs.
    pub fn supervisor_request(&self, line: &str) -> Result<String, String> {
        let Some(supervisor) = self.supervisor.as_ref() else {
            return Err(
                "no supervisor control channel is configured: set SupervisorStackFile to a stack.json \
                 with control_port, then start kryta-supervisor"
                    .into(),
            );
        };
        let address = SocketAddr::from(([127, 0, 0, 1], supervisor.port));
        let mut stream = TcpStream::connect_timeout(&address, supervisor.timeout)
            .map_err(|error| format!("the supervisor at {address} is unavailable: {error}"))?;
        stream
            .set_read_timeout(Some(supervisor.timeout))
            .map_err(|error| error.to_string())?;
        stream
            .write_all(format!("{}\n", line.trim()).as_bytes())
            .map_err(|error| format!("the request to the supervisor failed: {error}"))?;
        let mut answer = String::new();
        stream
            .read_to_string(&mut answer)
            .map_err(|error| format!("the supervisor's answer could not be read: {error}"))?;
        let answer = answer.trim().to_string();
        if answer.starts_with("err:") {
            return Err(answer);
        }
        Ok(answer)
    }

    /// Inspect the extracted graphs. This manager API reads an operator asset, not state the running
    /// world owns, so it does not go through the world-operation queue.
    pub fn inspect_navigation(&self) -> Result<navigation::Inspection, String> {
        navigation::inspect(&self.path_graphs_file)
    }

    fn dump_navigation(&self, map_id: u32) -> Result<navigation::DumpResult, String> {
        if self.map_archive_file.trim().is_empty() {
            return Err(
                "MapArchiveFile is empty; the manager has no archive selected for offline extraction"
                    .to_string(),
            );
        }
        let pack = self.world_pack()?;
        let (maps, _) = pack.load_maps()?;
        let map = maps
            .iter()
            .find(|map| map.id == map_id)
            .ok_or_else(|| format!("map {map_id} is not in the world pack"))?;
        navigation::dump_map(&self.path_graphs_file, &self.map_archive_file, map)
    }

    fn navigation_api(&self) -> Response {
        match self.inspect_navigation() {
            Ok(inspection) => Response::new(
                200,
                "application/json; charset=utf-8",
                serde_json::json!({
                    "path": inspection.path,
                    "bytes": inspection.bytes,
                    "maps": inspection.maps.iter().map(|map| serde_json::json!({
                        "map_id": map.map_id,
                        "file_id": map.file_id,
                        "planes": map.planes,
                        "trapezoids": map.trapezoids,
                    })).collect::<Vec<_>>(),
                })
                .to_string(),
            ),
            Err(error) => Response::new(
                500,
                "application/json; charset=utf-8",
                serde_json::json!({"error": error}).to_string(),
            ),
        }
    }

    fn dump_navigation_api(&self, request: &Request) -> Response {
        let map_id = web::http::form_fields(&request.body_text())
            .get("map_id")
            .and_then(|value| value.parse::<u32>().ok());
        let Some(map_id) = map_id else {
            return Response::new(
                400,
                "application/json; charset=utf-8",
                "{\"error\":\"map_id is required\"}",
            );
        };
        match self.dump_navigation(map_id) {
            Ok(result) => Response::new(
                200,
                "application/json; charset=utf-8",
                serde_json::json!({
                    "map_id": result.map_id,
                    "file_id": result.file_id,
                    "cache": result.cache,
                    "graph_count": result.graph_count,
                    "bytes": result.bytes,
                })
                .to_string(),
            ),
            Err(error) => Response::new(
                400,
                "application/json; charset=utf-8",
                serde_json::json!({"error": error}).to_string(),
            ),
        }
    }

    fn dump_navigation_page(&self, request: &Request) -> Response {
        let map_id = web::http::form_fields(&request.body_text())
            .get("map_id")
            .and_then(|value| value.parse::<u32>().ok());
        let result = match map_id {
            Some(map_id) => self.dump_navigation(map_id).map(|result| {
                format!(
                    "Dumped map {} from blob {:#X}; {} graph(s), {} bytes in {}.",
                    result.map_id, result.file_id, result.graph_count, result.bytes, result.cache
                )
            }),
            None => Err("A numeric map id is required.".to_string()),
        };
        pages::navigation::page(self, Some(result), true)
    }

    /// Each database's schema files, with what its own `updates` record says about them.
    ///
    /// The record is the database's, read by the updater's own reader; the files are the ones on
    /// disk. A file recorded with another hash has been edited after it was applied - the updater
    /// refuses to apply it again, and this page is where that is seen before a server refuses to start.
    pub fn schema_page(&self) -> Vec<(&'static str, Vec<SchemaFile>)> {
        let mut out = Vec::new();
        for database in &self.schemas {
            let applied = match database.pool.lock() {
                Ok(pool) => database::updater::apply::read_applied(&pool).unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            let files = database::updater::collect_files(
                database.dir,
                database::updater::UpdateState::Released,
            )
            .unwrap_or_default();
            let mut rows = Vec::new();
            for file in &files {
                let name = file.name();
                let state = match applied.iter().find(|(recorded, _)| *recorded == name) {
                    Some((_, hash)) => {
                        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
                        if database::updater::apply::sha1_hex(&text).eq_ignore_ascii_case(hash) {
                            "applied"
                        } else {
                            "changed"
                        }
                    }
                    None => "new",
                };
                rows.push(SchemaFile { name, state });
            }
            for (name, _) in &applied {
                if !files.iter().any(|file| file.name() == *name) {
                    rows.push(SchemaFile {
                        name: name.clone(),
                        state: "recorded, not on disk",
                    });
                }
            }
            out.push((database.label, rows));
        }
        out
    }
}

/// One connection to the tool.
pub struct ManageSession {
    manage: Arc<Manage>,
}

impl ManageSession {
    /// Answer one connection: read a request, answer it on the blocking pool, close.
    ///
    /// Every repository call is a blocking wait on the database pool's worker queue, so the page is
    /// built off the async thread and the runtime keeps accepting connections meanwhile.
    async fn answer(self, mut io: SessionIo) {
        let request = match read_request(&mut io).await {
            Ok(request) => request,
            Err(HttpError::Incomplete) => {
                let _ = io.close().await;
                return;
            }
            Err(error) => {
                let _ = write_response(
                    &mut io,
                    &Response::text(400, format!("a request was expected: {error}\n")),
                )
                .await;
                let _ = io.close().await;
                return;
            }
        };

        self.manage
            .say(format!("{} {} ->", request.method, request.path));
        let manage = Arc::clone(&self.manage);
        let response = match tokio::task::spawn_blocking(move || manage.answer(&request)).await {
            Ok(response) => response,
            Err(_) => Response::text(500, "The management request could not be completed.\n"),
        };
        let _ = write_response(&mut io, &response).await;
        let _ = io.close().await;
    }
}

impl Session for ManageSession {
    fn run(self: Box<Self>, io: SessionIo) -> SessionFuture {
        Box::pin(async move { self.answer(io).await })
    }
}

/// Read one URL segment as the row id it names.
fn by_id(segment: &str, route: impl FnOnce(i64) -> Response) -> Response {
    match segment.parse::<i64>() {
        Ok(id) => route(id),
        Err(_) => pages::error(404, "players", "No page is at this address."),
    }
}

/// Some entropy for an identifier a request did not state.
pub(crate) fn random_u64() -> u64 {
    let mut bytes = [0u8; 8];
    if getrandom(&mut bytes).is_err() {
        return 1;
    }
    u64::from_le_bytes(bytes)
}

/// The operating system's entropy, through the same call the servers use.
fn getrandom(bytes: &mut [u8]) -> Result<(), ()> {
    shared::cryptography::handshake::random_into(bytes).map_err(|_| ())
}

/// Read only the existing supervisor control port from its stack file. The service rows remain owned
/// by the supervisor and are never parsed into manager state.
fn supervisor_port(path: &str) -> Result<u16, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read SupervisorStackFile {path}: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("SupervisorStackFile {path} is not JSON: {error}"))?;
    let port = value
        .get("control_port")
        .and_then(serde_json::Value::as_u64)
        .filter(|port| *port > 0 && *port <= u16::MAX as u64)
        .ok_or_else(|| format!("SupervisorStackFile {path} has no usable control_port"))?;
    Ok(port as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_control_port_is_read_from_a_stack_file() {
        let path =
            std::env::temp_dir().join(format!("kryta-manage-stack-{}.json", std::process::id()));
        std::fs::write(&path, "{\"control_port\":7612,\"services\":[]}").expect("stack is written");
        assert_eq!(
            supervisor_port(&path.to_string_lossy()).expect("port reads"),
            7612
        );
        std::fs::write(&path, "{\"control_port\":0}").expect("stack is rewritten");
        assert!(supervisor_port(&path.to_string_lossy()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_route_that_names_a_row_id_refuses_a_word() {
        let response = by_id("not-a-number", |_| {
            unreachable!("a refused route never calls its page")
        });
        assert_eq!(response.status, 404);
    }
}

/// Run the management service or its compatible legacy executable.
pub fn main_entry() -> std::process::ExitCode {
    let mut config_path = std::path::PathBuf::from("tools/manage/manage.json");
    let mut port_override = None;
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
            other
                if port_override.is_none() && other.parse::<u16>().is_ok_and(|port| port != 0) =>
            {
                port_override = other.parse::<u16>().ok();
            }
            other => {
                eprintln!("unknown argument: {other}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    match run(&config_path, port_override) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kryta-manage: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(config_path: &std::path::Path, port_override: Option<u16>) -> Result<(), String> {
    let mut config = Config::load_json(config_path, false, Policy::default())
        .map_err(|error| error.to_string())?;

    let log = Log::new();
    log.initialize(&config).map_err(|error| error.to_string())?;
    log.out(
        LogLevel::Info,
        "manage",
        format!("kryta-manage reading {}", config_path.display()),
    );

    let (bind, _) = config
        .value_string("Bind", "127.0.0.1")
        .map_err(|error| error.to_string())?;
    let (port, _) = config
        .value::<u16>("Port", 6602)
        .map_err(|error| error.to_string())?;
    let port = port_override.unwrap_or(port);
    let (token, _) = config
        .value_string("Token", "")
        .map_err(|error| error.to_string())?;
    config.mark_critical("LoginDatabaseInfo");
    let settings = read_settings(&config, DatabaseKind::Login)?;
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)?;
    pool.ping()
        .map_err(|error| format!("the login database does not answer: {error}"))?;

    // The world list is the login's own reader, merged with the operator's file: the same rows the
    // auth program sends a client to, not a second reading of them.
    let mut world_list = shared::worlds::WorldList::new();
    let loaded = world_list.load(pool.connection_mut(), &log)?;
    log.out(
        LogLevel::Info,
        "manage",
        format!("World list: {loaded} row(s) in the realmlist table."),
    );
    let (worlds_file, _) = config
        .value_string("WorldsFile", "data/realms.json")
        .map_err(|error| error.to_string())?;
    if !worlds_file.is_empty() {
        world_list.merge_file(&worlds_file, &log)?;
    }
    let login = Arc::new(Mutex::new(pool));
    let accounts = Arc::new(game::accounts::DatabaseAccounts::new(
        Arc::clone(&login),
        log.clone(),
    ));

    // The character database: what the login lists, read through the noun's own repository. A page
    // without one says so rather than failing every request.
    let character_pool = match config.value_string("CharacterDatabaseInfo", "") {
        Ok((info, _)) if !info.is_empty() => {
            let settings = read_settings(&config, DatabaseKind::Character)?;
            match DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log) {
                Ok(mut pool) => {
                    pool.ping().map_err(|error| {
                        format!("the character database does not answer: {error}")
                    })?;
                    Some(Arc::new(Mutex::new(pool)))
                }
                Err(error) => {
                    log.out(
                        LogLevel::Warn,
                        "manage",
                        format!("the character database is off: {error}"),
                    );
                    None
                }
            }
        }
        _ => None,
    };
    let characters = character_pool.as_ref().map(|pool| {
        Arc::new(game::characters::DatabaseCharacters::new(
            Arc::clone(pool),
            log.clone(),
        ))
    });

    // The world database: the item templates a creation confirm points at, and the running copies
    // its tick disposes. The world's maps and items are the pack's, read through `world_pack`.
    let world_pool = match config.value_string("WorldDatabaseInfo", "") {
        Ok((info, _)) if !info.is_empty() => {
            let settings = read_settings(&config, DatabaseKind::World)?;
            match DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log) {
                Ok(mut pool) => {
                    pool.ping()
                        .map_err(|error| format!("the world database does not answer: {error}"))?;
                    Some(Arc::new(Mutex::new(pool)))
                }
                Err(error) => {
                    log.out(
                        LogLevel::Warn,
                        "manage",
                        format!("the world database is off: {error}"),
                    );
                    None
                }
            }
        }
        _ => None,
    };
    let items = world_pool.as_ref().map(|pool| {
        Arc::new(game::data_stores::DatabaseItems::new(
            Arc::clone(pool),
            log.clone(),
        ))
    });
    let instances = world_pool.as_ref().map(|pool| {
        Arc::new(game::districts::DatabaseInstances::new(
            Arc::clone(pool),
            log.clone(),
        ))
    });

    // The schema page: each database's own record of what it has applied, against the files on disk.
    let mut schemas = vec![DatabaseSchema {
        label: "login",
        pool: Arc::clone(&login),
        dir: "data/sql/base/db_auth",
    }];
    if let Some(pool) = character_pool {
        schemas.push(DatabaseSchema {
            label: "characters",
            pool,
            dir: "data/sql/base/db_characters",
        });
    }
    if let Some(pool) = world_pool {
        schemas.push(DatabaseSchema {
            label: "world",
            pool,
            dir: "data/sql/base/db_world",
        });
    }

    // The two programs' own settings files, shown by the settings page. Relative to where the tool
    // is started, which is the repository root in the manual test's own commands.
    let (world_config, _) = config
        .value_string("WorldConfigFile", "worldserver/worldserver.json")
        .map_err(|error| error.to_string())?;
    let (auth_config, _) = config
        .value_string("AuthConfigFile", "authserver/authserver.json")
        .map_err(|error| error.to_string())?;
    let world_config = if world_config.is_empty() {
        None
    } else {
        Some(world_config)
    };
    let auth_config = if auth_config.is_empty() {
        None
    } else {
        Some(auth_config)
    };
    let (path_graphs_file, _) = config
        .value_string("PathGraphsFile", "data/path-graphs.bin")
        .map_err(|error| error.to_string())?;
    let (map_archive_file, _) = config
        .value_string("MapArchiveFile", "")
        .map_err(|error| error.to_string())?;

    let (control_port, _) = config
        .value::<u16>("ControlPort", 0)
        .map_err(|error| error.to_string())?;
    let (control_credential_file, _) = config
        .value_string("ControlCredentialFile", "")
        .map_err(|error| error.to_string())?;
    let (control_timeout, _) = config
        .value::<u64>("ControlTimeoutSeconds", 5)
        .map_err(|error| error.to_string())?;
    let control = if control_port == 0 || control_credential_file.is_empty() {
        None
    } else {
        let credential = std::fs::read_to_string(&control_credential_file)
            .map_err(|error| {
                format!("cannot read ControlCredentialFile {control_credential_file}: {error}")
            })?
            .trim()
            .to_owned();
        if credential.len() < 32 {
            return Err(format!(
                "ControlCredentialFile {control_credential_file} must hold at least 32 characters"
            ));
        }
        Some(Control {
            port: control_port,
            credential,
            timeout: Duration::from_secs(control_timeout.max(1)),
        })
    };

    // The supervisor owns the port and the process list in stack.json. The manager reads only the
    // existing control_port, so a browser action remains a client request rather than a second
    // process supervisor.
    let (supervisor_stack_file, _) = config
        .value_string("SupervisorStackFile", "tools/supervisor/stack.json")
        .map_err(|error| error.to_string())?;
    let (supervisor_timeout, _) = config
        .value::<u64>("SupervisorTimeoutSeconds", 20)
        .map_err(|error| error.to_string())?;
    let supervisor = if supervisor_stack_file.is_empty() {
        None
    } else {
        match supervisor_port(&supervisor_stack_file) {
            Ok(port) => Some(Supervisor {
                port,
                timeout: Duration::from_secs(supervisor_timeout.max(1)),
            }),
            Err(error) => {
                log.out(LogLevel::Warn, "manage", error);
                None
            }
        }
    };

    let manage = Arc::new(Manage {
        accounts,
        characters,
        items,
        instances,
        worlds: Some(Arc::new(world_list)),
        schemas,
        world_config,
        auth_config,
        path_graphs_file,
        map_archive_file,
        control,
        supervisor,
        token: Some(token),
        log: log.clone(),
    });

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    // The last reference to the manager must not be dropped on the runtime's thread: the database
    // pool's connections are closed by `postgres`'s blocking client, whose `Drop` starts a tokio
    // runtime of its own, and a runtime started inside a runtime aborts the process. The serving
    // future takes only a clone and the original stays here, so a failed bind answers an error page
    // instead of a crash (proved by starting the panel while another holds the port).
    let serving = Arc::clone(&manage);
    let log_for_loop = log.clone();
    let served = runtime.block_on(async move {
        let listener = TcpListener::bind(format!("{bind}:{port}"))
            .await
            .map_err(|error| format!("cannot listen on {bind}:{port}: {error}"))?;
        // The supervisor reads this line, and an independent launch displays the panel URL.
        let ready = format!("READY: listening on http://{bind}:{port}/ - the administration tool");
        println!("{ready}");
        log_for_loop.out(LogLevel::Info, "manage", ready);

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    log_for_loop.out(
                        LogLevel::Warn,
                        "manage",
                        format!("a connection could not be accepted: {error}"),
                    );
                    continue;
                }
            };
            let manage = Arc::clone(&serving);
            let log = log_for_loop.clone();
            // Browsers can open idle connections before sending requests. Each connection must
            // run independently, and blocking database and control calls run on the blocking pool.
            tokio::spawn(async move {
                let session = ManageSession { manage };
                session.answer(SessionIo::new(stream)).await;
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
