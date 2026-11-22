//! An account: who may log in, and the state that belongs to the person rather than to a character.
//!
//! Reference: the reference's `src/server/game/Accounts/`, and the `account` table both programs read
//! (`docs/decisions/0006-accounts-are-rows.md`). This module is that noun's data access: the queries
//! live here, in the library, so the auth program, the world program and the administration tool ask
//! the same code for the same rows instead of each holding its own copy of the SQL.
//!
//! A row is state, never a setting: an account is created by an operator and changed by playing, which
//! is why the EULA version is a column (`data/sql/base/db_auth/account_eula.sql`) and not a key in a
//! file.

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

/// One account row, as the server and the tool read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The row's own id.
    pub id: i64,
    /// The login name the client types.
    pub username: String,
    /// The portal identifier the client carries in its login message.
    pub user_id: String,
    /// The token the portal issued.
    pub token: String,
    /// The EULA version this account accepted, or 0 for none.
    pub eula: i64,
    /// Whether the account may start a new portal login.
    pub active: bool,
}

const SELECT_ALL: &str = "SELECT \"id\"::bigint, \"username\", \"user_id\", \"token\", \"eula\"::bigint, \"active\" FROM \"account\" ORDER BY \"id\"";

const SELECT_ONE: &str = "SELECT \"id\"::bigint, \"username\", \"user_id\", \"token\", \"eula\"::bigint, \"active\" FROM \"account\" WHERE \"username\" = $1";

/// An account is created with the identifiers the portal will issue for it, so a fresh account can log
/// in without a second step.
const INSERT_ACCOUNT: &str = "INSERT INTO \"account\" (\"username\", \"user_id\", \"token\") VALUES ($1, $2, $3) ON CONFLICT (\"username\") DO NOTHING RETURNING \"id\"::bigint";

// The version travels as text and is cast in the statement: a value from a form is text, and one cast
// here keeps the pool's parameter types out of the shape of the query.
const SET_EULA: &str = "UPDATE \"account\" SET \"eula\" = $2::text::int WHERE \"username\" = $1::text RETURNING \"id\"::bigint";

/// Changes the identifiers the portal login checks, while the row keeps its own identity.
const UPDATE_ACCOUNT: &str = "UPDATE \"account\" SET \"username\" = $2, \"user_id\" = $3, \"token\" = $4 WHERE \"id\" = $1::text::int RETURNING \"id\"::bigint";

/// Keeps the row and its character records while admitting or refusing future portal logins.
const SET_ACTIVE: &str = "UPDATE \"account\" SET \"active\" = $2::text::boolean WHERE \"id\" = $1::text::int RETURNING \"id\"::bigint";

const DELETE_ACCOUNT: &str =
    "DELETE FROM \"account\" WHERE \"username\" = $1 RETURNING \"id\"::bigint";

/// The account table, through a worker pool.
pub struct DatabaseAccounts {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

impl DatabaseAccounts {
    /// A repository that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// Every account, oldest first.
    pub fn list(&self) -> Result<Vec<Account>, String> {
        self.query(SELECT_ALL, Vec::new())?
            .iter()
            .map(account_of)
            .collect()
    }

    /// How many accounts there are, without reading their rows.
    ///
    /// The manager's overview is a count of what exists, so it asks the database to count instead
    /// of listing every account and discarding the rows it did not need.
    pub fn count(&self) -> Result<usize, String> {
        let rows = self.query("SELECT count(*)::bigint FROM \"account\"", Vec::new())?;
        rows.first()
            .map(|row| row.get::<usize, i64>(0) as usize)
            .ok_or_else(|| "the account table counted no row".to_string())
    }

    /// One account by the name the client types.
    pub fn find(&self, username: &str) -> Result<Option<Account>, String> {
        let rows = self.query(SELECT_ONE, vec![Param::Text(username.to_string())])?;
        rows.first().map(account_of).transpose()
    }

    /// Create an account, with the identifiers its portal login will present.
    ///
    /// Answers whether the row was written: a name that is taken writes nothing, and the caller says so
    /// rather than pretending.
    pub fn create(&self, username: &str, user_id: &str, token: &str) -> Result<bool, String> {
        let rows = self.query(
            INSERT_ACCOUNT,
            vec![
                Param::Text(username.to_string()),
                Param::Text(user_id.to_string()),
                Param::Text(token.to_string()),
            ],
        )?;
        Ok(!rows.is_empty())
    }

    /// Record the EULA version an account accepted, or 0 to make the client ask again.
    pub fn set_eula(&self, username: &str, version: i64) -> Result<bool, String> {
        let rows = self.query(
            SET_EULA,
            vec![
                Param::Text(username.to_string()),
                Param::Text(version.to_string()),
            ],
        )?;
        Ok(!rows.is_empty())
    }

    /// Change the portal identifiers an operator entered for this account.
    ///
    /// The row id is stable even when the login name changes, so editing a name cannot accidentally
    /// update another account selected by that new name.
    pub fn update(
        &self,
        id: i64,
        username: &str,
        user_id: &str,
        token: &str,
    ) -> Result<bool, String> {
        let rows = self.query(
            UPDATE_ACCOUNT,
            vec![
                Param::Text(id.to_string()),
                Param::Text(username.to_string()),
                Param::Text(user_id.to_string()),
                Param::Text(token.to_string()),
            ],
        )?;
        Ok(!rows.is_empty())
    }

    /// Admit or deactivate an account without deleting the account or its character data.
    pub fn set_active(&self, id: i64, active: bool) -> Result<bool, String> {
        let rows = self.query(
            SET_ACTIVE,
            vec![Param::Text(id.to_string()), Param::Text(active.to_string())],
        )?;
        Ok(!rows.is_empty())
    }

    /// Record the EULA version for the account the portal identified, by its own identifier.
    ///
    /// The client accepts the EULA before the character list with only a version byte; the session
    /// knows the account by the portal's uuid, and this is the same column `set_eula` writes by name.
    pub fn set_eula_for_user(&self, user_id: &str, version: i64) -> Result<bool, String> {
        let rows = self.query(
            "UPDATE \"account\" SET \"eula\" = $2::text::int WHERE \"user_id\" = $1::text RETURNING \"id\"::bigint",
            vec![Param::Text(user_id.to_string()), Param::Text(version.to_string())],
        )?;
        Ok(!rows.is_empty())
    }

    /// The EULA version the account the portal identified has accepted, or `None` when no row is keyed
    /// by that identifier.
    ///
    /// This is the read the login answer makes: the version goes back in the account info, and the
    /// client stops asking (`AuthSrv_SendAccountInfo`; `ASURA/crates/asura-auth/src/lib.rs:613`).
    pub fn eula_for_user(&self, user_id: &str) -> Result<Option<i64>, String> {
        let rows = self.query(
            "SELECT \"eula\"::bigint FROM \"account\" WHERE \"user_id\" = $1::text",
            vec![Param::Text(user_id.to_string())],
        )?;
        Ok(rows.first().map(|row| row.get::<usize, i64>(0)))
    }

    /// The UI settings blob the account the portal identified last uploaded, or `None` when it never
    /// uploaded one (or no row is keyed by that identifier).
    ///
    /// This is the read the login answer makes: `AUTH_SMSG_ACCOUNT_SETTINGS` carries the blob back, so
    /// the client starts with the layout it saved instead of the committed default
    /// (`data/sql/base/db_auth/account_settings.sql`; `ASURA/crates/asura-db/src/account.rs:36-52`). The
    /// bytes travel as hex text because the pool binds text, integers and flags only.
    pub fn settings_for_user(&self, user_id: &str) -> Result<Option<Vec<u8>>, String> {
        let rows = self.query(
            "SELECT encode(\"settings\", 'hex') FROM \"account\" WHERE \"user_id\" = $1::text",
            vec![Param::Text(user_id.to_string())],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let hex: Option<String> = row
            .try_get(0)
            .map_err(|error| format!("column settings: {error}"))?;
        // An empty blob is returned as one, not as "never uploaded": the login path refuses it and
        // says so, as ASURA's does (`ASURA/crates/asura-auth/src/lib.rs:850-868`).
        hex.map(|hex| decode_hex(&hex)).transpose()
    }

    /// Record the UI settings blob the client uploaded for the account the portal identified.
    ///
    /// Ported from `ASURA/crates/asura-db/src/account.rs:54-65` (`record_settings`).
    /// `false` means no row matched, which the caller can only report: the upload was already
    /// acknowledged, and refusing after the fact would make the client retry a write with nowhere to
    /// land.
    pub fn set_settings_for_user(&self, user_id: &str, settings: &[u8]) -> Result<bool, String> {
        let rows = self.query(
            "UPDATE \"account\" SET \"settings\" = decode($2::text, 'hex') WHERE \"user_id\" = $1::text RETURNING \"id\"::bigint",
            vec![
                Param::Text(user_id.to_string()),
                Param::Text(encode_hex(settings)),
            ],
        )?;
        Ok(!rows.is_empty())
    }

    /// Remove an account. Its characters are the character database's business and are left alone.
    pub fn delete(&self, username: &str) -> Result<bool, String> {
        let rows = self.query(DELETE_ACCOUNT, vec![Param::Text(username.to_string())])?;
        Ok(!rows.is_empty())
    }

    /// Run one statement and read every row back, or say why it could not.
    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<postgres::Row>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(sql, params),
            Err(_) => return Err("the login database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!("the account table could not be read: {error}"),
                );
                Err(error.to_string())
            }
            Err(_) => Err("the login database answered no query: it is shutting down".to_string()),
        }
    }
}

/// Bytes as lowercase hex, the form `decode(..., 'hex')` reads.
fn encode_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Hex text, as `encode(..., 'hex')` writes it, back to bytes.
fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err(format!(
            "the settings column is {} hex digits, an odd number",
            text.len()
        ));
    }
    (0..text.len())
        .step_by(2)
        .map(|at| {
            u8::from_str_radix(&text[at..at + 2], 16)
                .map_err(|error| format!("the settings column is not hex: {error}"))
        })
        .collect()
}

/// One row, as an [`Account`]. A column of the wrong width is an error, not a panic: a page that cannot
/// read a row says so instead of taking the process down with it.
fn account_of(row: &postgres::Row) -> Result<Account, String> {
    let read = |column: &str| {
        row.try_get::<_, String>(column)
            .map_err(|error| format!("column {column}: {error}"))
    };
    let read_number = |column: &str| {
        row.try_get::<_, i64>(column)
            .map_err(|error| format!("column {column}: {error}"))
    };
    Ok(Account {
        id: read_number("id")?,
        username: read("username")?,
        user_id: read("user_id")?,
        token: read("token")?,
        eula: read_number("eula")?,
        active: row
            .try_get("active")
            .map_err(|error| format!("column active: {error}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::configuration::{Config, Policy};
    use database::database::{DatabaseKind, read_settings};

    #[test]
    fn the_settings_blob_survives_the_hex_round_trip() {
        let blob: Vec<u8> = (0..=255u8).chain([0x08, 0x00, 0x66, 0x21]).collect();
        assert_eq!(decode_hex(&encode_hex(&blob)).unwrap(), blob);
        assert_eq!(encode_hex(&[0x08, 0x00, 0xff]), "0800ff");
        assert!(decode_hex("abc").is_err());
        assert!(decode_hex("zz").is_err());
    }

    #[test]
    #[ignore = "needs the login database"]
    fn the_accounts_ui_settings_are_stored_and_read_back() {
        let Some(accounts) = repository() else {
            panic!("the login database is not reachable");
        };
        let name = "ui-settings-test@example.com";
        let user = "00000000-0000-0000-0000-00000000AAC1";
        let _ = accounts.delete(name);
        assert!(
            accounts
                .create(name, user, "00000000-0000-0000-0000-00000000AAC2")
                .expect("created")
        );
        assert_eq!(
            accounts.settings_for_user(user).expect("read"),
            None,
            "an account that never uploaded has no blob"
        );
        let blob: Vec<u8> = (0..=255u8).chain([0x08, 0x00, 0x66, 0x21]).collect();
        assert!(
            accounts
                .set_settings_for_user(user, &blob)
                .expect("stored"),
            "the account row was written"
        );
        assert_eq!(
            accounts.settings_for_user(user).expect("read"),
            Some(blob),
            "the next login reads back the client's own bytes"
        );
        assert!(accounts.delete(name).expect("removed"));
    }

    /// The live login database, when this machine has one.
    fn repository() -> Option<DatabaseAccounts> {
        let log = Log::new();
        let config =
            Config::load_json("../authserver/authserver.json", false, Policy::default()).ok()?;
        let settings = read_settings(&config, DatabaseKind::Login).ok()?;
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log).ok()?;
        pool.ping().ok()?;
        Some(DatabaseAccounts::new(Arc::new(Mutex::new(pool)), log))
    }

    #[test]
    #[ignore = "needs the login database"]
    fn an_account_can_be_created_read_changed_and_removed() {
        let Some(accounts) = repository() else {
            panic!("the login database is not reachable");
        };
        let name = "manage-test@example.com";
        let _ = accounts.delete(name);

        assert!(
            accounts
                .create(
                    name,
                    "00000000-0000-0000-0000-00000000AAA1",
                    "00000000-0000-0000-0000-00000000AAA2"
                )
                .expect("created")
        );
        let row = accounts
            .find(name)
            .expect("read")
            .expect("the row is there");
        assert_eq!(row.username, name);
        assert_eq!(row.eula, 0, "a new account has accepted no EULA");
        assert!(row.active, "a new account can log in");

        assert!(
            accounts
                .update(
                    row.id,
                    "manage-renamed@example.com",
                    "00000000-0000-0000-0000-00000000AAB1",
                    "00000000-0000-0000-0000-00000000AAB2"
                )
                .expect("updated")
        );
        let row = accounts
            .find("manage-renamed@example.com")
            .expect("read")
            .expect("the renamed row is there");
        assert!(accounts.set_active(row.id, false).expect("deactivated"));
        assert!(
            !accounts
                .find("manage-renamed@example.com")
                .expect("read")
                .expect("the row remains")
                .active
        );

        assert!(accounts.set_eula(name, 26).expect("the EULA is set"));
        assert_eq!(accounts.find(name).expect("read").expect("there").eula, 26);

        assert!(
            accounts
                .delete("manage-renamed@example.com")
                .expect("removed")
        );
        assert!(
            accounts
                .find("manage-renamed@example.com")
                .expect("read")
                .is_none()
        );
    }
}
