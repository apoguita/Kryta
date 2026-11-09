//! The library both programs link beside `common`, translated from the reference's
//! `src/server/shared/`.
//!
//! The reference's `Realms/` is this server's `Worlds/`, the one rename in this
//! directory: a realm in the reference is the server a player picks, and in this game
//! that thing is a world.

#[path = "Network/mod.rs"]
pub mod network;
#[path = "Packets/mod.rs"]
pub mod packets;
#[path = "Worlds/mod.rs"]
pub mod worlds;
#[path = "Secrets/mod.rs"]
pub mod secrets;
#[path = "DataStores/mod.rs"]
pub mod data_stores;
