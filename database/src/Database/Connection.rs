//! One connection to one database.
//!
//! Reference: `src/server/database/Database/MySQLConnection.{h,cpp}`, with PostgreSQL behind it
//! (`docs/decisions/0001-postgresql.md`). The client is used in its blocking form because the pool
//! keeps the reference's threads and callback queue
//! (`docs/decisions/0002-worker-pool-threads.md`).

use postgres::{Client, NoTls};

use super::DatabaseInfo;

/// One open connection.
pub struct Connection {
    client: Client,
    database: String,
    /// Where every statement is reported, when tracing is on. Off unless a program asks for it.
    trace: Option<common::logging::Log>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Open a connection from a connection string's own fields.
    ///
    /// The TLS mode is refused rather than ignored: this build has no TLS, and a connection that
    /// silently dropped it would be a security property nobody asked for.
    pub fn open(info: &DatabaseInfo) -> Result<Self, String> {
        if let Some(ssl) = &info.ssl
            && !ssl.is_empty()
            && ssl != "disable"
        {
            return Err(format!(
                "sslmode '{ssl}' is not supported: this build's PostgreSQL client has no TLS"
            ));
        }

        let mut config = postgres::Config::new();
        config
            .host(&info.host)
            .dbname(&info.database)
            .user(&info.user)
            .password(&info.password);

        // The port field is the reference's `port_or_socket`: a number, or something to connect
        // over instead. A number is the only form this build reads.
        match info.port.parse::<u16>() {
            Ok(port) => {
                config.port(port);
            }
            Err(_) => {
                return Err(format!(
                    "the port field '{}' is not a port: this build does not read a socket path",
                    info.port
                ));
            }
        }

        let client = config
            .connect(NoTls)
            .map_err(|error| format!("cannot connect to {}: {error}", info.database))?;

        Ok(Self {
            client,
            database: info.database.clone(),
            trace: None,
        })
    }

    /// Report every statement this connection runs to this log.
    ///
    /// This is the database's side of the same instrument the socket layer has: without it a failed
    /// statement is an error with no statement in it, and a startup that applied the wrong file
    /// looks like a startup that worked.
    pub fn set_trace(&mut self, log: common::logging::Log) {
        self.trace = Some(log);
    }

    /// Report a statement, if tracing is on.
    fn trace_statement(&self, kind: &str, sql: &str) {
        if let Some(log) = self.trace.as_ref() {
            log.out(
                common::logging::LogLevel::Debug,
                "sql.trace",
                format!("{kind} [{}] {sql}", self.database),
            );
        }
    }

    /// The database this connection is to.
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Run a statement and answer how many rows it touched.
    ///
    /// Reference: `MySQLConnection::Execute` for a string.
    pub fn execute(&mut self, sql: &str) -> Result<u64, String> {
        self.trace_statement("execute", &one_line(sql));
        let rows = self
            .client
            .execute(sql, &[])
            .map_err(|error| format!("statement failed: {error}"))?;
        self.trace_statement("affected", &rows.to_string());
        Ok(rows)
    }

    /// Run a query and answer its rows.
    ///
    /// Reference: `MySQLConnection::Query`.
    pub fn query(&mut self, sql: &str) -> Result<Vec<postgres::Row>, String> {
        self.trace_statement("query", &one_line(sql));
        let rows = self
            .client
            .query(sql, &[])
            .map_err(|error| format!("query failed: {error}"))?;
        self.trace_statement("rows", &rows.len().to_string());
        Ok(rows)
    }

    /// Answer one value from a one-row, one-column query.
    pub fn query_scalar<T>(&mut self, sql: &str) -> Result<T, String>
    where
        T: for<'a> postgres::types::FromSql<'a>,
    {
        let rows = self.query(sql)?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| format!("query answered no rows: {sql}"))?;
        row.try_get::<usize, T>(0)
            .map_err(|error| format!("column 0 is not the expected type: {error}"))
    }

    /// Run a script: one or more statements, as a file holds them.
    ///
    /// Reference: `MySQLConnection::Execute` applied to a whole file, which is what the updater
    /// hands it. A simple query is used rather than a prepared statement, because a file is not a
    /// parameterised statement.
    pub fn batch_execute(&mut self, sql: &str) -> Result<(), String> {
        self.trace_statement("batch", &one_line(sql));
        self.client
            .batch_execute(sql)
            .map_err(|error| format!("statement failed: {error}"))
    }

    /// Record that a schema file has been applied.
    ///
    /// Reference: `UpdateFetcher::UpdateEntry`, which writes the name, the hash, the state, the
    /// time and the speed into the database's own history table.
    pub fn record_update(
        &mut self,
        name: &str,
        hash: &str,
        state: &str,
        speed: i32,
    ) -> Result<(), String> {
        let name_owned = name.to_string();
        let hash_owned = hash.to_string();
        let state_owned = state.to_string();

        self.client
            .execute(
                "INSERT INTO \"updates\" (\"name\", \"hash\", \"state\", \"speed\") \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (\"name\") DO UPDATE SET \
                 \"hash\" = EXCLUDED.\"hash\", \"state\" = EXCLUDED.\"state\", \"speed\" = EXCLUDED.\"speed\"",
                &[&name_owned, &hash_owned, &state_owned, &speed],
            )
            .map(|_| ())
            .map_err(|error| format!("cannot record the applied update '{name}': {error}"))
    }

    /// Whether the server still answers.
    ///
    /// Reference: the ping the reference runs on `MaxPingTime` to keep a connection from going
    /// quiet long enough for the server to drop it.
    pub fn ping(&mut self) -> Result<(), String> {
        self.query_scalar::<i32>("SELECT 1").map(|_| ())
    }
}

/// A statement as one line, for a log line that would otherwise be several.
pub fn one_line(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_multi_line_statement_becomes_one_line() {
        assert_eq!(
            super::one_line("SELECT 1\n  FROM t\n WHERE x = 2"),
            "SELECT 1 FROM t WHERE x = 2"
        );
    }
    use super::*;

    #[test]
    fn a_socket_path_in_the_port_field_is_refused() {
        let info = DatabaseInfo::parse("127.0.0.1;/var/run/mysqld.sock;kryta;pw;kryta_login").unwrap();
        let error = Connection::open(&info).unwrap_err();
        assert!(error.contains("is not a port"), "{error}");
    }

    #[test]
    fn a_tls_mode_this_build_cannot_honour_is_refused() {
        let info = DatabaseInfo::parse("127.0.0.1;5432;kryta;pw;kryta_login;require").unwrap();
        let error = Connection::open(&info).unwrap_err();
        assert!(error.contains("no TLS"), "{error}");
    }
}
