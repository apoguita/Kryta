//! Where the portal's accounts come from: one query on the login database.
//!
//! Accounts are rows, not settings. They are the server's state - a person is created once, plays,
//! and is looked up on every login - so they live in the login database's `account` table
//! (`data/sql/base/db_auth/account.sql`), and nothing about them is a file an operator shares.
//! `docs/decisions/0006-accounts-are-rows.md` is the record of that, taken because a JSON file had
//! held them while there was no schema and that was never meant to last.
//!
//! Reference: `AuthSocket::HandleAuthSession`, which asks `LoginDatabase` for the account row and
//! reads the answer. The reference sends the query through the pool's worker threads and reads the
//! row in the callback; this does the same thing with a channel, because the pool's blocking client
//! cannot be driven from inside the socket layer's runtime.

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

use game::portal::{Account, Accounts};

/// The statement. The login name arrives as a parameter, never in the text: it came off a socket.
const SELECT_ACCOUNT: &str =
    "SELECT \"user_id\", \"token\" FROM \"account\" WHERE \"username\" = $1 AND \"active\"";

/// How many accounts the table holds: what the live test below checks before it resolves one.
#[cfg(test)]
const COUNT_ACCOUNTS: &str = "SELECT COUNT(*) FROM \"account\"";

/// The accounts a client may log in as, read from the login database.
pub struct DatabaseAccounts {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

impl DatabaseAccounts {
    /// A source that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// How many accounts are there.
    #[cfg(test)]
    pub fn count(&self) -> Result<usize, String> {
        let mut pool = self
            .pool
            .lock()
            .map_err(|_| "the login database pool is poisoned".to_string())?;
        let rows = pool.query_with(COUNT_ACCOUNTS, &[])?;
        let count: i64 = rows.first().map(|row| row.get(0)).unwrap_or(0);
        Ok(count.max(0) as usize)
    }
}

impl Accounts for DatabaseAccounts {
    fn resolve(&self, login_name: &str) -> Option<Account> {
        // The query goes to a worker thread: the pool's blocking client cannot be driven from this
        // one, because it is inside the socket layer's runtime (see `DatabaseWorkerPool::query_reply`).
        // The lock covers the hand-over only; the wait below holds nothing.
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(SELECT_ACCOUNT, vec![Param::Text(login_name.to_string())]),
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.authserver",
                    "the login database pool is poisoned; nobody can be resolved",
                );
                return None;
            }
        };

        match reply.recv() {
            // A row is the account. No row is "who are you?", which the portal says itself.
            Ok(Ok(rows)) => rows.first().map(|row| Account {
                user_id: row.get("user_id"),
                token: row.get("token"),
            }),
            // A lookup that failed is not an unknown name: saying so is the difference between a
            // client typing the wrong login and a database nobody can reach.
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!("Cannot read the account '{login_name}': {error}"),
                );
                None
            }
            Err(_) => {
                self.log.out(
                    LogLevel::Error,
                    "server.authserver",
                    "The login database pool answered no query: it is shutting down.",
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::configuration::Config;
    use database::database::{DatabaseKind, read_settings};

    /// The live table, if this machine has one: the same `#[ignore]`d pattern the database crate
    /// uses, because a pool needs a server and a served client does not need this test.
    fn pool() -> Option<(Arc<Mutex<DatabaseWorkerPool>>, Log)> {
        let log = Log::new();
        let config =
            Config::load_json("../authserver/authserver.json", false, Default::default()).ok()?;
        let settings = read_settings(&config, DatabaseKind::Login).ok()?;
        let pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log).ok()?;
        Some((Arc::new(Mutex::new(pool)), log))
    }

    #[test]
    fn the_statement_takes_the_login_name_as_a_parameter() {
        // A name off a socket must never be concatenated into SQL. This is the invariant that keeps
        // it out: the statement has a placeholder and no other way in.
        assert!(SELECT_ACCOUNT.contains("$1"));
        assert!(!SELECT_ACCOUNT.contains("{}"));
    }

    #[test]
    fn an_inactive_account_uses_the_existing_unknown_login_path() {
        // The resolver only yields a row to the portal when it is active. A missing result already
        // follows the reference's unknown-login refusal path, so deactivation adds no wire response.
        assert!(SELECT_ACCOUNT.contains("AND \"active\""));
    }

    #[test]
    #[ignore = "needs the login database"]
    fn an_account_is_read_from_the_table() {
        let Some((pool, log)) = pool() else {
            panic!("the login database is not reachable");
        };
        let accounts = DatabaseAccounts::new(pool, log);

        assert!(accounts.count().unwrap() >= 2);
        let account = accounts
            .resolve("user1@example.com")
            .expect("the name this machine's client types is a row");
        assert_eq!(account.user_id.len(), 36);
        assert_eq!(account.token.len(), 36);
        assert!(accounts.resolve("nobody@example.com").is_none());
    }
}
