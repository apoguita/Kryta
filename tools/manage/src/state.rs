//! The service's state: the repositories it reads through, the files it is told about, and its two
//! outbound clients.
//!
//! Every accessor answers a noun's own repository (`game::accounts`, `game::characters`,
//! `game::maps::pack`, ...), so an area never owns a second copy of the data and the panel writes no
//! SQL: what it changes is exactly what the server reads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::configuration::{Config, Policy};
use common::logging::{Log, LogLevel};
use database::database::DatabaseWorkerPool;

use crate::api::{Failure, Guard, unavailable};
use crate::clients::{Control, Supervisor};

/// One database the schema area reads: its label, the pool, and where its files live.
pub struct DatabaseSchema {
    /// What the page calls it.
    pub label: &'static str,
    /// The database itself.
    pub pool: Arc<Mutex<DatabaseWorkerPool>>,
    /// The directory its schema files live in.
    pub dir: &'static str,
}

/// What the service answers from.
pub struct Manage {
    /// The login database's account table.
    pub(crate) accounts: Arc<game::accounts::DatabaseAccounts>,
    /// The character database, when the tool opened one.
    pub(crate) characters: Option<Arc<game::characters::DatabaseCharacters>>,
    /// The instance table, when the world database is open.
    pub(crate) instances: Option<Arc<game::districts::DatabaseInstances>>,
    /// The world list, read from `realmlist` and merged with `data/realms.json` at start-up.
    pub(crate) worlds: Arc<shared::worlds::WorldList>,
    /// The databases whose updater records the schema area shows.
    pub(crate) schemas: Vec<DatabaseSchema>,
    /// The world program's settings file, shown and changed by the settings area. Its `WorldPack` names the
    /// content the world serves.
    pub(crate) world_config: Option<String>,
    /// The login program's settings file, shown and changed by the settings area.
    pub(crate) auth_config: Option<String>,
    /// The portable graph cache the navigation area inspects and updates.
    pub(crate) path_graphs_file: String,
    /// An explicit client archive used only by the offline dump action.
    pub(crate) map_archive_file: String,
    /// The running world's local control endpoint, when `manage.json` names one.
    pub(crate) control: Option<Control>,
    /// The running stack's loopback channel, discovered from its `stack.json`.
    pub(crate) supervisor: Option<Supervisor>,
    /// Who may be answered.
    pub(crate) guard: Guard,
    /// The tool's log.
    pub(crate) log: Log,
}

impl Manage {
    /// The login database's account table.
    pub fn accounts(&self) -> &game::accounts::DatabaseAccounts {
        &self.accounts
    }

    /// The character database, or the 503 that says none is configured.
    pub fn characters(&self) -> Result<&game::characters::DatabaseCharacters, Failure> {
        self.characters.as_deref().ok_or_else(|| {
            unavailable("No character database is configured in manage.json (CharacterDatabaseInfo).")
        })
    }

    /// Say one line, as every change this tool makes does.
    pub fn say(&self, text: impl Into<String>) {
        self.log.out(LogLevel::Info, "manage", text);
    }

    /// The world program's settings, read from its own file.
    pub fn world_settings(&self) -> Result<Config, String> {
        let path = self.world_config.as_deref().ok_or_else(|| {
            "the world program's settings file is not named in manage.json (WorldConfigFile)"
                .to_string()
        })?;
        Config::load_json(path, false, Policy::default()).map_err(|error| error.to_string())
    }

    /// The folder of the world's pack (`docs/decisions/0014-the-world-content-is-a-pack.md`).
    pub fn world_pack_path(&self) -> Result<String, String> {
        let (pack_path, _) = self
            .world_settings()?
            .value_string("WorldPack", "config/World/Vanilla")
            .map_err(|error| error.to_string())?;
        if pack_path.trim().is_empty() {
            return Err("the world program names no WorldPack".to_string());
        }
        Ok(pack_path)
    }

    /// The world's own map pack, read through the pack reader the world uses, so an area shows the
    /// maps, entry points and arrivals the world will serve.
    pub fn world_pack(&self) -> Result<game::maps::pack::Pack, String> {
        game::maps::pack::Pack::load(self.world_pack_path()?, &self.log)
    }

    /// Every map name the world's pack states, keyed by id: what the pages call an outpost.
    pub fn map_names(&self) -> HashMap<u32, String> {
        self.world_pack()
            .ok()
            .and_then(|pack| pack.load_maps().ok())
            .map(|(maps, _)| maps.into_iter().map(|map| (map.id, map.name)).collect())
            .unwrap_or_default()
    }

    /// The creation record lists the world runs from, read through the noun's own loader.
    pub fn creation_tables(&self) -> Result<game::creation::Tables, String> {
        let config = self.world_settings()?;
        let read = |key: &str, default: &str| {
            config
                .value_string(key, default)
                .map(|(value, _)| value)
                .map_err(|error| error.to_string())
        };
        let inventory = read("InventoryFile", "config/World/Vanilla/creation/inventory.json")?;
        let equipment = read("EquipmentFile", "config/World/Vanilla/creation/equipment.json")?;
        let worn = read("EquippedSlotsFile", "config/World/Vanilla/creation/worn_slots.json")?;
        game::creation::Tables::load(
            &inventory,
            &equipment,
            Some(std::path::Path::new(&worn)),
            &self.log,
        )
    }
}
