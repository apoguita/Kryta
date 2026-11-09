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
        })
    }

    /// The database this connection is to.
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Run a statement and answer how many rows it touched.
    ///
    /// Reference: `MySQLConnection::Execute` for a string.
    pub fn execute(&mut self, sql: &str) -> Result<u64, String> {
        self.client
            .execute(sql, &[])
            .map_err(|error| format!("statement failed: {error}"))
    }

    /// Run a query and answer its rows.
    ///
    /// Reference: `MySQLConnection::Query`.
    pub fn query(&mut self, sql: &str) -> Result<Vec<postgres::Row>, String> {
        self.client
            .query(sql, &[])
            .map_err(|error| format!("query failed: {error}"))
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

    /// Whether the server still answers.
    ///
    /// Reference: the ping the reference runs on `MaxPingTime` to keep a connection from going
    /// quiet long enough for the server to drop it.
    pub fn ping(&mut self) -> Result<(), String> {
        self.query_scalar::<i32>("SELECT 1").map(|_| ())
    }
}

#[cfg(test)]
mod tests {
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
