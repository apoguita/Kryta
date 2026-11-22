//! Where a character's row comes from: the queries both programs read it with.
//!
//! The login's answer carries one `0x0007` per character of an account, and each of those is a row of
//! `kryta_characters.characters` read back (`docs/decisions/0007-character-schema.md`). The
//! administration tool lists every character the same way. A character is the player's, so it is a
//! row and not a file - the same line the account table sits on.
//!
//! The repository lives here rather than in the program that first used it, because two programs
//! read the same rows and one implementation of the data is the point (`AGENTS.md`, rule 11's move:
//! the auth program re-exports it).
//!
//! Reference: `AuthSrv_HandlePortalAccountLogin`, whose character list is a query of the character
//! database.

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

use crate::characters::{Character, CharacterSettings, uuid_bytes};
use crate::creation::Inventory;

/// `BagModelId_EquippedItems` (`GmInventory.h`): the bag whose slots are the nine worn items.
/// `ASURA/crates/asura-store/src/state/inventory.rs:176-177`.
pub const EQUIPPED_ITEMS_MODEL: u8 = 21;

/// `BagModelId_Backpack`: the first bag a carrying-slot search looks in.
/// `ASURA/crates/asura-store/src/state/inventory.rs:179-180`.
pub const BACKPACK_MODEL: u8 = 0;

/// `BagModelId_Bag2`: the last bag that search looks in
/// (`ASURA/crates/asura-store/src/state/inventory.rs:182-184`).
pub const LAST_CARRYING_MODEL: u8 = 3;

/// Why an item was not moved. `ASURA/crates/asura-store/src/state/inventory.rs:842-855`, minus its
/// `NotInASlot`: an item row here always names the bag and slot it sits in, so that case cannot arise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// No such item for this character.
    NoSuchItem,
    /// No such bag for this character.
    NoSuchBag,
    /// The slot is outside the bag: slots are `0..slot_count`.
    OutOfRange,
    /// Something else is in that slot. Equipping evicts; a move does not.
    Occupied,
}

/// What an attempt to put an item somewhere did.
/// `ASURA/crates/asura-store/src/state/inventory.rs:857-882`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveOutcome {
    /// It is now at `(bag_id, slot)`.
    At {
        /// Where it now sits.
        bag_id: u16,
        /// Which slot of that bag.
        slot: u8,
    },
    /// It was already there; nothing was written.
    Unchanged,
    /// Nothing happened, and `place` is where the item actually is - a caller uses that to put the
    /// client's own model back, because the client moves the item optimistically before it asks.
    Refused {
        /// Where the item really is, when the character still has it.
        place: Option<(u16, u8)>,
        /// Why the move did not happen.
        reason: Refused,
    },
}

/// The columns the list shows, in the order the list builds a character from them.
const SELECT_CHARACTERS: &str = "SELECT \"uuid\", \"name\", \"appearance\", \"primary_profession\", \
\"secondary_profession\", \"campaign\", \"level\", \"is_pvp\", \"helm_status\", \"last_outpost\" \
FROM \"characters\"";

/// Every character with the portal uuid of the account that owns it: the panel's player list, which
/// names both the owner and the player on one line.
const SELECT_CHARACTERS_WITH_ACCOUNT: &str = "SELECT \"account_uuid\", \"uuid\", \"name\", \
\"appearance\", \"primary_profession\", \"secondary_profession\", \"campaign\", \"level\", \
\"is_pvp\", \"helm_status\", \"last_outpost\" FROM \"characters\" ORDER BY \"name\"";

/// The statement a creation confirm runs.
///
/// `ON CONFLICT (name) DO NOTHING` is the rule the reference enforces with a unique name: a name that
/// is taken writes nothing, and the empty answer is what says so. The account is the portal's own uuid
/// from the gameplay connection's version message, so the world needs no second database.
const INSERT_CHARACTER: &str = "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \
\"appearance\", \"primary_profession\", \"secondary_profession\", \"unlocked_professions\", \
\"campaign\", \"level\", \"is_pvp\", \"helm_status\", \"gold\", \"last_outpost\", \"experience\") \
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
ON CONFLICT (\"name\") DO NOTHING RETURNING 1";

/// The row an item a character owns is: everything the client is told about it, plus where it sits.
///
/// The definition lives on the row, not behind `template_id`: a load draws a character's items from
/// these rows alone, so an item cannot go missing because a template did
/// (`data/sql/base/db_characters/0005-item-instance-definition.sql`).
const INSERT_HELD_ITEM: &str = "INSERT INTO \"item_instance\" (\"character_uuid\", \"template_id\", \
\"file_id\", \"dye_tint\", \"dye_colors\", \"item_id\", \"bag_id\", \"slot\", \"quantity\", \
\"profession\", \"item_type\", \"model_id\", \"materials\", \"unk1\", \"flags\", \"value\", \
\"name\", \"modifiers\") \
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) \
RETURNING 1";

const SELECT_HELD_ITEMS: &str = "SELECT \"file_id\"::bigint, \"dye_tint\"::bigint, \
\"dye_colors\"::bigint, \"item_id\"::bigint, \"bag_id\"::bigint, \"slot\"::bigint, \
\"quantity\"::bigint, \"profession\"::bigint, \"template_id\"::bigint, \"item_type\"::bigint, \
\"model_id\"::bigint, \"materials\"::bigint, \"unk1\"::bigint, \"flags\"::bigint, \
\"value\"::bigint, \"name\", \"modifiers\" FROM \"item_instance\" \
WHERE \"character_uuid\" = $1 ORDER BY \"bag_id\", \"slot\"";

/// One item a character holds: the whole item, and where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldItem {
    /// The `item_template` row the item was written from; 0 for an item no template describes.
    pub template_id: i64,
    /// The `Gw.dat` file id the client draws.
    pub file_id: u32,
    /// The item's kind code.
    pub item_type: u8,
    /// Which dye slot mask applies.
    pub dye_tint: u8,
    /// The four packed dye colours.
    pub dye_colors: u16,
    /// Crafting materials, as the reference packs them.
    pub materials: u16,
    /// The reference's own unverified byte.
    pub unk1: u8,
    /// Item flags.
    pub flags: u32,
    /// The item's value in gold.
    pub value: u32,
    /// The model id the client draws.
    pub model_id: u32,
    /// The runtime id the client keys the item by.
    pub item_id: u32,
    /// The bag it sits in.
    pub bag_id: u16,
    /// Its slot in that bag.
    pub slot: u8,
    /// How many are in the stack.
    pub quantity: u32,
    /// The profession the item was made for, or zero.
    pub profession: u8,
    /// The name, as UTF-16 code units.
    pub name: Vec<u16>,
    /// The modifier list, as dwords.
    pub modifiers: Vec<u32>,
}

/// The character database, as a program uses it.
pub struct DatabaseCharacters {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

impl DatabaseCharacters {
    /// A source that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// Every character of the account the portal identified, read as the list needs them.
    ///
    /// A lookup that failed is reported and answers an empty list: a login whose character list cannot
    /// be read is a login with no character, and saying so plainly is better than failing the
    /// conversation the client is already in.
    pub fn for_account(&self, account_uuid: &str) -> Vec<Character> {
        self.read(
            &format!("{SELECT_CHARACTERS} WHERE \"account_uuid\" = $1 ORDER BY \"name\""),
            vec![Param::Text(account_uuid.to_string())],
        )
    }

    /// Every character, for the administration tool's page: the rows as they are, in name order.
    pub fn list(&self) -> Vec<Character> {
        self.read(
            &format!("{SELECT_CHARACTERS} ORDER BY \"name\""),
            Vec::new(),
        )
    }

    /// Every character with the portal uuid of the account that owns it.
    ///
    /// The panel's Players page lists every character across accounts, so the owner is read with
    /// the row it owns instead of asking the account table once per character.
    pub fn all_with_account(&self) -> Vec<(String, Character)> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(SELECT_CHARACTERS_WITH_ACCOUNT, Vec::new()),
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    "the character database pool is poisoned; no character can be listed",
                );
                return Vec::new();
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => rows
                .iter()
                .filter_map(|row| {
                    let account: String = row.get("account_uuid");
                    self.character(row).map(|character| (account, character))
                })
                .collect(),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot read the characters: {error}"),
                );
                Vec::new()
            }
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    "the character database answered no query: it is shutting down",
                );
                Vec::new()
            }
        }
    }

    /// A character's stored experience points, when its row states them.
    ///
    /// Experience is character-bound progress like the level beside it; the panel reads it for the
    /// player's own page, and the column is `BIGINT` like every other number in this table.
    pub fn experience(&self, uuid: &str) -> Result<Option<i64>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "SELECT \"experience\"::bigint FROM \"characters\" WHERE \"uuid\" = $1",
                vec![Param::Text(uuid.to_string())],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; no experience can be read"
                        .to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => rows
                .first()
                .map(|row| {
                    row.try_get::<usize, i64>(0)
                        .map_err(|error| format!("column experience: {error}"))
                })
                .transpose(),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// How many characters there are, without reading their rows.
    pub fn count(&self) -> Result<usize, String> {
        let reply = self
            .pool
            .lock()
            .map_err(|_| {
                "the character database pool is poisoned; no character can be counted".to_string()
            })?
            .query_reply("SELECT count(*)::bigint FROM \"characters\"", Vec::new());
        match reply.recv() {
            Ok(Ok(rows)) => rows
                .first()
                .map(|row| row.get::<usize, i64>(0) as usize)
                .ok_or_else(|| "the character table counted no row".to_string()),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// How many characters each account owns, keyed by the portal uuid the account's own list uses.
    ///
    /// The manager's directory shows one line per account; asking once for every account's count is
    /// the difference between one grouped query and one query per account.
    pub fn counts_by_account(&self) -> Result<std::collections::HashMap<String, usize>, String> {
        let reply = self.pool.lock().map_err(|_| {
            "the character database pool is poisoned; no character can be counted".to_string()
        })?
        .query_reply(
            "SELECT \"account_uuid\", count(*)::bigint FROM \"characters\" GROUP BY \"account_uuid\"",
            Vec::new(),
        );
        let rows = match reply.recv() {
            Ok(Ok(rows)) => rows,
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(
                    "the character database answered no query: it is shutting down".to_string(),
                );
            }
        };
        let mut counts = std::collections::HashMap::new();
        for row in &rows {
            let account: String = row.get(0);
            let count: i64 = row.get(1);
            counts.insert(account, count.max(0) as usize);
        }
        Ok(counts)
    }

    /// Write the creation row used by the game confirm and the operator panel.
    /// ASURA/crates/asura-world/src/creation.rs:381-475 creates the traits and character.
    pub fn insert_creation(
        &self,
        uuid: &str,
        account: &str,
        name: &str,
        appearance: u32,
        profession: u8,
        campaign: u8,
        start_map: u16,
    ) -> Result<bool, String> {
        let reply = self
            .pool
            .lock()
            .map_err(|_| "the character database pool is poisoned".to_string())?
            .query_reply(
                INSERT_CHARACTER,
                vec![
                    Param::Text(uuid.to_string()),
                    Param::Text(account.to_string()),
                    Param::Text(name.to_string()),
                    Param::Integer(i64::from(appearance)),
                    Param::Integer(i64::from(profession)),
                    Param::Integer(0),
                    Param::Integer(1i64 << (profession & 0x1f)),
                    Param::Integer(i64::from(campaign)),
                    Param::Integer(1),
                    Param::Flag(campaign == 0),
                    Param::Integer(1),
                    Param::Integer(0),
                    Param::Integer(i64::from(start_map)),
                    Param::Integer(0),
                ],
            );
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("the character database is shutting down".to_string()),
        }
    }

    /// Rename a character only within the account selected by the operator.
    pub fn rename_for_account(
        &self,
        account: &str,
        uuid: &str,
        name: &str,
    ) -> Result<bool, String> {
        let reply = self.pool.lock().map_err(|_| "the character database pool is poisoned".to_string())?.query_reply(
            "UPDATE \"characters\" SET \"name\" = $1 WHERE \"account_uuid\" = $2 AND \"uuid\" = $3 RETURNING 1",
            vec![Param::Text(name.to_string()), Param::Text(account.to_string()), Param::Text(uuid.to_string())]);
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("the character database is shutting down".to_string()),
        }
    }

    /// Set a character's level - the row's own column, which a load reads back into the list and the
    /// character's own body (R46, R48).
    ///
    /// Reference: `CharacterDatabase`'s level update, which the reference's console reaches. The
    /// administration tool is the caller; the level's own range (1 to 20, R48) is the caller's check
    /// because it is a rule of the game rather than of the row. Answers whether the character row is
    /// there.
    pub fn set_level(&self, uuid: &str, level: u8) -> Result<bool, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"characters\" SET \"level\" = $1 WHERE \"uuid\" = $2 RETURNING 1",
                vec![
                    Param::Integer(i64::from(level)),
                    Param::Text(uuid.to_string()),
                ],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; the level was not set".to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot set the level of {uuid}: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// Write a character's progress - experience, level and the skill points held and earned - in one
    /// row update (`ASURA/crates/asura-db/src/game.rs:207-230`, `set_progress`). The caller computes the
    /// four numbers from the curve (`Characters/Experience.rs`); this is the row, not the rule. Answers
    /// whether the character row is there.
    pub fn set_progress(
        &self,
        uuid: &str,
        progress: &crate::characters::experience::Progress,
    ) -> Result<bool, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"characters\" SET \"experience\" = $2, \"level\" = $3, \
                 \"skill_points\" = $4, \"skill_points_total\" = $5 \
                 WHERE \"uuid\" = $1 RETURNING 1",
                vec![
                    Param::Text(uuid.to_string()),
                    Param::Integer(i64::from(progress.experience)),
                    Param::Integer(i64::from(progress.level)),
                    Param::Integer(i64::from(progress.skill_points)),
                    Param::Integer(i64::from(progress.skill_points_total)),
                ],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; the progress was not written"
                        .to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot write the progress of {uuid}: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// Record the outpost a character is now in - R14's "last outpost visited", which a later load
    /// (and the login's list) places it at.
    ///
    /// The row is updated through the worker queue like every other statement here. An update that
    /// failed is reported: the character is still in the map, but the world does not know it for a
    /// later reconnect.
    pub fn set_last_outpost(&self, uuid: &str, map_id: u16) -> Result<(), String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"characters\" SET \"last_outpost\" = $1 WHERE \"uuid\" = $2",
                vec![
                    Param::Integer(i64::from(map_id)),
                    Param::Text(uuid.to_string()),
                ],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; the outpost was not recorded"
                        .to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot record the last outpost of {uuid}: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// The character's own row as a load reads it - or the sheet a load falls back to when the row
    /// cannot be read.
    ///
    /// Reference: `Player::LoadFromDB` (`src/server/game/Entities/Player/Player.cpp`), the row a
    /// session reads before it draws the player it is about to put in a map. This lived in
    /// `WorldSession::character_sheet` until 2026-11-14: the statement, and the sheet a failure is
    /// replaced by, are the noun's. A row that cannot be read is not an error to the caller - a load
    /// gives the client a body either way (`CharacterSheet::missing`).
    pub fn sheet(&self, uuid: &str, character: [u8; 16]) -> crate::characters::CharacterSheet {
        let missing = crate::characters::CharacterSheet::missing(character);
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "SELECT \"name\", \"campaign\", \"primary_profession\", \"secondary_profession\", \
                 \"gold\"::bigint, \"appearance\"::bigint, \"level\"::bigint, \
                 \"unlocked_professions\"::bigint, \"prophecies_finished\", \"factions_finished\", \
                 \"nightfall_finished\", \"last_outpost\", \"experience\"::bigint, \
                 \"skill_points\"::bigint, \"skill_points_total\"::bigint \
                 FROM \"characters\" WHERE \"uuid\" = $1",
                vec![Param::Text(uuid.to_string())],
            ),
            Err(_) => return missing,
        };
        let rows = match reply.recv() {
            Ok(Ok(rows)) => rows,
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("the character's row cannot be read: {error}"),
                );
                return missing;
            }
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    "the character database answered no query: it is shutting down",
                );
                return missing;
            }
        };
        rows.first()
            .map(|row| crate::characters::CharacterSheet {
                name: row.get::<usize, String>(0),
                campaign: row.get::<usize, i64>(1) as u8,
                profession: row.get::<usize, i64>(2) as u8,
                secondary_profession: row.get::<usize, i64>(3) as u8,
                uuid: character,
                gold: row.get::<usize, i64>(4) as u32,
                appearance: row.get::<usize, i64>(5) as u32,
                level: row.get::<usize, i64>(6) as u8,
                unlocked_professions: row.get::<usize, i64>(7) as u32,
                campaigns_finished: u8::from(row.get::<usize, bool>(8))
                    | (u8::from(row.get::<usize, bool>(9)) << 1)
                    | (u8::from(row.get::<usize, bool>(10)) << 2),
                last_outpost: row.get::<usize, i64>(11) as u16,
                experience: row.get::<usize, i64>(12).clamp(0, i64::from(u32::MAX)) as u32,
                skill_points: row.get::<usize, i64>(13).clamp(0, i64::from(u32::MAX)) as u32,
                skill_points_total: row.get::<usize, i64>(14).clamp(0, i64::from(u32::MAX)) as u32,
            })
            .unwrap_or(missing)
    }

    /// Where a character last stood, or `None` when it has never been saved.
    ///
    /// `ASURA/crates/asura-store/src/state/character.rs:226-246`, `position`: `pos_plane IS NULL` is
    /// what "never saved" means, and the whole row is then ignored - a character that has never been
    /// played has `pos_x`/`pos_y` at their default of 0, which is a legal coordinate and must not be
    /// read as a position.
    pub fn position(&self, uuid: &str) -> Result<Option<crate::characters::Position>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "SELECT \"map_id\"::bigint, \"pos_x\", \"pos_y\", \"pos_plane\"::bigint \
                 FROM \"characters\" WHERE \"uuid\" = $1",
                vec![Param::Text(uuid.to_string())],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; no position can be read".to_string(),
                );
            }
        };
        let rows = match reply.recv() {
            Ok(Ok(rows)) => rows,
            Ok(Err(error)) => return Err(format!("the position cannot be read: {error}")),
            Err(_) => {
                return Err(
                    "the character database answered no query: it is shutting down".to_string(),
                );
            }
        };
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let plane: Option<i64> = row.get("pos_plane");
        let Some(plane) = plane else {
            return Ok(None);
        };
        Ok(Some(crate::characters::Position {
            map_id: row.get::<&str, Option<i64>>("map_id").unwrap_or(0).max(0) as u32,
            x: row.get::<&str, Option<f32>>("pos_x").unwrap_or(0.0),
            y: row.get::<&str, Option<f32>>("pos_y").unwrap_or(0.0),
            plane: plane as i16,
        }))
    }

    /// Save where a character stands, and which map it stands in.
    ///
    /// `ASURA/crates/asura-store/src/state/character.rs:248-274`, `record_position`: written when the
    /// player leaves, and by a doorway crossing, which knows where the receiving map puts it.
    pub fn record_position(
        &self,
        uuid: &str,
        position: &crate::characters::Position,
    ) -> Result<bool, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"characters\" SET \"map_id\" = $1::bigint, \"pos_x\" = $2::text::real, \
                 \"pos_y\" = $3::text::real, \"pos_plane\" = $4::bigint WHERE \"uuid\" = $5 \
                 RETURNING 1",
                vec![
                    Param::Integer(i64::from(position.map_id)),
                    Param::Text(position.x.to_string()),
                    Param::Text(position.y.to_string()),
                    Param::Integer(i64::from(position.plane)),
                    Param::Text(uuid.to_string()),
                ],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; the position was not recorded"
                        .to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => Err(format!("the position was not recorded: {error}")),
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// Forget where a character stood, so the next arrival cannot resume it.
    ///
    /// `ASURA/crates/asura-store/src/state/character.rs:276-294`, `forget_position`: `pos_plane` is
    /// cleared, which reads back as a character that has never been played. `map_id` is left alone - it
    /// is the map the character is in, which the arrival itself records.
    pub fn forget_position(&self, uuid: &str) -> Result<bool, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"characters\" SET \"pos_plane\" = NULL WHERE \"uuid\" = $1 RETURNING 1",
                vec![Param::Text(uuid.to_string())],
            ),
            Err(_) => {
                return Err(
                    "the character database pool is poisoned; the position was not cleared"
                        .to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => Err(format!("the position was not cleared: {error}")),
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// One query, and the characters it answered.
    fn read(&self, sql: &str, params: Vec<Param>) -> Vec<Character> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(sql.to_string(), params),
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    "the character database pool is poisoned; no character can be listed",
                );
                return Vec::new();
            }
        };

        match reply.recv() {
            Ok(Ok(rows)) => rows.iter().filter_map(|row| self.character(row)).collect(),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot read the characters: {error}"),
                );
                Vec::new()
            }
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    "the character database answered no query: it is shutting down",
                );
                Vec::new()
            }
        }
    }

    /// Write the items a new character starts with: one row per item, where it sits and which
    /// template it was written from.
    ///
    /// The list comes from the creation record set the screen drew (`Inventory`), which is a setting;
    /// what this writes is the character's own state - the rows the load then draws from. An item the
    /// templates do not describe is reported and not written: the client cannot be drawn an item the
    /// world does not know.
    pub fn write_starting_items(
        &self,
        uuid: &str,
        inventory: &Inventory,
        templates: &[crate::data_stores::ItemDefinition],
    ) -> Result<usize, String> {
        let mut written = 0usize;
        for item in &inventory.items {
            // The template is the row's key, not its content: everything the client is told lives on
            // the row, so an item no template describes is still written and still drawn. It is said
            // in the log because it means the world's template table and the creation record
            // disagree.
            let template_id = match templates.iter().find(|template| {
                template.file_id == item.file_id
                    && template.item_type == item.item_type
                    && template.dye_tint == item.dye_tint
                    && template.dye_colors == item.dye_colors
                    && template.materials == item.materials
                    && template.unk1 == item.unk1
                    && template.flags == item.flags
                    && template.value == item.value
                    && template.model_id == item.model_id
                    && template.quantity == item.quantity
                    && template.profession == item.profession
            }) {
                Some(template) => template.id,
                None => {
                    self.log.out(
                        LogLevel::Warn,
                        "server.characters",
                        format!(
                            "The creation set names item {} (file {}), which no item template describes; it is written from the record list alone.",
                            item.id, item.file_id
                        ),
                    );
                    0
                }
            };
            let reply = match self.pool.lock() {
                Ok(pool) => pool.query_reply(
                    INSERT_HELD_ITEM,
                    vec![
                        Param::Text(uuid.to_string()),
                        Param::Integer(template_id),
                        Param::Integer(i64::from(item.file_id)),
                        Param::Integer(i64::from(item.dye_tint)),
                        Param::Integer(i64::from(item.dye_colors)),
                        Param::Integer(i64::from(item.id)),
                        Param::Integer(i64::from(item.bag_id.unwrap_or(0))),
                        Param::Integer(i64::from(item.slot.unwrap_or(0))),
                        Param::Integer(i64::from(item.quantity)),
                        Param::Integer(i64::from(item.profession)),
                        Param::Integer(i64::from(item.item_type)),
                        Param::Integer(i64::from(item.model_id)),
                        Param::Integer(i64::from(item.materials)),
                        Param::Integer(i64::from(item.unk1)),
                        Param::Integer(i64::from(item.flags)),
                        Param::Integer(i64::from(item.value)),
                        Param::Text(units_text(&item.name)),
                        Param::Text(dwords_text(&item.modifiers)),
                    ],
                ),
                Err(_) => {
                    return Err(
                        "the character database pool is poisoned; the items were not written"
                            .to_string(),
                    );
                }
            };
            match reply.recv() {
                Ok(Ok(rows)) => written += rows.len(),
                Ok(Err(error)) => {
                    return Err(format!("an item row cannot be written: {error}"));
                }
                Err(_) => {
                    return Err(
                        "the character database answered no query: it is shutting down".to_string(),
                    );
                }
            }
        }
        Ok(written)
    }

    /// What a character holds, as its rows say: the definition's key and where each item sits.
    pub fn items_of(&self, uuid: &str) -> Result<Vec<HeldItem>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(SELECT_HELD_ITEMS, vec![Param::Text(uuid.to_string())]),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        let rows = match reply.recv() {
            Ok(Ok(rows)) => rows,
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot read the items of {uuid}: {error}"),
                );
                return Err(error);
            }
            Err(_) => {
                return Err(
                    "the character database answered no query: it is shutting down".to_string(),
                );
            }
        };
        rows.iter()
            .map(|row| {
                let column = |at: usize| {
                    row.try_get::<usize, i64>(at)
                        .map_err(|error| format!("column {at}: {error}"))
                };
                let text = |at: usize| {
                    row.try_get::<usize, String>(at)
                        .map_err(|error| format!("column {at}: {error}"))
                };
                Ok(HeldItem {
                    file_id: column(0)? as u32,
                    dye_tint: column(1)? as u8,
                    dye_colors: column(2)? as u16,
                    item_id: column(3)? as u32,
                    bag_id: column(4)? as u16,
                    slot: column(5)? as u8,
                    quantity: column(6)? as u32,
                    profession: column(7)? as u8,
                    template_id: column(8)?,
                    item_type: column(9)? as u8,
                    model_id: column(10)? as u32,
                    materials: column(11)? as u16,
                    unk1: column(12)? as u8,
                    flags: column(13)? as u32,
                    value: column(14)? as u32,
                    name: units(&text(15)?),
                    modifiers: dwords(&text(16)?),
                })
            })
            .collect()
    }

    /// Move one of a character's items to a bag slot, with the rules a client can be held to.
    ///
    /// Ported from `ASURA/crates/asura-store/src/state/inventory.rs:884-975`, `move_item`: the bag must
    /// be one this character has, the slot must be inside it, and the slot must be free - and a move that
    /// did not happen answers where the item **really is**, because the client has already moved it in
    /// its own model (`docs/reference/client-protocol.md`, "a refused move is answered with
    /// `ITEM_MOVE_TO_LOCATION` for where the item really is"). Equipping is a move into the
    /// equipped-items bag, so it goes through here too.
    ///
    /// The bags are this tree's content (`game/src/creation/mod.rs`, `Bag`), not rows: their ids, models
    /// and slot counts are the same for every character, so the list the caller holds is the list that
    /// decides. ASURA keeps them as `t_bag` rows and asks the database; the rules are its own either way.
    pub fn move_item(
        &self,
        uuid: &str,
        item_id: u32,
        bag_id: u16,
        slot: u8,
        bags: &[crate::creation::Bag],
    ) -> Result<MoveOutcome, String> {
        let held = self.items_of(uuid)?;
        let Some(item) = held.iter().find(|row| row.item_id == item_id) else {
            return Ok(MoveOutcome::Refused {
                place: None,
                reason: Refused::NoSuchItem,
            });
        };
        let here = (item.bag_id, item.slot);
        if here == (bag_id, slot) {
            return Ok(MoveOutcome::Unchanged);
        }
        let Some(bag) = bags.iter().find(|bag| bag.id == bag_id) else {
            return Ok(MoveOutcome::Refused {
                place: Some(here),
                reason: Refused::NoSuchBag,
            });
        };
        if slot >= bag.slots {
            return Ok(MoveOutcome::Refused {
                place: Some(here),
                reason: Refused::OutOfRange,
            });
        }
        if held
            .iter()
            .any(|row| row.bag_id == bag_id && row.slot == slot && row.item_id != item_id)
        {
            return Ok(MoveOutcome::Refused {
                place: Some(here),
                reason: Refused::Occupied,
            });
        }

        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"item_instance\" SET \"bag_id\" = $1, \"slot\" = $2 \
                 WHERE \"character_uuid\" = $3 AND \"item_id\" = $4 RETURNING 1",
                vec![
                    Param::Integer(i64::from(bag_id)),
                    Param::Integer(i64::from(slot)),
                    Param::Text(uuid.to_string()),
                    Param::Integer(i64::from(item_id)),
                ],
            ),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) if !rows.is_empty() => Ok(MoveOutcome::At { bag_id, slot }),
            Ok(Ok(_)) => Ok(MoveOutcome::Refused {
                place: Some(here),
                reason: Refused::NoSuchItem,
            }),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot move item {item_id} of {uuid}: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// The first free slot in the bags a character carries, backpack first.
    ///
    /// Ported from `ASURA/crates/asura-store/src/state/inventory.rs:977-1007`,
    /// `first_free_carrying_slot`: the search covers the bags from [`BACKPACK_MODEL`] to
    /// [`LAST_CARRYING_MODEL`], ordered by model and then by id and slot. The working implementation says
    /// why this exists at all: the reference's own `GmBag_TryAddToBag` tests `items[idx] != 0` where it
    /// means `== 0`, so it overwrites an occupied slot instead of finding an empty one. This is the
    /// intent, not the bug - unequipping something has to have somewhere to put it.
    pub fn first_free_carrying_slot(
        &self,
        uuid: &str,
        bags: &[crate::creation::Bag],
    ) -> Result<Option<(u16, u8)>, String> {
        let held = self.items_of(uuid)?;
        let mut carrying: Vec<&crate::creation::Bag> = bags
            .iter()
            .filter(|bag| (BACKPACK_MODEL..=LAST_CARRYING_MODEL).contains(&bag.model))
            .collect();
        carrying.sort_by_key(|bag| (bag.model, bag.id));
        for bag in carrying {
            for slot in 0..bag.slots {
                if !held
                    .iter()
                    .any(|row| row.bag_id == bag.id && row.slot == slot)
                {
                    return Ok(Some((bag.id, slot)));
                }
            }
        }
        Ok(None)
    }

    /// The nine item ids `UPDATE_AGENT_VISUAL_EQUIPMENT` carries: what the agent is wearing, by slot.
    ///
    /// `ASURA/crates/asura-store/src/state/inventory.rs:136-158`, `equipped_item_ids`: read out of the
    /// equipped-items bag rather than from a constant, because the ids belong to this character's items -
    /// a second character's armour has different ones. A slot the character wears nothing in is zero.
    pub fn equipped_item_ids(
        &self,
        uuid: &str,
        bags: &[crate::creation::Bag],
    ) -> Result<[u32; 9], String> {
        let mut ids = [0u32; 9];
        let Some(bag) = bags.iter().find(|bag| bag.model == EQUIPPED_ITEMS_MODEL) else {
            return Ok(ids);
        };
        for row in self.items_of(uuid)? {
            if row.bag_id != bag.id {
                continue;
            }
            if let Some(held) = ids.get_mut(usize::from(row.slot)) {
                *held = row.item_id;
            }
        }
        Ok(ids)
    }

    /// Remove one of a character's items, and answer whether the row was the character's.
    ///
    /// The item's own row is the state: a destroy the client asked for is this delete, and the answer
    /// to the client is `0x013C`, the item gone.
    pub fn remove_item(&self, uuid: &str, item_id: u32) -> Result<bool, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1 AND \"item_id\" = $2 \
                 RETURNING 1",
                vec![
                    Param::Text(uuid.to_string()),
                    Param::Integer(i64::from(item_id)),
                ],
            ),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot remove item {item_id} of {uuid}: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// Take `count` off one of a character's stacks, and answer what is left of it.
    ///
    /// `None` when the row is not the character's, or when the stack holds fewer than `count`: nothing
    /// is written in either case. A stack taken to zero is deleted, which is what the reference's own
    /// drop does (`ASURA/crates/asura-store/src/state/inventory.rs`, `take_from_stack`).
    pub fn take_from_stack(
        &self,
        uuid: &str,
        item_id: u32,
        count: u32,
    ) -> Result<Option<u32>, String> {
        if count == 0 {
            return Ok(None);
        }
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "UPDATE \"item_instance\" SET \"quantity\" = \"quantity\" - $1 \
                 WHERE \"character_uuid\" = $2 AND \"item_id\" = $3 AND \"quantity\" >= $1 \
                 RETURNING \"quantity\"::bigint",
                vec![
                    Param::Integer(i64::from(count)),
                    Param::Text(uuid.to_string()),
                    Param::Integer(i64::from(item_id)),
                ],
            ),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        let remaining = match reply.recv() {
            Ok(Ok(rows)) => match rows.first() {
                Some(row) => row.try_get::<usize, i64>(0).unwrap_or(0) as u32,
                None => return Ok(None),
            },
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot take {count} of item {item_id} of {uuid}: {error}"),
                );
                return Err(error);
            }
            Err(_) => {
                return Err(
                    "the character database answered no query: it is shutting down".to_string(),
                );
            }
        };
        if remaining == 0 && self.remove_item(uuid, item_id)? {
            return Ok(Some(0));
        }
        Ok(Some(remaining))
    }

    /// Remove a character and every record that is its own - its items, its unlocks, its guild rows,
    /// its row.
    ///
    /// Reference: `CharacterDatabase.DeleteFromDB`, which deletes the character and the records that
    /// point at it together. The administration tool is the caller; neither a login's list nor a
    /// load ever deletes a row. Answers whether the character's own row was there.
    pub fn remove(&self, uuid: &str) -> Result<bool, String> {
        let mut gone = false;
        for statement in [
            "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1 RETURNING 1",
            "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1 RETURNING 1",
            // A character's guild membership and its open invitations go with it: ASURA's
            // `ON DELETE CASCADE` (`0029_guilds.sql`), written out because the guild tables carry no
            // foreign key to `characters` here (`0009-guilds.sql`).
            "DELETE FROM \"guild_members\" WHERE \"character_uuid\" = $1 RETURNING 1",
            "DELETE FROM \"guild_invites\" WHERE \"character_uuid\" = $1 RETURNING 1",
            "DELETE FROM \"characters\" WHERE \"uuid\" = $1 RETURNING 1",
        ] {
            let reply = match self.pool.lock() {
                Ok(pool) => pool.query_reply(statement, vec![Param::Text(uuid.to_string())]),
                Err(_) => return Err("the character database pool is poisoned".to_string()),
            };
            match reply.recv() {
                Ok(Ok(rows)) => {
                    if statement.contains("\"characters\"") {
                        gone = !rows.is_empty();
                    }
                }
                Ok(Err(error)) => {
                    self.log.out(
                        LogLevel::Error,
                        "server.characters",
                        format!("Cannot remove the records of {uuid}: {error}"),
                    );
                    return Err(error);
                }
                Err(_) => {
                    return Err(
                        "the character database answered no query: it is shutting down".to_string(),
                    );
                }
            }
        }
        Ok(gone)
    }

    /// Give a character one more map it may travel to (R12), answering whether the row was new.
    pub fn unlock_map(&self, uuid: &str, map_id: u32) -> Result<bool, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "INSERT INTO \"character_unlock\" (\"character_uuid\", \"map_id\") \
                 VALUES ($1, $2::bigint) ON CONFLICT DO NOTHING RETURNING 1",
                vec![
                    Param::Text(uuid.to_string()),
                    Param::Integer(i64::from(map_id)),
                ],
            ),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(!rows.is_empty()),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot unlock map {map_id} for {uuid}: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// The maps a character may travel to (R12), in id order.
    pub fn unlocked_maps(&self, uuid: &str) -> Result<Vec<u32>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                "SELECT \"map_id\"::bigint FROM \"character_unlock\" \
                 WHERE \"character_uuid\" = $1 ORDER BY \"map_id\"",
                vec![Param::Text(uuid.to_string())],
            ),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        let rows = match reply.recv() {
            Ok(Ok(rows)) => rows,
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("Cannot read the unlocks of {uuid}: {error}"),
                );
                return Err(error);
            }
            Err(_) => {
                return Err(
                    "the character database answered no query: it is shutting down".to_string(),
                );
            }
        };
        Ok(rows
            .iter()
            .map(|row| row.get::<usize, i64>(0) as u32)
            .collect())
    }

    /// One row, as the list reads it.
    fn character(&self, row: &postgres::Row) -> Option<Character> {
        let uuid_text: String = row.get("uuid");
        let uuid = match uuid_bytes(&uuid_text) {
            Ok(uuid) => uuid,
            Err(error) => {
                self.log.out(
                    LogLevel::Error,
                    "server.characters",
                    format!("A character's uuid cannot be read: {error}"),
                );
                return None;
            }
        };

        // Every numeric column is BIGINT: a query parameter is a 64-bit integer in this pool, and a
        // narrower column would refuse it (docs/decisions/0007-character-schema.md).
        let appearance: i64 = row.get("appearance");
        let campaign: i64 = row.get("campaign");
        let level: i64 = row.get("level");
        let secondary: i64 = row.get("secondary_profession");
        let helm: i64 = row.get("helm_status");
        let outpost: i64 = row.get("last_outpost");

        Some(Character {
            uuid,
            name: row.get("name"),
            settings: CharacterSettings {
                last_outpost: outpost as u16,
                appearance: appearance as u32,
                campaign: campaign as u8,
                level: level as u8,
                is_pvp: row.get("is_pvp"),
                secondary_profession: secondary as u8,
                helm_status: helm as u8,
                // The armour pieces are the equipped bag's, and this tree has no items yet: a
                // character wearing nothing sends five empty slots, which is what the reference's
                // own record does.
                pieces: Vec::new(),
            },
        })
    }
}

/// A list of code units as comma-separated decimal text, the form the definition columns keep.
fn units_text(units: &[u16]) -> String {
    units
        .iter()
        .map(|unit| unit.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// A list of dwords as comma-separated decimal text.
fn dwords_text(values: &[u32]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// A column's code units, from the comma-separated decimal text it keeps.
fn units(text: &str) -> Vec<u16> {
    text.split(',')
        .filter(|part| !part.trim().is_empty())
        .filter_map(|part| part.trim().parse::<u16>().ok())
        .collect()
}

/// A column's dwords, from the comma-separated decimal text it keeps.
fn dwords(text: &str) -> Vec<u32> {
    text.split(',')
        .filter(|part| !part.trim().is_empty())
        .filter_map(|part| part.trim().parse::<u32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::configuration::{Config, Policy};
    use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};

    /// The live character database, when this machine has one.
    ///
    /// A source that cannot open its pool answers `None`, so a checkout without a database skips the
    /// test rather than failing it - the same shape the database crate's own live tests have.
    fn source() -> Option<DatabaseCharacters> {
        let log = Log::new();
        let config =
            Config::load_json("../authserver/authserver.json", false, Policy::default()).ok()?;
        let settings = read_settings(&config, DatabaseKind::Character).ok()?;
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log).ok()?;
        pool.ping().ok()?;
        Some(DatabaseCharacters::new(Arc::new(Mutex::new(pool)), log))
    }

    #[test]
    #[ignore = "needs the character database"]
    fn an_accounts_characters_are_listed_with_their_settings() {
        // Serialized with the other live tests that write the same table: this one counts one
        // account's rows, and another test's own row (on the same account) would be counted if it
        // interleaved.
        let _serial = database::updater::apply::test_lock("characters");
        let Some(source) = source() else {
            panic!("the character database is not reachable");
        };

        // A character of its own, so the test does not depend on what a client created.
        let account = "00000000-0000-0000-0000-00000000C10D";
        let uuid = "00000000-0000-0000-0000-0000000000AA";
        let pool = Arc::clone(&source.pool);
        {
            let mut pool = pool.lock().unwrap();
            pool.query_with("DELETE FROM \"characters\" WHERE \"uuid\" = $1", &[&uuid])
                .unwrap();
            pool.query_with(
                "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \"appearance\", \"primary_profession\", \"unlocked_professions\", \"campaign\", \"level\") \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&uuid, &account, &"Listed One", &0xABCDEFi64, &1i64, &2i64, &0i64, &1i64],
            )
            .unwrap();
        }

        let characters = source.for_account(account);
        assert_eq!(characters.len(), 1, "{characters:?}");
        assert_eq!(characters[0].name, "Listed One");
        assert_eq!(characters[0].settings.appearance, 0xAB_CDEF);
        assert_eq!(characters[0].settings.level, 1);
        assert_eq!(
            characters[0].settings.to_blob().len(),
            crate::characters::CHARACTER_SETTINGS_LEN,
            "the list carries a whole settings block, whatever the character wears"
        );

        // And a different account's characters are not in it.
        assert!(
            source
                .for_account("00000000-0000-0000-0000-000000000000")
                .is_empty()
        );

        // The whole table lists it too - the administration tool's own view.
        assert!(
            source
                .list()
                .iter()
                .any(|character| character.name == "Listed One"),
            "the list holds the character"
        );

        // R14: the outpost the character is in is recorded, and the next read of the list says so.
        source
            .set_last_outpost(uuid, 148)
            .expect("the outpost is recorded");
        let listed = source.list();
        assert_eq!(
            listed
                .iter()
                .find(|character| character.name == "Listed One")
                .map(|character| character.settings.last_outpost),
            Some(148),
            "the row now says the character is in map 148"
        );

        {
            let mut pool = pool.lock().unwrap();
            pool.query_with("DELETE FROM \"characters\" WHERE \"uuid\" = $1", &[&uuid])
                .unwrap();
        }
    }

    /// The live world database, when this machine has one: the item templates live there.
    fn world_source() -> Option<crate::data_stores::DatabaseItems> {
        let log = Log::new();
        let config =
            Config::load_json("../worldserver/worldserver.json", false, Policy::default()).ok()?;
        let settings = read_settings(&config, DatabaseKind::World).ok()?;
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log).ok()?;
        pool.ping().ok()?;
        Some(crate::data_stores::DatabaseItems::new(
            Arc::new(Mutex::new(pool)),
            log,
        ))
    }

    #[test]
    #[ignore = "needs the character database"]
    fn a_starting_item_write_answers_the_rows_it_wrote() {
        // The count is what the confirm's log line says, so it must be the rows, not the INSERT's
        // own answer: a statement without RETURNING answers no row even when it writes one, and a
        // log line that says "0 row(s) written" over written rows is a lie a load then trusts.
        //
        // Serialized with the other live tests that apply schemas to the same databases, so a run
        // of the whole suite cannot interleave a DDL pass with this write.
        let _serial = database::updater::apply::test_lock("characters");
        let _serial_world = database::updater::apply::test_lock("world");
        let Some(source) = source() else {
            panic!("the character database is not reachable");
        };
        let Some(items) = world_source() else {
            panic!("the world database is not reachable");
        };
        let log = Log::new();
        let mut world = crate::world::World::new(2);
        world
            .load_creation(
                "../config/World/Vanilla/creation/inventory.json",
                "../config/World/Vanilla/creation/equipment.json",
                Some(std::path::Path::new(
                    "../config/World/Vanilla/creation/worn_slots.json",
                )),
                &log,
            )
            .expect("the creation data");
        let tables = world.creation();
        let inventory = tables.inventory(0, 1);
        assert!(!inventory.items.is_empty(), "the set has items");

        // The templates the program reads back at start-up, from the same rows in the world database.
        items
            .seed(&tables.definitions())
            .expect("the definitions are seeded");
        let templates = items.list().expect("the templates are readable");

        let uuid = "00000000-0000-0000-0000-0000000000AB";
        let account = "00000000-0000-0000-0000-00000000C10D";
        {
            let mut pool = source.pool.lock().unwrap();
            for statement in [
                "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
                "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
            ] {
                pool.query_with(statement, &[&uuid]).unwrap();
            }
            pool.query_with(
                "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \"appearance\", \"primary_profession\", \"unlocked_professions\", \"campaign\", \"level\") \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&uuid, &account, &"Written One", &0i64, &1i64, &2i64, &0i64, &1i64],
            )
            .unwrap();
        }

        let written = source
            .write_starting_items(uuid, &inventory, &templates)
            .expect("the starting items are written");
        let stored = source.items_of(uuid).expect("the items are readable");
        // The count the confirm logs is the rows that are there.
        assert_eq!(written, stored.len(), "the count is the rows written");
        // And every item of the set became a row: a missing template would leave one out, with a
        // warning of its own.
        assert_eq!(
            stored.len(),
            inventory.items.len(),
            "every item of the set is a row"
        );

        {
            let mut pool = source.pool.lock().unwrap();
            pool.query_with(
                "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
                &[&uuid],
            )
            .unwrap();
            pool.query_with("DELETE FROM \"characters\" WHERE \"uuid\" = $1", &[&uuid])
                .unwrap();
        }
    }

    #[test]
    #[ignore = "needs the character database"]
    fn a_character_is_removed_with_its_own_records() {
        // The administration tool's remove: the row, its items and its unlocks, in one call. A row
        // that is not there answers false rather than failing, so a second click is harmless.
        let _serial = database::updater::apply::test_lock("characters");
        let Some(source) = source() else {
            panic!("the character database is not reachable");
        };
        let uuid = "00000000-0000-0000-0000-0000000000AC";
        let account = "00000000-0000-0000-0000-00000000C10D";
        {
            let mut pool = source.pool.lock().unwrap();
            pool.query_with(
                "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \"appearance\", \"primary_profession\", \"unlocked_professions\", \"campaign\", \"level\") \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[&uuid, &account, &"Removable One", &0i64, &1i64, &2i64, &0i64, &1i64],
            )
            .expect("the character row is written");
            pool.query_with(
                "INSERT INTO \"item_instance\" (\"character_uuid\", \"template_id\", \"file_id\", \"dye_tint\", \"dye_colors\", \"item_id\", \"bag_id\", \"slot\", \"quantity\", \"profession\") \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[&uuid, &1i64, &100i64, &0i64, &0i64, &9001i64, &1i64, &0i64, &1i64, &1i64],
            )
            .expect("the item row is written");
            pool.query_with(
                "INSERT INTO \"character_unlock\" (\"character_uuid\", \"map_id\") VALUES ($1, $2)",
                &[&uuid, &148i64],
            )
            .expect("the unlock row is written");
        }

        assert!(
            source.remove(uuid).expect("the character is removed"),
            "the row was there"
        );
        assert!(
            !source.remove(uuid).expect("a second remove answers"),
            "nothing is left to remove"
        );
        assert!(
            source
                .items_of(uuid)
                .expect("the items are readable")
                .is_empty(),
            "its items are gone"
        );
        assert!(
            source
                .unlocked_maps(uuid)
                .expect("the unlocks are readable")
                .is_empty(),
            "its unlocks are gone"
        );
    }
}
