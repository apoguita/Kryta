//! The database layer, translated from the reference's `src/server/database/`.
//!
//! One service, several logical databases, one pool per program - the reference's split, kept.
//!
//! One deviation, and it is the engine only: the reference talks MySQL/MariaDB through its own
//! wrapper, and this talks PostgreSQL. The arrangement is the reference's - `DatabaseEnv`, a pool
//! per logical database, prepared statements, transactions, result sets, the worker pool with its
//! callback queue, and the updater with its schema version. `docs/decisions/0001-postgresql.md`
//! records why and what it changes.

#[path = "Database/mod.rs"]
pub mod database;
#[path = "Logging/mod.rs"]
pub mod logging;
#[path = "Updater/mod.rs"]
pub mod updater;
