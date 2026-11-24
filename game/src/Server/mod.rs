//! `game/Server/` - the reference's `src/server/game/Server/` (53 files).
//!
//! The socket layer of the game library: the object a gameplay connection becomes, the session
//! behind it, the manager that owns the listener, and the protocol's tables and messages. It is not
//! a program - `worldserver` starts it - which is why one process owns one listener and still keeps
//! the network as a subsystem (`AGENTS.md`).
//!
//! Here now: `Protocol/` (the game table and the gameplay connection's version message), `Packets/`
//! (one file per message family, eight of them), `Capture` (the server-side packet capture),
//! `WorldSession` (the conversation) and `WorldSocketMgr` (the listener). The world socket object
//! itself, with its socket options and its
//! packet queue, arrives with the map that needs them.

#[path = "Capture.rs"]
pub mod capture;
#[path = "Packets/mod.rs"]
pub mod packets;
#[path = "Protocol/mod.rs"]
pub mod protocol;
#[path = "WorldSession.rs"]
pub mod world_session;
#[path = "WorldSocketMgr.rs"]
pub mod world_socket_mgr;

pub use world_session::WorldSession;
pub use world_socket_mgr::WorldSocketMgr;
