//! The gameplay conversation: the second connection, from its version message to a loaded map.
//!
//! Reference: `src/server/game/Server/WorldSocket.cpp` and `WorldSession.cpp` - the object a
//! gameplay connection becomes, and the session behind it. What happens here, in the reference's
//! order:
//!
//! 1. The client's 64-byte version message, which names the world it was sent to (`0x000C0500`).
//! 2. The seed exchange, the same one the login port makes: `0x4200` in, `0x1601` out, one key
//!    derived from the master, one RC4 per direction.
//! 3. The travel record the login wrote is claimed once, by the token pair the version message
//!    presents (`docs/port/06-world-plan.md`, "The three seams"); the record's map decides what
//!    loads, and a connection with no record keeps the map its message names.
//! 4. The block the world sends unasked. For the character-creation instance that is two messages -
//!    the load head and the creation start - because the creation instance has no geometry and no
//!    agent of its own. For a map it is the load head, the player-data start, the character's name
//!    and the instance's record, followed by the ping pair, and then the client's asks are answered
//!    in the order a working server measured (`recordings/gwps/world/load-stream.log`).
//!
//! Reference for the sizes: `shared/cryptography/handshake.rs` and `Packets/InstancePackets.rs`.

use common::logging::{Log, LogLevel};
use shared::cryptography::AuthCrypt;
use shared::cryptography::handshake::{self, CLIENT_SEED_HEADER, CLIENT_SEED_LEN, SERVER_SEED_LEN};
use shared::network::socket::{Session, SessionFuture, SessionIo};

use crate::agents::player::{PLAYER_TEAM_TOKEN, UNLOCKED_MAP_ZEROS};
use crate::handlers::character::Creation;
use crate::server::packets::{
    agent_packets, creation_load_block, guild_packets, instance_packets, misc_packets,
    party_packets, title_packets,
};
use crate::server::protocol::FrameError;
use crate::server::protocol::GameVersion;
use crate::server::protocol::tables::game::{GAME_CMSG_TITLE_DISPLAY, GAME_CMSG_TITLE_HIDE};
use crate::world::World;

/// What one step of the map's clock produced: the segment the agent walked across a doorway, if it
/// crossed one, and the waypoint frames the tick carries.
type StepOutcome = (Option<((f32, f32), (f32, f32))>, Option<Vec<u8>>);

/// The agent id a character-creation connection's player is drawn as.
///
/// A creation screen shows one body - the character being made - and a world here has no agent
/// registry yet, so the first id is the one it gets. The reference allocates from its own pool; the
/// number is the world's to give, and nothing else reads it until a map exists.
pub(crate) const CREATION_AGENT: u32 = 1;

/// What a load reads from the character's own row. It is the noun's now
/// (`game/src/Characters/Sheet.rs`, the reference's `Player::LoadFromDB`); this name stays because
/// the handlers and the travel path already speak it.
pub(crate) use crate::characters::CharacterSheet;

/// The effect word a travel puts on the travelling agent before the redirect
/// (`ASURA/crates/asura-world/src/travel.rs:19`, `TRAVEL_EFFECT`; the live capture's own `0x00F1` frame
/// carries it).
pub(crate) const TRAVEL_EFFECT: u32 = 0x100;

/// An IPv4 `host:port` as the four address bytes and the port, which is what a redirect carries.
pub(crate) fn parse_address(text: &str) -> Option<([u8; 4], u16)> {
    let (host, port) = text.rsplit_once(':')?;
    let octets: Vec<u8> = host
        .split('.')
        .map(|part| part.trim().parse::<u8>().ok())
        .collect::<Option<Vec<u8>>>()?;
    let octets: [u8; 4] = octets.as_slice().try_into().ok()?;
    Some((octets, port.trim().parse::<u16>().ok()?))
}

/// The client's asks on a gameplay connection, as the reference's dispatch reads them.
///
/// Sizes from `docs/evidence/client-protocol.md`'s table: the request-player and
/// request-armors asks are the header alone, a profession change is a campaign and a profession, the
/// confirm carries the character's name and appearance, and a ping is the header alone.
const CHAR_CREATION_REQUEST_PLAYER: u16 = 0x8089;
const CHAR_CREATION_REQUEST_ARMORS: u16 = 0x808A;
const CHAR_CREATION_CHANGE_PROF: u16 = 0x8060;
const CHAR_CREATION_CONFIRM: u16 = 0x808B;
/// `GAME_CMSG_PING_REQUEST`: two bytes on the gameplay connection.
const GAME_PING: u16 = 0x800D;

/// Where a claimed travel said to stand: the receiving map's arrival, and the facing it states.
///
/// `ASURA/crates/asura-world/src/travel.rs:359-362` writes the arrival's point as the character's
/// position before the redirect, and `portals::arrival_at` recovers its facing when the next connection
/// enters the map. Here both travel on the hand-off row, and the facing is looked up in the arrival rows
/// the world read with its maps.
#[derive(Debug, Clone, Copy)]
struct Claimed {
    /// The map the landing belongs to: a landing for another map is not this connection's.
    map_id: u32,
    /// The point, and the plane it is on.
    landing: crate::maps::travels::Landing,
    /// The direction the arriving agent is created facing, as `Arrival::direction` gives it.
    facing: (f32, f32),
}

/// The last client-reported position checked for a doorway, and the segment between two finite
/// reports.
///
/// `ASURA/crates/asura-world/src/player.rs:43-74`: `portal_position` and `last_step`. The doorway test
/// is the wall the step crosses, so the pair is what a crossing needs - not the agent's position, which
/// a simulation tick may already have advanced.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PortalStep {
    pub(crate) position: (f32, f32),
    pub(crate) last_step: Option<((f32, f32), (f32, f32))>,
}

/// `GAME_CMSG_ITEM_MOVE` (`0x8072`): a drag in the inventory - the item, the bag and the slot.
const CLIENT_ITEM_MOVE: u16 = 0x8072;
/// `GAME_CMSG_ITEM_DESTROY` (`0x8069`): the item to destroy.
const CLIENT_DROP_ITEM: u16 = 0x802C;
const CLIENT_UNEQUIP_ITEM: u16 = 0x804F;
const CLIENT_PING_REPLY: u16 = 0x8009;
const CLIENT_HEARTBEAT: u16 = 0x800C;
const CLIENT_DISCONNECT: u16 = 0x8008;
/// `GAME_CMSG_EQUIP_ITEM` (`0x8030`): the item to wear.
const CLIENT_EQUIP_ITEM: u16 = 0x8030;

/// The client's movement reports: `0x803D` carries the direction as well, `0x803E` the point a walk is
/// asked for, `0x8047` the stopping point of a cancel, and `0x8028` the cancel itself.
const CLIENT_MOVE_TO_COORD_WITH_DIR: u16 = 0x803D;
const CLIENT_MOVE_TO_COORD: u16 = 0x803E;
const CLIENT_LAST_POS_BEFORE_MOVE_CANCELED: u16 = 0x8047;
const CLIENT_CANCEL_MOVEMENT: u16 = 0x8028;

/// The client's in-game travel request - `GAME_CMSG_PARTY_TRAVEL`, `0x80B1`, nine bytes
/// (`ASURA/crates/asura-protocol/src/packets.rs`; the client's own frame is `B1 80 | <instance> | ...`).
/// This tree listened for `0x00B1`, which is the *server's* party message, until 2026-11-12.
const CLIENT_PARTY_TRAVEL: u16 = 0x80B1;

/// `GAME_CMSG_TRAVEL_INIT` (`0x80C1`, ten bytes): the client touched a travel object. Two agent ids;
/// `ASURA/crates/asura-world/src/dispatch.rs:73` answers it with `travel::handle_travel_init`.
const CLIENT_TRAVEL_INIT: u16 = 0x80C1;

/// `GAME_CMSG_SEND_CHAT_MESSAGE` (`0x8064`): the client speaks - `{agent u32, text String16}`, whose
/// first unit is the channel (`ASURA/crates/asura-protocol/src/packets.rs`, and
/// `ASURA/docs/reference/client-protocol.md:920-926`).
const CLIENT_CHAT_MESSAGE: u16 = 0x8064;

// The simulation's tick period is a setting (R33): `World.SimulationTickMs` (default 105 - the
// reference sent one `0x001E` every 90-110 ms, average 105 ms over 1780 ticks,
// `recordings/asura/feature/travel/first-crossing-20261031.jsonl`; the client's world time is this
// world's tick, so the cadence is an operator's number, not a law). `WorldConfig::simulation_tick_ms`
// carries it and the program logs it.

/// One gameplay connection.
pub struct WorldSession {
    pub(crate) log: Log,
    /// Which world this is: a connection that names another is not this world's.
    pub(crate) realm: u32,
    /// The maps the world hosts, so a connection can be told what it may enter.
    pub(crate) maps: Vec<crate::world::MapEntry>,
    /// The chat command surface: the names a player may type and the operation each one runs
    /// (`config/Server/commands.json`, `docs/decisions/0011-the-command-surface-is-content.md`).
    pub(crate) commands: crate::chat::commands::Table,
    /// Where a character may stand on each map, for the entry placement of R13.
    spawns: Vec<crate::maps::MapSpawn>,
    /// The numbers this world runs by: the spawn scatter and the rest.
    pub(crate) settings: crate::world::world_config::WorldConfig,
    /// The account the version message named, kept for the travel hand-off a redirect writes: the
    /// login database's travel row is keyed by account and player token.
    pub(crate) account: std::sync::Mutex<[u8; 16]>,
    /// Whether a travel is already in flight: the client is about to reconnect, so a second crossing
    /// must not start another one (`ASURA/crates/asura-world/src/travel.rs:600`, `player.moved`).
    pub(crate) moved: std::sync::Mutex<bool>,
    /// The map's walkable graphs, which a walk request is answered from: one finder per map, imported
    /// from the client archive at start-up (`ASURA/crates/asura-world/src/pathfind.rs:1484`). A map
    /// whose graph was never bound answers `Walk::NoGraph`.
    pub(crate) graphs: std::sync::Arc<crate::terrain::pathfind::PathGraphs>,
    /// The doorways a step is judged against, and the arrivals a crossing lands on.
    pub(crate) portals: std::sync::Arc<crate::maps::Portals>,
    /// The arrival points the world read with its maps (R21), so a crossing knows where the receiving
    /// map puts the character.
    pub(crate) arrivals: Vec<crate::maps::Arrival>,
    /// The character this connection is playing, as the version message named it: what the saved
    /// position and the last outpost are written for.
    character: std::sync::Mutex<Option<[u8; 16]>>,
    /// Where a claimed travel says the receiving map puts this character, when it was a doorway
    /// crossing: the load stands it there and faces it along the arrival, rather than scattering it
    /// around a spawn point (R13/R21).
    landing: std::sync::Mutex<Option<Claimed>>,
    /// The last client-reported position checked for a doorway, and the segment between finite
    /// reports: `ASURA/crates/asura-world/src/player.rs:43-74` (`portal_position`, `last_step`,
    /// `record_portal_step`). Simulation ticks never move it - a tick can cross a wall between
    /// reports, and that movement must remain in the next check.
    pub(crate) portal: std::sync::Mutex<PortalStep>,
    /// The character database, when the program opened one: a creation screen's confirm writes a row
    /// there. A session without it can draw the screen but cannot finish it.
    pub(crate) characters:
        Option<std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>>,
    /// The login database, when the program opened one: the travel record a login wrote is claimed
    /// here, once, and decides the map and the district the connection loads
    /// (`docs/port/06-world-plan.md`, "The three seams").
    pub(crate) login:
        Option<std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>>,
    /// The world's running district copies, shared with every session: entering a map claims a place
    /// in a district, and the connection gives it back when it ends.
    districts: std::sync::Arc<std::sync::Mutex<crate::districts::Districts>>,
    /// The district this connection entered, so [`Self::leave_district`] can give the place back.
    /// `None` until a place has been entered.
    entered: std::sync::Mutex<Option<(u32, u32)>>,
    /// The player's own agent: where the client says it is. The client moves it; the world keeps
    /// what it reports, which is what a later load or a second client would be told.
    agent: std::sync::Mutex<Option<crate::agents::Agent>>,
    /// The bags, items and profession sets a creation screen is built from, when the program read them.
    pub(crate) creation: Option<crate::creation::Tables>,
    /// The item templates the world holds, so a load can resolve a character's held items.
    templates: Vec<crate::data_stores::ItemDefinition>,
    /// The master secret to draw the exchange from, when the caller wants a known one. A program
    /// leaves it empty and every connection draws its own.
    master: Option<[u8; shared::cryptography::handshake::MASTER_SECRET_LEN]>,
    /// The players loaded in this world's running copies: a connection tells them it arrived, and is
    /// told when one of them does (game/Districts/Peers.rs).
    pub(crate) peers: std::sync::Arc<std::sync::Mutex<crate::districts::Peers>>,
    /// The frames other sessions pushed to this one, which it scrambles with its own cipher and writes
    /// when it next looks. A session that has not loaded a map has no channel at all.
    outbox: std::sync::Mutex<Option<std::sync::mpsc::Receiver<Vec<u8>>>>,
    /// The session number this connection filled in the peer registry. Cleanup removes the slot only
    /// when it is still this session's: a travel reconnects with the same player id, and the old
    /// connection's tail must not remove the new connection's slot.
    session_token: std::sync::Mutex<Option<u64>>,
    /// Set by the listener when the world is shutting down: the session ends itself on its next
    /// turn, so its own tail runs - the district place is given back and the position is
    /// written. Reference: `sWorldSessionMgr->KickAll()` (`WorldSessionMgr.cpp:220`), the first
    /// thing the reference's shutdown does (`Main.cpp:372`). `None` in a session a test builds.
    closing: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// The id this connection's player and agent are known by, drawn the moment a map load begins.
    ///
    /// One number for both, as every message in this tree already treats them: the load draws the body
    /// as `CHAR_CLASS_PLAYER_BASE | player`, the party records name the player, and the agent messages
    /// name the agent. It is **drawn per connection** (`Peers::allocate`), not a constant, because two
    /// players in one map are two players: a second connection that carried the first one's id would be
    /// the same body to every client. `None` until a real map is loaded - the creation instance has no
    /// population to be told apart in.
    id: std::sync::Mutex<Option<u32>>,
    /// How many times this connection has sent a message this world does not answer, per header.
    ///
    /// A client retries an ask it got no answer to, so a line per arrival is one line per retry, per
    /// connection - unbounded. The first sighting of a header is the report; the rest are a count
    /// (`WorldSession::report_unanswered`).
    unanswered: std::sync::Mutex<std::collections::BTreeMap<u16, u32>>,
    /// The world this connection answers for: its operation registry is what a command resolves
    /// through, so the session holds the world and not only copies of its parts.
    pub(crate) world: std::sync::Arc<World>,
    /// This character's guild and its own member row, as read when the map's players were asked for
    /// and as this connection has changed it since (`ASURA/crates/asura-world/src/player.rs:69-71`,
    /// `guild`). `None` for a character in no guild (`Handlers/GuildHandler.rs`).
    pub(crate) guild: std::sync::Mutex<Option<crate::guilds::Membership>>,
    /// This character's progress - experience, level, skill points - as the load read it and as
    /// `/.exp` has changed it since (`ASURA/crates/asura-world/src/player.rs:489-492`, `traits`). `None`
    /// until a map load reads the row (`Handlers/CharacterHandler.rs`, `answer_experience`).
    pub(crate) progress: std::sync::Mutex<Option<crate::characters::experience::Progress>>,
}

impl WorldSession {
    /// A session for one connection, for this world.
    ///
    /// The world itself is kept, not only copies of its parts: a command resolves through the world's
    /// operation registry (`docs/port/14-operator-surfaces.md`), so the session needs the world it is
    /// answering for.
    pub fn new(log: Log, world: &std::sync::Arc<World>) -> Self {
        Self {
            log,
            world: std::sync::Arc::clone(world),
            realm: world.realm(),
            maps: world.maps().to_vec(),
            commands: world.commands().clone(),
            spawns: world.spawns().to_vec(),
            settings: world.settings().clone(),
            account: std::sync::Mutex::new([0u8; 16]),
            moved: std::sync::Mutex::new(false),
            graphs: std::sync::Arc::clone(world.graphs()),
            portals: std::sync::Arc::clone(world.portals()),
            arrivals: world.arrivals().to_vec(),
            character: std::sync::Mutex::new(None),
            landing: std::sync::Mutex::new(None),
            portal: std::sync::Mutex::new(PortalStep::default()),
            characters: None,
            login: None,
            districts: world.districts(),
            entered: std::sync::Mutex::new(None),
            agent: std::sync::Mutex::new(None),
            creation: Some(world.creation().clone()),
            templates: world.templates().to_vec(),
            master: None,
            peers: world.peers(),
            outbox: std::sync::Mutex::new(None),
            session_token: std::sync::Mutex::new(None),
            closing: None,
            id: std::sync::Mutex::new(None),
            unanswered: std::sync::Mutex::new(std::collections::BTreeMap::new()),
            guild: std::sync::Mutex::new(None),
            progress: std::sync::Mutex::new(None),
        }
    }

    /// A session that can write a character.
    pub fn with_characters(
        log: Log,
        world: &std::sync::Arc<World>,
        characters: std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>,
    ) -> Self {
        Self {
            characters: Some(characters),
            ..Self::new(log, world)
        }
    }

    /// A session that can write a character and claim a travel.
    ///
    /// The program's own shape: the travel table is in the login database, and the version message's
    /// token pair is what claims a row.
    pub fn with_characters_and_login(
        log: Log,
        world: &std::sync::Arc<World>,
        characters: std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>,
        login: std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>,
    ) -> Self {
        Self {
            characters: Some(characters),
            login: Some(login),
            ..Self::new(log, world)
        }
    }

    /// A session whose exchange uses this master secret instead of one drawn from entropy.
    ///
    /// For a test that wants to read what the client would read; a program never calls this.
    pub fn with_master(
        log: Log,
        world: &std::sync::Arc<World>,
        master: [u8; shared::cryptography::handshake::MASTER_SECRET_LEN],
    ) -> Self {
        Self {
            master: Some(master),
            ..Self::new(log, world)
        }
    }

    /// A session whose creation screen has no inventory: the shape a test uses when it wants the
    /// frames without the content files.
    pub fn without_creation_data(log: Log, world: &std::sync::Arc<World>) -> Self {
        Self {
            creation: None,
            ..Self::new(log, world)
        }
    }

    /// A session with both a known master and a character database.
    ///
    /// The shape a test that plays a whole character creation uses: it needs to read the exchange's
    /// answer and to see the row the confirm writes.
    pub fn with_master_and_characters(
        log: Log,
        world: &std::sync::Arc<World>,
        master: [u8; shared::cryptography::handshake::MASTER_SECRET_LEN],
        characters: std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>,
    ) -> Self {
        Self {
            characters: Some(characters),
            ..Self::with_master(log, world, master)
        }
    }

    /// A session with a known master, a character database and a login database.
    ///
    /// The shape a test that plays a whole travel uses: the row the login would have written is
    /// claimed by the connection, and the map it names is the one loaded.
    pub fn with_master_and_characters_and_login(
        log: Log,
        world: &std::sync::Arc<World>,
        master: [u8; shared::cryptography::handshake::MASTER_SECRET_LEN],
        characters: std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>,
        login: std::sync::Arc<std::sync::Mutex<database::database::DatabaseWorkerPool>>,
    ) -> Self {
        Self {
            characters: Some(characters),
            login: Some(login),
            ..Self::with_master(log, world, master)
        }
    }

    /// The inventory a load draws: the character's own rows when it has them - what a confirm wrote -
    /// and the creation record set otherwise, which is what every character starts from.
    fn load_inventory(&self, sheet: &CharacterSheet) -> crate::creation::Inventory {
        if let Some(pool) = self.characters.as_ref() {
            let characters = crate::characters::DatabaseCharacters::new(
                std::sync::Arc::clone(pool),
                self.log.clone(),
            );
            match characters.items_of(&crate::characters::uuid_text(&sheet.uuid)) {
                Ok(held) if !held.is_empty() => {
                    let bags = self
                        .creation
                        .as_ref()
                        .map(crate::creation::bags)
                        .unwrap_or(&[]);
                    return crate::creation::inventory_from_held(
                        bags,
                        &held,
                        &self.templates,
                        &self.log,
                    );
                }
                Ok(_) => {}
                Err(error) => self.say(
                    LogLevel::Warn,
                    format!(
                        "The character's items cannot be read ({error}); the creation record's set is drawn instead."
                    ),
                ),
            }
        }
        self.creation
            .as_ref()
            .map(|tables| tables.inventory(sheet.campaign, sheet.profession))
            .unwrap_or_default()
    }

    /// The bags this world's content gives a character, or none when the world read no creation data.
    /// The item templates this world read at start-up: what an item's kind and slot come from.
    pub(crate) fn item_templates(&self) -> &[crate::data_stores::ItemDefinition] {
        &self.templates
    }

    /// The bag a character wears things in, when the content names one (`BagType_Equipped`).
    pub(crate) fn equipped_bag(&self) -> Option<crate::creation::Bag> {
        self.creation
            .as_ref()
            .and_then(crate::creation::equipped_bag)
            .copied()
    }

    pub(crate) fn bags(&self) -> Vec<crate::creation::Bag> {
        self.creation
            .as_ref()
            .map(|tables| crate::creation::bags(tables).to_vec())
            .unwrap_or_default()
    }

    /// Which slot each kind of item is worn in, from the content (`config/World/Vanilla/creation/worn_slots.json`). Empty
    /// when the world read no such file, and then nothing is worn: the caller refuses rather than
    /// choosing a slot.
    pub(crate) fn worn(&self) -> &[crate::creation::WornSlot] {
        self.creation
            .as_ref()
            .map(|tables| tables.worn())
            .unwrap_or(&[])
    }

    /// The character database, when the program opened one.
    pub(crate) fn characters(&self) -> Option<crate::characters::DatabaseCharacters> {
        self.characters.as_ref().map(|pool| {
            crate::characters::DatabaseCharacters::new(
                std::sync::Arc::clone(pool),
                self.log.clone(),
            )
        })
    }

    pub(crate) fn say(&self, level: LogLevel, text: impl Into<String>) {
        self.log.out(level, "server.worldserver", text);
    }

    /// Report a message this world does not answer: once per header per connection, then a count.
    ///
    /// Reference: the reference names a message only where it has a handler for it, and the gap this
    /// line reports is a gap to fill, not an event - the first sighting says the client asked, and a
    /// client that got no answer asks again. Logging every arrival made one stuck ask into the whole
    /// log (`docs/port/03-tracing.md`); the count is kept beside the first sighting, and later sightings
    /// are a `Trace` line, below both the console and the default log file.
    fn report_unanswered(&self, peer: &str, header: u16, what: &str, bytes: usize) {
        let mut unanswered = match self.unanswered.lock() {
            Ok(unanswered) => unanswered,
            Err(_) => return,
        };
        let seen = unanswered.entry(header).or_insert(0);
        *seen += 1;
        let sighting = *seen;
        match sighting {
            1 => self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: message {what} ({bytes} bytes), which this world does not answer yet."
                ),
            ),
            _ => self.say(
                LogLevel::Trace,
                format!(
                    "Gameplay connection from {peer}: message {what} ({bytes} bytes) again - sighting {sighting}; it is not answered yet."
                ),
            ),
        }
    }

    /// The whole conversation, as far as it goes.
    async fn converse(&self, io: &mut SessionIo) {
        let peer = io
            .peer()
            .map(|address| address.to_string())
            .unwrap_or_else(|| "an unknown address".to_string());

        // 1. The version message. Its first four bytes are what the connection was classified with.
        let mut message = vec![0u8; crate::server::protocol::GAME_CMSG_VERSION_LEN];
        if let Err(error) = read_exactly(io, &mut message).await {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer} ended inside its version message: {error}"
                ),
            );
            let _ = io.close().await;
            return;
        }

        let version = match GameVersion::decode(&message) {
            Ok((version, _)) => version,
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Gameplay connection from {peer}: {error}; closing it."),
                );
                let _ = io.close().await;
                return;
            }
        };

        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: build {}, world {}, map {}, player token 0x{:08X}.",
                version.version, version.map_token, version.map_id, version.player_token
            ),
        );

        if let Ok(mut account) = self.account.lock() {
            *account = version.account;
        }

        // The world's own id is the check the reference makes through its transfer record: a
        // connection that names another world is not this one's.
        if version.map_token != self.realm {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer} names world {} and this world is {}; closing it.",
                    version.map_token, self.realm
                ),
            );
            let _ = io.close().await;
            return;
        }

        // 2. The seed exchange, the same as the login's.
        let mut seed = vec![0u8; CLIENT_SEED_LEN];
        if let Err(error) = read_exactly(io, &mut seed).await {
            self.say(
                LogLevel::Warn,
                format!("Gameplay connection from {peer} ended inside its seed: {error}"),
            );
            let _ = io.close().await;
            return;
        }
        let header = u16::from_le_bytes([seed[0], seed[1]]);
        if header != CLIENT_SEED_HEADER {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer} sent a seed with header 0x{header:04X}; the exchange starts with 0x{CLIENT_SEED_HEADER:04X}."
                ),
            );
            let _ = io.close().await;
            return;
        }

        let shared = match handshake::shared_secret(&seed[2..]) {
            Ok(shared) => shared,
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!(
                        "Gameplay connection from {peer}: the exchange cannot proceed: {error}"
                    ),
                );
                let _ = io.close().await;
                return;
            }
        };

        let master = match self.master {
            Some(master) => master,
            None => match handshake::random_master() {
                Ok(master) => master,
                Err(error) => {
                    self.say(
                        LogLevel::Error,
                        format!(
                            "Gameplay connection from {peer}: {error}; the exchange will use zeros."
                        ),
                    );
                    [0u8; handshake::MASTER_SECRET_LEN]
                }
            },
        };
        let answer = handshake::server_seed(&master, &shared);
        debug_assert_eq!(answer.len(), SERVER_SEED_LEN);
        if let Err(error) = io.write_all(&answer).await {
            self.say(
                LogLevel::Warn,
                format!("Gameplay connection from {peer} could not be answered: {error}"),
            );
            let _ = io.close().await;
            return;
        }

        let mut crypt = AuthCrypt::new(&handshake::derive_key(&master));
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: the exchange is answered. Everything after it is scrambled."
            ),
        );

        // 3. The travel record. The login wrote one when it answered the client's request for a map
        // (`0x8029`), and this connection presents the same world, account and token; claiming it is
        // what makes the map and the district the login's decision rather than the client's. A
        // connection with no record - the creation screen, or a client that never asked - keeps the
        // map its version message names (`docs/port/06-world-plan.md`, "The three seams").
        let travel = match self.login.as_ref() {
            Some(login) => {
                let account = crate::characters::uuid_text(&version.account);
                let travels = crate::maps::DatabaseTravels::new(
                    std::sync::Arc::clone(login),
                    self.log.clone(),
                );
                match travels.claim(self.realm, &account, version.player_token) {
                    Ok(Some(travel)) => {
                        self.say(
                            LogLevel::Debug,
                            format!(
                                "Gameplay connection from {peer}: the travel record is claimed - map {}, district {}, language {}{}.",
                                travel.map_id,
                                travel.district,
                                travel.language,
                                match travel.landing {
                                    Some(landing) => format!(
                                        ", landing at ({:.1}, {:.1}) plane {} from map {}",
                                        landing.x, landing.y, landing.plane, landing.from_map
                                    ),
                                    None => String::new(),
                                }
                            ),
                        );
                        // A crossing states where the receiving map puts the character; the load
                        // stands it there instead of scattering it around a spawn point (R13/R21,
                        // `ASURA/crates/asura-world/src/travel.rs:446-468`), and faces it along the
                        // arrival the receiving map states for the map it came from.
                        if let (Some(landing), Ok(mut claimed)) =
                            (travel.landing, self.landing.lock())
                        {
                            let facing = self
                                .arrivals
                                .iter()
                                .find(|arrival| {
                                    arrival.map_id == travel.map_id
                                        && arrival.from_map == landing.from_map
                                })
                                .map(|arrival| arrival.direction())
                                .unwrap_or((1.0, 0.0));
                            *claimed = Some(Claimed {
                                map_id: travel.map_id,
                                landing,
                                facing,
                            });
                        }
                        Some(travel)
                    }
                    Ok(None) => {
                        self.say(
                            LogLevel::Debug,
                            format!(
                                "Gameplay connection from {peer}: no travel record was claimed; the map {} is the one its own version message names.",
                                version.map_id
                            ),
                        );
                        None
                    }
                    Err(error) => {
                        self.say(
                            LogLevel::Error,
                            format!(
                                "Gameplay connection from {peer}: the travel table could not be read ({error}); the version message's map is used."
                            ),
                        );
                        None
                    }
                }
            }
            None => None,
        };
        let map_id = travel.map(|travel| travel.map_id).unwrap_or(version.map_id);
        // The language the client named, or the world's own when it named none: a travel may carry
        // `-1` (`0xFFFFFFFF`) for "any", which is not a language the client's district record can
        // hold. Measured 2026-11-12: the map request that follows a character click carries exactly
        // that, so an unclamped `-1` reaches the load block as 255.
        let language = travel
            .map(|travel| {
                if travel.language > u32::from(u8::MAX) {
                    0
                } else {
                    travel.language as u8
                }
            })
            .unwrap_or(0);

        // 4. The block the world sends unasked, once the connection is scrambled.
        let Some(map) = self.maps.iter().find(|map| map.id == map_id) else {
            self.say(
                LogLevel::Error,
                format!(
                    "Gameplay connection from {peer} asks for map {map_id}, which this world does not host ({} map(s) in the catalogue); closing it.",
                    self.maps.len()
                ),
            );
            let _ = io.close().await;
            return;
        };

        // A map that is a place is loaded: the head, the player-data start, the character's name and the
        // instance's own record, unasked - the order `docs/evidence/` and ASURA's
        // `reference/instance-initialization.md` measured on a live load, and the rows
        // `shared/src/Packets/../Tables.rs` names.
        if !map.is_creation() {
            // The unlock is **not** a gate here. `ASURA/crates/asura-world/src/player.rs:442-462`:
            // "The instance the character is standing in is reached, so it is unlocked - that is the
            // whole rule: the walk sets the bit, the bit never gates the walk." The bit is written when
            // the players ask is answered (it needs the map the character actually loaded), and the
            // client uses the bitmap to decide what to *offer* on the world map.
            //
            // This block used to close the connection for a claimed map the character had not
            // unlocked. That is what a doorway crossing into a new map met: the client reconnected onto
            // the map it had just walked into, and this world dropped it - the client's `007`.

            // The district the connection enters: the travel's own number when it named one (R57),
            // otherwise the first with room (R22) - and a named full district is refused rather than
            // moved (R58). The allotment is the world's setting (R5).
            let entered = match self.districts.lock() {
                Ok(mut districts) => districts.enter(
                    map,
                    travel.map(|travel| travel.district).unwrap_or(0),
                    u32::from(language),
                    self.settings.district_allotment,
                ),
                Err(_) => Err("the world's district copies are poisoned".to_string()),
            };
            let district = match entered {
                Ok(district) => district,
                Err(refusal) => {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Gameplay connection from {peer}: {refusal}; the connection is closed, because the refusal's own wire shape is not captured (R58)."
                        ),
                    );
                    let _ = io.close().await;
                    return;
                }
            };
            if let Ok(mut entered) = self.entered.lock() {
                *entered = Some((map.id, district));
            }

            // R14: the outpost the character is now in is its last outpost, so a later load - or the
            // login's list - places it there. **Only an outpost**: an explorable must not overwrite it
            // (`ASURA/crates/asura-world/src/player.rs:447`), or a party that walks into one and logs out
            // would be sent back into it instead of the town it came from.
            if map.kind == Some(crate::world::map_kind::OUTPOST)
                && let Some(pool) = self.characters.as_ref()
            {
                let characters = crate::characters::DatabaseCharacters::new(
                    std::sync::Arc::clone(pool),
                    self.log.clone(),
                );
                if let Err(error) = characters.set_last_outpost(
                    &crate::characters::uuid_text(&version.character),
                    map.id as u16,
                ) {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Gameplay connection from {peer}: the character's last outpost cannot be recorded: {error}"
                        ),
                    );
                }
            }

            let sheet = self.character_sheet(&version);
            if let Ok(mut character) = self.character.lock() {
                *character = Some(sheet.uuid);
            }
            // The id must be drawn **before** `0x019A`, because that message says which agent this
            // client controls. It used to be drawn later, inside the players answer, so every load
            // told the client "agent 1" while a second connection's own agent was 2 (or a travel's
            // was 3): its health and energy then updated a body the client was not controlling, and
            // its own bars stayed empty. The first connection only worked because its id was 1.
            self.draw_player_id(sheet.uuid);
            // `0x019A`'s explorable byte is the map's own kind, not a constant: 0 for a town or
            // outpost and 1 for an explorable (`ASURA/crates/asura-world/src/map.rs:76-86`,
            // `load_info_explorable`). Sending 1 for Ascalon City makes the client treat the town as
            // an explorable.
            let is_explorable = u8::from(map.kind != Some(crate::world::map_kind::OUTPOST));
            let mut frames = vec![
                instance_packets::instance_load_head(),
                instance_packets::instance_player_data_start(),
                instance_packets::instance_load_player_name(&sheet.name),
                instance_packets::instance_load_info(
                    self.player_id(),
                    map.id as u16,
                    is_explorable,
                    district,
                    language,
                    0,
                ),
            ];
            // The exploration grid, right after the instance's record: that is where a live server
            // sends it, once per instance (`ASURA/crates/asura-world/src/map.rs:22-48`), and a client
            // without it asserts on its own world map the first time the map view opens
            // (`GmMapView.cpp(1731)`, client build 39112).
            let grid =
                crate::maps::cartography::init_frames(crate::maps::cartography::grid_for(map.id));
            frames.extend(grid);
            // And the destinations it can resolve, in the batches a load uses - the working
            // implementation sends these in the same phase (`ASURA/crates/asura-world/src/map.rs:49-58`).
            let (table, notes) = crate::maps::instance_table::table_for(&self.maps);
            for note in &notes {
                self.say(
                    LogLevel::Warn,
                    format!("Gameplay connection from {peer}: instance table: {note}"),
                );
            }
            frames.extend(table.frames());
            let mut block = Vec::new();
            for frame in &frames {
                block.extend_from_slice(frame);
            }
            crypt.encrypt(&mut block);
            match io.write_all(&block).await {
                Ok(()) => self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: {} is loading - {} message(s), {} bytes: the load head, the player-data start, the name '{}', the instance's record (district {district}, language {language}), the exploration grid ({} x {}, nothing explored) and the instance table ({} pair(s)).",
                        map.label(),
                        frames.len(),
                        block.len(),
                        sheet.name,
                        crate::maps::cartography::grid_for(map.id).0,
                        crate::maps::cartography::grid_for(map.id).1,
                        table.pairs.len()
                    ),
                ),
                Err(error) => {
                    self.say(
                        LogLevel::Warn,
                        format!("Gameplay connection from {peer} could not be sent its load block: {error}"),
                    );
                    let _ = io.close().await;
                    return;
                }
            }

            // What the client asks next, and what a load answers: the items, where it stands, and
            // then its own body and the end of the load.
            self.load_map(io, &peer, map, &sheet, &mut crypt).await;
            return;
        }

        let mut block = creation_load_block();
        crypt.encrypt(&mut block);
        match io.write_all(&block).await {
            Ok(()) => self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: the character-creation instance is opened, {} bytes (load head and creation start).",
                    block.len()
                ),
            ),
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Gameplay connection from {peer} could not be sent its load block: {error}"),
                );
                let _ = io.close().await;
                return;
            }
        }

        // From here the client asks what it needs for the creation screen, and what it asks for is
        // answered in the reference's order: the player-data block, the PvP half, a profession
        // change, and the confirm.
        let mut creation = Creation::start();
        let mut pending: Vec<u8> = Vec::new();
        let mut buffer = vec![0u8; 1024];
        // A message that never finishes framing is otherwise invisible: the client sends one, this
        // world waits for bytes that are not coming, and all the client can say is its own connection
        // error. It happens when a header's row disagrees with the client's own packing, so a wait
        // longer than a client tolerates is said out loud, once.
        let mut stalled: Option<(u16, usize, std::time::Instant)> = None;
        loop {
            // The world is going down: the listener set this before it stopped, and a session that
            // keeps turning would answer a client that is being kicked (`KickAll`).
            if self.closing() {
                return;
            }
            // Everything already unscrambled and not yet answered, before any more is read.
            while let Some(bytes) = pending.first_chunk::<2>() {
                let header = u16::from_le_bytes(*bytes);
                // The table decides where the message ends: a counted field carries its own count, so
                // the walk is exact for every header the protocol has, not just the ones answered here.
                let used = match crate::server::protocol::decode_game_message(&pending, true) {
                    Ok((frame, used)) => {
                        self.log.out(
                            LogLevel::Debug,
                            "network.trace",
                            format!(
                                "trace {peer} game in  {} bytes, {}",
                                used,
                                crate::server::protocol::label(frame.header())
                            ),
                        );
                        used
                    }
                    Err(FrameError::Incomplete { needed }) => {
                        let now = std::time::Instant::now();
                        let held = pending.len();
                        match stalled {
                            Some((seen, seen_len, since)) if seen == header && seen_len == held => {
                                if now.duration_since(since)
                                    >= std::time::Duration::from_millis(900)
                                {
                                    self.say(
                                        LogLevel::Warn,
                                        format!(
                                            "Gameplay connection from {peer}: a message with header 0x{header:04X} has been waiting {held} byte(s) for {} ms, and the row needs at least {needed}; the client is not sending the rest. The bytes so far are {}.",
                                            now.duration_since(since).as_millis(),
                                            shared::network::socket::hex(&pending[..pending.len().min(64)])
                                        ),
                                    );
                                    stalled = None;
                                }
                            }
                            _ => stalled = Some((header, held, now)),
                        }
                        break;
                    }
                    Err(error) => {
                        self.say(
                            LogLevel::Warn,
                            format!(
                                "Gameplay connection from {peer}: a message with header 0x{header:04X} cannot be framed ({error}), so the stream cannot be split any further. The bytes so far are {}.",
                                shared::network::socket::hex(&pending[..pending.len().min(64)])
                            ),
                        );
                        let _ = io.close().await;
                        return;
                    }
                };
                let message: Vec<u8> = pending.drain(..used).collect();

                // One complete incoming frame: the capture records the wire layer, after ARC4 came off.
                crate::server::capture::record("CToS", header, &message);

                self.log.out(
                    LogLevel::Debug,
                    "network.trace",
                    format!(
                        "trace {peer} game in  {} bytes: {}",
                        message.len(),
                        shared::network::socket::hex(&message[..message.len().min(64)])
                    ),
                );

                match self
                    .answer(
                        io,
                        &peer,
                        header,
                        &message,
                        &mut creation,
                        &version,
                        &mut crypt,
                    )
                    .await
                {
                    Some(true) => {}
                    Some(false) => return,
                    None => {
                        // A message this world does not answer: say so once per header, and keep the
                        // connection (`WorldSession::report_unanswered`).
                        self.report_unanswered(
                            &peer,
                            header,
                            &format!("0x{header:04X}"),
                            message.len(),
                        );
                    }
                }
            }

            let read = match io.read(&mut buffer).await {
                Ok(0) => {
                    self.say(
                        LogLevel::Debug,
                        format!("Gameplay connection from {peer} closed."),
                    );
                    return;
                }
                Ok(read) => read,
                Err(error) => {
                    self.say(
                        LogLevel::Warn,
                        format!("Gameplay connection from {peer} could not be read: {error}"),
                    );
                    return;
                }
            };

            let mut plain = buffer[..read].to_vec();
            crypt.decrypt(&mut plain);
            pending.extend_from_slice(&plain);
        }
    }

    /// Answer one gameplay message.
    ///
    /// `Some(true)` keeps the connection, `Some(false)` ends it, `None` is a message this world has
    /// no behaviour for - the caller logs its header and size, which is what says what is missing.
    #[allow(clippy::too_many_arguments)]
    async fn answer(
        &self,
        io: &mut SessionIo,
        peer: &str,
        header: u16,
        message: &[u8],
        creation: &mut Creation,
        version: &GameVersion,
        crypt: &mut AuthCrypt,
    ) -> Option<bool> {
        match header {
            // The four creation messages, in the order ASURA dispatches them
            // (`ASURA/crates/asura-world/src/dispatch.rs:88-103`), each answered by its own
            // `creation.rs` handler in `game/Handlers/CharacterHandler.rs`.
            CHAR_CREATION_REQUEST_PLAYER => {
                self.answer_creation_request_player(io, peer, message, creation, crypt)
                    .await;
                Some(true)
            }
            CHAR_CREATION_REQUEST_ARMORS => {
                self.answer_creation_request_armors(io, peer, crypt).await;
                Some(true)
            }
            CHAR_CREATION_CHANGE_PROF => {
                self.answer_creation_change_prof(io, peer, message, creation, crypt)
                    .await;
                Some(true)
            }
            CHAR_CREATION_CONFIRM => {
                // The confirm carries the name as a counted string and then the packed
                // appearance word; the reference reads exactly those and writes the rows.
                self.confirm_creation(io, peer, message, creation, version, crypt)
                    .await;
                Some(true)
            }
            // The client is going away (GAME_CMSG_DISCONNECT, 0x8008, two bytes): the working
            // implementation's dispatch ends the connection on it
            // (ASURA/crates/asura-world/src/dispatch.rs:86).
            CLIENT_DISCONNECT => Some(false),
            // `GAME_CMSG_PING_REQUEST` (`0x800D`): the client asks to be pinged back, and the answer is
            // the ping reply - `0x000D` and the value this world runs with, **not** the request echoed
            // (`ASURA/crates/asura-world/src/connection.rs:579-587`, `handle_ping_request`).
            GAME_PING => {
                self.send(
                    io,
                    peer,
                    crypt,
                    &[misc_packets::ping_reply(self.settings.ping_value)],
                    "the ping reply",
                )
                .await;
                Some(true)
            }
            _ => None,
        }
    }

    /// The character's own row, as the load reads it - or the sheet a load falls back to when the row
    /// cannot be read (`CharacterSheet::missing`, the noun's own fallback).
    ///
    /// Reference: `Player::LoadFromDB`. The statement and the fallback are
    /// `game/src/Characters/Sheet.rs`'s and `DatabaseCharacters::sheet`'s; what this keeps is the one
    /// thing only the connection knows: which character its version message named.
    fn character_sheet(&self, version: &crate::server::protocol::GameVersion) -> CharacterSheet {
        let Some(pool) = self.characters.as_ref() else {
            return CharacterSheet::missing(version.character);
        };
        let characters = crate::characters::DatabaseCharacters::new(
            std::sync::Arc::clone(pool),
            self.log.clone(),
        );
        characters.sheet(
            &crate::characters::uuid_text(&version.character),
            version.character,
        )
    }

    /// Tell this session how to learn the world is shutting down.
    ///
    /// The listener sets the flag and waits; the session reads it at the top of each turn and ends
    /// itself, which is the reference's kick (`WorldSessionMgr::KickAll`) in a tree whose sessions are
    /// tasks rather than objects in a loop.
    pub(crate) fn set_closing(&mut self, closing: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.closing = Some(closing);
    }

    /// Whether the world has told this session to stop.
    fn closing(&self) -> bool {
        self.closing
            .as_ref()
            .is_some_and(|closing| closing.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Preserve the full reported movement even when simulation ticks advanced the agent meanwhile.
    ///
    /// `ASURA/crates/asura-world/src/player.rs:60-74`, `record_portal_step`: the segment between two
    /// finite client reports, and nothing at all when the client reported the same point again.
    pub(crate) fn record_portal_step(&self, position: (f32, f32)) {
        let Ok(mut portal) = self.portal.lock() else {
            return;
        };
        portal.last_step = (portal.position != position).then_some((portal.position, position));
        portal.position = position;
    }

    /// Where the character entered: the first report is measured from here, not from nowhere.
    ///
    /// `ASURA/crates/asura-world/src/connection.rs:318-319`: `portal_position` starts at the standing
    /// position the load put the agent on, so a step across a doorway at the entry point is a step.
    fn start_portal_position(&self, position: (f32, f32)) {
        if let Ok(mut portal) = self.portal.lock() {
            portal.position = position;
            portal.last_step = None;
        }
    }

    /// Take the segment the doorway test consumes, if a report left one.
    fn take_portal_step(&self) -> Option<((f32, f32), (f32, f32))> {
        self.portal
            .lock()
            .ok()
            .and_then(|mut portal| portal.last_step.take())
    }

    /// Write everything other sessions have pushed at this one, and nothing when they pushed nothing.
    async fn flush_outbox(&self, io: &mut SessionIo, peer: &str, crypt: &mut AuthCrypt) {
        let mut frames: Vec<Vec<u8>> = Vec::new();
        if let Ok(outbox) = self.outbox.lock()
            && let Some(receiver) = outbox.as_ref()
        {
            while let Ok(frame) = receiver.try_recv() {
                frames.push(frame);
            }
        }
        if !frames.is_empty() {
            self.send(io, peer, crypt, &frames, "what another session announced")
                .await;
        }
    }

    /// Run something against this session's own agent, when it has one.
    ///
    /// Every message that moves a body goes through here, so this is also where the peer registry learns
    /// where the body stands: an introduction names the point a player is at **now**, not the one it
    /// entered at (`ASURA/crates/asura-world/src/instance.rs:249-255`, `standing`, and `PeerView::from`,
    /// which reads the live agent).
    pub(crate) fn with_agent<T>(
        &self,
        work: impl FnOnce(&mut crate::agents::Agent) -> T,
    ) -> Option<T> {
        let mut agent = self.agent.lock().ok()?;
        let agent = agent.as_mut()?;
        let answer = work(agent);
        if let Ok(mut peers) = self.peers.lock() {
            peers.moved(
                self.player_id(),
                crate::districts::Standing {
                    position: agent.position,
                    plane: agent.plane,
                    direction: agent.facing,
                },
            );
        }
        Some(answer)
    }

    /// The id this connection's player and agent are known by: drawn once, when a map load begins, and
    /// the first id until then (the creation instance's own body).
    pub(crate) fn player_id(&self) -> u32 {
        match self.id.lock() {
            Ok(id) => id.unwrap_or(CREATION_AGENT),
            Err(_) => CREATION_AGENT,
        }
    }

    /// Draw this character's id, once. A connection that already has one keeps it, and a travel to
    /// another map draws the id the character already has (`Peers::allocate_for`).
    fn draw_player_id(&self, character: [u8; 16]) {
        if let Ok(mut id) = self.id.lock()
            && id.is_none()
            && let Ok(mut peers) = self.peers.lock()
        {
            *id = Some(peers.allocate_for(character));
        }
    }

    /// The district this connection entered, when it has entered one: the number a copy is known by
    /// (`game/Districts/`), and the half a peer registry keys on.
    pub(crate) fn district(&self) -> u32 {
        match self.entered.lock() {
            Ok(entered) => entered.map(|(_, district)| district).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// The outpost this world starts a fresh character in: `World.StartingOutpost`, or the lowest-id
    /// outpost it hosts when that row is absent, which is said in the log rather than hidden.
    ///
    /// R12 gives a new character exactly one unlock; R14 places a load at the character's last outpost.
    pub(crate) fn starting_outpost(&self) -> Option<&crate::world::MapEntry> {
        // The rule is the world's (`crate::world::first_outpost`); what the connection has is the
        // copy of the maps it was built with, and the number from the settings.
        crate::world::first_outpost(&self.maps, self.settings.starting_outpost)
    }

    /// The map this connection is in, as the graph lookup wants it.
    pub(crate) fn map_id(&self) -> u32 {
        match self.entered.lock() {
            Ok(entered) => entered.map(|(map_id, _)| map_id).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// The rest of a map's load: the ping pair, the inventory and the map's record, where the
    /// character stands, its own body, and the end of the load.
    ///
    /// The client asks and the world answers, in the order a working server measured
    /// (`recordings/gwps/world/load-stream.log`): `0x8091` wants the items and is answered with the
    /// inventory stream and the map's record, `0x8088` wants the entry point and is answered with a
    /// point of this map's own list (scattered per R13) and the first tick, and `0x8090` wants the
    /// players and ends the load. What is still missing is named here rather than implied: the
    /// agent stream of step 6 and the party stream of step 7 are not sent, and the PvP halves carry
    /// the empty values a fresh character has.
    async fn load_map(
        &self,
        io: &mut SessionIo,
        peer: &str,
        map: &crate::world::MapEntry,
        sheet: &CharacterSheet,
        crypt: &mut AuthCrypt,
    ) {
        // The unasked pair the measured load follows the head block with.
        self.send(
            io,
            peer,
            crypt,
            &[misc_packets::ping_request(), misc_packets::ping_reply(0x14)],
            "the ping pair",
        )
        .await;

        let mut pending: Vec<u8> = Vec::new();
        let mut buffer = vec![0u8; 1024];
        // This connection's own id. It is already drawn before `0x019A` in the load head; this is the
        // idempotent second call, kept so the load block and the entry point cannot get a later one.
        self.draw_player_id(sheet.uuid);
        let inventory = self.load_inventory(sheet);
        // The instant the last tick went out, so the next one carries how long it really has been.
        let mut last_tick = std::time::Instant::now();
        let mut loaded = false;
        // Where the character stands, chosen once: the entry-position answer and the agent the load
        // creates have to be the same place. The saved position is read here, once, because it is what
        // a reconnect to the map the character left resumes at (R14,
        // `ASURA/crates/asura-world/src/agent.rs:531-545`).
        // The character's progress as its row states it, which `/.exp` changes from here on
        // (`Handlers/CharacterHandler.rs`, `answer_experience`).
        if let Ok(mut progress) = self.progress.lock() {
            *progress = Some(sheet.progress());
        }
        let saved = self.saved_position(sheet);
        let entry = self.entry_point(map, saved);
        let load_started = std::time::Instant::now();
        // The character as the load is told it: the two ids it is named by, where it stands, what it
        // wears. Everything here is decided once, before the first ask, because the entry position,
        // the block and the player record all have to name the same place.
        let agent = self.player_id();
        let player = self.player_id();
        let position = entry.map(|point| (point.0, point.1)).unwrap_or((0.0, 0.0));
        let plane = entry.map(|point| point.2).unwrap_or(0);
        // The entry point's own facing (its spawn point's angle, or a doorway arrival's), and
        // `(1, 0)` when it states none - `map_spawns.rs:44-53`.
        let direction = entry.map(|point| point.3).unwrap_or((1.0, 0.0));
        // The look the client builds the model from: the character's own bits, or the world's default
        // when its row carries none (an all-zero word is `Profession_None`, which the client cannot
        // draw).
        let appearance = if sheet.appearance == 0 {
            self.settings.appearance
        } else {
            sheet.appearance
        };
        // The item ids the agent is drawn wearing, from the equipped bag, in its own order. A slot
        // the character wears nothing in is zero.
        // The nine-dword list is the client's wire order, which is not the equipped bag's own order:
        // a piece's cell in the bag and its position here are two numbers, and the table names both
        // (`config/World/Vanilla/creation/worn_slots.json`).
        let mut equipped = [0u32; 9];
        let mut wearing = Vec::new();
        if let Some(bag) = self
            .creation
            .as_ref()
            .and_then(crate::creation::equipped_bag)
        {
            for item in inventory.items_in(bag.id) {
                if let Some(slot) = crate::creation::wire_slot(self.worn(), item.item_type)
                    && let Some(held) = equipped.get_mut(slot as usize)
                {
                    *held = item.id;
                }
                // The same rows, in the form another player's introduction declares them.
                wearing.push(crate::creation::named_item(item));
            }
        }
        let state = crate::agents::Player {
            agent,
            player,
            position,
            plane,
            direction,
            appearance,
            equipped,
            gold: sheet.gold,
            profession: sheet.profession,
            unlocked_professions: sheet.unlocked_professions,
            campaigns_finished: sheet.campaigns_finished,
            level: sheet.level,
            experience: sheet.experience,
            skill_points: sheet.skill_points,
            skill_points_total: sheet.skill_points_total,
            name: sheet.name.clone(),
        };
        // The map's clock: ASURA's `Instance::run` owns one per map and wakes on its own interval
        // (`ASURA/crates/asura-world/src/instance.rs:394-403`), independent of anything a client sends -
        // "a map still ticks every interval, it just no longer owns a task that sleeps". A tick driven
        // by the socket's own read timeout is starved by exactly the traffic it should be pacing (each
        // arriving message restarts the wait), and a starving clock is a doorway crossed late.
        //
        // So the deadline is **absolute**: `next_tick` moves by one period from the tick that fired,
        // whatever the client sends in between.
        let period = std::time::Duration::from_millis(u64::from(self.settings.simulation_tick_ms));
        let mut next_tick = std::time::Instant::now() + period;

        loop {
            // The world is going down: the same kick the pre-load loop reads, and here it is what
            // lets the session finish through its own tail instead of being dropped mid-future.
            if self.closing() {
                return;
            }
            // A party crossing asks every other member's own session to run the same redirect here.
            // Each connection writes its own hand-off row and token; the destination district is the
            // one the crossing chose, so they land in the same copy.
            if loaded && let Some(order) = self.world.take_travel_order(player) {
                self.begin_transfer(
                    io,
                    peer,
                    crypt,
                    sheet,
                    &order.destination,
                    order.landing,
                    order.district,
                )
                .await;
                return;
            }
            // Whatever other sessions pushed at this one - an introduction, a farewell, a move - is
            // written before anything else is read: it travels plaintext and is scrambled here, with
            // this connection's own cipher (`game/Districts/Peers.rs`).
            if loaded {
                self.flush_outbox(io, peer, crypt).await;
            }
            while let Some(bytes) = pending.first_chunk::<2>() {
                let header = u16::from_le_bytes(*bytes);
                let used = match crate::server::protocol::decode_game_message(&pending, true) {
                    Ok((_, used)) => used,
                    Err(FrameError::Incomplete { .. }) => break,
                    Err(error) => {
                        self.say(
                            LogLevel::Warn,
                            format!(
                                "Gameplay connection from {peer}: a message could not be framed ({error}); the load stops here."
                            ),
                        );
                        let _ = io.close().await;
                        return;
                    }
                };
                let message: Vec<u8> = pending.drain(..used).collect();

                // One complete incoming frame: the capture records the wire layer, after ARC4 came off.
                crate::server::capture::record("CToS", header, &message);

                match header {
                    // The items ask: the inventory stream and the map's record, measured in one
                    // block. The manifest pair that ends it names the map in its second form.
                    instance_packets::CLIENT_LOAD_REQUEST_ITEMS => {
                        let frames = state.items_answer(&inventory, map);
                        self.send(
                            io,
                            peer,
                            crypt,
                            &frames,
                            "the inventory and the map's record",
                        )
                        .await;
                    }
                    // The spawn request: a random point of this map's own list, scattered (R13),
                    // and the first simulation tick.
                    instance_packets::CLIENT_LOAD_REQUEST_SPAWN => {
                        let point = entry;
                        let frames = match point {
                            Some(point) => {
                                self.say(
                                    LogLevel::Debug,
                                    format!(
                                        "Gameplay connection from {peer}: the character stands at {:.2}, {:.2} (plane {}) on map {}.",
                                        point.0, point.1, point.2, map.id
                                    ),
                                );
                                if let Ok(mut agent) = self.agent.lock() {
                                    *agent = Some(crate::agents::Agent::new(
                                        self.player_id(),
                                        (point.0, point.1),
                                        point.2,
                                        // A doorway crossing hands its arrival's facing over, and
                                        // every other entry faces the direction its spawn point
                                        // states (`Standing::at_spawn`, `agent.rs:460-476`; a point
                                        // with no angle keeps `(1, 0)`, `map_spawns.rs:44-53`).
                                        point.3,
                                        self.settings.agent_speed,
                                    ));
                                }
                                self.start_portal_position((point.0, point.1));
                                vec![state.spawn_answer(map)]
                            }
                            None => {
                                // A map with no entry spawn and no doorway arrival cannot be entered, and
                                // the working implementation says so by refusing the load rather than
                                // leaving the client on a loading screen
                                // (`ASURA/crates/asura-world/src/agent.rs:513-527`). The connection ends
                                // with the reason, exactly as there.
                                self.say(
                                    LogLevel::Error,
                                    format!(
                                        "Gameplay connection from {peer}: map {} '{}' has no configured entry spawn or doorway arrival; the load is refused.",
                                        map.id, map.name
                                    ),
                                );
                                return;
                            }
                        };
                        // The answer is the entry point and nothing else: `spawn_block` in the working
                        // implementation is exactly one `0x0196` frame
                        // (`ASURA/crates/asura-world/src/map.rs:13-20`), and the simulation's ticks
                        // come from the ticker, not from this answer. This answer used to append a
                        // first `0x001E` of its own, which is one message the client was never sent
                        // by the working implementation.
                        self.send(io, peer, crypt, &frames, "the entry position")
                            .await;
                    }
                    // The players request: the whole block the reference answers it with, frame for
                    // frame and in its own order (`GameSrv_HandleInstanceLoadRequestPlayers`, ported
                    // from `ASURA/crates/asura-world/src/player.rs`, `player_block`). It is one block,
                    // not a menu: the PvP unlocks and the unlockable-item table, the account features,
                    // the unlocked lists and the team token, the skills and the progress record, the
                    // visibility and player flags, the player and the agent it is drawn as, the
                    // agent's own properties and look, the party, and the message that ends the load.
                    instance_packets::CLIENT_LOAD_REQUEST_PLAYERS => {
                        // The instance this character is now standing in is **reached**, so it is
                        // unlocked - the walk sets the bit and the bit never gates the walk
                        // (`ASURA/crates/asura-world/src/player.rs:442-462`). Written before the bitmap
                        // is read, so the list the client gets names the map it is in even on the first
                        // arrival; the creation instance is not a place a character "reaches" and sets
                        // nothing.
                        let mut reached_here = false;
                        // The instances the character has reached, as the client addresses them: bit
                        // `n % 32` of word `n / 32`.
                        let mut unlocked = UNLOCKED_MAP_ZEROS.to_vec();
                        if let Some(pool) = self.characters.as_ref() {
                            let characters = crate::characters::DatabaseCharacters::new(
                                std::sync::Arc::clone(pool),
                                self.log.clone(),
                            );
                            let uuid = crate::characters::uuid_text(&sheet.uuid);
                            if !map.is_creation() {
                                match characters.unlock_map(&uuid, map.id) {
                                    Ok(true) => {
                                        reached_here = true;
                                        self.say(
                                            LogLevel::Debug,
                                            format!(
                                                "Gameplay connection from {peer}: map {} '{}' is unlocked - the character arrived in it (R12).",
                                                map.id, map.name
                                            ),
                                        );
                                    }
                                    Ok(false) => {}
                                    Err(error) => self.say(
                                        LogLevel::Warn,
                                        format!(
                                            "Gameplay connection from {peer}: the arrival in map {} cannot be recorded: {error}",
                                            map.id
                                        ),
                                    ),
                                }
                            }
                            match characters.unlocked_maps(&uuid) {
                                Ok(maps) => {
                                    for map_id in maps {
                                        let word = (map_id / 32) as usize;
                                        if let Some(slot) = unlocked.get_mut(word) {
                                            *slot |= 1 << (map_id % 32);
                                        }
                                    }
                                }
                                Err(error) => self.say(
                                    LogLevel::Warn,
                                    format!(
                                        "Gameplay connection from {peer}: the character's reached instances cannot be read ({error}); the list is sent with only what this load knows."
                                    ),
                                ),
                            }
                            // "a client told it is nowhere can offer nowhere": the map it is in is on
                            // the list whether or not the read above worked (`player.rs:463-471`).
                            if !map.is_creation() || reached_here {
                                let word = (map.id / 32) as usize;
                                if let Some(slot) = unlocked.get_mut(word) {
                                    *slot |= 1 << (map.id % 32);
                                }
                            }
                        }
                        let load_ms =
                            u32::try_from(load_started.elapsed().as_millis()).unwrap_or(u32::MAX);

                        // Everyone already in this copy of the map is introduced first: the player, its
                        // party size, its party and its agent, in that order
                        // (`ASURA/crates/asura-world/src/player.rs:499-501`, `introduction`).
                        let standing = crate::districts::Standing {
                            position,
                            plane,
                            direction: (0.0, 1.0),
                        };
                        // The character's guild, read now and shown with this copy's local id for it
                        // (`ASURA/crates/asura-world/src/connection.rs:405-410` and `:445-449`). A
                        // creation instance has no character to be in a guild yet.
                        let guild_badge = if map.is_creation() {
                            None
                        } else {
                            self.load_guild(peer, sheet)
                        };
                        let me = crate::districts::PeerView {
                            player_id: player,
                            character: sheet.uuid,
                            agent_id: agent,
                            name: sheet.name.clone(),
                            standing,
                            equipped,
                            appearance,
                            profession: sheet.profession,
                            secondary_profession: sheet.secondary_profession,
                            level: sheet.level,
                            wearing: wearing.clone(),
                            displayed_rank: self.world.titles().initial_display_rank(),
                            map_id: map.id,
                            district: self.district(),
                            guild: guild_badge.clone(),
                        };
                        let (sending, receiving) = std::sync::mpsc::channel();
                        let (others, session_token) = match self.peers.lock() {
                            Ok(mut peers) => peers.join(me.clone(), sending),
                            Err(_) => (Vec::new(), 0),
                        };
                        if let Ok(mut outbox) = self.outbox.lock() {
                            *outbox = Some(receiving);
                        }
                        if let Ok(mut token) = self.session_token.lock() {
                            *token = Some(session_token);
                        }
                        // This player's party. A member of a party that crossed a doorway adopts the
                        // roster the crossing carried, so the arrivals are one party and not one each
                        // (`ASURA/crates/asura-world/src/party.rs:1207-1525`, `arrive`).
                        // roster the crossing carried, so the arrivals are one party and not one each
                        // (`ASURA/crates/asura-world/src/party.rs:1207-1525`, `arrive`).
                        let held_party =
                            self.world.take_held_party(map.id, self.district(), player);
                        // This player's party of one, founded in the copy's table the moment it joins it,
                        // and every other player's party as the table has it - read after the peers'
                        // lock is released, the lock order `game/Parties/mod.rs` keeps
                        // (`ASURA/crates/asura-world/src/instance.rs:561-572`, `party.rs:376-381`).
                        let (party, other_parties) = match self.world.parties().lock() {
                            Ok(mut parties) => {
                                let table = parties.table(map.id, self.district());
                                if let Some(held) = held_party {
                                    table.adopt(held, player);
                                } else if table
                                    .party_of(player)
                                    .is_some_and(|party| party.absent.contains_key(&player))
                                {
                                    // A member of a party whose roster was adopted by the first
                                    // arrival: its own connection has landed, so it stops being
                                    // absent. This is the travel case, not a login.
                                    table.arrive_member(player);
                                } else {
                                    // A login is a new session: a party the table still remembers is
                                    // stale (the old connection is gone). Do not put the returning
                                    // character back in it.
                                    table.forget_member(player);
                                    table.found(player);
                                }
                                let others: Vec<(u32, u32)> = others
                                    .iter()
                                    .map(|other| table.leader_and_size(other.player_id))
                                    .collect();
                                (table.party_of(player).cloned(), others)
                            }
                            Err(_) => (None, Vec::new()),
                        };
                        eprintln!(
                            "world: party: load player {player} on map {} district {}: party {:?}, peers {:?}",
                            map.id,
                            self.district(),
                            party.as_ref().map(|party| (
                                &party.party_id,
                                &party.leader,
                                &party.members,
                                &party.absent
                            )),
                            others.iter().map(|peer| peer.player_id).collect::<Vec<_>>()
                        );
                        let mut frames = Vec::with_capacity(40);
                        let mut block = state.load_block(
                            &self.settings,
                            load_ms,
                            &unlocked,
                            self.world.titles(),
                        );
                        // The party state every live load ends with, before its finish, and the
                        // player's own party as the table has it in place of the block's party of one
                        // (`ASURA/crates/asura-world/src/player.rs:735-738` and `770-775`).
                        let in_outpost = map.kind == Some(crate::world::map_kind::OUTPOST);
                        crate::parties::splice_load_tail(&mut block, in_outpost);
                        // The guild block in front of the party block, where a live load carries it:
                        // `0x011C` alone for a character in no guild. Not in a creation instance, which
                        // no capture shows a guild message in
                        // (`ASURA/crates/asura-world/src/player.rs:740-756`).
                        if !map.is_creation() {
                            self.splice_guild_block(peer, &mut block, guild_badge.as_ref(), agent);
                        }
                        if let Some(party) = &party {
                            crate::parties::splice_load_section(&mut block, party, player);
                        }
                        frames.extend(block);
                        // Everyone already in the map is introduced **after** the load block, in the
                        // reference's own order (`ASURA/crates/asura-world/src/player.rs:784-822`).
                        // Before it, the client created those players before it had been told its own
                        // team token (`0x00F2`, inside the block), so it treated them as hostile.
                        for (index, peer) in others.iter().enumerate() {
                            let peer_party = other_parties
                                .get(index)
                                .copied()
                                .unwrap_or((peer.player_id, 1));
                            frames.extend(crate::districts::introduction(
                                peer,
                                peer_party,
                                self.settings.agent_speed,
                                PLAYER_TEAM_TOKEN,
                            ));
                        }
                        // The party members it was just introduced to are in the map
                        // (`ASURA/crates/asura-world/src/player.rs:823-830`, `in_map_notices`).
                        if let Some(party) = &party {
                            let entered: Vec<u32> =
                                others.iter().map(|other| other.player_id).collect();
                            frames.extend(crate::parties::in_map_notices(party, player, &entered));
                        }
                        let count = frames.len();
                        self.send(
                            io,
                            peer,
                            crypt,
                            &frames,
                            "the players and the load's last message",
                        )
                        .await;
                        self.say(
                            LogLevel::Debug,
                            format!(
                                "Gameplay connection from {peer}: map {} is loaded - the character '{}' is in it ({count} message(s), {} other player(s) introduced).",
                                map.id,
                                sheet.name,
                                others.len()
                            ),
                        );
                        // And the others are told this body is here with the same whole introduction
                        // they were just given for it: the player, its party pair, its professions and
                        // level, its flags, its effects, the agent, what it wears, visual equipment and
                        // cape (`ASURA/crates/asura-world/src/player.rs:844-851`). Only to the players
                        // this block introduced: one still loading meets it in its own block.
                        let my_party = party
                            .as_ref()
                            .map(|party| (party.leader, party.members.len() as u32))
                            .unwrap_or((player, 1));
                        let hello = crate::districts::introduction(
                            &me,
                            my_party,
                            self.settings.agent_speed,
                            PLAYER_TEAM_TOKEN,
                        );
                        if let Ok(peers) = self.peers.lock() {
                            peers.broadcast(player, map.id, self.district(), &hello);
                        }
                        // **After** the introduction, never before it: the client needs the new
                        // member's player record before the party-entered notice can name it. ASURA
                        // sends `introduction` and then `announce_arrival` (`player.rs:844-854`); the
                        // reverse is the blank party row that Kryta had.
                        if let Some(party) = &party
                            && let Ok(peers) = self.peers.lock()
                        {
                            for member in party.present() {
                                if member != player {
                                    peers.send_to(
                                        member,
                                        &[party_packets::party_member_enter_world(
                                            party.party_id,
                                            player as u16,
                                        )],
                                    );
                                }
                            }
                        }
                        loaded = true;
                    }
                    // ASURA crates/asura-protocol/src/packets.rs:422-423,772 supplies the layouts.
                    // Retail login-unfiltered-9292 seq 659-661 replies with the selected rank.
                    // Preforged packets-1360-15821281.jsonl:36252 shows rank zero for no title.
                    GAME_CMSG_TITLE_DISPLAY | GAME_CMSG_TITLE_HIDE if loaded => {
                        let rank = if header == GAME_CMSG_TITLE_HIDE {
                            Some(0)
                        } else {
                            let title = u32::from_le_bytes(message[2..6].try_into().unwrap());
                            self.world.titles().selectable_rank(title)
                        };
                        if let Some(rank) = rank {
                            let reply = [title_packets::rank_display(player as u16, rank)];
                            if let Ok(mut peers) = self.peers.lock() {
                                peers.set_displayed_rank(player, rank);
                                peers.broadcast(player, map.id, self.district(), &reply);
                            }
                            self.send(io, peer, crypt, &reply, "the displayed title")
                                .await;
                        }
                    }
                    // A drag in the inventory: where the item goes is a row, the rules are ASURA's, and
                    // the client is told the placement back. `0x8072` is the client's own `9` bytes - the
                    // item, the bag and the slot - and `0x013E` is the placement.
                    CLIENT_ITEM_MOVE => {
                        self.answer_item_move(io, peer, &message, crypt, sheet)
                            .await;
                    }
                    // `GAME_CMSG_PING_REPLY` (`0x8009`, 10 bytes) and `GAME_CMSG_HEARTBEAT` (`0x800C`,
                    // 2 bytes): the working implementation names both and discards both
                    // (`ASURA/crates/asura-world/src/dispatch.rs:116-122`), because the reference does.
                    CLIENT_PING_REPLY | CLIENT_HEARTBEAT => {}
                    // The client's own ping on a live connection: the reference logs it and
                    // answers nothing, so this does the same.
                    // `GAME_CMSG_PING_REQUEST` (`0x800D`) on a loaded connection: the same answer as
                    // before the load, because the working implementation has one handler for the
                    // connection's whole life (`ASURA/crates/asura-world/src/connection.rs:579-587`).
                    GAME_PING => {
                        self.send(
                            io,
                            peer,
                            crypt,
                            &[misc_packets::ping_reply(self.settings.ping_value)],
                            "the ping reply",
                        )
                        .await;
                    }
                    // The client's movement. Four messages, four handlers, ported from the working
                    // implementation's own (`ASURA/crates/asura-world/src/agent.rs`): `0x803E` asks for
                    // a walk, `0x803D` reports where the client is and the direction it is walking in,
                    // `0x8028` cancels, and `0x8047` is the stopping point a cancel carries.
                    CLIENT_MOVE_TO_COORD => {
                        self.answer_move_to_coord(io, peer, &message, crypt).await;
                    }
                    CLIENT_MOVE_TO_COORD_WITH_DIR => {
                        self.answer_position_update(io, peer, &message, crypt).await;
                        // The step the report moved the agent along, checked against the map's own
                        // walls (`ASURA/crates/asura-world/src/dispatch.rs:44-54`).
                        if let Some((from, to)) = self.take_portal_step() {
                            self.cross_portal(io, peer, crypt, sheet, from, to).await;
                        }
                    }
                    CLIENT_CANCEL_MOVEMENT => {
                        self.answer_cancel_movement(io, peer, crypt).await;
                    }
                    CLIENT_LAST_POS_BEFORE_MOVE_CANCELED => {
                        self.answer_last_position(io, peer, &message, crypt).await;
                        // A correction the server accepted is a movement the reports lagged behind, so
                        // it is checked for a doorway the same way a walk is
                        // (`ASURA/crates/asura-world/src/dispatch.rs:60-68`).
                        if let Some((from, to)) = self.take_portal_step() {
                            self.cross_portal(io, peer, crypt, sheet, from, to).await;
                        }
                    }
                    // The client's own in-game travel request. `ASURA/crates/asura-world/src/travel.rs`
                    // answers it with the same cycle `/tp` ends in: a map this world hosts goes to
                    // `begin_transfer`, and one it cannot serve is refused in the log.
                    CLIENT_PARTY_TRAVEL => {
                        self.answer_party_travel(io, peer, &message, crypt, sheet)
                            .await;
                    }
                    party_packets::CLIENT_PARTY_RETURN_TO_OUTPOST if loaded => {
                        self.answer_return_to_outpost(io, peer, crypt, sheet).await;
                    }
                    party_packets::CLIENT_PARTY_CANCEL_ENTER_CHALLENGE if loaded => {
                        let cancelled = self.world.parties().lock().ok().and_then(|mut parties| {
                            parties.cancel_countdown(
                                self.map_id(),
                                self.district(),
                                self.player_id(),
                            )
                        });
                        if let Some((present, ticket)) = cancelled {
                            self.world.cancel_countdown_ticket(ticket);
                            if let Ok(peers) = self.peers.lock() {
                                for member in present {
                                    peers.send_to(member, &[party_packets::party_lock(0, 0)]);
                                }
                            }
                        }
                    }
                    // The party window (`game/Parties/mod.rs`, ported from
                    // `ASURA/crates/asura-world/src/party.rs`; the arms are ASURA's
                    // `dispatch.rs:102-114`). Each request is answered through the copy's party table and
                    // its batches reach the players it names through their own sessions.
                    party_packets::CLIENT_PARTY_INVITE_PLAYER
                    | party_packets::CLIENT_PARTY_ACCEPT_INVITE
                    | party_packets::CLIENT_PARTY_ACCEPT_CANCEL
                    | party_packets::CLIENT_PARTY_ACCEPT_REFUSE
                    | party_packets::CLIENT_PARTY_LEAVE_GROUP
                    | party_packets::CLIENT_PARTY_KICK_PLAYER
                        if loaded =>
                    {
                        self.answer_party_request(peer, map, header, &message);
                    }
                    // The client touched a travel object (`0x80C1`): named in the log and nothing more,
                    // because a crossing is decided from the player's own step, not from the touch
                    // (`ASURA/crates/asura-world/src/travel.rs:132-169`).
                    CLIENT_TRAVEL_INIT => {
                        self.answer_travel_init(peer, &message, sheet).await;
                    }
                    // The client speaks: a line on a channel, or one of the reference's commands
                    // (`ASURA/crates/asura-world/src/chat.rs`).
                    CLIENT_CHAT_MESSAGE => {
                        self.answer_chat_message(io, peer, &message, crypt, sheet)
                            .await;
                    }
                    // The guild panel and the registration window
                    // (`ASURA/crates/asura-world/src/dispatch.rs:116-129`; `Handlers/GuildHandler.rs`).
                    guild_packets::CLIENT_GUILD_CREATE if loaded => {
                        self.answer_guild_create(io, peer, crypt, &message, sheet)
                            .await;
                    }
                    guild_packets::CLIENT_GUILD_INVITE if loaded => {
                        self.answer_guild_invite(io, peer, crypt, &message, sheet)
                            .await;
                    }
                    guild_packets::CLIENT_GUILD_LEAVE if loaded => {
                        self.answer_guild_leave(io, peer, crypt, &message, sheet)
                            .await;
                    }
                    guild_packets::CLIENT_GUILD_ANNOUNCEMENT if loaded => {
                        self.answer_guild_announcement(io, peer, crypt, &message, sheet)
                            .await;
                    }
                    // The client destroys one of its items: the row goes, and the item is told gone
                    // with `0x014D`.
                    // The client drops `count` of an item (`GAME_CMSG_DROP_ITEM`, `0x802C`: the item as a
                    // dword and the count as a byte). The working implementation's `drop_item`
                    // (`ASURA/crates/asura-world/src/item.rs:187`) takes them off the stack and puts
                    // **nothing on the wire**: both messages a drop could send crashed a live client -
                    // `ITEM_REMOVE` with the stack re-created cloned the item, and the ground agent
                    // (`0x0020`, `AgentType_Item`) crashed with that message as the log's last line -
                    // and neither appears in any capture (`item.rs:236-246`). So the stack is the state
                    // and silence is the answer.
                    CLIENT_DROP_ITEM => self.answer_drop_item(peer, &message, sheet),
                    // `GAME_CMSG_UNEQUIP_ITEM` (`0x804F`): the working implementation answers it by
                    // saying what it cannot do - the message carries no item id and no capture shows
                    // what it means, and dragging the item out of the equipped bag works instead
                    // (`ASURA/crates/asura-world/src/item.rs:555`).
                    CLIENT_UNEQUIP_ITEM => self.answer_unequip_item(peer),
                    // The client wears one of its items: the slot its kind is worn in, in the equipped
                    // bag, with whatever was worn there taken off to the first free carrying slot - and
                    // the agent's worn items sent again, which is the one thing the client cannot work
                    // out for itself (`ASURA/crates/asura-world/src/item.rs:314-436`).
                    CLIENT_EQUIP_ITEM => {
                        self.answer_equip_item(io, peer, &message, crypt, sheet)
                            .await;
                    }
                    other => {
                        // Once per header, with the protocol's own name for it; repeats are a count
                        // (`WorldSession::report_unanswered`).
                        let what = crate::server::protocol::label(other);
                        self.report_unanswered(peer, other, &what, message.len());
                    }
                }
            }

            // The two things this connection waits on, and neither is a poll: the client's next bytes,
            // and the map's clock. `ASURA/crates/asura-world/src/connection.rs:494-548` races the same
            // two, and the clock's moment does not move because traffic arrived.
            let wait = next_tick.saturating_duration_since(std::time::Instant::now());
            let read = io.read_timeout(&mut buffer, wait).await;
            match read {
                Ok(shared::network::socket::ReadOutcome::TimedOut) => {
                    // Cadence, not drift: the next deadline moves by one period from the one that
                    // fired, the way `tokio::time::interval` does in `Instance::run`
                    // (`ASURA/crates/asura-world/src/instance.rs:394-403`). Measuring the period from
                    // "now" instead would add the tick's own work to every interval, and the map would
                    // run a few percent slow for as long as it runs. A missed deadline is caught up one
                    // tick at a time, which is that interval's own default.
                    next_tick += period;
                    if loaded {
                        // **How long it has been since the last tick**, not a counter: the client advances
                        // its simulation - and its sync copy of every agent - by this number
                        // (`ASURA/crates/asura-world/src/instance.rs:369`, `world_simulation_tick`); a
                        // counter here makes everything the client draws run at the wrong speed.
                        let now = std::time::Instant::now();
                        let elapsed = now.duration_since(last_tick);
                        last_tick = now;
                        // The map's clock advances every agent by the elapsed time, and **the doorway is
                        // decided on the segment the advance walked** - not on a report's - because that is
                        // the copy this server believes and the one the client is drawn from
                        // (`ASURA/crates/asura-world/src/instance.rs:290-308`). A crossing found here is
                        // pushed to the connection and served before anything else, as `portal_inbox` is in
                        // `connection.rs:498-502`.
                        let (crossed, walk) = self.step_map(elapsed);
                        let mut frames = vec![misc_packets::world_simulation_tick(
                            u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX),
                        )];
                        if let Some(walk) = walk {
                            frames.push(walk);
                        }
                        // The tick is periodic, so its report is routine: one line per connection per
                        // `World.SimulationTickMs` at `Info` was the whole of a flowing log.
                        self.send_quiet(io, peer, crypt, &frames, "a tick").await;
                        if let Some((from, to)) = crossed {
                            // The travelling body leaves the map it is in: the clock that crossed it has
                            // already been announced, then the transfer is served.
                            self.cross_portal(io, peer, crypt, sheet, from, to).await;
                            return;
                        }
                    }
                }
                Ok(shared::network::socket::ReadOutcome::Closed) => return,
                Ok(shared::network::socket::ReadOutcome::Read(count)) => {
                    // The asks after the seed are scrambled with the key both sides derive, the
                    // same as in the run loop above.
                    let mut plain = buffer[..count].to_vec();
                    crypt.decrypt(&mut plain);
                    pending.extend_from_slice(&plain);
                }
                Err(error) => {
                    self.say(
                        LogLevel::Warn,
                        format!("Gameplay connection from {peer} could not be read: {error}"),
                    );
                    return;
                }
            }
        }
    }

    /// Write where the destination puts the character before the redirect, or clear the saved position
    /// when the trigger states none.
    ///
    /// `ASURA/crates/asura-world/src/travel.rs:446-468`: best-effort - a failed write must not refuse a
    /// travel the player asked for, and a spawn point is the worst case if it does.
    pub(crate) fn place_for_travel(
        &self,
        destination: &crate::world::MapEntry,
        landing: Option<(f32, f32)>,
        sheet: &CharacterSheet,
    ) {
        let Some(pool) = self.characters.as_ref() else {
            return;
        };
        let repository = crate::characters::DatabaseCharacters::new(
            std::sync::Arc::clone(pool),
            self.log.clone(),
        );
        let uuid = crate::characters::uuid_text(&sheet.uuid);
        match landing {
            Some((x, y)) => {
                let position = crate::characters::Position {
                    map_id: destination.id,
                    x,
                    y,
                    plane: 0,
                };
                if let Err(error) = repository.record_position(&uuid, &position) {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "The landing point for map {} was not written ({error}); the destination's spawn list answers instead.",
                            destination.id
                        ),
                    );
                }
            }
            None => {
                if let Err(error) = repository.forget_position(&uuid) {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "The saved position was not cleared for the travel to map {} ({error}); the destination may resume where the character stood.",
                            destination.id
                        ),
                    );
                }
            }
        }
    }

    /// The position this character was saved at, or `None` when it has never been saved.
    ///
    /// `ASURA/crates/asura-world/src/connection.rs:257-261`: read once, at the load, and handed to the
    /// entry rule. `pos_plane IS NULL` is what "never saved" means
    /// (`ASURA/crates/asura-store/src/state/character.rs:226-246`).
    fn saved_position(&self, sheet: &CharacterSheet) -> Option<crate::characters::Position> {
        let pool = self.characters.as_ref()?;
        let repository = crate::characters::DatabaseCharacters::new(
            std::sync::Arc::clone(pool),
            self.log.clone(),
        );
        match repository.position(&crate::characters::uuid_text(&sheet.uuid)) {
            Ok(position) => position,
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!(
                        "The character's saved position cannot be read ({error}); the entry rule places it without one."
                    ),
                );
                None
            }
        }
    }

    /// One point of a map's own spawn list, scattered by the world's setting (R13) - or, when the
    /// travel this connection claimed was a doorway crossing, the landing that doorway states (R21).
    ///
    /// The rule is the agent's own and lives in `crate::agents::entry_point`
    /// (`game/src/Agents/Entry.rs`, ASURA's `Standing::at_spawn`/`arrival`/`resume`, `agent.rs:460-545`);
    /// what this connection adds is the three things the rule needs that only it knows: the landing the
    /// claimed travel stated, the position the character was saved at, and the world's scatter.
    fn entry_point(
        &self,
        map: &crate::world::MapEntry,
        saved: Option<crate::characters::Position>,
    ) -> Option<(f32, f32, u16, (f32, f32))> {
        let landing = self
            .landing
            .lock()
            .ok()
            .and_then(|claimed| *claimed)
            .map(|claimed| crate::agents::Landing {
                map_id: claimed.map_id,
                x: claimed.landing.x,
                y: claimed.landing.y,
                plane: claimed.landing.plane,
                facing: claimed.facing,
            });
        crate::agents::entry_point(&crate::agents::Entering {
            map,
            spawns: &self.spawns,
            landing,
            saved,
            scatter: self.settings.spawn_scatter,
        })
    }

    /// Scramble and send whole frames, and say so.
    ///
    /// The report is `Info`: a batch this world was asked for is an event. A periodic frame is not - the
    /// simulation tick goes through [`WorldSession::send_quiet`] instead, because at `Info` it wrote one
    /// line every `World.SimulationTickMs` for every connection (`docs/port/03-tracing.md`, which reports
    /// the tick by its effect rather than by a line).
    pub(crate) async fn send(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        frames: &[Vec<u8>],
        what: &str,
    ) -> bool {
        self.send_with(LogLevel::Debug, io, peer, crypt, frames, what)
            .await
    }

    /// Send a batch whose report is routine: a periodic frame, which is a `Trace` line - below both
    /// the console (`Info`) and the default log file (`Debug`), so it appears only when asked for.
    pub(crate) async fn send_quiet(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        frames: &[Vec<u8>],
        what: &str,
    ) -> bool {
        self.send_with(LogLevel::Trace, io, peer, crypt, frames, what)
            .await
    }

    async fn send_with(
        &self,
        level: LogLevel,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        frames: &[Vec<u8>],
        what: &str,
    ) -> bool {
        // The capture records the wire layer: the plaintext frame, header first, before ARC4
        // (`game/Server/Capture.rs`, and `docs/port/02-wire.md` for why the layer is stated).
        for frame in frames {
            if frame.len() >= 2 {
                crate::server::capture::record(
                    "StoC",
                    u16::from_le_bytes([frame[0], frame[1]]),
                    frame,
                );
            }
        }
        let mut answer = Vec::new();
        for frame in frames {
            answer.extend_from_slice(frame);
        }
        crypt.encrypt(&mut answer);
        match io.write_all(&answer).await {
            Ok(()) => {
                // The batch the client has just been handed is evidence the moment it is sent: put it
                // on the disk now, so a program killed moments later still holds its last messages
                // (`server/Capture.rs`, FLUSH_EVERY is the bound for everything else).
                crate::server::capture::flush();
                self.log.out(
                    LogLevel::Debug,
                    "network.trace",
                    format!(
                        "trace {peer} game out {} bytes: {}",
                        answer.len(),
                        shared::network::socket::hex(&answer[..answer.len().min(64)])
                    ),
                );
                self.say(
                    level,
                    format!(
                        "Gameplay connection from {peer}: {what} answered with {} message(s), {} bytes.",
                        frames.len(),
                        answer.len()
                    ),
                );
                true
            }
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Gameplay connection from {peer} could not be answered: {error}"),
                );
                false
            }
        }
    }
}

/// Seconds since the unix epoch, the clock the district hold counts on (R37).
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

impl Session for WorldSession {
    fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
        Box::pin(async move {
            self.converse(&mut io).await;
            // The connection ended, whatever ended it: its place in the district is given back. The
            // copy itself stays open - R37 holds an outpost for 15 minutes of inactivity and R40
            // disposes exactly as the lifecycle says - so this only keeps the count honest.
            self.leave_district();
            // The last frames of a connection are the ones a diagnostic capture exists for, and a
            // session that ends quietly would leave them in the capture's buffer until the process is
            // killed (`server/Capture.rs`, FLUSH_EVERY). Flush them here, where the connection ends.
            crate::server::capture::flush();
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

impl WorldSession {
    /// One party request from the party window, answered through this copy's party table
    /// (`game/Parties/mod.rs`; ASURA `dispatch.rs:102-114` and the handlers in `party.rs:547-1090`).
    ///
    /// The copy's population is read first and its lock released; the party table is then asked; its
    /// batches are delivered once that lock is released too - so the two locks are never held together.
    /// A refused request is answered with nothing on the wire, which is ASURA's answer (what a live server
    /// does then is not captured), and the refusal is the log line.
    fn answer_party_request(
        &self,
        peer: &str,
        map: &crate::world::MapEntry,
        header: u16,
        message: &[u8],
    ) {
        let me = self.player_id();
        let district = self.district();
        let body = message.get(2..).unwrap_or_default();
        let word = party_packets::read_party_word(body);
        let everyone = self
            .peers
            .lock()
            .map(|peers| peers.everyone(map.id, district))
            .unwrap_or_default();
        let here = crate::parties::Here {
            me,
            in_outpost: map.kind == Some(crate::world::map_kind::OUTPOST),
            max_size: self.world.max_party_size(map.id),
            everyone,
        };
        let parties = self.world.parties();
        let (what, answer) = {
            let Ok(mut parties) = parties.lock() else {
                return;
            };
            let table = parties.table(map.id, district);
            match (header, word) {
                (party_packets::CLIENT_PARTY_INVITE_PLAYER, Some(target)) => (
                    format!("invites player {target}"),
                    crate::parties::handle_invite(table, &here, u32::from(target)),
                ),
                (party_packets::CLIENT_PARTY_ACCEPT_INVITE, Some(party)) => (
                    format!("accepts the invitation of party {party}"),
                    crate::parties::handle_accept(table, &here, party),
                ),
                (party_packets::CLIENT_PARTY_ACCEPT_CANCEL, Some(party)) => (
                    format!("withdraws the invitation of party {party}"),
                    crate::parties::handle_cancel(table, &here, party),
                ),
                (party_packets::CLIENT_PARTY_ACCEPT_REFUSE, Some(party)) => (
                    format!("declines the invitation of party {party}"),
                    crate::parties::handle_refuse(table, &here, party),
                ),
                (party_packets::CLIENT_PARTY_LEAVE_GROUP, _) => (
                    "leaves its party".to_string(),
                    crate::parties::handle_leave(table, &here),
                ),
                (party_packets::CLIENT_PARTY_KICK_PLAYER, Some(target)) => (
                    format!("kicks player {target}"),
                    crate::parties::handle_kick(table, &here, u32::from(target)),
                ),
                _ => (
                    format!("sends party message 0x{header:04X}"),
                    Err(format!("{} byte(s) carry no word", body.len())),
                ),
            }
        };
        match answer {
            Ok(plan) => {
                self.say(
                    LogLevel::Info,
                    format!(
                        "Gameplay connection from {peer}: party: player {me} {what}; {} batch(es) sent.",
                        plan.len()
                    ),
                );
                self.deliver_party_plan(plan);
            }
            Err(why) => self.say(
                LogLevel::Info,
                format!(
                    "Gameplay connection from {peer}: party: player {me} {what}: refused ({why}); nothing sent."
                ),
            ),
        }
    }

    /// Deliver a party plan: each player's frames through its own session
    /// (`ASURA/crates/asura-world/src/party.rs:507-511`, `deliver`; `Peers::send_to`).
    pub(crate) fn deliver_party_plan(&self, plan: crate::parties::Plan) {
        if plan.is_empty() {
            return;
        }
        if let Ok(peers) = self.peers.lock() {
            for (to, frames) in plan {
                peers.send_to(to, &frames);
            }
        }
    }

    /// The connection ended: its place in the district is given back.
    ///
    /// The copy itself stays open - R37 holds an outpost for 15 minutes of inactivity and R40
    /// disposes exactly as the lifecycle says - so this only keeps the count honest; the lifecycle
    /// that acts on an empty copy arrives with the instance table.
    fn leave_district(&self) {
        // The position the character leaves at is written before the place is given back: after the
        // take below this connection no longer knows which map it was in.
        self.save_position();
        let entered = self
            .entered
            .lock()
            .ok()
            .and_then(|mut entered| entered.take());
        if let Some((map_id, district)) = entered
            && let Ok(mut districts) = self.districts.lock()
        {
            districts.leave(map_id, district, unix_now());
        }
        // Its parties are told it left the map before its body goes: the invitations it was in are
        // closed and its own party, if it had company, sees it leave (`game/Parties/mod.rs`,
        // `left_instance`; ASURA sends these ahead of the farewell, `connection.rs:552-579`).
        if let Some((map_id, district)) = entered {
            let player_id = self.player_id();
            let everyone = self
                .peers
                .lock()
                .map(|peers| peers.everyone(map_id, district))
                .unwrap_or_default();
            let here = crate::parties::Here {
                me: player_id,
                in_outpost: self
                    .world
                    .map(map_id)
                    .is_some_and(|map| map.kind == Some(crate::world::map_kind::OUTPOST)),
                max_size: self.world.max_party_size(map_id),
                everyone,
            };
            let plan = match self.world.parties().lock() {
                Ok(mut parties) => {
                    let plan =
                        crate::parties::left_instance(parties.table(map_id, district), &here);
                    parties.tidy(map_id, district);
                    plan
                }
                Err(_) => Vec::new(),
            };
            self.deliver_party_plan(plan);
        }
        // And the body is taken back out of everyone else's world: the player destroyed, then its agent
        // removed, in that order (`ASURA/crates/asura-world/src/instance.rs:466-473`, `farewell`). A
        // travel's old connection must not remove the registry slot its own new connection already
        // filled, so the removal is by channel.
        let session = self.session_token.lock().ok().and_then(|token| *token);
        if let Ok(mut peers) = self.peers.lock() {
            let left = match session {
                Some(session) => peers.leave_if(self.player_id(), session),
                None => peers.leave(self.player_id()),
            };
            if let Some(peer) = left {
                let frames = crate::districts::farewell(peer.player_id, peer.agent_id);
                peers.broadcast(peer.player_id, peer.map_id, peer.district, &frames);
            }
        }
    }

    /// One step of the map's clock: advance this agent by `elapsed`, and answer the waypoint it crossed
    /// and the doorway its own movement crossed.
    ///
    /// `ASURA/crates/asura-world/src/instance.rs:290-308`, `Instance::step`: the agents are advanced
    /// first, and **the doorway is tested on the segment the advance walked** - `before` to `after`,
    /// where both are this server's own copy - because that is the position the client is drawn from. A
    /// crossing found here is served immediately (`connection.rs:498-502`, biased ahead of everything).
    fn step_map(&self, elapsed: std::time::Duration) -> StepOutcome {
        let map_id = self.map_id();
        self.with_agent(|agent| {
            let before = agent.position;
            let waypoint = agent.advance(elapsed.as_secs_f32());
            let after = agent.position;
            let crossed = if before != after {
                self.portals.crossed(map_id, before, after)
            } else {
                None
            };
            let walk = waypoint.map(|waypoint| {
                agent_packets::agent_move_to_point(
                    agent.agent_id,
                    (waypoint.pos.x, waypoint.pos.y),
                    waypoint.pos.plane,
                    agent.plane,
                )
                .1
            });
            (crossed.map(|_| (before, after)), walk)
        })
        .unwrap_or((None, None))
    }

    /// Save where the character stood, so a later load resumes there (R14).
    ///
    /// `ASURA/crates/asura-world/src/connection.rs:405-423`: written when the player leaves, and **not**
    /// when the connection was moved by a travel (the seat and the position belong to the connection
    /// the client is about to open), and never from the creation instance - that map is a conversation
    /// rather than a place, and saving it would strand the character there on the next login.
    fn save_position(&self) {
        let moved = self.moved.lock().map(|moved| *moved).unwrap_or(true);
        if moved {
            return;
        }
        let Some(map) = self.map() else {
            return;
        };
        if map.is_creation() {
            return;
        }
        let Some(uuid) = self.character.lock().ok().and_then(|character| *character) else {
            return;
        };
        let Some(((x, y), plane)) = self.with_agent(|agent| (agent.position, agent.plane)) else {
            return;
        };
        let Some(pool) = self.characters.as_ref() else {
            return;
        };
        let repository = crate::characters::DatabaseCharacters::new(
            std::sync::Arc::clone(pool),
            self.log.clone(),
        );
        let position = crate::characters::Position {
            map_id: map.id,
            x,
            y,
            plane: plane as i16,
        };
        if let Err(error) =
            repository.record_position(&crate::characters::uuid_text(&uuid), &position)
        {
            self.say(
                LogLevel::Warn,
                format!(
                    "The character's position at ({x:.1}, {y:.1}) plane {plane} on map {} was not saved: {error}",
                    map.id
                ),
            );
        }
    }

    /// The map this connection loaded, when it loaded one.
    fn map(&self) -> Option<crate::world::MapEntry> {
        let map_id = self.map_id();
        self.maps.iter().find(|map| map.id == map_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::character::creation_player_block;
    use crate::server::packets::character_packets;
    use crate::server::packets::creation_load_block;
    use crate::server::packets::item_packets;
    use crate::server::protocol::GameVersion;
    use crate::world::{MapEntry, map_kind};
    use shared::cryptography::handshake::{self, CLIENT_SEED_HEADER, KEY_LEN, MASTER_SECRET_LEN};
    use std::io::{Read, Write};
    use std::sync::Arc;

    /// The world a test serves: world 2, the character-creation instance and one outpost.
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
                    file_id: 113_021,
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
        assert_eq!(world.maps().len(), 2);

        // And the creation content, which the same start-up pass reads: a screen with no items has no
        // inventory model to draw.
        let (bags, items, sets) = world
            .load_creation(
                "../config/World/Vanilla/creation/inventory.json",
                "../config/World/Vanilla/creation/equipment.json",
                Some(std::path::Path::new(
                    "../config/World/Vanilla/creation/worn_slots.json",
                )),
                &Log::new(),
            )
            .expect("the creation data");
        assert_eq!(
            (bags, items, sets),
            (6, 9, 32),
            "the reference's own counts"
        );
        world.set_templates(templates_from(world.creation().definitions()));
        world
    }

    fn factory(
        log: Log,
        world: std::sync::Arc<World>,
        master: [u8; MASTER_SECRET_LEN],
    ) -> shared::network::socket::SessionFactory {
        Arc::new(move || {
            Box::new(WorldSession::with_master(log.clone(), &world, master)) as Box<dyn Session>
        })
    }
    /// A session the listener has told the world is closing answers nothing and ends (`KickAll`,
    /// `WorldSessionMgr.cpp:220`): its next turn is its last, which is what lets its tail give the
    /// district place back and write the position instead of being dropped mid-future.
    #[test]
    fn a_session_told_the_world_is_closing_ends_without_answering() {
        let log = Log::new();
        let world = Arc::new(world());
        let closing = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let factory: shared::network::socket::SessionFactory = {
            let log = log.clone();
            let world = Arc::clone(&world);
            let closing = Arc::clone(&closing);
            Arc::new(move || {
                let mut session =
                    WorldSession::with_master(log.clone(), &world, [0x5Bu8; MASTER_SECRET_LEN]);
                session.set_closing(Arc::clone(&closing));
                Box::new(session) as Box<dyn Session>
            })
        };
        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 99,
            map_id: 0,
            player_token: 1,
            account: [0; 16],
            character: [0; 16],
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        let mut byte = [0u8; 1];
        let mut total = 0;
        while let Ok(read) = client.read(&mut byte) {
            if read == 0 {
                break;
            }
            total += read;
        }
        assert_eq!(total, 0, "a kicked connection is closed without a word");

        manager.stop_network();
        log.shutdown();
    }

    /// The map's clock decides a doorway: `ASURA/crates/asura-world/src/instance.rs:290-308` advances the
    /// agent and tests the segment **the advance walked**, so a walk that carries the character into an
    /// arch is crossed by the server's own copy - no client report has to straddle the wall, which is
    /// what made a click-walk into a door late.
    #[test]
    fn the_map_clock_crosses_a_doorway_its_own_walk_reached() {
        let arch = crate::maps::Arch {
            from_map: 148,
            idx: 0,
            to_map: Some(146),
            // The wall map 148 states, as `data/sql/base/db_world/0012-map-content.sql` holds it.
            from: (7523.469, 5129.551),
            to: (7055.75, 5592.349),
        };
        let mut prepared = world();
        prepared.set_portals(crate::maps::Portals::new(vec![arch], 250.0));
        let prepared = std::sync::Arc::new(prepared);
        let session = WorldSession::new(Log::new(), &prepared);
        {
            let mut entered = session.entered.lock().unwrap();
            *entered = Some((148, 1));
        }
        // The agent is on the near side of the arch, walking a straight line through it, at the world's
        // own speed.
        let speed = prepared.settings().agent_speed;
        let mut agent = crate::agents::Agent::new(1, (7277.4, 5417.9), 0, (1.0, 0.0), speed);
        let walked = agent.begin_walk(vec![crate::terrain::Waypoint {
            pos: crate::terrain::Pos {
                x: 7184.3,
                y: 5307.3,
                plane: 0,
            },
            trap_id: 0,
        }]);
        assert!(walked.is_some(), "the walk starts");
        *session.agent.lock().unwrap() = Some(agent);

        // One step of the clock, long enough to carry the agent over the wall - and no client report
        // anywhere near it.
        let (crossed, _) = session.step_map(std::time::Duration::from_millis(500));
        let (from, to) = crossed.expect("the clock crossed the arch the walk reached");
        assert!(from != to, "the segment is the advance's own");
        assert!(
            prepared
                .portals()
                .crossed(148, from, to)
                .is_some_and(|crossed| crossed.to_map == Some(146)),
            "and it is the arch that leads to 146"
        );
    }

    /// `ASURA/crates/asura-world/src/player.rs:60-74`: the doorway test consumes the segment between two
    /// finite reports, once, and a point reported twice is not a step at all. A simulation tick between
    /// the reports must not shorten it, which is why the segment is kept beside the agent.
    #[test]
    fn a_reported_step_is_the_segment_between_two_reports_and_is_consumed_once() {
        let world = std::sync::Arc::new(world());
        let session = WorldSession::new(Log::new(), &world);
        session.start_portal_position((7277.4, 5417.9));
        session.record_portal_step((7277.4, 5417.9));
        assert!(
            session.take_portal_step().is_none(),
            "a report of the point the character entered at is not a step"
        );
        session.record_portal_step((7184.3, 5307.3));
        assert_eq!(
            session.take_portal_step(),
            Some(((7277.4, 5417.9), (7184.3, 5307.3))),
            "the step is the pair of reports"
        );
        assert!(
            session.take_portal_step().is_none(),
            "a step is consumed once, so a second message cannot cross the same wall"
        );
        session.record_portal_step((7184.3, 5307.3));
        assert!(
            session.take_portal_step().is_none(),
            "a report of the same point is not a step"
        );
    }

    #[test]
    fn a_travel_request_names_the_instance_in_its_first_two_bytes() {
        // The client's own frame, as the working implementation's log records it: `B1 80 | 94 00 | 02 |
        // 00 00 | 0...` - the header `0x80B1`, then the instance. This tree read it as `0x00B1` (the
        // server's own party message) and parsed five fields the working implementation does not read.
        let mut message = vec![0xB1, 0x80];
        message.extend_from_slice(&148u16.to_le_bytes());
        message.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(message.len(), 9, "the frame is nine bytes");
        assert_eq!(
            u16::from_le_bytes([message[0], message[1]]),
            CLIENT_PARTY_TRAVEL,
            "the header is the party travel's"
        );
        assert_eq!(
            u16::from_le_bytes([message[2], message[3]]),
            148,
            "and the instance is at its head"
        );
    }

    /// The templates a test's world runs with, built from the creation content's own definitions: what
    /// the world program reads back from its table, without a database.
    fn templates_from(
        items: Vec<crate::creation::Item>,
    ) -> Vec<crate::data_stores::ItemDefinition> {
        items
            .iter()
            .enumerate()
            .map(|(index, item)| crate::data_stores::ItemDefinition {
                id: index as i64 + 1,
                file_id: item.file_id,
                item_type: item.item_type,
                dye_tint: item.dye_tint,
                dye_colors: item.dye_colors,
                materials: item.materials,
                unk1: item.unk1,
                flags: item.flags,
                value: item.value,
                model_id: item.model_id,
                quantity: item.quantity,
                profession: item.profession,
                name: item.name.clone(),
                modifiers: item.modifiers.clone(),
            })
            .collect()
    }

    /// Read whole frames off a scrambled connection until the message that ends a load, and answer
    /// every frame that arrived: its header and its bytes.
    fn read_load_frames(
        client: &mut std::net::TcpStream,
        reading: &mut common::cryptography::Arc4,
    ) -> Vec<(u16, Vec<u8>)> {
        // The load block ends at `INSTANCE_LOAD_FINISH`; the caller reads what follows it (this
        // player's own title, then the introductions of everyone already in the map).
        read_frames_until(client, reading, instance_packets::INSTANCE_LOAD_FINISH)
    }

    fn read_title_frames(
        client: &mut std::net::TcpStream,
        reading: &mut common::cryptography::Arc4,
    ) -> Vec<(u16, Vec<u8>)> {
        read_frames_until(
            client,
            reading,
            crate::server::packets::title_packets::TITLE_RANK_DISPLAY,
        )
    }

    /// Read whole frames off a scrambled connection until `terminal` arrives, answering every frame.
    fn read_frames_until(
        client: &mut std::net::TcpStream,
        reading: &mut common::cryptography::Arc4,
        terminal: u16,
    ) -> Vec<(u16, Vec<u8>)> {
        read_frames_until_n(client, reading, terminal, 1)
    }

    /// The same, but waiting for the `times`-th `terminal`: a load's manifest carries two ends.
    fn read_frames_until_n(
        client: &mut std::net::TcpStream,
        reading: &mut common::cryptography::Arc4,
        terminal: u16,
        times: usize,
    ) -> Vec<(u16, Vec<u8>)> {
        let mut frames = Vec::new();
        let mut pending: Vec<u8> = Vec::new();
        let mut seen = 0usize;
        loop {
            match crate::server::protocol::decode_game_message(&pending, false) {
                Ok((frame, used)) => {
                    let header = frame.header();
                    assert_eq!(pending.len(), used);
                    frames.push((header, std::mem::take(&mut pending)));
                    if header == terminal {
                        seen += 1;
                        if seen == times {
                            return frames;
                        }
                    }
                }
                Err(FrameError::Incomplete { needed }) => {
                    // Leave the next frame, and its ciphertext, for the next read.
                    let present = pending.len();
                    pending.resize(needed, 0);
                    let incoming = &mut pending[present..];
                    client
                        .read_exact(incoming)
                        .expect("the load keeps arriving");
                    reading.apply(incoming);
                }
                Err(error) => panic!("a frame of the load did not frame: {error}"),
            }
        }
    }

    #[test]
    fn a_gameplay_connection_is_answered_with_the_creation_block() {
        // The whole conversation a client makes, played by a test: the version message, the seed
        // exchange, and the block the world sends unasked. The master is fixed so the test can read
        // what a client would read.
        let log = Log::new();
        let world = Arc::new(world());
        let master = [0x5Au8; MASTER_SECRET_LEN];

        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory(
            log.clone(),
            Arc::clone(&world),
            master,
        ));
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        // 1. The 64-byte version message, as the client sends it.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 0,
            player_token: 0xA2E3_DB8C,
            account: [0xAA; 16],
            character: [0; 16],
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        // 2. The seed: a public value with one byte set, so the arithmetic is cheap.
        let mut public = [0u8; handshake::PUBLIC_LEN];
        public[0] = 2;
        let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
        seed.extend_from_slice(&public);
        client.write_all(&seed).unwrap();

        // 3. The server's seed: the master XORed with the shared secret.
        let mut answer = [0u8; handshake::SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();
        assert_eq!(&answer[..2], &0x1601u16.to_le_bytes());

        let shared = handshake::shared_secret(&public).unwrap();
        for index in 0..MASTER_SECRET_LEN {
            assert_eq!(
                answer[2 + index] ^ shared[index],
                0x5A,
                "the master is the one drawn"
            );
        }

        // 4. The block: scrambled with the key both sides derive, and it is exactly the creation
        // block - the load head and the creation start, which is what a working server sends.
        let mut block = vec![0u8; creation_load_block().len()];
        client.read_exact(&mut block).unwrap();
        let key: [u8; KEY_LEN] = handshake::derive_key(&master);
        let mut opening = common::cryptography::Arc4::new(&key);
        opening.apply(&mut block);
        assert_eq!(block, creation_load_block());
        assert_eq!(block, vec![0x7C, 0x01, 0x3F, 0x3F, 0x00, 0x00, 0x89, 0x01]);

        // 5. The creation screen's first ask: the player-data block, whole and in order.
        let expected = creation_player_block(world.creation().into(), &Creation::start());
        let expected_bytes: usize = expected.iter().map(Vec::len).sum();
        let mut ask = vec![0x89, 0x80];
        let mut sending = common::cryptography::Arc4::new(&key);
        // The client's own outbound stream: the version message and the seed were plaintext, so its
        // cipher starts here.
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();

        let mut answered = vec![0u8; expected_bytes];
        client.read_exact(&mut answered).unwrap();
        opening.apply(&mut answered);

        let mut whole = Vec::new();
        for frame in &expected {
            whole.extend_from_slice(frame);
        }
        assert_eq!(
            answered, whole,
            "the player-data block is the reference's order"
        );

        // The headers in order, which is what the client dispatches on.
        let mut at = 0;
        let mut headers = Vec::new();
        for frame in &expected {
            headers.push(u16::from_le_bytes([frame[0], frame[1]]));
            at += frame.len();
        }
        assert_eq!(at, answered.len(), "every byte is accounted for");
        // The order the reference sends: the player-data start, the item stream (a bag's own item,
        // the bag, then each item and its slot), the weapon sets and the gold, then the player's own
        // state, and finally the player-data end.
        assert_eq!(headers[0], 0x0186, "instance_player_data_start");
        assert_eq!(headers[1], 0x0144, "the item stream opens");
        assert_eq!(
            *headers.last().unwrap(),
            0x018A,
            "instance_player_data_done"
        );
        for header in [
            0x013F, 0x0161, 0x013E, // a bag, its items, and where each sits
            0x0148, 0x0147, 0x0141, // the active weapon set, four weapon sets, the gold
            0x00EA, 0x00EB, 0x00EC, 0x00ED, // the four faction caps
            0x0037, 0x00B7, 0x00B6, 0x00DA, 0x00DB, 0x00E9, 0x003A,
            0x009F, // energy, health, and the third property
        ] {
            assert!(
                headers.contains(&header),
                "header 0x{header:04X} is in the block"
            );
        }
        let item_count = headers.iter().filter(|header| **header == 0x0161).count();
        assert!(
            item_count > 9,
            "the screen is sent the profession's equipment too"
        );

        // 6. The screen chooses another campaign and profession (`0x8060`: campaign, profession).
        // The reference answers `GameSrv_HandleCharCreationChangeProf`'s own way, and this is the
        // step whose omission crashed the client: it keeps holding a set the server no longer names,
        // so the old pieces come off (`0x014D`) before the new ones are created (`0x0161`) and placed
        // (`0x013E`). The profession is told in between (`0x00B7`), and nothing else is.
        let tables = world.creation();
        let previous_set = tables.inventory(0, 1);
        let previous = previous_set.equipped_items();
        let next_set = tables.inventory(1, 2);
        let next = next_set.equipped_items();
        let equipped_bag = crate::creation::equipped_bag(tables)
            .expect("the creation data names an equipped bag")
            .id;
        let mut expected: Vec<Vec<u8>> = Vec::new();
        for item in &previous {
            expected.push(item_packets::item_remove(
                item_packets::ITEM_STREAM,
                item.id,
            ));
        }
        expected.push(character_packets::update_unlocked_skills(&[]));
        expected.push(character_packets::player_update_profession(
            CREATION_AGENT,
            2,
            0,
            0,
        ));
        for item in &next {
            expected.push(item_packets::create_named_item(
                &crate::creation::named_item(item),
            ));
            if let Some(slot) = item.slot {
                expected.push(item_packets::item_move_to_location(
                    item_packets::ITEM_STREAM,
                    item.id,
                    equipped_bag,
                    slot,
                ));
            }
        }
        let expected_bytes: usize = expected.iter().map(Vec::len).sum();

        let mut choose = vec![0x60, 0x80, 1, 2];
        sending.apply(&mut choose);
        client.write_all(&choose).unwrap();

        let mut answered = vec![0u8; expected_bytes];
        client.read_exact(&mut answered).unwrap();
        opening.apply(&mut answered);
        let mut whole = Vec::new();
        for frame in &expected {
            whole.extend_from_slice(frame);
        }
        assert_eq!(
            answered, whole,
            "the change of profession is the reference's answer, byte for byte"
        );

        // The headers, in the reference's order: every old piece removed, the profession, then each
        // new piece created and placed.
        let mut at = 0usize;
        let mut answer_headers = Vec::new();
        for frame in &expected {
            answer_headers.push(u16::from_le_bytes([answered[at], answered[at + 1]]));
            at += frame.len();
        }
        assert_eq!(at, answered.len(), "every byte is accounted for");
        let mut wanted: Vec<u16> = vec![0x014D; previous.len()];
        wanted.push(0x00DB);
        wanted.push(0x00B7);
        for item in &next {
            wanted.push(0x0161);
            if item.slot.is_some() {
                wanted.push(0x013E);
            }
        }
        assert_eq!(
            answer_headers, wanted,
            "the old set goes first, then the profession, then the new set"
        );
        assert!(
            !answer_headers.contains(&0x00B6),
            "a profession change does not re-state the unlocked-profession bitmap"
        );

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    fn a_world_that_is_not_this_one_is_closed() {
        let log = Log::new();
        let world = Arc::new(world());
        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory(
            log.clone(),
            Arc::clone(&world),
            [0x5Bu8; MASTER_SECRET_LEN],
        ));
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 99,
            map_id: 0,
            player_token: 1,
            account: [0; 16],
            character: [0; 16],
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        let mut byte = [0u8; 1];
        let mut total = 0;
        while let Ok(read) = client.read(&mut byte) {
            if read == 0 {
                break;
            }
            total += read;
        }
        assert_eq!(
            total, 0,
            "a connection for another world is closed without a word"
        );

        manager.stop_network();
        log.shutdown();
    }

    /// A connection for a real map is loaded: the head block, the entry position and the end of the
    /// load. Played against a listener with no database, because a load needs no row - the name falls
    /// back to "Player" and says so.
    #[test]
    fn an_outpost_connection_is_loaded_with_its_entry_point() {
        // A console logger, so the session's own words are visible while this is being proven.
        let log = Log::new();
        log.initialize(&common::configuration::Config::parse(
            "Appender.Console = 1,6,0
Logger.root = 6,Console
",
            std::path::PathBuf::from("test.conf"),
            common::configuration::Policy::default(),
        ))
        .expect("the test can speak");
        let mut configured = world();
        let titles_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../config/World/Vanilla/titles.json");
        let catalog: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&titles_path).unwrap()).unwrap();
        let mut expected_ids: Vec<_> = catalog["vanilla_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as u32)
            .collect();
        expected_ids.extend((48..).take(catalog["custom_tracks"].as_array().unwrap().len()));
        expected_ids.sort_unstable();
        configured.load_titles(titles_path, &log).unwrap();
        let world = Arc::new(configured);
        let master = [0x5Au8; MASTER_SECRET_LEN];
        let factory = factory(log.clone(), Arc::clone(&world), master);
        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        // 1. The version message, naming map 148 - the outpost - and a character id no row holds.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 148,
            player_token: 0x1234,
            account: [0xAA; 16],
            character: [0xBB; 16],
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        // 2. The seed exchange.
        let mut public = [0u8; handshake::PUBLIC_LEN];
        public[0] = 2;
        let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
        seed.extend_from_slice(&public);
        client.write_all(&seed).unwrap();

        let mut answer = [0u8; handshake::SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();
        let key: [u8; KEY_LEN] = handshake::derive_key(&master);
        let mut reading = common::cryptography::Arc4::new(&key);
        let mut sending = common::cryptography::Arc4::new(&key);

        // 3. The head block: load head, player-data start, the name, the instance's record, and the
        // exploration grid a live server sends right after it.
        let grid = crate::maps::cartography::init_frames(crate::maps::cartography::grid_for(148));
        let mut expected = vec![
            instance_packets::instance_load_head(),
            instance_packets::instance_player_data_start(),
            instance_packets::instance_load_player_name("Player"),
            // 148 is Ascalon City, an outpost: the explorable byte is 0. Only an explorable sends 1
            // (`ASURA/crates/asura-world/src/map.rs:76-86`, `load_info_explorable`).
            instance_packets::instance_load_info(1, 148, 0, 1, 0, 0),
        ];
        expected.extend(grid);
        // And the instance table: one batch, because this world's one map with a value states it.
        let (table, notes) = crate::maps::instance_table::table_for(world.maps());
        assert!(
            notes.is_empty(),
            "every map's value is stated or absent: {notes:?}"
        );
        expected.extend(table.frames());
        let expected_bytes: usize = expected.iter().map(Vec::len).sum();
        let mut block = vec![0u8; expected_bytes];
        client.read_exact(&mut block).unwrap();
        reading.apply(&mut block);
        let mut whole = Vec::new();
        for frame in &expected {
            whole.extend_from_slice(frame);
        }
        assert_eq!(block, whole, "the load's head, in the measured order");
        let headers: Vec<u16> = expected
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert_eq!(
            headers,
            vec![0x017C, 0x0186, 0x017D, 0x019A, 0x008B, 0x008A, 0x01A0]
        );

        // 4. The ping pair the measured load follows the head block with.
        let mut ping = vec![0u8; 8];
        client.read_exact(&mut ping).unwrap();
        reading.apply(&mut ping);
        assert_eq!(
            ping,
            vec![0x0C, 0x00, 0x0D, 0x00, 0x14, 0x00, 0x00, 0x00],
            "the ping request and its reply, as a working server sent them"
        );

        // 5. The items ask: the inventory stream and the map's record, in the measured order.
        let mut ask = vec![0x91, 0x80, 0x00, 0x00];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();

        let inventory = world
            .creation()
            .inventory(0, character_packets::DEFAULT_PROFESSION);
        let mut expected_items = crate::creation::load_item_stream(&inventory, 0);
        expected_items.push(misc_packets::hard_mode_unlocked(0));
        expected_items.push(character_packets::character_update_info(""));
        expected_items.push(misc_packets::map_update_current(148, 0));
        expected_items.push(instance_packets::ready_for_map_spawn(0));
        expected_items.push(instance_packets::instance_manifest_phase(0));
        expected_items.push(instance_packets::instance_manifest_phase(1));
        expected_items.push(instance_packets::instance_manifest_done(2, 877, 0));
        expected_items.push(instance_packets::instance_manifest_phase(0));
        expected_items.push(instance_packets::instance_manifest_phase(1));
        expected_items.push(instance_packets::instance_manifest_done(0, 148, 0));
        let expected_bytes: usize = expected_items.iter().map(Vec::len).sum();
        let mut items = vec![0u8; expected_bytes];
        client.read_exact(&mut items).unwrap();
        reading.apply(&mut items);
        let mut whole = Vec::new();
        for frame in &expected_items {
            whole.extend_from_slice(frame);
        }
        assert_eq!(
            items, whole,
            "the items and the record, in the measured order"
        );
        // The measured header sequence, so a reordering fails here rather than at the client.
        let headers: Vec<u16> = expected_items
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        for header in [
            0x0144, 0x0148, 0x0147, 0x0141, 0x00EA, 0x00ED, 0x0070, 0x0030, 0x0099, 0x01AC, 0x0199,
            0x0198,
        ] {
            assert!(
                headers.contains(&header),
                "header 0x{header:04X} is in the answer"
            );
        }
        assert_eq!(
            *headers.last().unwrap(),
            0x0198,
            "the manifest ends the block"
        );

        // 6. The spawn request: the world answers with the map file and a point of its own list, and
        // nothing else - one `0x0196` frame, which is `spawn_block` exactly
        // (`ASURA/crates/asura-world/src/map.rs:13-20`). The simulation's ticks come from the ticker.
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();

        let mut spawn = vec![0u8; 26];
        client.read_exact(&mut spawn).unwrap();
        reading.apply(&mut spawn);
        assert_eq!(&spawn[..2], &[0x96, 0x01], "the entry position");
        assert_eq!(
            &spawn[2..6],
            &113_021u32.to_le_bytes(),
            "map 148's file id, as the recording carried it"
        );
        // The point is scattered around the map's own spawn, so it is near it and not equal to it.
        let x = f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]);
        let y = f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]);
        assert!(
            (x - 9541.87).abs() <= 250.0 + 1.0 && (y - 4796.46).abs() <= 250.0 + 1.0,
            "the point {x}, {y} is within the scatter of the map's own"
        );

        // 7. The players request: the whole load block, in the reference's order, ending at the
        // message that takes the client off the loading screen. Read until that message arrives.
        let mut ask = vec![
            0x90, 0x80, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0xBB,
            0xBB, 0xBB, 0xBB, 0xBB, 0x00,
        ];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();

        let blocks = read_frames_until(
            &mut client,
            &mut reading,
            instance_packets::INSTANCE_LOAD_FINISH,
        );
        read_title_frames(&mut client, &mut reading);
        let title_points: Vec<_> = blocks
            .iter()
            .filter(|(header, _)| *header == 0x00F6)
            .map(|(_, bytes)| {
                let id = u32::from_le_bytes(bytes[2..6].try_into().unwrap());
                let points = u32::from_le_bytes(bytes[10..14].try_into().unwrap());
                (id, points)
            })
            .collect();
        assert_eq!(
            title_points.iter().map(|&(id, _)| id).collect::<Vec<_>>(),
            expected_ids,
            "the test load sends the configured IDs"
        );
        assert!(title_points.iter().all(|&(_, points)| points > 0));
        assert!(
            title_points.contains(&(26, 1)),
            "Canthan Skill Hunter stays at one point"
        );
        assert!(
            title_points.contains(&(32, 3)),
            "Legendary Skill Hunter uses the captured completed state"
        );
        for (header, title, expected_rank) in [
            (GAME_CMSG_TITLE_DISPLAY, Some(32u32), 246),
            (GAME_CMSG_TITLE_DISPLAY, Some(0), 6),
            (GAME_CMSG_TITLE_HIDE, None, 0),
        ] {
            let mut ask = header.to_le_bytes().to_vec();
            if let Some(title) = title {
                ask.extend_from_slice(&title.to_le_bytes());
            }
            sending.apply(&mut ask);
            client.write_all(&ask).unwrap();
            let reply = read_title_frames(&mut client, &mut reading);
            assert_eq!(
                reply.last().unwrap().1,
                title_packets::rank_display(1, expected_rank)
            );
        }
        let defender = blocks
            .iter()
            .find(|(header, bytes)| *header == 0x00F6 && bytes[2..6] == 21u32.to_le_bytes())
            .expect("Defender of Ascalon is sent on the normal player load");
        assert_eq!(defender.1[30..34], 20u32.to_le_bytes());
        let headers: Vec<u16> = blocks.iter().map(|(header, _)| *header).collect();
        let flags = headers.iter().position(|&header| header == 0x003C).unwrap();
        assert_eq!(
            headers[flags + 1],
            0x00F3,
            "rank records follow player flags"
        );
        // Preforged packets-6468-5728843.jsonl:110585-110587 and
        // Northlands seq 567-569 give the Canthan Skill Hunter and Sweet Tooth order.
        for (track, ranks) in [(26u32, [213u32, 214u32]), (34, [273, 274])] {
            let track_at = blocks
                .iter()
                .position(|(header, frame)| *header == 0x00F6 && frame[2..6] == track.to_le_bytes())
                .unwrap();
            for ((header, frame), rank) in blocks[track_at - 2..track_at].iter().zip(ranks) {
                assert_eq!(*header, 0x00F3);
                assert_eq!(frame[2..6], rank.to_le_bytes());
            }
        }
        // The reference's own order (`ASURA/crates/asura-world/src/player.rs`, `player_block`): the
        // PvP halves and the unlockable items, the account features, the unlocked lists and the team
        // token, the skills and the progress record, the flags, the player and the agent, the agent's
        // properties and look, the party, and the end.
        for header in [
            0x001D, 0x0018, 0x001A, 0x001B, // the PvP unlocks and the item table
            0x000F, // an account feature
            0x0094, 0x00F2, // the unlocked lists and the team token
            0x0037, 0x00B7, 0x00B6, 0x00DA, 0x00DB, // attribute points, profession, skills
            0x00E9, // the progress record
            0x00EF, 0x003C, 0x003A, 0x001F, // the flags, the attributes, the load time
            0x0059, 0x0020, 0x009C, // the player, the agent, its morale
            0x009F, 0x00A2, 0x00F0, // the agent's properties and effects
            0x006E, 0x0048, 0x0022, // what it wears, its cape, the agent the client drives
            0x01D3, 0x01CC, 0x01D4, 0x01B3, // the party
            0x018E, // and the load is over
        ] {
            assert!(
                headers.contains(&header),
                "the players answer carries 0x{header:04X}: {headers:04X?}"
            );
        }
        assert_eq!(headers.first(), Some(&0x001D), "it opens with the PvP half");
        assert_eq!(
            headers.last(),
            Some(&0x018E),
            "and ends with the load's own end"
        );
        // The party comes after the agent and before the end, in the reference's own order: a client
        // told about a party it was never given asserts `m_partyClient`.
        let party_at = headers
            .iter()
            .position(|header| *header == 0x01D3)
            .expect("the party is created");
        assert_eq!(
            &headers[party_at..party_at + 4],
            &[0x01D3, 0x01CC, 0x01D4, 0x01B3],
            "the party before the character's own party word: {headers:04X?}"
        );
        // The guild block goes right in front of the party: a character in no guild is told `0x011C`
        // alone there (`ASURA/crates/asura-world/src/player.rs:740-756`, `guild.rs:165-187`).
        let guild_at = headers
            .iter()
            .position(|header| *header == 0x011C)
            .expect("the guild block's end marker");
        assert!(
            guild_at < party_at,
            "the guild block ends before the party: {headers:04X?}"
        );
        // The agent's own body is the frame that puts a body in the map, and it carries the point the
        // entry-position answer named.
        assert!(
            headers.contains(&0x0020),
            "the load creates the agent it is played as: {headers:04X?}"
        );

        // 8. And the connection stays: the world ticks it, one per interval, with no prompt. The
        // payload is how long it has been since the last tick, not a counter - the client advances its
        // simulation by it (`ASURA/crates/asura-world/src/instance.rs:369`).
        let mut tick = vec![0u8; 6];
        client.read_exact(&mut tick).unwrap();
        reading.apply(&mut tick);
        assert_eq!(&tick[..2], &[0x1E, 0x00], "a world simulation tick");
        let elapsed = u32::from_le_bytes([tick[2], tick[3], tick[4], tick[5]]);
        assert!(
            elapsed > 0 && elapsed <= 1000,
            "the tick carries the elapsed milliseconds of one interval, not a counter: {elapsed}"
        );

        // 9. The client's own traffic does not end the session: a movement, then a travel request,
        // and the world ticks again after both.
        let mut movement = vec![0x3D, 0x80];
        movement.extend_from_slice(&9550.0f32.to_le_bytes());
        movement.extend_from_slice(&4800.0f32.to_le_bytes());
        movement.extend_from_slice(&[0u8; 16]);
        sending.apply(&mut movement);
        client.write_all(&movement).unwrap();

        // The client's own travel request: `0x80B1`, nine bytes - `B1 80 | 94 00 | 02 | 00 00 | 00 | 01`
        // when the client asks for map 148 (the measured frame). This test sent it as `0x00B1`, which is
        // the *server's* party message, and the table carried a row for that mistake until round 20.
        let mut travel = vec![0xB1, 0x80];
        travel.extend_from_slice(&148u16.to_le_bytes());
        travel.push(0xFE); // territory -2
        travel.extend_from_slice(&0u16.to_le_bytes());
        travel.push(0); // language
        travel.push(1); // flag
        sending.apply(&mut travel);
        client.write_all(&travel).unwrap();

        let mut tick = vec![0u8; 6];
        client.read_exact(&mut tick).unwrap();
        reading.apply(&mut tick);
        assert_eq!(&tick[..2], &[0x1E, 0x00], "the world is still ticking");

        manager.stop_network();
        log.shutdown();
    }

    /// Play one connection up to a loaded map, and answer the frames its players request carried.
    ///
    /// The exchange, the head block, the ping pair, the items, the entry point and the players - the
    /// same steps `an_outpost_connection_is_loaded_with_its_entry_point` walks in detail, kept here so a
    /// second player can be put in the same map without repeating them.
    fn load_a_player(
        port: u16,
        master: [u8; MASTER_SECRET_LEN],
        character: [u8; 16],
    ) -> (
        std::net::TcpStream,
        common::cryptography::Arc4,
        Vec<(u16, Vec<u8>)>,
        u32,
    ) {
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 148,
            player_token: 0x1234,
            account: [0xAA; 16],
            character,
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();
        let mut public = [0u8; handshake::PUBLIC_LEN];
        public[0] = 2;
        let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
        seed.extend_from_slice(&public);
        client.write_all(&seed).unwrap();
        let mut answer = [0u8; handshake::SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();
        let key: [u8; KEY_LEN] = handshake::derive_key(&master);
        let mut reading = common::cryptography::Arc4::new(&key);
        let mut sending = common::cryptography::Arc4::new(&key);

        // The head block ends with the instance table; the ping pair follows it unasked.
        let head = read_frames_until(&mut client, &mut reading, instance_packets::INSTANCE_TABLE);
        let controlled = head
            .iter()
            .find(|(header, _)| *header == instance_packets::INSTANCE_LOAD_INFO)
            .map(|(_, body)| u32::from_le_bytes(body[2..6].try_into().unwrap()))
            .expect("the load head names the controlled agent");
        let mut ping = vec![0u8; 8];
        client.read_exact(&mut ping).unwrap();
        reading.apply(&mut ping);

        // The items ask, the entry point, and the players.
        let mut ask = vec![0x91, 0x80, 0x00, 0x00];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let items = read_frames_until_n(
            &mut client,
            &mut reading,
            instance_packets::INSTANCE_MANIFEST_DONE,
            2,
        );
        assert_eq!(
            items.last().map(|(header, _)| *header),
            Some(instance_packets::INSTANCE_MANIFEST_DONE),
            "the items answer ends with the second manifest end"
        );
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        // The entry point, one frame: `spawn_block` in the working implementation is exactly that
        // (`ASURA/crates/asura-world/src/map.rs:13-20`).
        let mut spawn = vec![0u8; 26];
        client.read_exact(&mut spawn).unwrap();
        reading.apply(&mut spawn);
        let mut ask = vec![0x90, 0x80];
        ask.extend_from_slice(&[0u8; 17]);
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let players = read_load_frames(&mut client, &mut reading);
        (client, reading, players, controlled)
    }

    /// Two players in one copy of a map: the second is told about the first **after** its own load
    /// block (so it already knows its team token), the first is told the second arrived, and told again
    /// when it leaves.
    ///
    /// `ASURA/crates/asura-world/src/player.rs:784-851` and `instance.rs:1031-1124`: the introduction
    /// is the player, its party pair, its professions and level, its flags or title, its effects, its
    /// agent, what it wears, visual equipment and cape; the farewell is the player destroyed and the
    /// agent removed.
    #[test]
    fn two_connections_in_one_map_introduce_each_other() {
        let log = Log::new();
        let mut configured = world();
        let catalog = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../config/World/Vanilla/titles.json");
        configured.load_titles(catalog, &log).unwrap();
        let world = Arc::new(configured);
        let master = [0x5Au8; MASTER_SECRET_LEN];
        let factory = factory(log.clone(), Arc::clone(&world), master);
        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let (mut first_client, mut reading, first_frames, first_controlled) =
            load_a_player(port, master, [0xBB; 16]);
        let first: Vec<u16> = first_frames.iter().map(|(header, _)| *header).collect();
        assert_eq!(
            first.first(),
            Some(&0x001D),
            "the first player has no introductions before its own block: {first:04X?}"
        );
        assert_eq!(
            first.iter().filter(|header| **header == 0x0020).count(),
            1,
            "one body in the map, its own: {first:04X?}"
        );
        // `0x019A` names the agent this client controls; its own `0x0020` must be that same body.
        let first_own_agent = first_frames
            .iter()
            .find(|(header, _)| *header == 0x0020)
            .map(|(_, body)| u32::from_le_bytes(body[2..6].try_into().unwrap()))
            .expect("the load creates its own agent");
        assert_eq!(
            first_controlled, first_own_agent,
            "the first load controls the agent it creates"
        );
        // The block ends at `INSTANCE_LOAD_FINISH`; its own displayed rank is the frame after it.
        let first_own_title = read_title_frames(&mut first_client, &mut reading);
        assert_eq!(
            first_own_title.last().unwrap().1,
            title_packets::rank_display(1, world.titles().initial_display_rank())
        );

        let (mut second_client, mut second_reading, second_frames, second_controlled) =
            load_a_player(port, master, [0xCC; 16]);
        let second: Vec<u16> = second_frames.iter().map(|(header, _)| *header).collect();
        assert_eq!(
            second.first(),
            Some(&0x001D),
            "the second player's block comes first, before any introduction: {second:04X?}"
        );
        assert_eq!(
            second.last(),
            Some(&instance_packets::INSTANCE_LOAD_FINISH),
            "and ends before the other players are introduced: {second:04X?}"
        );
        // The bug this guards: `0x019A` used to name agent 1 for every connection, while the second
        // connection's own `0x0020` was agent 2. Its health and energy then went to a body the client
        // was not controlling, and its own bars stayed empty.
        let second_own_agent = second_frames
            .iter()
            .find(|(header, _)| *header == 0x0020)
            .map(|(_, body)| u32::from_le_bytes(body[2..6].try_into().unwrap()))
            .expect("the load creates its own agent");
        assert_eq!(
            second_controlled, second_own_agent,
            "the second load controls the agent it creates, not the first player's"
        );
        let second_own_title = read_title_frames(&mut second_client, &mut second_reading);
        assert_eq!(
            second_own_title.last().unwrap().1,
            title_packets::rank_display(2, world.titles().initial_display_rank())
        );

        // The first player is introduced **after** the second's own block, so the second already knows
        // its team token when it creates the first (`ASURA/crates/asura-world/src/player.rs:784-822`).
        let intro = read_frames_until(
            &mut second_client,
            &mut second_reading,
            misc_packets::AGENT_DISPLAY_CAPE,
        );
        let headers: Vec<u16> = intro
            .iter()
            .map(|(header, _)| *header)
            .filter(|header| *header != misc_packets::WORLD_SIMULATION_TICK)
            .collect();
        assert_eq!(
            headers.first(),
            Some(&0x0059),
            "the first player's introduction: {headers:04X?}"
        );
        assert!(
            headers.contains(&0x00A6),
            "its professions, which the target frame needs: {headers:04X?}"
        );
        assert!(
            headers.contains(&0x009F),
            "its public level, which the party window needs: {headers:04X?}"
        );
        assert!(
            headers.contains(&misc_packets::UPDATE_AGENT_VISUAL_EQUIPMENT),
            "and the nine items its agent is drawn wearing: {headers:04X?}"
        );
        assert_eq!(headers.last(), Some(&misc_packets::AGENT_DISPLAY_CAPE));
        let profession = intro
            .iter()
            .find(|(header, _)| *header == 0x00A6)
            .expect("an agent profession");
        assert_eq!(
            &profession.1[2..6],
            &1u32.to_le_bytes(),
            "the first player's agent"
        );
        assert_eq!(
            profession.1[6], 1,
            "its primary profession, not the newcomer's"
        );
        let level = intro
            .iter()
            .find(|(header, body)| {
                *header == 0x009F
                    && body[2..6] == agent_packets::AGENT_PROPERTY_PUBLIC_LEVEL.to_le_bytes()
            })
            .expect("a public-level property");
        assert_eq!(&level.1[10..14], &1u32.to_le_bytes(), "its own level");

        // And the first player is told the second arrived, with the same whole introduction.
        let arrived = read_frames_until(
            &mut first_client,
            &mut reading,
            misc_packets::AGENT_DISPLAY_CAPE,
        );
        let headers: Vec<u16> = arrived
            .iter()
            .map(|(header, _)| *header)
            .filter(|header| *header != misc_packets::WORLD_SIMULATION_TICK)
            .collect();
        assert_eq!(
            headers.first(),
            Some(&0x0059),
            "the newcomer's introduction: {headers:04X?}"
        );
        assert!(
            headers.contains(&0x00A6),
            "with its professions: {headers:04X?}"
        );
        assert!(
            headers.contains(&item_packets::ITEM_LOW_DETAIL),
            "and what it wears, in the form an introduction declares: {headers:04X?}"
        );
        assert!(
            headers.contains(&misc_packets::UPDATE_AGENT_VISUAL_EQUIPMENT),
            "and the nine items its agent is drawn wearing: {headers:04X?}"
        );
        assert_eq!(headers.last(), Some(&misc_packets::AGENT_DISPLAY_CAPE));

        // The second player leaves; the first is told the player is gone and the agent with it.
        let _ = second_client.shutdown(std::net::Shutdown::Both);
        let gone = read_frames_until(&mut first_client, &mut reading, 0x0021);
        let headers: Vec<u16> = gone
            .iter()
            .map(|(header, _)| *header)
            .filter(|header| *header != misc_packets::WORLD_SIMULATION_TICK)
            .collect();
        assert_eq!(
            headers,
            vec![0x005A, 0x0021],
            "the farewell, in the reference's order"
        );

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    #[ignore = "needs the login and character databases"]
    fn a_travel_record_decides_the_map_and_the_district() {
        // The login writes a travel row when it answers a client's request for a map (`0x8029`), and
        // this connection claims it once (`docs/port/06-world-plan.md`, "The three seams"). The
        // version message names the creation instance on purpose: what loads must be the map the
        // record names, with the district and language the request carried (R57).
        use common::configuration::{Config, Policy};
        use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
        use database::updater::{UpdateState, collect_files};

        let _serial_auth = database::updater::apply::test_lock("auth");
        let _serial_characters = database::updater::apply::test_lock("characters");
        let log = Log::new();
        let config = Config::load_json("../worldserver/worldserver.json", false, Policy::default())
            .expect("the world program's own configuration");

        let settings = read_settings(&config, DatabaseKind::Login).expect("the login settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)
            .expect("the login pool opens");
        let schema = collect_files(
            std::path::Path::new("../data/sql/base/db_auth"),
            UpdateState::Released,
        )
        .expect("the login schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let login = Arc::new(std::sync::Mutex::new(pool));

        let settings =
            read_settings(&config, DatabaseKind::Character).expect("the character settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
            .expect("the character pool opens");
        let schema = collect_files(
            std::path::Path::new("../data/sql/base/db_characters"),
            UpdateState::Released,
        )
        .expect("the character schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let characters = Arc::new(std::sync::Mutex::new(pool));

        // The record the login would have written for this connection.
        let account = [0xAAu8; 16];
        crate::maps::DatabaseTravels::new(Arc::clone(&login), log.clone())
            .record(&crate::maps::Travel {
                account,
                world: 2,
                map_id: 148,
                district: 3,
                language: 2,
                player_token: 0x1234,
                landing: None,
            })
            .expect("the travel is recorded");

        // The character the connection names: its row, and the items a confirm would have written.
        let character = [0xBBu8; 16];
        let character_text = crate::characters::uuid_text(&character);
        let prepared_world = world();
        let starting = prepared_world.creation().inventory(0, 1);
        {
            let mut pool = characters.lock().unwrap();
            pool.query_with(
                "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
            pool.query_with(
                "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
            pool.query_with(
                "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \"appearance\", \"primary_profession\", \"unlocked_professions\", \"campaign\", \"level\", \"gold\") \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &character_text,
                    &crate::characters::uuid_text(&account),
                    &"Travelled One",
                    &0i64,
                    &1i64,
                    &2i64,
                    &0i64,
                    &1i64,
                    &1234i64,
                ],
            )
            .unwrap();
        }
        crate::characters::DatabaseCharacters::new(Arc::clone(&characters), log.clone())
            .write_starting_items(&character_text, &starting, prepared_world.templates())
            .expect("the character's items are written");
        let world = Arc::new(prepared_world);
        let master = [0x33u8; MASTER_SECRET_LEN];
        let factory: shared::network::socket::SessionFactory = {
            let log = log.clone();
            let world = Arc::clone(&world);
            let characters = Arc::clone(&characters);
            let login = Arc::clone(&login);
            Arc::new(move || {
                Box::new(WorldSession::with_master_and_characters_and_login(
                    log.clone(),
                    &world,
                    master,
                    Arc::clone(&characters),
                    Arc::clone(&login),
                )) as Box<dyn Session>
            })
        };
        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        // 1. The version message: the creation instance, and the token pair the login drew.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 0,
            player_token: 0x1234,
            account,
            character: [0xBB; 16],
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        // 2. The seed exchange.
        let mut public = [0u8; handshake::PUBLIC_LEN];
        public[0] = 2;
        let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
        seed.extend_from_slice(&public);
        client.write_all(&seed).unwrap();
        let mut answer = [0u8; handshake::SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();
        let key: [u8; KEY_LEN] = handshake::derive_key(&master);
        let mut reading = common::cryptography::Arc4::new(&key);
        let mut sending = common::cryptography::Arc4::new(&key);

        // 3. The block: the outpost's load, not the creation instance's, with the record's district
        // and language, and the character's own name. The block carries the exploration grid and the
        // instance table after the instance's record, so it is read frame by frame up to the table.
        let grid = crate::maps::cartography::init_frames(crate::maps::cartography::grid_for(148));
        let (table, notes) = crate::maps::instance_table::table_for(world.maps());
        assert!(
            notes.is_empty(),
            "every map's value is stated or absent: {notes:?}"
        );
        let mut expected = vec![
            instance_packets::instance_load_head(),
            instance_packets::instance_player_data_start(),
            instance_packets::instance_load_player_name("Travelled One"),
            instance_packets::instance_load_info(1, 148, 0, 3, 2, 0),
        ];
        expected.extend(grid);
        expected.extend(table.frames());
        let expected_bytes: usize = expected.iter().map(Vec::len).sum();
        let mut block = vec![0u8; expected_bytes];
        client.read_exact(&mut block).unwrap();
        reading.apply(&mut block);
        let mut whole = Vec::new();
        for frame in &expected {
            whole.extend_from_slice(frame);
        }
        assert_eq!(
            block, whole,
            "the travel's map, district and language, not the version message's"
        );

        // R12: the character had no unlock rows, so the load wrote the world's first outpost for
        // it before entering - the same starting unlock a confirm writes.
        let unlocked =
            crate::characters::DatabaseCharacters::new(Arc::clone(&characters), log.clone())
                .unlocked_maps(&character_text)
                .expect("the character's unlocks");
        assert_eq!(unlocked, vec![148], "the starting unlock the load wrote");

        // 4. The ping pair, then the character's own items - the rows the confirm wrote, drawn the
        // same way the creation set would be - and then where it stands on map 148's own file.
        let mut ping = vec![0u8; 8];
        client.read_exact(&mut ping).unwrap();
        reading.apply(&mut ping);
        assert_eq!(ping.len(), 8, "the measured ping pair");

        let mut ask = vec![0x91, 0x80, 0x00, 0x00];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let mut expected_items = crate::creation::load_item_stream(&starting, 1234);
        expected_items.push(misc_packets::hard_mode_unlocked(0));
        expected_items.push(character_packets::character_update_info(""));
        expected_items.push(misc_packets::map_update_current(148, 0));
        expected_items.push(instance_packets::ready_for_map_spawn(0));
        expected_items.push(instance_packets::instance_manifest_phase(0));
        expected_items.push(instance_packets::instance_manifest_phase(1));
        expected_items.push(instance_packets::instance_manifest_done(2, 877, 0));
        expected_items.push(instance_packets::instance_manifest_phase(0));
        expected_items.push(instance_packets::instance_manifest_phase(1));
        expected_items.push(instance_packets::instance_manifest_done(0, 148, 0));
        let expected_bytes: usize = expected_items.iter().map(Vec::len).sum();
        let mut items = vec![0u8; expected_bytes];
        client.read_exact(&mut items).unwrap();
        reading.apply(&mut items);
        let mut whole = Vec::new();
        for frame in &expected_items {
            whole.extend_from_slice(frame);
        }
        assert_eq!(
            items, whole,
            "the character's held items, drawn from its rows, in the measured order"
        );

        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        // The entry point, one frame: `spawn_block` in the working implementation is exactly that
        // (`ASURA/crates/asura-world/src/map.rs:13-20`).
        let mut spawn = vec![0u8; 26];
        client.read_exact(&mut spawn).unwrap();
        reading.apply(&mut spawn);
        assert_eq!(
            &spawn[2..6],
            &113_021u32.to_le_bytes(),
            "the outpost's file, so the record decided the map"
        );
        assert_eq!(
            &spawn[26..32],
            &[0x1E, 0x00, 0xF4, 0x01, 0x00, 0x00],
            "the first tick"
        );

        // 4c. A drag in the inventory: the item's row moves, and the client is told the placement.
        let mut ask = vec![0x72, 0x80];
        ask.extend_from_slice(&1u32.to_le_bytes());
        ask.extend_from_slice(&3u16.to_le_bytes());
        ask.push(7);
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let mut moved = vec![0u8; 11];
        client.read_exact(&mut moved).unwrap();
        reading.apply(&mut moved);
        assert_eq!(&moved[..2], &[0x3E, 0x01], "the placement");
        assert_eq!(
            &moved[2..4],
            &item_packets::ITEM_STREAM.to_le_bytes(),
            "the stream"
        );
        assert_eq!(&moved[4..8], &1u32.to_le_bytes(), "the item");
        assert_eq!(&moved[8..10], &3u16.to_le_bytes(), "the bag");
        assert_eq!(moved[10], 7, "the slot");
        let held = crate::characters::DatabaseCharacters::new(Arc::clone(&characters), log.clone())
            .items_of(&character_text)
            .expect("the item rows are readable");
        let moved_row = held
            .iter()
            .find(|row| row.item_id == 1)
            .expect("the item is one of the character's");
        assert_eq!(
            (moved_row.bag_id, moved_row.slot),
            (3, 7),
            "the row moved with it"
        );

        // 4d. An equip: the item goes to the slot its kind is worn in, in the equipped bag, and whatever
        // was worn there comes off to the first free carrying slot
        // (`ASURA/crates/asura-world/src/item.rs:314-436`). An item already in its own worn slot is
        // `MoveOutcome::Unchanged` and answers nothing, so the one picked is somewhere else.
        let equipped_bag = crate::creation::equipped_bag(world.creation())
            .expect("the setting names the equipped bag");
        // The starting set's wearable items are already worn, so the sequence is the player's own: take
        // one off to the backpack first, then wear it again.
        let worn_table = world.creation().worn();
        let worn = starting
            .items
            .iter()
            .find(|item| crate::creation::worn_slot(worn_table, item.item_type).is_some())
            .expect("the set carries something wearable");
        let worn_slot = crate::creation::worn_slot(worn_table, worn.item_type).unwrap();
        let backpack = crate::creation::bags(world.creation())
            .iter()
            .find(|bag| bag.model == crate::characters::BACKPACK_MODEL)
            .copied()
            .expect("the content has a backpack");
        let free = (0..backpack.slots)
            .find(|slot| {
                !starting
                    .items
                    .iter()
                    .any(|item| item.bag_id == Some(backpack.id) && item.slot == Some(*slot))
            })
            .expect("the backpack has a free slot");
        let mut take_off = vec![0u8; 9];
        take_off[0..2].copy_from_slice(&0x8072u16.to_le_bytes());
        take_off[2..6].copy_from_slice(&worn.id.to_le_bytes());
        take_off[6..8].copy_from_slice(&backpack.id.to_le_bytes());
        take_off[8] = free;
        sending.apply(&mut take_off);
        client.write_all(&take_off).unwrap();
        let mut off = vec![0u8; 11];
        client.read_exact(&mut off).unwrap();
        reading.apply(&mut off);
        assert_eq!(&off[..2], &[0x3E, 0x01], "the drag's placement");
        assert_eq!(&off[8..10], &backpack.id.to_le_bytes(), "into the backpack");
        // Taking it off changes what the agent is drawn wearing, so that is sent again too.
        let mut off_drawn = vec![0u8; 2 + 4 + 9 * 4];
        client.read_exact(&mut off_drawn).unwrap();
        reading.apply(&mut off_drawn);
        assert_eq!(&off_drawn[..2], &[0x6E, 0x00], "the agent's worn items");
        let occupant = starting
            .items
            .iter()
            .find(|item| {
                item.bag_id == Some(equipped_bag.id)
                    && item.slot == Some(worn_slot)
                    && item.id != worn.id
            })
            .map(|item| item.id);
        let mut ask = vec![0x30, 0x80];
        ask.extend_from_slice(&worn.id.to_le_bytes());
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        if let Some(occupant) = occupant {
            let mut off = vec![0u8; 11];
            client.read_exact(&mut off).unwrap();
            reading.apply(&mut off);
            assert_eq!(&off[..2], &[0x3E, 0x01], "the eviction's placement");
            assert_eq!(&off[4..8], &occupant.to_le_bytes(), "the item taken off");
        }
        let mut placed = vec![0u8; 11];
        client.read_exact(&mut placed).unwrap();
        reading.apply(&mut placed);
        assert_eq!(&placed[..2], &[0x3E, 0x01], "the placement");
        assert_eq!(&placed[4..8], &worn.id.to_le_bytes(), "the item");
        assert_eq!(
            &placed[8..10],
            &equipped_bag.id.to_le_bytes(),
            "the equipped bag"
        );
        assert_eq!(placed[10], worn_slot, "the slot its kind is worn in");
        // And what the agent is drawn wearing is sent again.
        let mut drawn = vec![0u8; 2 + 4 + 9 * 4];
        client.read_exact(&mut drawn).unwrap();
        reading.apply(&mut drawn);
        assert_eq!(&drawn[..2], &[0x6E, 0x00], "the agent's worn items");

        // 4e. A drop (`0x802C`, the item and a count byte): the stack is the state and **nothing goes
        // back**, which is what the working implementation does and what its notes on the two
        // candidates say each cost a live client (`ASURA/crates/asura-world/src/item.rs:187-246`).
        let dropped = starting
            .items
            .iter()
            .find(|item| item.id != worn.id && item.id != 1 && item.quantity > 1)
            .expect("the set carries a stack of more than one")
            .id;
        let before =
            crate::characters::DatabaseCharacters::new(Arc::clone(&characters), log.clone())
                .items_of(&character_text)
                .expect("the item rows are readable")
                .iter()
                .find(|row| row.item_id == dropped)
                .map(|row| row.quantity)
                .expect("the stack is one of the character's");
        let mut ask = vec![0x2C, 0x80];
        ask.extend_from_slice(&dropped.to_le_bytes());
        ask.push(1);
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        // **No answer**: the next thing the connection says is the answer to what is asked next -
        // here the entry position, which is `0x0196` and the first tick. A drop puts nothing on the
        // wire at all (`ASURA/crates/asura-world/src/item.rs:236-246`).
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let mut next = vec![0u8; 32];
        client.read_exact(&mut next).unwrap();
        reading.apply(&mut next);
        assert_eq!(
            &next[..2],
            &[0x96, 0x01],
            "the drop is answered with nothing, so the entry position is next; got {}",
            shared::network::socket::hex(&next)
        );
        let after =
            crate::characters::DatabaseCharacters::new(Arc::clone(&characters), log.clone())
                .items_of(&character_text)
                .expect("the item rows are readable")
                .iter()
                .find(|row| row.item_id == dropped)
                .map(|row| row.quantity)
                .expect("the stack is still there");
        assert_eq!(after, before - 1, "one was taken off the stack");

        // 5. The record is claimed once: a second claim finds nothing.
        let claimed = crate::maps::DatabaseTravels::new(Arc::clone(&login), log.clone())
            .claim(2, &crate::characters::uuid_text(&account), 0x1234)
            .expect("the travel table reads");
        assert!(claimed.is_none(), "the row was consumed by the connection");

        // The test's own character and its items go, leaving the databases as they were found.
        {
            let mut pool = characters.lock().unwrap();
            pool.query_with(
                "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
            pool.query_with(
                "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
        }

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    #[ignore = "needs the login and character databases"]
    fn an_arrival_unlocks_the_map_it_reaches() {
        // R12 is not a gate the server applies, and this test used to assert the opposite: it closed
        // the connection for a claimed map the character had not unlocked, which is what a doorway
        // crossing into a new map met (the client's `007`). `ASURA/crates/asura-world/src/player.rs:442-471`
        // is the rule: "the instance the character is standing in is reached, so it is unlocked - the
        // walk sets the bit, the bit never gates the walk". The character here has unlocked 148; the
        // login records a travel to 147; the load proceeds, and answering the players ask writes 147
        // into the reached instances.
        use common::configuration::{Config, Policy};
        use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
        use database::updater::{UpdateState, collect_files};

        let _serial_auth = database::updater::apply::test_lock("auth");
        let _serial_characters = database::updater::apply::test_lock("characters");
        let log = Log::new();
        let config = Config::load_json("../worldserver/worldserver.json", false, Policy::default())
            .expect("the world program's own configuration");

        let settings = read_settings(&config, DatabaseKind::Login).expect("the login settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)
            .expect("the login pool opens");
        let schema = collect_files(
            std::path::Path::new("../data/sql/base/db_auth"),
            UpdateState::Released,
        )
        .expect("the login schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let login = Arc::new(std::sync::Mutex::new(pool));

        let settings =
            read_settings(&config, DatabaseKind::Character).expect("the character settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
            .expect("the character pool opens");
        let schema = collect_files(
            std::path::Path::new("../data/sql/base/db_characters"),
            UpdateState::Released,
        )
        .expect("the character schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let characters = Arc::new(std::sync::Mutex::new(pool));

        // The travel the login would have written for this connection: map 147, which the character
        // below has not unlocked.
        let account = [0xCCu8; 16];
        crate::maps::DatabaseTravels::new(Arc::clone(&login), log.clone())
            .record(&crate::maps::Travel {
                account,
                world: 2,
                map_id: 147,
                district: 1,
                language: 0,
                player_token: 0x2345,
                landing: None,
            })
            .expect("the travel is recorded");

        // The character: one row, one unlock (148), no items needed - the gate decides before any of
        // the load's own messages.
        let character = [0xDDu8; 16];
        let character_text = crate::characters::uuid_text(&character);
        {
            let mut pool = characters.lock().unwrap();
            pool.query_with(
                "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
            pool.query_with(
                "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
            pool.query_with(
                "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \"appearance\", \"primary_profession\", \"unlocked_professions\", \"campaign\", \"level\", \"gold\") \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &character_text,
                    &crate::characters::uuid_text(&account),
                    &"Ununlocked One",
                    &0i64,
                    &1i64,
                    &2i64,
                    &0i64,
                    &1i64,
                    &0i64,
                ],
            )
            .unwrap();
        }
        let repository =
            crate::characters::DatabaseCharacters::new(Arc::clone(&characters), log.clone());
        repository
            .unlock_map(&character_text, 148)
            .expect("the character has one unlock");

        // The world hosts both outposts, so the claim is accepted and the gate is what refuses.
        let mut prepared = world();
        let mut maps = prepared.maps().to_vec();
        maps.push(crate::world::MapEntry {
            id: 147,
            kind: Some(crate::world::map_kind::OUTPOST),
            file_id: 112_508,
            name: "Ascalon Academy".to_string(),
            instance_value: None,
            name_id: None,
        });
        let mut spawns = prepared.spawns().to_vec();
        spawns.push(crate::maps::MapSpawn {
            map_id: 147,
            index: 0,
            x: 1000.0,
            y: 2000.0,
            plane: 0,
            angle: None,
        });
        prepared.set_maps(maps, spawns);
        let world = Arc::new(prepared);

        let master = [0x44u8; MASTER_SECRET_LEN];
        let factory: shared::network::socket::SessionFactory = {
            let log = log.clone();
            let world = Arc::clone(&world);
            let characters = Arc::clone(&characters);
            let login = Arc::clone(&login);
            Arc::new(move || {
                Box::new(WorldSession::with_master_and_characters_and_login(
                    log.clone(),
                    &world,
                    master,
                    Arc::clone(&characters),
                    Arc::clone(&login),
                )) as Box<dyn Session>
            })
        };
        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        // 1. The version message, naming the creation instance, and the token pair the travel holds.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 0,
            player_token: 0x2345,
            account,
            character,
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        // 2. The seed exchange, the load block, the ping pair, the entry point and the players ask:
        // the map is loaded, not refused.
        let mut public = [0u8; handshake::PUBLIC_LEN];
        public[0] = 2;
        let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
        seed.extend_from_slice(&public);
        client.write_all(&seed).unwrap();
        let mut answer = [0u8; handshake::SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();
        let master = {
            let shared = handshake::shared_secret(&public).expect("the exchange");
            let mut master = [0u8; MASTER_SECRET_LEN];
            for index in 0..MASTER_SECRET_LEN {
                master[index] = answer[2 + index] ^ shared[index];
            }
            master
        };
        let key = handshake::derive_key(&master);
        let mut reading = common::cryptography::Arc4::new(&key);
        let mut sending = common::cryptography::Arc4::new(&key);

        let grid = crate::maps::cartography::init_frames(crate::maps::cartography::grid_for(147));
        // The instance table closes the block, and this test's world states one value (map 148's).
        let (table, _) = crate::maps::instance_table::table_for(world.maps());
        let block_size = instance_packets::instance_load_head().len()
            + instance_packets::instance_player_data_start().len()
            + instance_packets::instance_load_player_name("Ununlocked One").len()
            + instance_packets::instance_load_info(1, 147, 1, 1, 0, 0).len()
            + grid.iter().map(Vec::len).sum::<usize>()
            + table.frames().iter().map(Vec::len).sum::<usize>();
        let mut block = vec![0u8; block_size];
        client.read_exact(&mut block).unwrap();
        reading.apply(&mut block);
        assert_eq!(&block[..2], &[0x7C, 0x01], "the load head arrives");

        let mut ping = vec![0u8; 8];
        client.read_exact(&mut ping).unwrap();
        reading.apply(&mut ping);
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let mut spawn = vec![0u8; 26];
        client.read_exact(&mut spawn).unwrap();
        reading.apply(&mut spawn);
        assert_eq!(&spawn[..2], &[0x96, 0x01], "the entry point arrives");
        let mut ask = vec![0x90, 0x80];
        ask.extend_from_slice(&[0u8; 17]);
        sending.apply(&mut ask);
        client.write_all(&ask).unwrap();
        let players = read_load_frames(&mut client, &mut reading);
        assert_eq!(
            players.last().map(|(header, _)| *header),
            Some(instance_packets::INSTANCE_LOAD_FINISH),
            "the load finishes on map 147"
        );

        // 3. The arrival is what wrote the bit: the character had 148, and now has 147 as well.
        let unlocked = repository
            .unlocked_maps(&character_text)
            .expect("the character's unlocks");
        assert_eq!(
            unlocked,
            vec![147, 148],
            "the map the character arrived in is unlocked (R12)"
        );

        // The character's own rows go, leaving the databases as they were found.
        {
            let mut pool = characters.lock().unwrap();
            pool.query_with(
                "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
            pool.query_with(
                "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
                &[&character_text],
            )
            .unwrap();
        }

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    #[ignore = "needs the character database"]
    fn a_creation_confirm_writes_a_character_row() {
        // The confirm, played against the real character database: the client's bytes go in, a row
        // comes out, and the answer says the same name. This is the stage's proof with a database in
        // it rather than a stub.
        use common::configuration::{Config, Policy};
        use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
        use database::updater::{UpdateState, collect_files};

        // A console appender, so the session's own words are visible in this test's output.
        let _serial = database::updater::apply::test_lock("characters");
        let log = Log::new();
        let speaking = Config::parse(
            "Appender.Console = 1,6,0
Logger.root = 6,Console
",
            std::path::PathBuf::from("test.conf"),
            Policy::default(),
        );
        log.initialize(&speaking).expect("the test can speak");
        let config = Config::load_json("../worldserver/worldserver.json", false, Policy::default())
            .expect("the world program's own configuration");
        let settings =
            read_settings(&config, DatabaseKind::Character).expect("the character settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
            .expect("the pool opens");
        let schema = collect_files(
            std::path::Path::new("../data/sql/base/db_characters"),
            UpdateState::Released,
        )
        .expect("the characters schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        // The name this test uses may be left behind by an earlier run, and a taken name is a
        // refusal, so the row goes first: the test's own character and nobody else's.
        pool.query_with("DELETE FROM characters WHERE name = $1", &[&"Kryta Test"])
            .expect("no leftover character");
        let pool = Arc::new(std::sync::Mutex::new(pool));

        let world = Arc::new(world());
        let master = [0x77u8; MASTER_SECRET_LEN];
        let factory: shared::network::socket::SessionFactory = {
            let log = log.clone();
            let world = Arc::clone(&world);
            let pool = Arc::clone(&pool);
            Arc::new(move || {
                Box::new(WorldSession::with_master_and_characters(
                    log.clone(),
                    &world,
                    master,
                    Arc::clone(&pool),
                )) as Box<dyn Session>
            })
        };

        let mut manager = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        // The version message, with an account this test owns.
        let mut account = [0u8; 16];
        account[0] = 0xC1;
        account[15] = 0x0D;
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 0,
            player_token: 0x1234,
            account,
            character: [0; 16],
            h0038: 0,
            h003c: 0,
        };
        client.write_all(&version.encode()).unwrap();

        // The seed exchange.
        let mut public = [0u8; handshake::PUBLIC_LEN];
        public[0] = 2;
        let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
        seed.extend_from_slice(&public);
        client.write_all(&seed).unwrap();

        let mut answer = [0u8; handshake::SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();
        let mut reading = common::cryptography::Arc4::new(&handshake::derive_key(&master));
        let mut sending = common::cryptography::Arc4::new(&handshake::derive_key(&master));

        let mut block = vec![0u8; creation_load_block().len()];
        client.read_exact(&mut block).unwrap();
        reading.apply(&mut block);

        // The confirm, in the client's own size: sixteen bytes plus twice the name
        // (`docs/game-rules.md`, E73 - "32 bytes for an 8-character name, 30 for 7"), which is the
        // header, the count, the name, the packed appearance word and eight more bytes.
        let name = "Kryta Test";
        let units: Vec<u16> = name.encode_utf16().collect();
        let appearance_at = 4 + units.len() * 2;
        let mut confirm = vec![0u8; 16 + units.len() * 2];
        confirm[0..2].copy_from_slice(&0x808Bu16.to_le_bytes());
        confirm[2..4].copy_from_slice(&(units.len() as u16).to_le_bytes());
        for (index, unit) in units.iter().enumerate() {
            let at = 4 + index * 2;
            confirm[at..at + 2].copy_from_slice(&unit.to_le_bytes());
        }
        confirm[appearance_at..appearance_at + 4].copy_from_slice(&0x00AB_CDEFu32.to_le_bytes());
        let mut scrambled = confirm.clone();
        sending.apply(&mut scrambled);
        client.write_all(&scrambled).unwrap();

        // The answer: `0x0188` with the character's id, its name, and the twenty-six bytes measured
        // after them.
        let mut answered =
            vec![
                0u8;
                2 + 16 + 2 + units.len() * 2 + 2 + 2 + crate::characters::CHARACTER_SETTINGS_LEN
            ];
        client.read_exact(&mut answered).unwrap();
        reading.apply(&mut answered);
        assert_eq!(
            &answered[..2],
            &0x0188u16.to_le_bytes(),
            "the creation succeeded"
        );

        manager.stop_network();

        // And the row is there, under the account the version message named.
        let uuid_text = crate::characters::uuid_text(&{
            let mut id = [0u8; 16];
            id.copy_from_slice(&answered[2..18]);
            id
        });
        let mut guard = pool.lock().unwrap();
        let rows = guard
            .query_with(
                "SELECT \"name\", \"appearance\" FROM \"characters\" WHERE \"uuid\" = $1",
                &[&uuid_text],
            )
            .expect("the row is readable");
        assert_eq!(rows.len(), 1, "one row was written");
        assert_eq!(rows[0].get::<usize, String>(0), name);
        drop(guard);

        // The character's items are rows now, and the rows draw the same stream the creation
        // record's set draws: one row per item, and the load's block is byte-identical.
        let characters = crate::characters::DatabaseCharacters::new(Arc::clone(&pool), log.clone());
        let held = characters
            .items_of(&uuid_text)
            .expect("the item rows are readable");
        assert!(
            !held.is_empty(),
            "the confirm writes the character's starting items"
        );
        let expected = world
            .creation()
            .inventory(0, character_packets::DEFAULT_PROFESSION);
        assert_eq!(
            held.len(),
            expected.items.len(),
            "one row per item the set carries"
        );
        let templates = templates_from(world.creation().definitions());
        let from_rows = crate::creation::inventory_from_held(
            crate::creation::bags(world.creation()),
            &held,
            &templates,
            &log,
        );
        assert_eq!(
            crate::creation::load_item_stream(&from_rows, 0),
            crate::creation::load_item_stream(&expected, 0),
            "the rows draw the same item stream the set does"
        );

        // R12: the character starts with the world's first outpost unlocked - map 148, the only
        // outpost this world hosts.
        assert_eq!(
            characters
                .unlocked_maps(&uuid_text)
                .expect("the unlocks are readable"),
            vec![148],
            "the starting map is unlocked"
        );
        log.shutdown();
    }
}
