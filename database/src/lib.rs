//! The database layer, translated from the reference's `src/server/database/`.
//!
//! One service, several logical databases, one pool per program - the reference's split.
//! The reference talks MySQL/MariaDB through its own wrapper; that is the vendor this
//! port keeps, because the port's subject is the reference's structure and services.

#[path = "Database/mod.rs"]
pub mod database;
#[path = "Logging/mod.rs"]
pub mod logging;
#[path = "Updater/mod.rs"]
pub mod updater;
