//! The world: the maps it hosts, and what it loads at start-up.
//!
//! Reference: `src/server/game/World/World.{h,cpp}`, whose `SetInitialWorldSettings` is the stage
//! that loads everything the game is made of before the listener opens. This is that stage at the
//! size this tree has reached: a catalogue of the maps a world can put a client in, where a
//! character may stand on each, the creation screen's content, the numbers it runs by, and the
//! district copies every session shares.
//!
//! What it is not: the map manager. A map's own state arrives with the stage that can host one.

#[path = "WorldConfig.rs"]
pub mod world_config;

use common::logging::{Log, LogLevel};
use std::sync::{Arc, Mutex};

/// What kind of place a map is. The numbers are the client's, not this server's.
pub mod map_kind {
    /// A town or outpost.
    pub const OUTPOST: u8 = 0;
    /// An explorable area.
    pub const EXPLORABLE: u8 = 1;
    /// The character-creation instance: the one destination that is a conversation rather than a
    /// place, and the only one a world can serve without geometry.
    pub const CREATION: u8 = 11;
}

/// A travel another session's crossing asked this player's session to perform. The destination is
/// resolved once by the crossing, and every party member runs the same redirect on its own socket.
#[derive(Debug, Clone)]
pub struct TravelOrder {
    /// The map every traveller in the party is sent to.
    pub destination: MapEntry,
    /// The doorway's landing point, when the crossing stated one.
    pub landing: Option<(f32, f32)>,
    /// The destination district, the same for every member of the crossing so one copy holds them.
    pub district: u32,
}

/// One map this world can put a client in.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    /// The client's own id for the map.
    pub id: u32,
    /// Which kind of place it is, from [`map_kind`], or `None` for a record the client can resolve and
    /// this world does not host: the pack states a type only for a map it can put a client in
    /// (`ASURA/crates/asura-config/src/world.rs:41-44`, `catalogue::load`).
    pub kind: Option<u8>,
    /// The `Gw.dat` file id the client loads the map from (`0x0196`).
    pub file_id: u32,
    /// What the map is called, for a log line and the administration tool.
    pub name: String,
    /// The value the client resolves this map's destination by, in `0x01A0`. `None` means the map is
    /// left out of the instance table - a wrong value is worse than a destination the client cannot
    /// resolve yet (`ASURA/crates/asura-world/src/instance_table.rs:12-13`, `44-42`).
    pub instance_value: Option<u32>,
    /// The client's string id for the map's name, used by the travel countdown (`0x01BC`). `None`
    /// when the map file states none; the countdown then has no name to draw.
    pub name_id: Option<u32>,
}

impl MapEntry {
    /// Whether this entry is the character-creation instance.
    pub fn is_creation(&self) -> bool {
        self.kind == Some(map_kind::CREATION)
    }

    /// Whether this world can put a client in this map at all.
    ///
    /// `ASURA/crates/asura-world/src/catalogue.rs:54-63`, `entry`: a record with no archive file is not
    /// hosted, **except** the character-creation instance, which is a conversation rather than a place
    /// and needs no geometry. A record with no type is not hosted either - it is a destination the
    /// client can name (`0x01A0`) and this world cannot load.
    pub fn is_hostable(&self) -> bool {
        match self.kind {
            None => false,
            Some(kind) => self.file_id != 0 || kind == map_kind::CREATION,
        }
    }

    /// How the map is named in an answer or a log line: its name when it has one, its id when it does
    /// not, and never an empty string.
    ///
    /// `ASURA/crates/asura-world/src/catalogue.rs:120-129`, `MapEntry::label`.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("map {}", self.id)
        } else {
            format!("{} ({})", self.name, self.id)
        }
    }

    /// What kind of place it is, in words: `0` and `1` are the reference's own `MapType` values, and
    /// every other value it can carry is a kind of instance this world does not host.
    ///
    /// `ASURA/crates/asura-world/src/catalogue.rs:144-152`, `MapEntry::kind`.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            Some(map_kind::OUTPOST) => "outpost",
            Some(map_kind::EXPLORABLE) => "explorable",
            Some(_) => "other",
            // A record with no type is a destination the client can resolve and this world cannot
            // serve (`ASURA/crates/asura-config/src/world.rs:41-44`).
            None => "destination only",
        }
    }

    /// The alias `/tp` would take for this map: the reference's aliases are lower-case names with
    /// single spaces ("lakeside county"), so this is the name lower-cased. An unnamed map has no alias
    /// to type and says so, rather than inventing one.
    ///
    /// `ASURA/crates/asura-world/src/catalogue.rs:134`.
    pub fn alias(&self) -> String {
        let alias = self.name.to_lowercase();
        let collapsed = alias.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            format!("map {}", self.id)
        } else {
            collapsed
        }
    }
}

/// The world a program represents.
#[derive(Default)]
pub struct World {
    realm: u32,
    maps: Vec<MapEntry>,
    /// The chat command surface: the names a player may type and the operation each runs, read from
    /// `config/Server/commands.json` (`docs/decisions/0011-the-command-surface-is-content.md`).
    commands: crate::chat::commands::Table,
    /// The operations every surface reaches (`game/src/Chat/Operations.rs`). The program builds the
    /// registry before the rows are read, so a row naming an operation this world does not have is
    /// refused at start-up rather than at a player's expense.
    operations: std::sync::Arc<crate::chat::operations::Registry>,
    /// The bags, items and profession sets a character-creation screen is built from.
    creation: crate::creation::Tables,
    /// Captured vanilla title metadata read from this world's pack.
    titles: crate::characters::titles::VanillaTitles,
    /// Where a character may stand on entry, per map (R13).
    spawns: Vec<crate::maps::MapSpawn>,
    /// Where a portal crossing puts a character, per destination and source map (R21).
    arrivals: Vec<crate::maps::Arrival>,
    /// The numbers this world runs by: a district's allotment, a party's cap, the spawn scatter and the
    /// rest of the rules that name a number (docs/game-rules.md, R53).
    settings: world_config::WorldConfig,
    /// The running copies of maps: one outpost is one copy per district (R1, R4), and the district a
    /// character enters is this world's state, so every session shares it.
    districts: Arc<Mutex<crate::districts::Districts>>,
    /// The item templates the world database holds, read once at start-up: what a load resolves a
    /// character's held items against.
    templates: Vec<crate::data_stores::ItemDefinition>,
    /// The path graphs this world serves, one finder per map: a walk is answered from these, and a map
    /// with none answers `NoGraph` (`ASURA/crates/asura-world/src/pathfind.rs:1484`, `PathGraphs`).
    graphs: std::sync::Arc<crate::terrain::pathfind::PathGraphs>,
    /// The doorways this world judges a step against, one arch list per map, and the arrivals a
    /// crossing lands on (game/Maps/Portals.rs, ASURA/crates/asura-world/src/portals.rs).
    portals: std::sync::Arc<crate::maps::Portals>,
    /// The players loaded in this world's running copies, shared by every session: a copy of a map is a
    /// place other players can be in, and a connection is told about them (`game/Districts/Peers.rs`,
    /// `ASURA/crates/asura-world/src/instance.rs:406-473`).
    peers: Arc<Mutex<crate::districts::Peers>>,
    /// The parties of every running copy, shared the way the peers are: who leads whom, and which
    /// invitations are open (`game/Parties/mod.rs`, `ASURA/crates/asura-world/src/party.rs`).
    parties: Arc<Mutex<crate::parties::Parties>>,
    /// Travels another session's crossing asked a player's own session to perform. A party crosses
    /// through one portal together, but each member's redirect and hand-off row belong to its own
    /// connection (`ASURA/crates/asura-world/src/travel.rs:740-817`, `portal_crossed`/`begin_move`).
    travel_orders: Arc<Mutex<std::collections::HashMap<u32, TravelOrder>>>,
    /// The rosters a travel carries into a destination map, by ticket. The first member to arrive
    /// adopts it; the others are already in it, absent until their own connection lands. Keyed by
    /// ticket, not district: an outpost's final district is chosen by the destination, not the sender.
    held_parties: Arc<Mutex<std::collections::HashMap<u64, (u32, crate::parties::Party)>>>,
    /// The next held-party ticket.
    next_held_party: std::sync::atomic::AtomicU64,
    /// Countdown tickets a leader cancelled. The timer thread checks this, not the party table:
    /// the table can have moved or been tidied by the time the ten seconds are up.
    cancelled_countdowns: Arc<Mutex<std::collections::HashSet<u64>>>,
    /// How many players a party may hold, per map, from the pack's `party_sizes.json`
    /// (`ASURA/crates/asura-world/src/party.rs:46-108`). A map the file does not list falls back to the
    /// world's own `PartySizeCap` (R15) - see [`Self::max_party_size`].
    party_sizes: std::collections::HashMap<u32, u32>,
    /// The scripts this world runs, shared the way the copies are: the program registers them before
    /// the world serves anything, and the world's own moments raise them
    /// (`game/Scripting/mod.rs`, the reference's `sScriptMgr`).
    scripts: Arc<Mutex<crate::scripting::ScriptMgr>>,
    /// The pack's experience curve (`experience.json`, `game/Characters/Experience.rs`), read once at
    /// start-up. `None` in a world that has not read it; a change of experience is then refused, as
    /// ASURA refuses it (`ASURA/crates/asura-world/src/player.rs:477-481`).
    experience: Option<crate::characters::experience::Curve>,
    /// The guild settings (`config/Server/guilds.json`, `game/Guilds/mod.rs`), read once at start-up.
    /// `None` in a world that has not read them; every guild request is then refused, as ASURA refuses
    /// them (`ASURA/crates/asura-world/src/guild.rs:86-95`).
    guilds: Option<crate::guilds::Settings>,
    /// How long it has been since the copies were last swept: the world's own slower timer, the
    /// reference's `WorldUpdateTimer` arrangement - the program's loop says how much time passed and
    /// this decides whether a thing that does not run every tick is due (`World.cpp:1131`).
    sweep_elapsed: Mutex<std::time::Duration>,
    /// Why the world was asked to stop, once it was: the reference's `World::m_stopEvent` and
    /// `World::GetExitCode()` (`World.cpp`, `World.h`). `None` until something asks.
    stop: std::sync::atomic::AtomicU8,
}

/// Read a `party_sizes.json` text: every `maps` key a map id, every value an object with a `value`.
fn parse_party_sizes(text: &str) -> Result<std::collections::HashMap<u32, u32>, String> {
    let document: serde_json::Value =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    let maps = document
        .get("maps")
        .and_then(serde_json::Value::as_object)
        .ok_or("it has no \"maps\" object")?;
    let mut sizes = std::collections::HashMap::new();
    for (key, entry) in maps {
        let map_id: u32 = key
            .parse()
            .map_err(|_| format!("the key \"{key}\" is not a map id"))?;
        let size = entry
            .get("value")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or(format!("map {map_id} states no positive \"value\""))?;
        sizes.insert(map_id, size);
    }
    Ok(sizes)
}

/// Why a world stopped, as the exit code the reference's `main` returns.
///
/// Reference: `src/server/apps/worldserver/Main.cpp:434-439` - `0` a normal shutdown, `1` shutdown at
/// error, `2` a restart was commanded, and the comment there says the code exists so an external
/// restarter can act on it. The program does not restart itself; the supervisor reads this code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// A normal shutdown: the operator asked, or the signal arrived.
    Normal = 0,
    /// A shutdown because something failed.
    Failure = 1,
    /// A restart was asked for; the process exits and whoever supervises it starts it again.
    Restart = 2,
}

/// The setting that records no stop request.
const NO_STOP: u8 = 0xFF;

/// The world's clock, as the district rows are stamped with it: the epoch second the hold is
/// measured against (`unix_now` in the programs is the same reading).
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The outpost this world starts a fresh character in: `wanted` when the world hosts that row, and the
/// lowest-id outpost it hosts when it does not.
///
/// R12 gives a new character exactly one unlock; R14 places a load at the character's last outpost. The
/// number is a setting of a world (this tree's `World.StartingOutpost`) and the list is the catalogue
/// the world hosts, which is why the rule takes both rather than reading them.
pub fn first_outpost(maps: &[MapEntry], wanted: u32) -> Option<&MapEntry> {
    if let Some(map) = maps
        .iter()
        .find(|map| map.id == wanted && map.is_hostable())
    {
        return Some(map);
    }
    maps.iter()
        .filter(|map| map.kind == Some(map_kind::OUTPOST))
        .min_by_key(|map| map.id)
}

impl World {
    /// Read this pack's title records before opening the listener.
    pub fn load_titles(
        &mut self,
        path: impl AsRef<std::path::Path>,
        log: &Log,
    ) -> Result<(), String> {
        let path = path.as_ref();
        self.titles = crate::characters::titles::VanillaTitles::load(path)
            .inspect_err(|error| log.out(LogLevel::Error, "server.worldserver", error))?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Loaded {} title track(s) from {}.",
                self.titles.track_count(),
                path.display()
            ),
        );
        Ok(())
    }

    /// Read the pack's experience curve (`experience.json`, copied from ASURA's file of the same name).
    /// A missing or malformed file refuses the start, as every reader of this tree's content does
    /// (`AGENTS.md`, rule 7); ASURA logs it and refuses each change instead
    /// (`ASURA/crates/asura-world/src/experience.rs:52-89`).
    pub fn load_experience(
        &mut self,
        path: impl AsRef<std::path::Path>,
        log: &Log,
    ) -> Result<(), String> {
        let path = path.as_ref();
        let curve = crate::characters::experience::load(path)
            .inspect_err(|error| log.out(LogLevel::Error, "server.worldserver", error))?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Experience curve from {}: level 2 at {}, level {} at {}.",
                path.display(),
                curve.threshold(2),
                curve.max_level,
                curve.max_total()
            ),
        );
        self.experience = Some(curve);
        Ok(())
    }

    /// The experience curve in force, when the world read one.
    pub fn experience(&self) -> Option<&crate::characters::experience::Curve> {
        self.experience.as_ref()
    }

    /// Read the guild settings (`config/Server/guilds.json`, copied from ASURA's file of the same name).
    /// A missing or malformed file refuses the start (`AGENTS.md`, rule 7).
    pub fn load_guilds(
        &mut self,
        path: impl AsRef<std::path::Path>,
        log: &Log,
    ) -> Result<(), String> {
        let path = path.as_ref();
        let settings = crate::guilds::Settings::load(path)
            .inspect_err(|error| log.out(LogLevel::Error, "server.worldserver", error))?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Guild settings from {}: a new guild's 0x0133 starts at {:#06X} and moves by {:#06X}; creation and invitations are free (the prices {} and {} are placeholders).",
                path.display(),
                settings.revision_on_create,
                settings.revision_step,
                settings.creation_price,
                settings.invite_fee
            ),
        );
        self.guilds = Some(settings);
        Ok(())
    }

    /// The guild settings in force, when the world read them.
    pub fn guilds(&self) -> Option<&crate::guilds::Settings> {
        self.guilds.as_ref()
    }

    /// Read the pack's party sizes (`party_sizes.json`, ported from ASURA's pack file of the same name):
    /// `{"maps": {"<map id>": {"value": <size>, "doc": ...}}}`. A missing or malformed file refuses the
    /// start, as every reader of this tree's content does (`AGENTS.md`, rule 7); ASURA logs it and lets no
    /// party grow (`party.rs:84-104`).
    pub fn load_party_sizes(
        &mut self,
        path: impl AsRef<std::path::Path>,
        log: &Log,
    ) -> Result<usize, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{} is not readable: {error}", path.display()))?;
        self.party_sizes = parse_party_sizes(&text)
            .map_err(|error| format!("{} is not a party sizes file: {error}", path.display()))?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Party sizes: {} map(s) from {}; every other map takes PartySizeCap {} (R15).",
                self.party_sizes.len(),
                path.display(),
                self.settings.party_size_cap
            ),
        );
        Ok(self.party_sizes.len())
    }

    /// The most players a party may hold on a map: the pack's own size for it, or the world's
    /// `PartySizeCap` (R15) when the pack does not list it. ASURA answers `None` for an unlisted map, so
    /// no party grows there (`party.rs:104-108`); this tree's owner rule R15 makes the cap a world setting
    /// whose default is 8, so an unlisted map takes it instead.
    pub fn max_party_size(&self, map_id: u32) -> Option<u32> {
        Some(
            self.party_sizes
                .get(&map_id)
                .copied()
                .unwrap_or(self.settings.party_size_cap),
        )
    }

    /// The parties of every running copy (`game/Parties/mod.rs`).
    pub fn parties(&self) -> Arc<Mutex<crate::parties::Parties>> {
        Arc::clone(&self.parties)
    }

    /// Ask another player's session to run the same travel, as a party crossing does.
    pub fn queue_travel(&self, player_id: u32, order: TravelOrder) {
        if let Ok(mut orders) = self.travel_orders.lock() {
            orders.insert(player_id, order);
        }
    }

    /// Take the travel this player's session has been asked to run, once.
    pub fn take_travel_order(&self, player_id: u32) -> Option<TravelOrder> {
        self.travel_orders
            .lock()
            .ok()
            .and_then(|mut orders| orders.remove(&player_id))
    }

    /// Carry a party roster into a destination map. The first member to arrive adopts it. The ticket
    /// is what a later member would need to name it; this tree finds it by membership.
    pub fn hold_party(&self, map_id: u32, _district: u32, party: crate::parties::Party) {
        let ticket = self
            .next_held_party
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        if let Ok(mut held) = self.held_parties.lock() {
            eprintln!(
                "world: party: held roster {ticket} for map {map_id}: members {:?}",
                party.members
            );
            held.insert(ticket, (map_id, party));
        }
    }

    /// Take the carried roster when this player is one of its members; a stranger's arrival leaves it
    /// for the party it belongs to. The destination district is chosen on arrival, so the lookup is by
    /// map and membership, never by the sender's guessed district.
    pub fn take_held_party(
        &self,
        map_id: u32,
        _district: u32,
        player_id: u32,
    ) -> Option<crate::parties::Party> {
        let mut held = self.held_parties.lock().ok()?;
        let ticket = held.iter().find_map(|(ticket, (map, party))| {
            (*map == map_id && party.members.contains(&player_id)).then_some(*ticket)
        })?;
        eprintln!("world: party: player {player_id} adopts held roster {ticket} on map {map_id}");
        held.remove(&ticket).map(|(_, party)| party)
    }

    /// The leader cancelled this countdown; the timer thread must not move the party.
    pub fn cancel_countdown_ticket(&self, ticket: u64) {
        if let Ok(mut cancelled) = self.cancelled_countdowns.lock() {
            cancelled.insert(ticket);
        }
    }

    /// Whether this countdown was cancelled, consuming the marker.
    pub fn take_cancelled_countdown(&self, ticket: u64) -> bool {
        self.cancelled_countdowns
            .lock()
            .ok()
            .is_some_and(|mut cancelled| cancelled.remove(&ticket))
    }

    /// Vanilla titles available to each load in this world.
    pub fn titles(&self) -> &crate::characters::titles::VanillaTitles {
        &self.titles
    }

    /// An empty world, before its settings are read.
    pub fn new(realm: u32) -> Self {
        Self {
            realm,
            maps: Vec::new(),
            commands: crate::chat::commands::Table::empty(),
            operations: std::sync::Arc::new(crate::chat::operations::Registry::new()),
            creation: crate::creation::Tables::default(),
            titles: crate::characters::titles::VanillaTitles::default(),
            spawns: Vec::new(),
            arrivals: Vec::new(),
            settings: world_config::WorldConfig::default(),
            districts: Arc::new(Mutex::new(crate::districts::Districts::new())),
            templates: Vec::new(),
            graphs: std::sync::Arc::new(crate::terrain::pathfind::PathGraphs::empty()),
            portals: std::sync::Arc::new(crate::maps::Portals::empty()),
            peers: Arc::new(Mutex::new(crate::districts::Peers::new())),
            parties: Arc::new(Mutex::new(crate::parties::Parties::new())),
            travel_orders: Arc::new(Mutex::new(std::collections::HashMap::new())),
            held_parties: Arc::new(Mutex::new(std::collections::HashMap::new())),
            next_held_party: std::sync::atomic::AtomicU64::new(0),
            cancelled_countdowns: Arc::new(Mutex::new(std::collections::HashSet::new())),
            party_sizes: std::collections::HashMap::new(),
            scripts: Arc::new(Mutex::new(crate::scripting::ScriptMgr::new())),
            experience: None,
            guilds: None,
            sweep_elapsed: Mutex::new(std::time::Duration::ZERO),
            stop: std::sync::atomic::AtomicU8::new(NO_STOP),
        }
    }

    /// Ask the world to stop, and say why. Reference: `World::StopNow(code)` (`World.cpp`).
    ///
    /// The first request wins: a later one does not rewrite the exit code, which is what the
    /// reference's `StopNow` does behind its own guard.
    pub fn request_stop(&self, code: ExitCode) {
        let _ = self.stop.compare_exchange(
            NO_STOP,
            code as u8,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Whether something asked the world to stop.
    pub fn stop_requested(&self) -> bool {
        self.stop.load(std::sync::atomic::Ordering::Relaxed) != NO_STOP
    }

    /// The exit code this world answers with: the reference's `World::GetExitCode()`, `0` when it was
    /// stopped normally or not stopped at all.
    pub fn exit_code(&self) -> ExitCode {
        match self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            x if x == ExitCode::Failure as u8 => ExitCode::Failure,
            x if x == ExitCode::Restart as u8 => ExitCode::Restart,
            _ => ExitCode::Normal,
        }
    }

    /// Which world this is: the id a login names as `map_token`.
    ///
    /// Reference: the reference's `realm.Id.Realm`, read from its own configuration as `RealmID`.
    pub fn realm(&self) -> u32 {
        self.realm
    }

    /// The maps this world hosts.
    pub fn maps(&self) -> &[MapEntry] {
        &self.maps
    }

    /// The chat command surface: the names a player may type, and the operation each one runs.
    ///
    /// Read from `config/Server/commands.json` at start-up (`docs/decisions/0011-the-command-surface-is-content.md`);
    /// a world with no such file answers no command and said so when it started.
    pub fn commands(&self) -> &crate::chat::commands::Table {
        &self.commands
    }

    /// The operations every surface reaches: the registry the rows bind their names to.
    pub fn operations(&self) -> &std::sync::Arc<crate::chat::operations::Registry> {
        &self.operations
    }

    /// Give the world its registrations. The program builds them, names the same `Arc` for the console
    /// and the control endpoint, and the rows are then read against them.
    pub fn set_operations(
        &mut self,
        operations: std::sync::Arc<crate::chat::operations::Registry>,
    ) {
        self.operations = operations;
    }

    /// Read the chat command table.
    ///
    /// A file that is present but wrong is an error - an operation this world does not register, a name
    /// stated twice, a parameter list the operation does not declare - because a command that cannot
    /// work is worse than one that does not exist. A file that is missing leaves the table empty and is
    /// reported once.
    pub fn load_commands(
        &mut self,
        path: impl AsRef<std::path::Path>,
        log: &Log,
    ) -> Result<usize, String> {
        self.commands = crate::chat::commands::Table::load(path, log, &self.operations)?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Chat commands: {} name(s) read: {}.",
                self.commands.len(),
                self.commands
                    .rows()
                    .iter()
                    .map(|row| row.names().collect::<Vec<_>>().join("/"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        Ok(self.commands.len())
    }

    /// Give the world its command table directly: what a test does.
    pub fn set_commands(&mut self, commands: crate::chat::commands::Table) {
        self.commands = commands;
    }
    /// The item templates this world runs with.
    pub fn templates(&self) -> &[crate::data_stores::ItemDefinition] {
        &self.templates
    }

    /// The map's walkable graph, as a walk request is answered from.
    ///
    /// `ASURA/crates/asura-world/src/pathfind.rs:1484`, `PathGraphs`: one finder per map, held for the
    /// life of the program. A map whose graph was never bound answers `Walk::NoGraph` - the working
    /// implementation's own answer for a map it holds no graph for.
    pub fn graphs(&self) -> &std::sync::Arc<crate::terrain::pathfind::PathGraphs> {
        &self.graphs
    }

    /// The doorways this world judges a step against.
    ///
    /// `ASURA/crates/asura-world/src/portals.rs:313-377`: the pack's table, read once at start-up. A
    /// map the rows describe is judged by its own walls, and a step that crosses none of them crosses
    /// nothing.
    pub fn portals(&self) -> &std::sync::Arc<crate::maps::Portals> {
        &self.portals
    }

    /// Install the doorway table, once, before the listener opens.
    pub fn set_portals(&mut self, portals: crate::maps::Portals) {
        self.portals = std::sync::Arc::new(portals);
    }

    /// Install the graphs this world serves, once, before the listener opens.
    ///
    /// `ASURA/crates/asura-worldserver/src/main.rs:366-391`: the graphs are built before the world
    /// accepts anyone, from a dump when one is named and from the client archive otherwise, and every
    /// walk afterwards is answered from them.
    pub fn set_graphs(&mut self, graphs: crate::terrain::pathfind::PathGraphs) {
        self.graphs = std::sync::Arc::new(graphs);
    }

    /// The players loaded in this world's running copies.
    ///
    /// `ASURA/crates/asura-world/src/instance.rs:474-499`: the working implementation keeps one
    /// `Instance` per map and every session shares it. This tree's copy of a map is a district
    /// (`game/Districts/`), and the population is the same kind of shared state, so it lives beside it.
    pub fn peers(&self) -> Arc<Mutex<crate::districts::Peers>> {
        Arc::clone(&self.peers)
    }

    /// Set the item templates, after the program read them from its world database.
    pub fn set_templates(&mut self, templates: Vec<crate::data_stores::ItemDefinition>) {
        self.templates = templates;
    }

    /// The numbers this world runs by.
    pub fn settings(&self) -> &world_config::WorldConfig {
        &self.settings
    }

    /// The running district copies, shared by every session.
    pub fn districts(&self) -> Arc<Mutex<crate::districts::Districts>> {
        Arc::clone(&self.districts)
    }

    /// The outpost this world starts a fresh character in, and unlocks for it (R12, R14).
    ///
    /// `World.StartingOutpost` when this world hosts that row, and the lowest-id outpost it hosts when
    /// it does not - a world whose configured outpost is missing still has to name one, and which one
    /// it chose is the program's to say. The rule itself is `first_outpost`, because a session carries
    /// a copy of the maps rather than the world and knows the number from its settings.
    pub fn starting_outpost(&self) -> Option<&MapEntry> {
        first_outpost(&self.maps, self.settings.starting_outpost)
    }

    /// The scripts this world runs (`game/Scripting/mod.rs`).
    pub fn scripts(&self) -> Arc<Mutex<crate::scripting::ScriptMgr>> {
        Arc::clone(&self.scripts)
    }

    /// Take the scripts the program registered, before the world serves anything.
    ///
    /// Reference: the reference's manager is a singleton built at start-up (`Main.cpp:272-273` set
    /// the loaders, `ScriptMgr::Initialize` runs them); this tree's is a value, and the world it
    /// belongs to takes it here.
    pub fn set_scripts(&mut self, scripts: crate::scripting::ScriptMgr) {
        self.scripts = Arc::new(Mutex::new(scripts));
    }

    /// The world is started: every script that declared the hook is told.
    ///
    /// Reference: `sScriptMgr->OnStartup()` once the world is ready (`Main.cpp:398`).
    pub fn on_startup(&self) {
        if let Ok(mut scripts) = self.scripts.lock() {
            scripts.on_startup();
        }
    }

    /// The world is shutting down: every script that declared the hook is told.
    ///
    /// Reference: `sScriptMgr->OnShutdown()` during shutdown (`Main.cpp:426`).
    pub fn on_shutdown(&self) {
        if let Ok(mut scripts) = self.scripts.lock() {
            scripts.on_shutdown();
        }
    }

    /// One tick of the world: the scripts are told every tick, and the copies are swept on the
    /// world's own slower timer - the ones that have been empty past their hold are disposed (R37,
    /// R40) and named, so the program can say which went.
    ///
    /// Reference: `World::Update(uint32 diff)` (`src/server/game/World/World.cpp:1131`). The program's
    /// loop calls it with how long it has been (`WorldUpdateLoop`, `Main.cpp:584-620`), its own
    /// timers decide inside whether a slower thing is due, and the scripts' world-update hook is
    /// raised at `:1355`. The sweep is this world's slower thing: `World.SweepIntervalMs` between
    /// checks, measured by the time that has passed rather than by a thread's own sleep.
    ///
    /// `World.InstanceTick` off is a world without that clock: it keeps every copy it opened and says
    /// so in its configuration dump (R33).
    pub fn update(&self, diff: std::time::Duration) -> Vec<(u32, u32)> {
        if let Ok(mut scripts) = self.scripts.lock() {
            scripts.on_world_update(diff);
        }
        if !self.settings.instance_tick {
            return Vec::new();
        }
        let period = std::time::Duration::from_millis(u64::from(self.settings.sweep_interval_ms));
        let due = match self.sweep_elapsed.lock() {
            Ok(mut elapsed) => {
                *elapsed += diff;
                if *elapsed >= period {
                    *elapsed = std::time::Duration::ZERO;
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        if !due {
            return Vec::new();
        }
        let hold = u64::from(self.settings.explorable_disconnect_hold);
        match self.districts.lock() {
            Ok(mut districts) => districts.sweep(now_unix(), hold),
            Err(_) => Vec::new(),
        }
    }

    /// Give the copies their table and their log: what a program does once the world database is
    /// open and the maps are read (`docs/port/06-world-plan.md`, step 3).
    pub fn set_instances(&mut self, store: Arc<crate::districts::DatabaseInstances>, log: Log) {
        if let Ok(mut districts) = self.districts.lock() {
            districts.attach(store, log);
        }
    }

    /// Set the numbers this world runs by, after the program read its file.
    pub fn set_settings(&mut self, settings: world_config::WorldConfig) {
        self.settings = settings;
    }

    /// The creation data: the bags, the items and the profession sets.
    pub fn creation(&self) -> &crate::creation::Tables {
        &self.creation
    }

    /// Read the two creation files, and report what came from them.
    ///
    /// Reference: the same `SetInitialWorldSettings` pass that loads a world's content; a creation
    /// screen's bags and equipment are as much content as a map is.
    pub fn load_creation(
        &mut self,
        inventory_path: impl AsRef<std::path::Path>,
        equipment_path: impl AsRef<std::path::Path>,
        worn_path: Option<&std::path::Path>,
        log: &Log,
    ) -> Result<(usize, usize, usize), String> {
        self.creation =
            crate::creation::Tables::load(inventory_path, equipment_path, worn_path, log)?;
        Ok(self.creation.counts())
    }

    /// One map, by the client's id.
    pub fn map(&self, id: u32) -> Option<&MapEntry> {
        self.maps.iter().find(|map| map.id == id)
    }

    /// Read the maps, and where a character may stand on each, from the world database.
    ///
    /// Reference: the same `SetInitialWorldSettings` pass - a world that opens with no map hosts nothing,
    /// and says so rather than pretending.
    pub fn load_maps(
        &mut self,
        maps: &impl crate::maps::MapSource,
        source: &str,
        log: &Log,
    ) -> Result<usize, String> {
        let (records, spawns) = maps.load_maps()?;
        self.maps = records;
        self.spawns = spawns;
        self.arrivals = maps.arrivals()?;
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "{} map(s), {} spawn point(s) and {} arrival point(s) read from {}.",
                self.maps.len(),
                self.spawns.len(),
                self.arrivals.len(),
                source
            ),
        );
        Ok(self.maps.len())
    }

    /// Give the world its maps and their spawn points directly: what a test does, and what a program
    /// that read them some other way would do.
    pub fn set_maps(&mut self, maps: Vec<MapEntry>, spawns: Vec<crate::maps::MapSpawn>) {
        self.maps = maps;
        self.spawns = spawns;
    }

    /// Every spawn point this world holds.
    pub fn spawns(&self) -> &[crate::maps::MapSpawn] {
        &self.spawns
    }

    /// Every arrival point this world holds: where a portal crossing on a map lands (R21).
    pub fn arrivals(&self) -> &[crate::maps::Arrival] {
        &self.arrivals
    }

    /// Where a character may stand on entry, for one map (R13).
    pub fn spawns_of(&self, map_id: u32) -> Vec<crate::maps::MapSpawn> {
        self.spawns
            .iter()
            .filter(|spawn| spawn.map_id == map_id)
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    /// The pack's own file: pre-Searing holds two players to a party, ASURA's sixteen maps
    /// (`ASURA/crates/asura-world/src/party.rs:1953-1965`), and an unlisted map takes R15's cap.
    #[test]
    fn the_vanilla_pack_caps_every_pre_searing_map_at_two() {
        let text = include_str!("../../../config/World/Vanilla/party_sizes.json");
        let sizes = parse_party_sizes(text).expect("the pack's party sizes");
        assert_eq!(sizes.len(), 16);
        for map in [
            145, 146, 147, 148, 149, 150, 151, 160, 161, 162, 163, 164, 165, 166, 877, 878,
        ] {
            assert_eq!(sizes.get(&map), Some(&2), "map {map}");
        }
        let mut world = World::new(1);
        world.party_sizes = sizes;
        assert_eq!(world.max_party_size(148), Some(2));
        assert_eq!(
            world.max_party_size(449),
            Some(8),
            "R15's cap for a map the file does not list"
        );
        assert!(parse_party_sizes("{\"maps\": {\"x\": {\"value\": 2}}}").is_err());
        assert!(parse_party_sizes("{\"maps\": {\"148\": {\"value\": 0}}}").is_err());
        assert!(parse_party_sizes("{}").is_err());
    }

    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// The exit-code contract the reference's `main` states (`Main.cpp:434-439`): a stop is asked for
    /// once, the first request decides the code, and a world that was never asked answers `0`.
    #[test]
    fn a_stop_is_requested_once_and_the_first_code_stands() {
        let world = World::new(7);
        assert!(!world.stop_requested());
        assert_eq!(
            world.exit_code(),
            ExitCode::Normal,
            "a world nobody stopped answers 0"
        );

        world.request_stop(ExitCode::Restart);
        assert!(world.stop_requested());
        assert_eq!(world.exit_code(), ExitCode::Restart);

        world.request_stop(ExitCode::Normal);
        assert_eq!(
            world.exit_code(),
            ExitCode::Restart,
            "the first request wins"
        );
    }

    /// A script that writes down the world's moments it was told about, in order.
    struct Witness {
        name: String,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl crate::scripting::ScriptObject for Witness {
        fn name(&self) -> &str {
            &self.name
        }
    }

    impl crate::scripting::WorldScript for Witness {
        fn on_startup(&mut self) {
            self.seen.lock().unwrap().push("startup".to_string());
        }

        fn on_update(&mut self, diff: Duration) {
            self.seen
                .lock()
                .unwrap()
                .push(format!("update:{}", diff.as_millis()));
        }

        fn on_shutdown(&mut self) {
            self.seen.lock().unwrap().push("shutdown".to_string());
        }
    }

    /// The route the reference has and this tree now has too: the world raises its own moments
    /// (`sScriptMgr->OnStartup()` at `Main.cpp:398`, `OnWorldUpdate(diff)` from `World::Update` at
    /// `World.cpp:1355`, `OnShutdown()` at `Main.cpp:426`) and a registered script is what answers
    /// them, while a script that did not declare a hook is not called for it.
    #[test]
    fn the_world_raises_its_moments_to_the_scripts_that_declared_them() {
        let mut world = World::new(1);
        let mut scripts = crate::scripting::ScriptMgr::new();
        let tuned: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let deaf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        scripts
            .add_world_script(
                Box::new(Witness {
                    name: "a-tuned".to_string(),
                    seen: Arc::clone(&tuned),
                }),
                &[
                    crate::scripting::WorldHook::OnStartup,
                    crate::scripting::WorldHook::OnUpdate,
                ],
            )
            .unwrap();
        scripts
            .add_world_script(
                Box::new(Witness {
                    name: "a-deaf-one".to_string(),
                    seen: Arc::clone(&deaf),
                }),
                &[crate::scripting::WorldHook::OnShutdown],
            )
            .unwrap();
        world.set_scripts(scripts);

        world.on_startup();
        world.update(Duration::from_millis(1000));
        world.on_shutdown();

        assert_eq!(
            *tuned.lock().unwrap(),
            vec!["startup".to_string(), "update:1000".to_string()]
        );
        assert_eq!(*deaf.lock().unwrap(), vec!["shutdown".to_string()]);
    }
    /// The world's own slower timer, which the program no longer keeps: a copy is swept only once
    /// `World.SweepIntervalMs` of passed time has accumulated, and the world reads the clock itself
    /// when it fires (`World::update`, the reference's `WorldUpdateTimer` arrangement at
    /// `World.cpp:1131`). `InstanceTick` off is a world with no such timer at all (R33).
    #[test]
    fn the_world_sweeps_its_copies_once_the_interval_has_passed() {
        let mut world = World::new(148);
        world.set_settings(crate::world::world_config::WorldConfig {
            sweep_interval_ms: 100,
            // Nothing is held: what the sweep is asked about is whether it *fires*, and a copy that
            // was entered and left is empty the moment it is left.
            explorable_disconnect_hold: 0,
            ..crate::world::world_config::WorldConfig::default()
        });
        let outpost = MapEntry {
            id: 148,
            kind: Some(map_kind::OUTPOST),
            file_id: 113_021,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: Some(0xBB14_F539),
            name_id: None,
        };
        {
            let districts = world.districts();
            let mut districts = districts.lock().unwrap();
            districts.enter(&outpost, 0, 0, 100).unwrap();
            districts.leave(148, 1, 0);
        }

        // Half of the interval: the scripts are told, and nothing is swept yet.
        assert!(world.update(Duration::from_millis(50)).is_empty());
        // The second half completes it, and the copy nobody is inside is disposed and named.
        assert_eq!(world.update(Duration::from_millis(50)), vec![(148, 1)]);
        // And the timer starts again: the next half-interval sweeps nothing.
        assert!(world.update(Duration::from_millis(50)).is_empty());
    }

    /// `InstanceTick` off keeps every copy: a world without the clock does not sweep (R33).
    #[test]
    fn a_world_without_the_instance_tick_never_sweeps() {
        let mut world = World::new(148);
        world.set_settings(crate::world::world_config::WorldConfig {
            instance_tick: false,
            sweep_interval_ms: 1,
            explorable_disconnect_hold: 0,
            ..crate::world::world_config::WorldConfig::default()
        });
        let outpost = MapEntry {
            id: 148,
            kind: Some(map_kind::OUTPOST),
            file_id: 113_021,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: Some(0xBB14_F539),
            name_id: None,
        };
        {
            let districts = world.districts();
            let mut districts = districts.lock().unwrap();
            districts.enter(&outpost, 0, 0, 100).unwrap();
            districts.leave(148, 1, 0);
        }
        assert!(world.update(Duration::from_millis(1000)).is_empty());
        assert_eq!(
            world.districts().lock().unwrap().count_of(148),
            1,
            "the copy is still held"
        );
    }

    /// R12/R14: the outpost a fresh character starts in is the configured one when the world hosts it,
    /// and the lowest-id outpost it hosts otherwise - a world whose configured row is missing still has
    /// to name one.
    #[test]
    fn the_starting_outpost_is_the_configured_one_or_the_lowest_the_world_hosts() {
        let outpost = |id: u32| MapEntry {
            id,
            kind: Some(map_kind::OUTPOST),
            file_id: id * 100,
            name: format!("outpost {id}"),
            instance_value: Some(id),
            name_id: None,
        };
        let explorable = |id: u32| MapEntry {
            id,
            kind: Some(map_kind::EXPLORABLE),
            file_id: id * 100,
            name: format!("explorable {id}"),
            instance_value: Some(id),
            name_id: None,
        };
        let mut world = World::new(1);
        world.set_maps(
            vec![explorable(146), outpost(148), outpost(149)],
            Vec::new(),
        );
        world.set_settings(crate::world::world_config::WorldConfig {
            starting_outpost: 149,
            ..crate::world::world_config::WorldConfig::default()
        });
        assert_eq!(
            world.starting_outpost().map(|map| map.id),
            Some(149),
            "the configured outpost, when the world hosts it"
        );

        world.set_settings(crate::world::world_config::WorldConfig {
            starting_outpost: 999,
            ..crate::world::world_config::WorldConfig::default()
        });
        assert_eq!(
            world.starting_outpost().map(|map| map.id),
            Some(148),
            "the lowest-id outpost the world hosts, when the configured one is missing"
        );

        world.set_maps(vec![explorable(146)], Vec::new());
        assert!(
            world.starting_outpost().is_none(),
            "and a world with no outpost names none"
        );
    }

    /// A map is named the way the working implementation names it: its name and its id, its id alone
    /// when it has no name, and never an empty string (`catalogue.rs:120-152`).
    #[test]
    fn a_map_is_named_as_the_working_implementation_names_it() {
        let named = MapEntry {
            id: 148,
            kind: Some(map_kind::OUTPOST),
            file_id: 113_021,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: Some(0xBB14_F539),
            name_id: None,
        };
        assert_eq!(named.label(), "Ascalon City (pre-Searing) (148)");
        assert_eq!(named.kind_name(), "outpost");
        let unnamed = MapEntry {
            id: 999,
            kind: Some(map_kind::EXPLORABLE),
            file_id: 0,
            name: String::new(),
            instance_value: None,
            name_id: None,
        };
        assert_eq!(unnamed.label(), "map 999", "an unnamed map names its id");
        assert_eq!(unnamed.kind_name(), "explorable");
        let other = MapEntry {
            id: 0,
            kind: Some(map_kind::CREATION),
            file_id: 0,
            name: "Character creation".to_string(),
            instance_value: None,
            name_id: None,
        };
        assert_eq!(
            other.kind_name(),
            "other",
            "a kind this world does not host"
        );
    }

    /// A world with two maps and one spawn point, as the database would give it.
    fn world() -> World {
        let mut world = World::new(2);
        world.set_maps(
            vec![
                MapEntry {
                    id: 0,
                    kind: Some(map_kind::CREATION),
                    file_id: 0,
                    name: "Character creation".to_string(),
                    instance_value: None,
                    name_id: None,
                },
                MapEntry {
                    id: 148,
                    kind: Some(map_kind::OUTPOST),
                    file_id: 112_509,
                    name: "Ascalon City (pre-Searing)".to_string(),
                    instance_value: Some(0xBB14_F539),
                    name_id: None,
                },
            ],
            vec![crate::maps::MapSpawn {
                map_id: 148,
                index: 0,
                x: 9541.87,
                y: 4796.46,
                plane: 0,
                angle: None,
            }],
        );
        world
    }

    #[test]
    fn the_creation_instance_is_recognised_and_an_outpost_is_not() {
        let world = world();
        assert_eq!(world.realm(), 2);
        assert!(world.map(0).unwrap().is_creation());
        assert!(!world.map(148).unwrap().is_creation());
        assert!(world.map(999).is_none());
        assert_eq!(world.map(148).unwrap().file_id, 112_509);
    }

    #[test]
    fn a_map_carries_the_points_a_character_may_stand_on() {
        let world = world();
        let spawns = world.spawns_of(148);
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].x, 9541.87);
        assert!(
            world.spawns_of(0).is_empty(),
            "the creation instance has no point to stand on"
        );
    }

    #[test]
    fn a_world_with_no_map_hosts_nothing() {
        let world = World::new(2);
        assert!(world.maps().is_empty());
        assert!(world.spawns_of(148).is_empty());
    }

    /// `ASURA/crates/asura-world/src/catalogue.rs:54-63`, `entry`: a map with no archive file is not
    /// hosted, **except** the character-creation instance; and a record the pack states no type for is
    /// a destination the client can resolve (`0x01A0`) that this world cannot load.
    #[test]
    fn only_a_record_with_a_type_and_a_blob_is_hosted() {
        let host = MapEntry {
            id: 148,
            kind: Some(map_kind::OUTPOST),
            file_id: 113_021,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: Some(0xBB14_F539),
            name_id: None,
        };
        assert!(host.is_hostable());
        let creation = MapEntry {
            id: 0,
            kind: Some(map_kind::CREATION),
            file_id: 0,
            name: "Character Creation".to_string(),
            instance_value: None,
            name_id: None,
        };
        assert!(
            creation.is_hostable(),
            "the creation instance needs no geometry"
        );
        let no_blob = MapEntry {
            file_id: 0,
            ..host.clone()
        };
        assert!(
            !no_blob.is_hostable(),
            "an outpost with no blob loads to a black screen"
        );
        let destination_only = MapEntry {
            id: 4,
            kind: None,
            file_id: 0,
            name: String::new(),
            instance_value: Some(943_034_109),
            name_id: None,
        };
        assert!(
            !destination_only.is_hostable(),
            "a record with no type is a destination the client can resolve and this world cannot load"
        );
        assert_eq!(destination_only.kind_name(), "destination only");
        assert_eq!(
            destination_only.label(),
            "map 4",
            "an unnamed map names its id"
        );
    }

    /// The catalogue's own test: every map this world hosts, walked from one of its own spawn points.
    ///
    /// This is what "the catalogue is wired" means in practice - a row with a type and an archive file,
    /// a spawn point to stand on (R13), and a path graph the finder answers from. It needs the world
    /// database and this machine's `Gw.dat`, so it is `#[ignore]`d; run it with
    /// `cargo test -p game --lib every_hosted_map -- --ignored --nocapture`.
    ///
    /// A map is counted as walkable when **any** of its own spawn points reaches any of a few nearby
    /// candidate points: a spawn outside the geometry, or a blob that does not import, is a real answer
    /// (`NoGraph`/`NoPath`) and not a test failure - the tally is the statement.
    #[test]
    #[ignore = "needs the world database and a local Gw.dat"]
    fn every_hosted_map_walks_from_one_of_its_own_spawn_points() {
        use common::configuration::{Config, Policy};
        use database::database::{DatabaseKind, read_settings};

        let log = Log::new();
        let Some(config) =
            Config::load_json("../worldserver/worldserver.json", false, Policy::default()).ok()
        else {
            panic!("the world program's own configuration reads");
        };
        let settings = read_settings(&config, DatabaseKind::World).expect("the world settings");
        let mut pool =
            database::database::DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log)
                .expect("the world pool opens");
        pool.ping().expect("the world database answers");
        let repository = crate::maps::DatabaseMaps::new(
            std::sync::Arc::new(std::sync::Mutex::new(pool)),
            log.clone(),
        );
        let (records, spawns) = repository.load().expect("the map tables read");
        let arch_list = repository.arches().expect("the arch rows read");
        let mut world = World::new(2);
        world.set_maps(records, spawns);
        world.set_portals(crate::maps::Portals::new(arch_list, 250.0));

        let (archive, _) = config
            .value_string("GwDat", "Gw.dat")
            .expect("GwDat is a string");
        let hostable: Vec<MapEntry> = world
            .maps()
            .iter()
            .filter(|map| map.is_hostable())
            .cloned()
            .collect();
        let graphs = crate::terrain::pathfind::PathGraphs::load(&archive, &hostable)
            .expect("the archive opens");

        let mut walkable = 0;
        let mut no_graph = Vec::new();
        let mut refused = Vec::new();
        for map in &hostable {
            let points: Vec<crate::maps::MapSpawn> = world
                .spawns_of(map.id)
                .into_iter()
                .filter(|spawn| spawn.plane >= 0)
                .collect();
            if points.is_empty() {
                continue;
            }
            let mut found = false;
            'outer: for point in &points {
                let from = crate::terrain::Pos {
                    x: point.x,
                    y: point.y,
                    plane: point.plane.max(0) as u16,
                };
                let mut candidates: Vec<crate::terrain::Pos> = points
                    .iter()
                    .filter(|other| other.x != point.x || other.y != point.y)
                    .map(|other| crate::terrain::Pos {
                        x: other.x,
                        y: other.y,
                        plane: other.plane.max(0) as u16,
                    })
                    .collect();
                for (dx, dy) in [
                    (10.0f32, 0.0f32),
                    (0.0, 10.0),
                    (-10.0, 0.0),
                    (0.0, -10.0),
                    (200.0, 0.0),
                    (0.0, 200.0),
                ] {
                    candidates.push(crate::terrain::Pos {
                        x: point.x + dx,
                        y: point.y + dy,
                        plane: from.plane,
                    });
                }
                for to in candidates {
                    match graphs.walk(map.id as i32, from, to) {
                        crate::terrain::Walk::Found(waypoints) if !waypoints.is_empty() => {
                            found = true;
                            break 'outer;
                        }
                        crate::terrain::Walk::NoGraph => {
                            no_graph.push(map.id);
                            break 'outer;
                        }
                        _ => {}
                    }
                }
            }
            if found {
                walkable += 1;
            } else if !no_graph.contains(&map.id) {
                refused.push(map.id);
            }
        }
        // Why a map refused: does its own spawn point land in a trapezoid at all? That separates "the
        // point is outside the geometry" from "the finder refused a route", which are different faults.
        let mut reasons: Vec<String> = Vec::new();
        if !refused.is_empty()
            && let Ok(mut archive) = crate::pack::GwDat::open(&archive)
        {
            for id in &refused {
                let Some(map) = hostable.iter().find(|map| map.id == *id) else {
                    continue;
                };
                let reason = match archive.file(map.file_id) {
                    Ok(Some(blob)) => match crate::terrain::map_data::PathStaticData::import(&blob)
                    {
                        Ok(data) => {
                            let mut lands = None;
                            for plane in 0..data.planes.len().min(256) {
                                let point = world.spawns_of(*id).first().map(|s| (s.x, s.y));
                                if let Some((x, y)) = point
                                    && data
                                        .trapezoid_at(crate::terrain::Vec2f { x, y }, plane as u16)
                                        .is_some()
                                {
                                    lands = Some(plane);
                                    break;
                                }
                            }
                            format!(
                                "{} plane(s), {} trapezoid(s), spawn lands on plane {lands:?}",
                                data.planes.len(),
                                data.traps_count
                            )
                        }
                        Err(err) => format!("the blob is refused: {err}"),
                    },
                    Ok(None) => "no such entry in the archive".to_string(),
                    Err(err) => format!("the entry cannot be read: {err}"),
                };
                reasons.push(format!("map {id}: {reason}"));
            }
        }
        let with_spawns = hostable
            .iter()
            .filter(|map| !world.spawns_of(map.id).is_empty())
            .count();
        eprintln!(
            "catalogue: {walkable} of {with_spawns} hosted map(s) with spawn points walk from one of their own points; {} answered NoGraph, {} refused every candidate ({refused:?})",
            no_graph.len(),
            refused.len()
        );
        for reason in &reasons {
            eprintln!("  {reason}");
        }
        assert!(
            walkable + no_graph.len() + refused.len() >= with_spawns,
            "every map is accounted for"
        );
        assert!(
            walkable * 10 >= with_spawns * 9,
            "at least nine in ten hosted maps walk: {walkable} of {with_spawns} (NoGraph: {no_graph:?}, refused: {refused:?})"
        );
    }

    /// What the world program does at start-up: the graphs are imported for the maps it hosts, and a
    /// walk asked through the world is answered from them - map 148 from its own spawn point to a point
    /// beside it, and a map the archive holds no graph for with `NoGraph`.
    ///
    /// `ASURA/crates/asura-worldserver/src/main.rs:366-391`: the graphs are installed before the
    /// listener opens, and every walk after that is answered from them.
    #[test]
    fn a_world_installed_with_the_archives_graphs_answers_a_walk() {
        let Some(path) = std::env::var("KRYTA_GW_DAT").ok().or_else(|| {
            let candidate = std::path::PathBuf::from("Gw.dat");
            candidate
                .exists()
                .then(|| candidate.to_string_lossy().into_owned())
        }) else {
            eprintln!("skipped: no local Gw.dat");
            return;
        };
        let mut world = world();
        world.set_maps(
            vec![
                MapEntry {
                    id: 0,
                    kind: Some(map_kind::CREATION),
                    file_id: 0,
                    name: "Character creation".to_string(),
                    instance_value: None,
                    name_id: None,
                },
                MapEntry {
                    id: 148,
                    kind: Some(map_kind::OUTPOST),
                    file_id: 0x1B97D,
                    name: "Ascalon City (pre-Searing)".to_string(),
                    instance_value: Some(0xBB14_F539),
                    name_id: None,
                },
                MapEntry {
                    id: 146,
                    kind: Some(map_kind::OUTPOST),
                    file_id: 0x1,
                    name: "a map with no graph".to_string(),
                    instance_value: None,
                    name_id: None,
                },
            ],
            world.spawns().to_vec(),
        );
        world.set_graphs(
            crate::terrain::pathfind::PathGraphs::load(&path, world.maps())
                .expect("the archive opens"),
        );
        assert_eq!(
            world.graphs().maps(),
            vec![148],
            "only the map with a blob is bound"
        );
        let from = crate::terrain::Pos {
            x: 9541.87,
            y: 4796.46,
            plane: 0,
        };
        let to = crate::terrain::Pos {
            x: 9600.0,
            y: 4850.0,
            plane: 0,
        };
        assert!(
            matches!(
                world.graphs().walk(148, from, to),
                crate::terrain::Walk::Found(_)
            ),
            "a bound map answers with a path"
        );
        assert!(
            matches!(
                world.graphs().walk(146, from, to),
                crate::terrain::Walk::NoGraph
            ),
            "a map the archive holds no graph for answers NoGraph"
        );
    }
}
