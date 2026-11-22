//! The `instance` table: one row per running copy of a map.
//!
//! A copy is the world's state, so the row is world data (`kryta_world.instance`,
//! `data/sql/base/db_world/0003-instances.sql`) and the repository is the noun's own: the reference
//! keeps its instance records in memory and in its instance manager; this tree keeps the row for
//! what outlives a connection - which district holds which number, and when it last emptied, which
//! is where R37's hold is measured from (`docs/game-rules.md`).
//!
//! Every method goes through the pool's worker queue: a caller here is a session running inside the
//! network's runtime, and the synchronous connection's own `block_on` is not allowed there
//! (`database/src/Database/Pool.rs`).

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

const INSERT_INSTANCE: &str = "INSERT INTO \"instance\" (\"map_id\", \"district\", \"language\", \
                               \"kind\") VALUES ($1, $2, $3, $4) RETURNING \"id\"::bigint";

const MARK_EMPTY: &str = "UPDATE \"instance\" SET \"empty_since\" = now() WHERE \"id\" = $1";

const DISPOSE: &str = "DELETE FROM \"instance\" WHERE \"id\" = $1";

const DISPOSE_ALL: &str = "DELETE FROM \"instance\" RETURNING 1";

/// The `instance` table, as a world uses it.
pub struct DatabaseInstances {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

/// One copy as the table records it, for a page that shows what is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRow {
    /// The map it is a copy of.
    pub map_id: u32,
    /// Its district number.
    pub district: u32,
    /// The language it was opened for.
    pub language: u32,
    /// 0 outpost, 1 explorable (the map's own type).
    pub kind: u8,
    /// When it opened, as the database prints it.
    pub opened_at: Option<String>,
    /// When it last became empty; `None` while somebody is inside.
    pub empty_since: Option<String>,
}

impl DatabaseInstances {
    /// A source that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// Write the row a new copy is given, and answer its id.
    pub fn open(&self, map_id: u32, district: u32, language: u32, kind: u8) -> Result<i64, String> {
        let rows = self.query(
            INSERT_INSTANCE,
            vec![
                Param::Integer(i64::from(map_id)),
                Param::Integer(i64::from(district)),
                Param::Integer(i64::from(language)),
                Param::Integer(i64::from(kind)),
            ],
        )?;
        rows.first()
            .map(|row| row.get::<usize, i64>(0))
            .ok_or_else(|| "the instance row was written but its id was not answered".to_string())
    }

    /// Record that the last member left, which starts R37's hold.
    pub fn mark_empty(&self, id: i64) -> Result<(), String> {
        self.query(MARK_EMPTY, vec![Param::Integer(id)])?;
        Ok(())
    }

    /// Delete one copy's row - the disposal R40 acts on.
    pub fn dispose(&self, id: i64) -> Result<(), String> {
        self.query(DISPOSE, vec![Param::Integer(id)])?;
        Ok(())
    }

    /// Delete every row, and answer how many were deleted.
    ///
    /// A world that just started has nobody inside anything: every row it finds is left over from a
    /// run that ended, so it is disposed rather than held.
    pub fn dispose_all(&self) -> Result<usize, String> {
        let rows = self.query(DISPOSE_ALL, Vec::new())?;
        Ok(rows.len())
    }

    /// Every copy the table records, for the administration tool's page: the open ones and the
    /// ones awaiting disposal.
    pub fn rows(&self) -> Result<Vec<InstanceRow>, String> {
        let rows = self.query(
            "SELECT \"map_id\"::bigint, \"district\"::bigint, \"language\"::bigint, \
             \"kind\"::bigint, to_char(\"opened_at\", 'YYYY-MM-DD HH24:MI:SS'), \
             to_char(\"empty_since\", 'YYYY-MM-DD HH24:MI:SS') FROM \"instance\" \
             ORDER BY \"map_id\", \"district\"",
            Vec::new(),
        )?;
        rows.iter()
            .map(|row| {
                Ok(InstanceRow {
                    map_id: row
                        .try_get::<usize, i64>(0)
                        .map_err(|error| format!("map: {error}"))?
                        as u32,
                    district: row
                        .try_get::<usize, i64>(1)
                        .map_err(|error| format!("district: {error}"))?
                        as u32,
                    language: row
                        .try_get::<usize, i64>(2)
                        .map_err(|error| format!("language: {error}"))?
                        as u32,
                    kind: row
                        .try_get::<usize, i64>(3)
                        .map_err(|error| format!("kind: {error}"))? as u8,
                    opened_at: row
                        .try_get::<usize, Option<String>>(4)
                        .map_err(|error| format!("opened at: {error}"))?,
                    empty_since: row
                        .try_get::<usize, Option<String>>(5)
                        .map_err(|error| format!("empty since: {error}"))?,
                })
            })
            .collect()
    }

    /// One statement, and every row it answered.
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<postgres::Row>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(sql, params),
            Err(_) => {
                return Err("the world database pool is poisoned".to_string());
            }
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.worldserver",
                    format!("the instance table could not be written: {error}"),
                );
                Err(error)
            }
            Err(_) => Err("the world database answered no query: it is shutting down".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::configuration::{Config, Policy};
    use database::database::{DatabaseKind, read_settings};
    use database::updater::{UpdateState, collect_files};
    use std::path::Path;

    #[test]
    #[ignore = "needs the world database"]
    fn a_copy_is_opened_emptied_and_disposed() {
        let _serial = database::updater::apply::test_lock("world");
        let log = Log::new();
        let Some(config) =
            Config::load_json("../worldserver/worldserver.json", false, Policy::default()).ok()
        else {
            panic!("the world program's configuration is not readable");
        };
        let settings = read_settings(&config, DatabaseKind::World).expect("the world settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log)
            .expect("the world pool opens");
        let schema = collect_files(
            Path::new("../data/sql/base/db_world"),
            UpdateState::Released,
        )
        .expect("the world schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let store = DatabaseInstances::new(Arc::new(Mutex::new(pool)), log.clone());

        let id = store.open(148, 7, 0, 0).expect("the copy opens");
        assert!(id > 0, "the row has an id");
        store.mark_empty(id).expect("the copy can be marked empty");
        store.dispose(id).expect("the copy can be disposed");

        store.open(148, 8, 0, 0).expect("another copy opens");
        let left = store.dispose_all().expect("every row disposes");
        assert!(left >= 1, "the copy that was opened is disposed");
        log.shutdown();
    }

    #[test]
    #[ignore = "needs the world database"]
    fn a_copy_is_read_as_a_row_for_the_tool() {
        let _serial = database::updater::apply::test_lock("world");
        let log = Log::new();
        let Some(config) =
            Config::load_json("../worldserver/worldserver.json", false, Policy::default()).ok()
        else {
            panic!("the world program's configuration is not readable");
        };
        let settings = read_settings(&config, DatabaseKind::World).expect("the world settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log)
            .expect("the world pool opens");
        let schema = collect_files(
            Path::new("../data/sql/base/db_world"),
            UpdateState::Released,
        )
        .expect("the world schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let store = DatabaseInstances::new(Arc::new(Mutex::new(pool)), log.clone());

        let id = store.open(148, 9, 1, 0).expect("the copy opens");
        let rows = store.rows().expect("the table reads");
        let row = rows
            .iter()
            .find(|row| row.map_id == 148 && row.district == 9)
            .expect("the copy is a row");
        assert_eq!(row.language, 1);
        assert_eq!(row.kind, 0, "an outpost");
        assert!(row.opened_at.is_some());
        assert_eq!(row.empty_since, None, "somebody is inside while it opens");

        store.mark_empty(id).expect("the copy can be marked empty");
        let rows = store.rows().expect("the table reads");
        let row = rows
            .iter()
            .find(|row| row.map_id == 148 && row.district == 9)
            .expect("the copy is still a row");
        assert!(row.empty_since.is_some(), "and its hold has begun");

        store.dispose(id).expect("the copy can be disposed");
        assert!(
            !store
                .rows()
                .expect("the table reads")
                .iter()
                .any(|row| row.map_id == 148 && row.district == 9),
            "the disposed copy is gone"
        );
        log.shutdown();
    }
}
