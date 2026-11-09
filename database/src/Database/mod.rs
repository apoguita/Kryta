//! The three logical databases and how the configuration names them.
//!
//! Reference: `src/server/database/Database/` - `DatabaseEnv`, `DatabaseLoader`,
//! `DatabaseWorkerPool` and `MySQLConnectionInfo`. What is here now is the part that needs no
//! server to be right: which databases exist, how a connection string is read, and what the
//! loader asks the configuration for.
//!
//! The engine is PostgreSQL rather than the reference's MySQL (`docs/decisions/0001-postgresql.md`)
//! and the pool keeps the reference's threads and callback queue
//! (`docs/decisions/0002-worker-pool-threads.md`). The connection string keeps the reference's
//! shape - five semicolon-separated fields - so a value moves between the two engines by changing
//! the port and the names in it, not the code that reads it.

use common::configuration::Config;

/// One of the three databases a program can open.
///
/// Reference: the three pools `DatabaseEnv` declares - `LoginDatabase`, `WorldDatabase` and
/// `CharacterDatabase` - and the names `DatabaseLoader` keys the configuration on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseKind {
    /// Accounts, sessions, the world list. Read by both programs.
    Login,
    /// The world's own tables: what the game is made of rather than what its players did.
    World,
    /// Characters, their items and their progress.
    Character,
}

impl DatabaseKind {
    /// The name the configuration keys use.
    ///
    /// Reference: the `name` a caller passes to `DatabaseLoader::AddDatabase`, which becomes
    /// `<name>DatabaseInfo` and `<name>Database.WorkerThreads`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Login => "Login",
            Self::World => "World",
            Self::Character => "Character",
        }
    }

    /// What the connection string is when the configuration does not name one.
    ///
    /// Reference: `GetDefaultDatabaseInfo`, which answers
    /// `127.0.0.1;3306;acore;acore;acore_auth` and its two siblings. These are this port's own
    /// defaults for its own engine: same shape, PostgreSQL's port and this project's names.
    pub fn default_info(self) -> &'static str {
        match self {
            Self::Login => "127.0.0.1;5432;kryta;kryta;kryta_login",
            Self::World => "127.0.0.1;5432;kryta;kryta;kryta_world",
            Self::Character => "127.0.0.1;5432;kryta;kryta;kryta_characters",
        }
    }
}

/// How to reach one database.
///
/// Reference: `MySQLConnectionInfo`, whose fields are `user`, `password`, `database`, `host`,
/// `port_or_socket` and `ssl`, read out of one semicolon-separated string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseInfo {
    /// The address.
    pub host: String,
    /// The port, or a socket path.
    pub port: String,
    /// The user.
    pub user: String,
    /// The password.
    pub password: String,
    /// The database name.
    pub database: String,
    /// The SSL mode, when the string carries one.
    pub ssl: Option<String>,
}

impl DatabaseInfo {
    /// Read a connection string: `host;port;user;password;database[;ssl]`.
    ///
    /// The reference's own defaults show the field order, and its struct shows the sixth field.
    pub fn parse(info: &str) -> Result<Self, String> {
        let fields: Vec<&str> = info.split(';').collect();

        if fields.len() < 5 {
            return Err(format!(
                "database connection string needs host;port;user;password;database, got '{}'",
                info
            ));
        }
        if fields.len() > 6 {
            return Err(format!(
                "database connection string has more than six fields: '{}'",
                info
            ));
        }

        Ok(Self {
            host: fields[0].to_string(),
            port: fields[1].to_string(),
            user: fields[2].to_string(),
            password: fields[3].to_string(),
            database: fields[4].to_string(),
            ssl: fields.get(5).map(|field| field.to_string()),
        })
    }
}

/// What a program needs to open one database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseSettings {
    /// Where it is.
    pub info: DatabaseInfo,
    /// How many worker threads serve it, each with its own connection.
    pub worker_threads: u8,
    /// How many synchronous connections the pool keeps.
    pub synch_threads: u8,
}

/// How many worker threads a pool may be given.
///
/// Reference: the same bound the loader applies, with the same complaint.
pub const WORKER_THREADS_MAX: u8 = 32;

/// Read one database's settings from a program's configuration.
///
/// Reference: `DatabaseLoader::AddDatabase`'s first lambda: `<name>DatabaseInfo` with a default,
/// `<name>Database.WorkerThreads` (1 to 32), and `<name>Database.SynchThreads`. An empty
/// connection string is the loader's `Database {} not specified in configuration file!`, and the
/// same answer here.
pub fn read_settings(config: &Config, kind: DatabaseKind) -> Result<DatabaseSettings, String> {
    let name = kind.name();

    let (info_text, _) = config
        .value_string(&format!("{name}DatabaseInfo"), kind.default_info())
        .map_err(|error| error.to_string())?;

    if info_text.is_empty() {
        return Err(format!(
            "Database {name} not specified in configuration file!"
        ));
    }

    let info = DatabaseInfo::parse(&info_text)?;

    let (worker_threads, _) = config
        .value::<u8>(&format!("{name}Database.WorkerThreads"), 1)
        .map_err(|error| error.to_string())?;

    if !(1..=WORKER_THREADS_MAX).contains(&worker_threads) {
        return Err(format!(
            "{name} database: invalid number of worker threads specified. Please pick a value between 1 and {WORKER_THREADS_MAX}."
        ));
    }

    let (synch_threads, _) = config
        .value::<u8>(&format!("{name}Database.SynchThreads"), 1)
        .map_err(|error| error.to_string())?;

    Ok(DatabaseSettings {
        info,
        worker_threads,
        synch_threads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config(source: &str) -> Config {
        Config::parse(source, PathBuf::from("test.conf"), Default::default())
    }

    #[test]
    fn the_default_strings_parsed_are_postgres_on_this_machine() {
        let info = DatabaseInfo::parse(DatabaseKind::Login.default_info()).unwrap();
        assert_eq!(info.host, "127.0.0.1");
        assert_eq!(info.port, "5432");
        assert_eq!(info.database, "kryta_login");
        assert_eq!(info.ssl, None);
    }

    #[test]
    fn the_field_order_is_host_then_port_then_user() {
        let info = DatabaseInfo::parse("10.0.0.5;5433;someone;secret;gw;require").unwrap();
        assert_eq!(info.host, "10.0.0.5");
        assert_eq!(info.port, "5433");
        assert_eq!(info.user, "someone");
        assert_eq!(info.password, "secret");
        assert_eq!(info.database, "gw");
        assert_eq!(info.ssl.as_deref(), Some("require"));
    }

    #[test]
    fn a_short_string_is_refused() {
        assert!(DatabaseInfo::parse("127.0.0.1;5432;kryta").is_err());
    }

    #[test]
    fn a_long_string_is_refused() {
        assert!(DatabaseInfo::parse("a;b;c;d;e;f;g").is_err());
    }

    #[test]
    fn an_unconfigured_database_takes_the_default() {
        let config = config("RealmServerPort = 3724\n");
        let settings = read_settings(&config, DatabaseKind::World).unwrap();
        assert_eq!(settings.info.database, "kryta_world");
        assert_eq!(settings.worker_threads, 1);
        assert_eq!(settings.synch_threads, 1);
    }

    #[test]
    fn a_configured_string_and_threads_are_read() {
        let config = config(
            "LoginDatabaseInfo = \"127.0.0.1;5432;me;pw;kryta_login\"\nLoginDatabase.WorkerThreads = 4\nLoginDatabase.SynchThreads = 2\n",
        );
        let settings = read_settings(&config, DatabaseKind::Login).unwrap();
        assert_eq!(settings.info.user, "me");
        assert_eq!(settings.worker_threads, 4);
        assert_eq!(settings.synch_threads, 2);
    }

    #[test]
    fn an_empty_string_says_the_database_is_not_specified() {
        let config = config("LoginDatabaseInfo = \"\"\n");
        let error = read_settings(&config, DatabaseKind::Login).unwrap_err();
        assert_eq!(error, "Database Login not specified in configuration file!");
    }

    #[test]
    fn zero_or_too_many_worker_threads_are_refused() {
        let zero = config("LoginDatabaseInfo = \"127.0.0.1;5432;kryta;kryta;kryta_login\"\nLoginDatabase.WorkerThreads = 0\n");
        assert!(read_settings(&zero, DatabaseKind::Login).is_err());

        let many = config("LoginDatabaseInfo = \"127.0.0.1;5432;kryta;kryta;kryta_login\"\nLoginDatabase.WorkerThreads = 33\n");
        assert!(read_settings(&many, DatabaseKind::Login).is_err());
    }
}
