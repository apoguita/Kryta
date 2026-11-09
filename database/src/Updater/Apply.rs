//! The apply half: run the files a database has not had, and record what ran.
//!
//! Reference: `DBUpdater::{Update, Apply}` and `UpdateFetcher::{ReceiveAppliedFiles, UpdateEntry}`.
//!
//! The rules, all the reference's:
//!
//! - A file whose name is already recorded and whose hash matches is skipped.
//! - A file whose name is recorded but whose hash does not match is **refused**, not re-applied:
//!   the bytes an operator is running are not the bytes the database already had.
//! - A file that is not recorded runs, and is recorded with its name, its sha1, its state, the
//!   time, and how long it took.
//! - A file that fails stops the update. The reference stops the server rather than continue with a
//!   database it could not bring up to date.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Instant;

use common::logging::{Log, LogLevel};

use super::{read_sql_update, UpdateFile};

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UpdateSummary {
    /// Files that ran in this pass.
    pub updated: usize,
    /// Files already recorded with the same bytes.
    pub skipped: usize,
}

/// A file that could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// The file's bytes are not the bytes the database recorded for that name.
    Modified(PathBuf, String, String),
    /// The statement failed.
    Rejected(PathBuf, String),
    /// Reading a file or asking the database what it has already had failed.
    Unreachable(String),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Modified(path, recorded, found) => write!(
                f,
                "the file '{}' was modified after it was applied: the database recorded {recorded}, the file is {found}",
                path.display()
            ),
            Self::Rejected(path, reason) => {
                write!(f, "the file '{}' failed to apply: {reason}", path.display())
            }
            Self::Unreachable(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for ApplyError {}

/// The `updates` table's name in every database.
///
/// Reference: the table each database keeps for itself.
pub const UPDATES_TABLE: &str = "updates";

/// Apply every file the database has not had, in the order given.
/// The DDL for the history table, used when a database has none.
///
/// Reference: `DBUpdater` creates its own `updates` table when it finds none, because the first
/// file it applies needs somewhere to record itself. This is the same table
/// `data/sql/base/db_auth/updates.sql` creates; that file is what a human reads.
const ENSURE_HISTORY_TABLE: &str = "CREATE TABLE IF NOT EXISTS \"updates\" (\n  \
\"name\"      VARCHAR(200) PRIMARY KEY,\n  \
\"hash\"      CHAR(40)     DEFAULT '',\n  \
\"state\"     VARCHAR(16)  NOT NULL DEFAULT 'RELEASED',\n  \
\"timestamp\" TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,\n  \
\"speed\"     INTEGER      NOT NULL DEFAULT 0\n)";

/// Apply every file the database has not had, in the order given.
pub fn apply(
    connection: &mut crate::database::Connection,
    files: &[UpdateFile],
    log: &Log,
) -> Result<UpdateSummary, ApplyError> {
    // A database with no history table has had nothing, and the first file needs a place to record
    // itself.
    connection
        .batch_execute(ENSURE_HISTORY_TABLE)
        .map_err(ApplyError::Unreachable)?;

    let recorded = read_recorded(connection)?;
    let mut summary = UpdateSummary::default();

    for file in files {
        let text = read_sql_update(&file.path).map_err(|error| ApplyError::Unreachable(error.to_string()))?;
        let hash = sha1_hex(&text);

        if let Some(existing) = recorded.get(&file.name()) {
            if existing.eq_ignore_ascii_case(&hash) {
                summary.skipped += 1;
                continue;
            }
            return Err(ApplyError::Modified(file.path.clone(), existing.clone(), hash));
        }

        let started = Instant::now();
        connection
            .batch_execute(&text)
            .map_err(|error| ApplyError::Rejected(file.path.clone(), error))?;
        let speed = started.elapsed().as_millis() as i32;

        connection
            .record_update(&file.name(), &hash, file.state.as_str(), speed)
            .map_err(|error| ApplyError::Rejected(file.path.clone(), error))?;

        log.out(
            LogLevel::Info,
            "sql.updates",
            format!("{} applied in {speed} ms.", file.name()),
        );

        summary.updated += 1;
    }

    Ok(summary)
}

/// What the database says it has already had.
///
/// A database whose `updates` table does not exist yet answers nothing, which is what the first run
/// against an empty database needs.
fn read_recorded(
    connection: &mut crate::database::Connection,
) -> Result<BTreeMap<String, String>, ApplyError> {
    let sql = format!("SELECT \"name\", \"hash\" FROM \"{UPDATES_TABLE}\"");
    match connection.query(&sql) {
        Ok(rows) => Ok(rows
            .iter()
            .map(|row| {
                let name: String = row.get(0);
                let hash: String = row.get(1);
                (name, hash)
            })
            .collect()),
        Err(_) => Ok(BTreeMap::new()),
    }
}

/// A file's sha1, which is what the record holds.
///
/// Reference: `UpdateFetcher::CalculateHash`, whose 40 hexadecimal characters are the same ones.
pub fn sha1_hex(text: &str) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();

    let mut out = String::with_capacity(40);
    for byte in digest {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_forty_hexadecimal_characters() {
        let hash = sha1_hex("CREATE TABLE t (x int);\n");
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|character| character.is_ascii_hexdigit()));

        // The same text hashes the same way, and a different text does not.
        assert_eq!(hash, sha1_hex("CREATE TABLE t (x int);\n"));
        assert_ne!(hash, sha1_hex("CREATE TABLE t (x int); \n"));
    }

    #[test]
    fn the_hash_is_the_sha1_of_the_bytes() {
        // The empty string's sha1, which is a published value.
        assert_eq!(sha1_hex(""), "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709");
    }

    /// The whole pass, against a real server.
    ///
    /// Ignored by default for the same reason the pool's live tests are: a checkout without a
    /// database should not fail its test run. Run it with `KRYTA_TEST_LOGIN_DB` set.
    #[test]
    #[ignore = "needs a live database: set KRYTA_TEST_LOGIN_DB"]
    fn a_file_is_applied_once_recorded_and_a_changed_file_is_refused() {
        use crate::database::{Connection, DatabaseInfo};
        use crate::updater::{collect_files, UpdateState};

        let Ok(info) = std::env::var("KRYTA_TEST_LOGIN_DB") else {
            return;
        };
        let info = DatabaseInfo::parse(&info).unwrap();
        let mut connection = Connection::open(&info).unwrap();
        let log = Log::new();

        // A directory of its own, so nothing here touches the checkout's own files.
        let dir = std::env::temp_dir().join(format!("kryta-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("01_history.sql"),
            "DROP TABLE IF EXISTS \"updates\";\n\
             CREATE TABLE \"updates\" (\n\
               \"name\" VARCHAR(200) PRIMARY KEY,\n\
               \"hash\" CHAR(40) DEFAULT '',\n\
               \"state\" VARCHAR(16) NOT NULL DEFAULT 'RELEASED',\n\
               \"timestamp\" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,\n\
               \"speed\" INTEGER NOT NULL DEFAULT 0\n\
             );\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("02_probe.sql"),
            "DROP TABLE IF EXISTS t_apply_probe;\nCREATE TABLE t_apply_probe (x int);\n",
        )
        .unwrap();

        let files = collect_files(&dir, UpdateState::Released).unwrap();
        assert_eq!(files.len(), 2, "the two files were not both collected");

        // First pass: everything runs.
        let first = apply(&mut connection, &files, &log).unwrap();
        assert_eq!(first.updated, 2, "{first:?}");
        assert_eq!(first.skipped, 0);

        // Second pass: nothing runs, because the record already holds those bytes.
        let second = apply(&mut connection, &files, &log).unwrap();
        assert_eq!(second.updated, 0, "{second:?}");
        assert_eq!(second.skipped, 2);

        // A file whose bytes changed is refused, not re-applied.
        std::fs::write(
            dir.join("02_probe.sql"),
            "DROP TABLE IF EXISTS t_apply_probe;\nCREATE TABLE t_apply_probe (x int, y int);\n",
        )
        .unwrap();
        let files = collect_files(&dir, UpdateState::Released).unwrap();
        let error = apply(&mut connection, &files, &log).unwrap_err();
        assert!(matches!(error, ApplyError::Modified(_, _, _)), "{error}");

        // Leave the database as it was found.
        connection
            .batch_execute("DROP TABLE IF EXISTS t_apply_probe")
            .unwrap();
        connection
            .batch_execute("DROP TABLE IF EXISTS \"updates\"")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        log.shutdown();
    }
    /// The schema this repository ships, applied to a real database.
    ///
    /// Ignored by default like the others. This is the end-to-end proof of the base tree: the files
    /// under `data/sql/base/db_auth/` are collected, applied in name order, recorded, and a second
    /// pass applies nothing.
    #[test]
    #[ignore = "needs a live database: set KRYTA_TEST_LOGIN_DB"]
    fn the_shipped_login_schema_applies_and_is_recorded() {
        use crate::database::{Connection, DatabaseInfo};
        use crate::updater::{collect_files, UpdateState};

        let Ok(info) = std::env::var("KRYTA_TEST_LOGIN_DB") else {
            return;
        };
        let info = DatabaseInfo::parse(&info).unwrap();
        let mut connection = Connection::open(&info).unwrap();
        let log = Log::new();

        // cargo runs a test from its package root, so the schema is one directory up.
        let schema = std::path::Path::new("../data/sql/base/db_auth");
        let files = collect_files(schema, UpdateState::Released).unwrap();
        assert!(files.len() >= 2, "the base tree should hold the history and a table");

        // Start from nothing, so the first pass is the whole schema.
        connection.batch_execute("DROP TABLE IF EXISTS \"updates\"").unwrap();
        connection.batch_execute("DROP TABLE IF EXISTS \"realmlist\"").unwrap();

        let first = apply(&mut connection, &files, &log).unwrap();
        assert_eq!(first.updated, files.len(), "{first:?}");

        // Both tables exist, and every file is recorded.
        let rows = connection
            .query("SELECT count(*) FROM \"updates\"")
            .unwrap();
        let recorded: i64 = rows[0].get(0);
        assert_eq!(recorded as usize, files.len());

        let rows = connection
            .query("SELECT count(*) FROM information_schema.tables WHERE table_name IN ('realmlist', 'updates')")
            .unwrap();
        let tables: i64 = rows[0].get(0);
        assert_eq!(tables, 2, "the schema did not create both tables");

        // A second pass applies nothing, which is what a restart has to do.
        let second = apply(&mut connection, &files, &log).unwrap();
        assert_eq!(second.updated, 0, "{second:?}");
        assert_eq!(second.skipped, files.len());

        // The column set the world list will read.
        let rows = connection
            .query(
                "SELECT column_name FROM information_schema.columns \
                 WHERE table_name = 'realmlist' ORDER BY ordinal_position",
            )
            .unwrap();
        let columns: Vec<String> = rows.iter().map(|row| row.get(0)).collect();
        assert_eq!(
            columns,
            vec![
                "id",
                "name",
                "address",
                "localAddress",
                "localSubnetMask",
                "port",
                "icon",
                "flag",
                "timezone",
                "allowedSecurityLevel",
                "population",
                "gamebuild"
            ]
        );

        log.shutdown();
    }
}