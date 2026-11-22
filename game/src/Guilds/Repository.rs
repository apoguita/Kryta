//! Guilds: the guild row, its members, and its open invitations
//! (`data/sql/base/db_characters/0009-guilds.sql`).
//!
//! Rows only, ported from `ASURA/crates/asura-db/src/guilds.rs` (343 lines, all carried: `Cape`,
//! `Guild`, `Member`, `NewGuild` `:12-96`; `membership` `:161-176`; `create` `:178-230`;
//! `character_named` `:232-238`; `is_member`, `is_invited` `:240-260`; `invite` `:262-283`;
//! `other_members` `:285-295`; `leave`, `dissolve` `:297-313`; `set_announcement` `:315-329`;
//! `advance_revision` `:331-342`). Who may create, invite, leave or set the announcement is
//! `game/Guilds/mod.rs`; what a new guild starts with is `config/Server/guilds.json`, handed in as
//! [`NewGuild`].
//!
//! **One difference of arrangement, named.** ASURA's `create` is a two-statement transaction; this
//! tree's worker pool runs one statement per request, so the guild and its founder are written by one
//! statement (a data-modifying `WITH`), which Postgres runs atomically. The founder's own guard (it is
//! in no guild) is in the same statement; if a race still leaves a guild with no member, it is removed
//! at once. Every key is the character's `uuid` text, which is how every other character table here is
//! keyed (`characters.sql`), where ASURA keys by its own uuid column.

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

use crate::characters::{uuid_bytes, uuid_text};

/// A guild's cape, as its row stores it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cape {
    /// Background colour.
    pub background_color: u8,
    /// Detail colour.
    pub detail_color: u8,
    /// Emblem colour.
    pub emblem_color: u8,
    /// Shape.
    pub shape: u8,
    /// Detail pattern.
    pub detail: u8,
    /// Emblem.
    pub emblem: u16,
    /// Trim.
    pub trim: u8,
}

/// One guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guild {
    /// The guild's id, which is also the sixteen-byte key its messages carry.
    pub id: [u8; 16],
    /// Its name.
    pub name: String,
    /// Its tag.
    pub tag: String,
    /// `0x0121`'s feature byte.
    pub feature: u8,
    /// `0x0121`'s territory byte.
    pub territory: u8,
    /// Its cape.
    pub cape: Cape,
    /// Its allegiance.
    pub allegiance: u8,
    /// Faction points.
    pub faction: u32,
    /// Qualifier points.
    pub qualifier: u32,
    /// Ladder rating.
    pub rating: u32,
    /// Ladder position.
    pub rank: u32,
    /// The announcement text.
    pub announcement: String,
    /// Who set it.
    pub announcement_author: String,
    /// The counter `0x0133` carries; advanced once per change.
    pub revision: u32,
}

/// One member row: the character's own place in its guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The client's member type (5 the leader).
    pub member_type: u8,
    /// The name it was invited under.
    pub invited_name: String,
    /// Who invited it.
    pub invited_by: String,
    /// When it joined, in seconds since 1970-01-01 UTC.
    pub joined_at: i64,
}

/// What a new guild's row starts with: the values in `config/Server/guilds.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewGuild {
    /// `0x0121`'s feature byte.
    pub feature: u8,
    /// `0x0121`'s territory byte.
    pub territory: u8,
    /// Allegiance.
    pub allegiance: u8,
    /// Ladder rating.
    pub rating: u32,
    /// Ladder position.
    pub rank: u32,
    /// The first value `0x0133` carries.
    pub revision: u32,
}

const GUILD_COLUMNS: &str = "g.\"id\", g.\"name\", g.\"tag\", g.\"feature\", g.\"territory\", \
g.\"cape_background\", g.\"cape_detail_color\", g.\"cape_emblem_color\", g.\"cape_shape\", \
g.\"cape_detail\", g.\"cape_emblem\", g.\"cape_trim\", g.\"allegiance\", g.\"faction\", \
g.\"qualifier\", g.\"rating\", g.\"rank\", g.\"announcement\", g.\"announcement_author\", \
g.\"revision\"";

/// The number of columns [`GUILD_COLUMNS`] selects.
const GUILD_WIDTH: usize = 20;

const MEMBER_COLUMNS: &str = "m.\"member_type\", m.\"invited_name\", m.\"invited_by\", \
extract(epoch from m.\"joined_at\")::bigint";

fn number(row: &postgres::Row, index: usize) -> Result<i64, String> {
    row.try_get::<usize, i64>(index)
        .map_err(|error| format!("column {index}: {error}"))
}

fn text(row: &postgres::Row, index: usize) -> Result<String, String> {
    row.try_get::<usize, String>(index)
        .map_err(|error| format!("column {index}: {error}"))
}

fn byte(row: &postgres::Row, index: usize) -> Result<u8, String> {
    Ok(number(row, index)?.clamp(0, 255) as u8)
}

fn dword(row: &postgres::Row, index: usize) -> Result<u32, String> {
    Ok(number(row, index)?.clamp(0, i64::from(u32::MAX)) as u32)
}

/// A guild from [`GUILD_COLUMNS`], starting at column `at`.
fn guild_at(row: &postgres::Row, at: usize) -> Result<Guild, String> {
    let id = text(row, at)?;
    Ok(Guild {
        id: uuid_bytes(&id).map_err(|_| format!("guild id {id:?} is not a uuid"))?,
        name: text(row, at + 1)?,
        tag: text(row, at + 2)?,
        feature: byte(row, at + 3)?,
        territory: byte(row, at + 4)?,
        cape: Cape {
            background_color: byte(row, at + 5)?,
            detail_color: byte(row, at + 6)?,
            emblem_color: byte(row, at + 7)?,
            shape: byte(row, at + 8)?,
            detail: byte(row, at + 9)?,
            emblem: number(row, at + 10)?.clamp(0, 65_535) as u16,
            trim: byte(row, at + 11)?,
        },
        allegiance: byte(row, at + 12)?,
        faction: dword(row, at + 13)?,
        qualifier: dword(row, at + 14)?,
        rating: dword(row, at + 15)?,
        rank: dword(row, at + 16)?,
        announcement: text(row, at + 17)?,
        announcement_author: text(row, at + 18)?,
        revision: dword(row, at + 19)?,
    })
}

/// A member from [`MEMBER_COLUMNS`], starting at column `at`.
fn member_at(row: &postgres::Row, at: usize) -> Result<Member, String> {
    Ok(Member {
        member_type: byte(row, at)?,
        invited_name: text(row, at + 1)?,
        invited_by: text(row, at + 2)?,
        joined_at: number(row, at + 3)?,
    })
}

/// The guild tables, as a program uses them.
pub struct DatabaseGuilds {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

impl DatabaseGuilds {
    /// A source that asks this pool (the character database: guilds are what characters write).
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// One statement through the worker queue, its rows or the reason there are none.
    fn query(&self, statement: &str, params: Vec<Param>) -> Result<Vec<postgres::Row>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(statement, params),
            Err(_) => return Err("the character database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.guilds",
                    format!("A guild statement failed: {error}"),
                );
                Err(error)
            }
            Err(_) => {
                Err("the character database answered no query: it is shutting down".to_string())
            }
        }
    }

    /// The guild a character is in and its own member row, or `None` when it is in none
    /// (`guilds.rs:161-176`).
    pub fn membership(&self, character: &[u8; 16]) -> Result<Option<(Guild, Member)>, String> {
        let statement = format!(
            "SELECT {GUILD_COLUMNS}, {MEMBER_COLUMNS} \
             FROM \"guild_members\" m JOIN \"guilds\" g ON g.\"id\" = m.\"guild_id\" \
             WHERE m.\"character_uuid\" = $1"
        );
        let rows = self.query(&statement, vec![Param::Text(uuid_text(character))])?;
        match rows.first() {
            Some(row) => Ok(Some((guild_at(row, 0)?, member_at(row, GUILD_WIDTH)?))),
            None => Ok(None),
        }
    }

    /// Create a guild with its founder as the only member (`guilds.rs:178-230`). `None`, and nothing
    /// written, when the name is taken or the founder is in a guild already: both are unique keys, so
    /// two founders racing for one name cannot both win.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        key: [u8; 16],
        name: &str,
        tag: &str,
        start: &NewGuild,
        founder: &[u8; 16],
        founder_name: &str,
        leader_type: u8,
    ) -> Result<Option<(Guild, Member)>, String> {
        let id = uuid_text(&key);
        let rows = self.query(
            "WITH made AS ( \
               INSERT INTO \"guilds\" (\"id\", \"name\", \"tag\", \"feature\", \"territory\", \
                 \"allegiance\", \"rating\", \"rank\", \"revision\") \
               SELECT $1::text, $2::text, $3::text, $4::bigint, $5::bigint, $6::bigint, $7::bigint, \
                 $8::bigint, $9::bigint \
               WHERE NOT EXISTS (SELECT 1 FROM \"guild_members\" WHERE \"character_uuid\" = $10::text) \
               ON CONFLICT DO NOTHING \
               RETURNING \"id\") \
             INSERT INTO \"guild_members\" (\"character_uuid\", \"guild_id\", \"member_type\", \
               \"invited_name\", \"invited_by\") \
             SELECT $10::text, made.\"id\", $11::bigint, $12::text, $12::text FROM made \
             ON CONFLICT DO NOTHING \
             RETURNING 1",
            vec![
                Param::Text(id.clone()),
                Param::Text(name.to_string()),
                Param::Text(tag.to_string()),
                Param::Integer(i64::from(start.feature)),
                Param::Integer(i64::from(start.territory)),
                Param::Integer(i64::from(start.allegiance)),
                Param::Integer(i64::from(start.rating)),
                Param::Integer(i64::from(start.rank)),
                Param::Integer(i64::from(start.revision)),
                Param::Text(uuid_text(founder)),
                Param::Integer(i64::from(leader_type)),
                Param::Text(founder_name.to_string()),
            ],
        )?;
        if rows.is_empty() {
            // A guild written without its founder (a race on the founder's own row) is not a guild.
            self.query(
                "DELETE FROM \"guilds\" WHERE \"id\" = $1 \
                 AND NOT EXISTS (SELECT 1 FROM \"guild_members\" WHERE \"guild_id\" = $1)",
                vec![Param::Text(id)],
            )?;
            return Ok(None);
        }
        self.membership(founder)
    }

    /// The character with this name, matched without regard to case: its id and its name as stored
    /// (`guilds.rs:232-238`).
    pub fn character_named(&self, name: &str) -> Result<Option<([u8; 16], String)>, String> {
        let rows = self.query(
            "SELECT \"uuid\", \"name\" FROM \"characters\" WHERE lower(\"name\") = lower($1) LIMIT 1",
            vec![Param::Text(name.to_string())],
        )?;
        match rows.first() {
            Some(row) => {
                let id = text(row, 0)?;
                let id =
                    uuid_bytes(&id).map_err(|_| format!("character id {id:?} is not a uuid"))?;
                Ok(Some((id, text(row, 1)?)))
            }
            None => Ok(None),
        }
    }

    /// Whether a character is in any guild (`guilds.rs:240-248`).
    pub fn is_member(&self, character: &[u8; 16]) -> Result<bool, String> {
        Ok(!self
            .query(
                "SELECT 1 FROM \"guild_members\" WHERE \"character_uuid\" = $1",
                vec![Param::Text(uuid_text(character))],
            )?
            .is_empty())
    }

    /// Whether this guild has an open invitation of this character (`guilds.rs:250-260`).
    pub fn is_invited(&self, guild: &[u8; 16], character: &[u8; 16]) -> Result<bool, String> {
        Ok(!self
            .query(
                "SELECT 1 FROM \"guild_invites\" WHERE \"guild_id\" = $1 AND \"character_uuid\" = $2",
                vec![Param::Text(uuid_text(guild)), Param::Text(uuid_text(character))],
            )?
            .is_empty())
    }

    /// Open an invitation. Returns when it was made, in seconds since 1970-01-01 UTC, or `None` when
    /// this guild had already invited the character (`guilds.rs:262-283`).
    pub fn invite(
        &self,
        guild: &[u8; 16],
        character: &[u8; 16],
        invited_name: &str,
        invited_by: &str,
    ) -> Result<Option<i64>, String> {
        let rows = self.query(
            "INSERT INTO \"guild_invites\" (\"guild_id\", \"character_uuid\", \"invited_name\", \
               \"invited_by\") \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT DO NOTHING \
             RETURNING extract(epoch from \"invited_at\")::bigint",
            vec![
                Param::Text(uuid_text(guild)),
                Param::Text(uuid_text(character)),
                Param::Text(invited_name.to_string()),
                Param::Text(invited_by.to_string()),
            ],
        )?;
        rows.first().map(|row| number(row, 0)).transpose()
    }

    /// How many members a guild has besides this character (`guilds.rs:285-295`).
    pub fn other_members(&self, guild: &[u8; 16], character: &[u8; 16]) -> Result<usize, String> {
        let rows = self.query(
            "SELECT count(*)::bigint FROM \"guild_members\" \
             WHERE \"guild_id\" = $1 AND \"character_uuid\" <> $2",
            vec![
                Param::Text(uuid_text(guild)),
                Param::Text(uuid_text(character)),
            ],
        )?;
        let count = rows.first().map(|row| number(row, 0)).transpose()?;
        Ok(usize::try_from(count.unwrap_or(0)).unwrap_or(0))
    }

    /// Take a character out of its guild. Whether there was a row to remove (`guilds.rs:297-304`).
    pub fn leave(&self, character: &[u8; 16]) -> Result<bool, String> {
        Ok(!self
            .query(
                "DELETE FROM \"guild_members\" WHERE \"character_uuid\" = $1 RETURNING 1",
                vec![Param::Text(uuid_text(character))],
            )?
            .is_empty())
    }

    /// Delete a guild; its members and invitations go with it (`ON DELETE CASCADE`;
    /// `guilds.rs:306-313`).
    pub fn dissolve(&self, guild: &[u8; 16]) -> Result<bool, String> {
        Ok(!self
            .query(
                "DELETE FROM \"guilds\" WHERE \"id\" = $1 RETURNING 1",
                vec![Param::Text(uuid_text(guild))],
            )?
            .is_empty())
    }

    /// Store the announcement and who set it (`guilds.rs:315-329`).
    pub fn set_announcement(
        &self,
        guild: &[u8; 16],
        text: &str,
        author: &str,
    ) -> Result<(), String> {
        self.query(
            "UPDATE \"guilds\" SET \"announcement\" = $2, \"announcement_author\" = $3 WHERE \"id\" = $1",
            vec![
                Param::Text(uuid_text(guild)),
                Param::Text(text.to_string()),
                Param::Text(author.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Advance the guild's `0x0133` counter by `step` and return the new value, wrapping within a
    /// dword. `None` when the guild is gone (`guilds.rs:331-342`).
    pub fn advance_revision(&self, guild: &[u8; 16], step: u32) -> Result<Option<u32>, String> {
        let rows = self.query(
            "UPDATE \"guilds\" SET \"revision\" = (\"revision\" + $2::bigint) % 4294967296 \
             WHERE \"id\" = $1 RETURNING \"revision\"",
            vec![
                Param::Text(uuid_text(guild)),
                Param::Integer(i64::from(step)),
            ],
        )?;
        rows.first().map(|row| dword(row, 0)).transpose()
    }
}
