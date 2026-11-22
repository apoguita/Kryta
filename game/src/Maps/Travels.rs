//! The travel record: how a login hands a client to a world.
//!
//! A client asks for a map (`0x8029`); the login answers with a world's address and a token pair
//! (`0x0009`); the gameplay connection that follows presents that pair in its version message. The
//! row is the hand-off - written by the login, claimed once by the world - so the two programs never
//! talk to each other (`docs/port/06-world-plan.md`, "The three seams").
//!
//! Rows live in the login database beside `account` and `realmlist`, because a travel is a fact of
//! the login's world (which world, which map, which district) rather than a designer's content; the
//! world reads that database the same way the reference's world program does. Every number is
//! BIGINT - a query parameter is 64-bit in this pool, and a narrower column would refuse it
//! (`docs/decisions/0007-character-schema.md`).
//!
//! Both methods go through the pool's worker queue rather than its synchronous connection: a caller
//! here is a session running inside the network's runtime, and the synchronous connection's own
//! `block_on` is not allowed there (`database/src/Database/Pool.rs`).

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

const INSERT_TRAVEL: &str = "INSERT INTO \"travel\" (\"account\", \"world\", \"map_id\", \"district\", \
                             \"language\", \"player_token\", \"landing_x\", \"landing_y\", \
                             \"landing_plane\", \"landing_from\") VALUES ($1, $2, $3, $4, $5, $6, \
                             NULLIF($7::text, '')::real, NULLIF($8::text, '')::real, \
                             NULLIF($9::text, '')::bigint, NULLIF($10::text, '')::bigint)";

const CLAIM_TRAVEL: &str = "DELETE FROM \"travel\" WHERE \"world\" = $1 AND \"account\" = $2 AND \
                            \"player_token\" = $3 RETURNING \"account\", \"map_id\", \"district\", \
                            \"language\", \"landing_x\", \"landing_y\", \"landing_plane\", \"landing_from\"";

/// One hand-off: the login's promise about where a client is going.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Travel {
    /// The account, in the wire's uuid text form, the form `characters.account_uuid` holds.
    pub account: [u8; 16],
    /// The world the login named in `0x0009`, as its server id.
    pub world: u32,
    /// The map the client asked for.
    pub map_id: u32,
    /// The district the request carried; 0 means "any", which the world resolves to the first
    /// district with room (R22 - one district to begin, R16).
    pub district: u32,
    /// The client's language id, which travels with the request (R57).
    pub language: u32,
    /// The number the login drew for this travel, carried back in the version message.
    pub player_token: u32,
    /// Where the destination puts the arriving character, when the trigger knew: a doorway crossing
    /// hands over the doorway's own landing point, and every other travel states none
    /// (`ASURA/crates/asura-world/src/travel.rs:446-468`).
    pub landing: Option<Landing>,
}

/// Where a crossing puts the arriving character, and which map it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    /// The point the destination states for a character arriving from `from_map` (R21).
    pub x: f32,
    /// The point's other axis.
    pub y: f32,
    /// Which plane it is on.
    pub plane: i16,
    /// The map the character crossed from.
    pub from_map: u32,
}

/// The login database's `travel` table, as both programs use it.
pub struct DatabaseTravels {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

impl DatabaseTravels {
    /// A source that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// Write the row a `0x8029` answer promises: the account, the world, the map, the district,
    /// the language and the token.
    ///
    /// A write that failed is reported, and the caller decides what to do about it. The login
    /// answers the client either way - the world reads the map from the version message when it
    /// finds no record - but a record that was not kept is a fault worth a log line at error.
    pub fn record(&self, travel: &Travel) -> Result<(), String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                INSERT_TRAVEL,
                vec![
                    Param::Text(crate::characters::uuid_text(&travel.account)),
                    Param::Integer(i64::from(travel.world)),
                    Param::Integer(i64::from(travel.map_id)),
                    Param::Integer(i64::from(travel.district)),
                    Param::Integer(i64::from(travel.language)),
                    Param::Integer(i64::from(travel.player_token)),
                    // A landing is four columns or none: the statement casts these text forms, and an
                    // empty text is the NULL that says this travel states no landing.
                    Param::Text(
                        travel
                            .landing
                            .map(|landing| landing.x.to_string())
                            .unwrap_or_default(),
                    ),
                    Param::Text(
                        travel
                            .landing
                            .map(|landing| landing.y.to_string())
                            .unwrap_or_default(),
                    ),
                    Param::Text(
                        travel
                            .landing
                            .map(|landing| landing.plane.to_string())
                            .unwrap_or_default(),
                    ),
                    Param::Text(
                        travel
                            .landing
                            .map(|landing| landing.from_map.to_string())
                            .unwrap_or_default(),
                    ),
                ],
            ),
            Err(_) => {
                return Err(
                    "the login database pool is poisoned; the travel record was not written"
                        .to_string(),
                );
            }
        };
        match reply.recv() {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!(
                        "Cannot record the travel to map {} on world {}: {error}",
                        travel.map_id, travel.world
                    ),
                );
                Err(error)
            }
            Err(_) => Err("the login database answered no query: it is shutting down".to_string()),
        }
    }

    /// Claim a travel once: read it and delete it in the same statement, so two connections
    /// presenting the same token pair cannot both take it.
    ///
    /// The row answers `None` when nothing matches - a connection that opened no travel asks for a
    /// map by its version message alone, which is how the character-creation instance arrives. A
    /// failed query is an error rather than "no row", because the two mean different things.
    pub fn claim(
        &self,
        world: u32,
        account: &str,
        player_token: u32,
    ) -> Result<Option<Travel>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(
                CLAIM_TRAVEL,
                vec![
                    Param::Integer(i64::from(world)),
                    Param::Text(account.to_string()),
                    Param::Integer(i64::from(player_token)),
                ],
            ),
            Err(_) => {
                return Err(
                    "the login database pool is poisoned; no travel can be claimed".to_string(),
                );
            }
        };
        let rows = match reply.recv() {
            Ok(Ok(rows)) => rows,
            Ok(Err(error)) => {
                return Err(format!("the travel table cannot be read: {error}"));
            }
            Err(_) => {
                return Err("the login database answered no query: it is shutting down".to_string());
            }
        };
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let account_text: String = row.get(0);
        // A landing is written as four columns or as none of them, so the position's two halves decide
        // whether there is one at all; a row that states only half is not a landing.
        let landing = match (
            row.get::<usize, Option<f32>>(4),
            row.get::<usize, Option<f32>>(5),
        ) {
            (Some(x), Some(y)) => Some(Landing {
                x,
                y,
                plane: row
                    .get::<usize, Option<i64>>(6)
                    .map(|plane| plane as i16)
                    .unwrap_or(0),
                from_map: row
                    .get::<usize, Option<i64>>(7)
                    .map(|map| map as u32)
                    .unwrap_or(0),
            }),
            _ => None,
        };
        Ok(Some(Travel {
            account: crate::characters::uuid_bytes(&account_text)
                .map_err(|error| format!("the claimed travel's account cannot be read: {error}"))?,
            world,
            map_id: row.get::<usize, i64>(1) as u32,
            district: row.get::<usize, i64>(2) as u32,
            language: row.get::<usize, i64>(3) as u32,
            player_token,
            landing,
        }))
    }
}
