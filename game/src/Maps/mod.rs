//! Where a world's maps come from: rows in `kryta_world` (`docs/decisions/0009-world-schema.md`).
//!
//! A map is the world's state - what exists, what file it loads from, where a character may stand - so it
//! is rows, not a settings file. The reference reads its map records the same way at
//! `SetInitialWorldSettings`: one query, held in memory for the life of the program, because a map's
//! definition does not change while the world serves.
//!
//! What is **not** here: an instance. A map is a definition; a running copy with a district, a population
//! and a clock is `game/Districts/`, and it is the half of the schema this repository does not read.

#[path = "Travels.rs"]
pub mod travels;

#[path = "Placement.rs"]
pub mod placement;

#[path = "TravelTargets.rs"]
pub mod travel_targets;

#[path = "Cartography.rs"]
pub mod cartography;

#[path = "InstanceTable.rs"]
pub mod instance_table;

#[path = "Portals.rs"]
pub mod portals;

#[path = "Pack.rs"]
pub mod pack;

pub use portals::{Arch, Crossed, Portals};

pub use travel_targets::{TRAVEL_TARGETS, TravelTarget};
pub use travels::{DatabaseTravels, Travel};

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

use crate::world::MapEntry;

/// One place a character may stand on entry (R13).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapSpawn {
    /// The map's id.
    pub map_id: u32,
    /// Which point of that map's list this is.
    pub index: i16,
    /// The point, in the game's own units.
    pub x: f32,
    /// The point's other axis.
    pub y: f32,
    /// Which plane it is on.
    pub plane: i16,
    /// The facing, in radians, a character entering at this point is created with, when the row states
    /// one (`ASURA/crates/asura-world/src/agent.rs:460-476`).
    pub angle: Option<f32>,
}

impl MapSpawn {
    /// The direction vector the agent is created facing: the angle as `(cos, sin)`, which is the
    /// unit-vector form `GAME_SMSG_WORLD_CREATE_AGENT` carries. A point with no angle keeps `(1, 0)`,
    /// which is what `ASURA/crates/asura-world/src/map_spawns.rs:44-53` answers.
    pub fn direction(&self) -> (f32, f32) {
        match self.angle {
            Some(angle) => (angle.cos(), angle.sin()),
            None => (1.0, 0.0),
        }
    }
}

/// Where a portal crossing puts a character: the destination's entry for the map it came from (R21).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrival {
    /// The map the character arrives on.
    pub map_id: u32,
    /// The map it left.
    pub from_map: u32,
    /// The point, in the game's own units.
    pub x: f32,
    /// The point's other axis.
    pub y: f32,
    /// Which plane it is on.
    pub plane: i16,
    /// The facing, in radians, when the map states one
    /// (`ASURA/crates/asura-world/src/portals.rs:129-148`, `Arrival::angle`).
    pub angle: Option<f32>,
}

impl Arrival {
    /// The direction vector the agent is created facing: `(cos, sin)`, or the default `(1, 0)`.
    ///
    /// `ASURA/crates/asura-world/src/portals.rs:140-147`, `Arrival::direction`.
    pub fn direction(&self) -> (f32, f32) {
        match self.angle {
            Some(angle) => (angle.cos(), angle.sin()),
            None => (1.0, 0.0),
        }
    }
}

const SELECT_ARRIVALS: &str = "SELECT \"map_id\"::bigint, \"from_map\"::bigint, \"x\", \"y\", \"plane\"::bigint, \"angle\" FROM \"map_arrival\" ORDER BY \"map_id\", \"from_map\"";

const SELECT_ARCHES: &str = "SELECT \"map_id\"::bigint, \"idx\"::bigint, \"to_map\"::bigint, \"from_x\", \"from_y\", \"to_x\", \"to_y\" FROM \"map_arch\" ORDER BY \"map_id\", \"idx\"";

const SELECT_MAPS: &str = "SELECT \"id\"::bigint, \"type\"::bigint, \"file_id\"::bigint, \"name\", \"party_cap\"::bigint, \"instance_value\"::bigint FROM \"map\" ORDER BY \"id\"";

const SELECT_SPAWNS: &str = "SELECT \"map_id\"::bigint, \"idx\"::bigint, \"x\", \"y\", \"plane\"::bigint, \"angle\" FROM \"map_spawn\" ORDER BY \"map_id\", \"idx\"";

/// The maps a world hosts, read from one database.
pub struct DatabaseMaps {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

impl DatabaseMaps {
    /// A repository that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// Every map definition, and every spawn point.
    pub fn load(&self) -> Result<(Vec<MapEntry>, Vec<MapSpawn>), String> {
        let maps = self
            .query(SELECT_MAPS)?
            .iter()
            .map(|row| {
                Ok(MapEntry {
                    id: read_number(row, "id")? as u32,
                    kind: row
                        .try_get::<_, Option<i64>>("type")
                        .map_err(|error| format!("column type: {error}"))?
                        .map(|value| value as u8),
                    file_id: read_number(row, "file_id")? as u32,
                    name: read_text(row, "name")?,
                    instance_value: row
                        .try_get::<_, Option<i64>>("instance_value")
                        .map_err(|error| format!("column instance_value: {error}"))?
                        .map(|value| value as u32),
                    name_id: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let spawns = self
            .query(SELECT_SPAWNS)?
            .iter()
            .map(|row| {
                Ok(MapSpawn {
                    map_id: read_number(row, "map_id")? as u32,
                    index: read_number(row, "idx")? as i16,
                    x: row
                        .try_get::<_, f32>("x")
                        .map_err(|error| format!("column x: {error}"))?,
                    y: row
                        .try_get::<_, f32>("y")
                        .map_err(|error| format!("column y: {error}"))?,
                    plane: read_number(row, "plane")? as i16,
                    angle: row
                        .try_get::<_, Option<f32>>("angle")
                        .map_err(|error| format!("column angle: {error}"))?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok((maps, spawns))
    }

    /// Every arrival point, for the portal crossings R21 decides.
    pub fn arrivals(&self) -> Result<Vec<Arrival>, String> {
        self.query(SELECT_ARRIVALS)?
            .iter()
            .map(|row| {
                Ok(Arrival {
                    map_id: read_number(row, "map_id")? as u32,
                    from_map: read_number(row, "from_map")? as u32,
                    x: row
                        .try_get::<_, f32>("x")
                        .map_err(|error| format!("column x: {error}"))?,
                    y: row
                        .try_get::<_, f32>("y")
                        .map_err(|error| format!("column y: {error}"))?,
                    plane: read_number(row, "plane")? as i16,
                    angle: row
                        .try_get::<_, Option<f32>>("angle")
                        .map_err(|error| format!("column angle: {error}"))?,
                })
            })
            .collect()
    }

    /// Every doorway wall, for the crossing a step decides
    /// (`ASURA/crates/asura-world/src/portals.rs:79-120`, `Arch`).
    pub fn arches(&self) -> Result<Vec<Arch>, String> {
        self.query(SELECT_ARCHES)?
            .iter()
            .map(|row| {
                Ok(Arch {
                    from_map: read_number(row, "map_id")? as u32,
                    idx: read_number(row, "idx")? as u32,
                    to_map: row
                        .try_get::<_, Option<i64>>("to_map")
                        .map_err(|error| format!("column to_map: {error}"))?
                        .map(|value| value as u32),
                    from: (
                        row.try_get::<_, f32>("from_x")
                            .map_err(|error| format!("column from_x: {error}"))?,
                        row.try_get::<_, f32>("from_y")
                            .map_err(|error| format!("column from_y: {error}"))?,
                    ),
                    to: (
                        row.try_get::<_, f32>("to_x")
                            .map_err(|error| format!("column to_x: {error}"))?,
                        row.try_get::<_, f32>("to_y")
                            .map_err(|error| format!("column to_y: {error}"))?,
                    ),
                })
            })
            .collect()
    }

    /// The party cap a map states, or `None` when the world's own setting applies (R15).
    ///
    /// The plan's map row carries the cap: "a map row overrides the world's own" - so the world's
    /// `PartySizeCap` is the default and this column is the exception a map makes.
    pub fn party_cap(&self, map_id: u32) -> Result<Option<u32>, String> {
        let rows = self.query_with(
            "SELECT \"party_cap\"::bigint FROM \"map\" WHERE \"id\" = $1::bigint::int",
            vec![Param::Integer(i64::from(map_id))],
        )?;
        let Some(row) = rows.first() else {
            return Err(format!("no map has id {map_id}"));
        };
        let cap: Option<i64> = row
            .try_get(0)
            .map_err(|error| format!("column party_cap: {error}"))?;
        Ok(cap.map(|value| value as u32))
    }

    /// State a map's own party cap (R15), or clear it with 0 so the world's `PartySizeCap` applies.
    ///
    /// The world's data is rows, and this is the administration tool's edit of one: the column the
    /// party stage (plan step 7) reads. Answers whether a map with that id is there.
    pub fn set_party_cap(&self, map_id: u32, cap: u32) -> Result<bool, String> {
        let rows = self.query_with(
            "UPDATE \"map\" SET \"party_cap\" = NULLIF($2::bigint::int, 0) \
             WHERE \"id\" = $1::bigint::int RETURNING 1",
            vec![
                Param::Integer(i64::from(map_id)),
                Param::Integer(i64::from(cap)),
            ],
        )?;
        Ok(!rows.is_empty())
    }

    /// Every map's own party cap, for a page that shows the whole table at once.
    ///
    /// A map that states no cap is absent from the answer, which is how the world's own
    /// `PartySizeCap` still applies (R15) without a second read per map.
    pub fn party_caps(&self) -> Result<std::collections::HashMap<u32, u32>, String> {
        let rows = self.query(
            "SELECT \"id\"::bigint, \"party_cap\"::bigint FROM \"map\" \
             WHERE \"party_cap\" IS NOT NULL ORDER BY \"id\"",
        )?;
        let mut caps = std::collections::HashMap::new();
        for row in &rows {
            let id: i64 = read_number(row, "id")?;
            let cap: i64 = read_number(row, "party_cap")?;
            caps.insert(id as u32, cap as u32);
        }
        Ok(caps)
    }

    /// Add a spawn point to a map at its next free index, and answer that index.
    ///
    /// The world's content is rows, and this is the administration tool's edit of one: the same
    /// `map_spawn` row the load's entry placement reads (R13). Every parameter is cast: the pool
    /// binds a number as 64-bit and these columns are narrower (`docs/decisions/0007-character-schema.md`).
    pub fn add_spawn(&self, map_id: u32, x: f32, y: f32, plane: i16) -> Result<i16, String> {
        let rows = self.query_with(
            "INSERT INTO \"map_spawn\" (\"map_id\", \"idx\", \"x\", \"y\", \"plane\") \
             SELECT $1::bigint::int, coalesce(max(\"idx\"), -1) + 1, $2::text::real, \
             $3::text::real, $4::bigint::int \
             FROM \"map_spawn\" WHERE \"map_id\" = $1::bigint::int RETURNING \"idx\"::bigint",
            vec![
                Param::Integer(i64::from(map_id)),
                Param::Text(format!("{x}")),
                Param::Text(format!("{y}")),
                Param::Integer(i64::from(plane)),
            ],
        )?;
        rows.first()
            .map(|row| read_number(row, "idx").map(|index| index as i16))
            .transpose()?
            .ok_or_else(|| "the spawn point was written but its index was not answered".to_string())
    }

    /// Remove a spawn point, and answer whether there was one at that index.
    pub fn remove_spawn(&self, map_id: u32, index: i16) -> Result<bool, String> {
        let rows = self.query_with(
            "DELETE FROM \"map_spawn\" WHERE \"map_id\" = $1::bigint::int AND \
             \"idx\" = $2::bigint::int RETURNING 1",
            vec![
                Param::Integer(i64::from(map_id)),
                Param::Integer(i64::from(index)),
            ],
        )?;
        Ok(!rows.is_empty())
    }

    /// One statement with parameters, and every row it answered.
    fn query_with(
        &self,
        sql: &str,
        params: Vec<database::database::Param>,
    ) -> Result<Vec<postgres::Row>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(sql.to_string(), params),
            Err(_) => return Err("the world database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.worldserver",
                    format!("the map tables could not be written: {error}"),
                );
                Err(error.to_string())
            }
            Err(_) => Err("the world database answered no query: it is shutting down".to_string()),
        }
    }

    /// One statement, and every row it answered.
    fn query(&self, sql: &str) -> Result<Vec<postgres::Row>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(sql, Vec::new()),
            Err(_) => return Err("the world database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.worldserver",
                    format!("the map tables could not be read: {error}"),
                );
                Err(error.to_string())
            }
            Err(_) => Err("the world database answered no query: it is shutting down".to_string()),
        }
    }
}

/// Where a world's maps come from.
///
/// Two sources answer the same three questions: the pack (the world's own files, which is what a
/// world *is*) and the database (rows, which is what a server *writes* and what a world was served
/// from before the pack existed). The world takes either, and its log says which it took, so the two
/// can be compared rather than silently swapped.
pub trait MapSource {
    /// Every map record, and every entry point.
    fn load_maps(&self) -> Result<(Vec<MapEntry>, Vec<MapSpawn>), String>;
    /// Every arrival point: where a crossing from another map lands.
    fn arrivals(&self) -> Result<Vec<Arrival>, String>;
    /// Every arch: the wall a step crosses.
    fn arches(&self) -> Result<Vec<Arch>, String>;
}

impl MapSource for DatabaseMaps {
    fn load_maps(&self) -> Result<(Vec<MapEntry>, Vec<MapSpawn>), String> {
        self.load()
    }

    fn arrivals(&self) -> Result<Vec<Arrival>, String> {
        DatabaseMaps::arrivals(self)
    }

    fn arches(&self) -> Result<Vec<Arch>, String> {
        DatabaseMaps::arches(self)
    }
}

/// One integer column, read as the pool's own width.
fn read_number(row: &postgres::Row, column: &str) -> Result<i64, String> {
    row.try_get::<_, i64>(column)
        .map_err(|error| format!("column {column}: {error}"))
}

/// One text column.
fn read_text(row: &postgres::Row, column: &str) -> Result<String, String> {
    row.try_get::<_, String>(column)
        .map_err(|error| format!("column {column}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::configuration::{Config, Policy};
    use database::database::{DatabaseKind, read_settings};

    fn repository() -> Option<DatabaseMaps> {
        let log = Log::new();
        let config =
            Config::load_json("../worldserver/worldserver.json", false, Policy::default()).ok()?;
        let settings = read_settings(&config, DatabaseKind::World).ok()?;
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log).ok()?;
        pool.ping().ok()?;
        Some(DatabaseMaps::new(Arc::new(Mutex::new(pool)), log))
    }

    #[test]
    #[ignore = "needs the world database"]
    fn the_world_holds_the_outpost_it_seeded() {
        let _serial = database::updater::apply::test_lock("world");
        let Some(maps) = repository() else {
            panic!("the world database is not reachable");
        };
        let (records, spawns) = maps.load().expect("the map tables read");
        let outpost = records
            .iter()
            .find(|map| map.id == 148)
            .expect("map 148 is seeded");
        assert_eq!(outpost.kind, Some(0), "Ascalon City is an outpost");
        assert_eq!(
            outpost.file_id, 113_021,
            "the file id the recording carried (0x0001B97D)"
        );
        assert!(
            spawns.iter().any(|spawn| spawn.map_id == 148),
            "and it has a point to stand on"
        );
        assert_eq!(
            maps.party_cap(148).expect("the cap is readable"),
            None,
            "the seed states no cap, so the world's own applies (R15)"
        );
        assert!(
            maps.party_cap(999_999).is_err(),
            "a map that is not there is not read"
        );
    }

    #[test]
    #[ignore = "needs the world database"]
    fn a_spawn_point_can_be_added_and_removed() {
        let Some(maps) = repository() else {
            panic!("the world database is not reachable");
        };
        let (_, before) = maps.load().expect("the map tables read");

        let index = maps
            .add_spawn(148, 9600.5, 4800.25, 0)
            .expect("the spawn point is written");
        let (_, spawns) = maps.load().expect("the map tables read");
        let added = spawns
            .iter()
            .find(|spawn| spawn.map_id == 148 && spawn.index == index)
            .expect("the point is the table's");
        assert!((added.x - 9600.5).abs() < 0.01 && (added.y - 4800.25).abs() < 0.01);

        assert!(
            maps.remove_spawn(148, index)
                .expect("the spawn point is removed")
        );
        let (_, after) = maps.load().expect("the map tables read");
        assert_eq!(after.len(), before.len(), "the point is gone again");
    }

    #[test]
    #[ignore = "needs the world database"]
    fn a_maps_own_party_cap_is_a_row_that_can_be_set_and_cleared() {
        let _serial = database::updater::apply::test_lock("world");
        let Some(maps) = repository() else {
            panic!("the world database is not reachable");
        };
        assert!(maps.set_party_cap(148, 12).expect("the cap is written"));
        assert_eq!(
            maps.party_cap(148).expect("the cap is readable"),
            Some(12),
            "the map states its own cap (R15)"
        );
        assert!(maps.set_party_cap(148, 0).expect("the cap is cleared"));
        assert_eq!(
            maps.party_cap(148).expect("the cap is readable"),
            None,
            "0 clears it, so the world's own PartySizeCap applies"
        );
        assert!(
            !maps
                .set_party_cap(999_999, 4)
                .expect("a map that is not there answers"),
            "a map that is not there is not updated"
        );
    }
}
