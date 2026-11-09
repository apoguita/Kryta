//! The session on a connected client, the exchange that starts it, and its arithmetic.
//!
//! Reference: `src/server/apps/authserver/Server/AuthSocket.{h,cpp}`, which is the object a
//! connection becomes and where the reference's own key exchange and conversation live.

#[path = "handshake.rs"]
pub mod handshake;
#[path = "session.rs"]
pub mod session;

pub use session::AuthSession;