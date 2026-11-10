//! The session on a connected client, the exchange that starts it, and its arithmetic.
//!
//! Reference: `src/server/apps/authserver/Server/AuthSocket.{h,cpp}`, which is the object a
//! connection becomes and where the reference's own key exchange and conversation live.

#[path = "account_settings.rs"]
pub mod account_settings;
#[path = "file_check.rs"]
pub mod file_check;
#[path = "handshake.rs"]pub mod handshake;
#[path = "login_reply.rs"]
pub mod login_reply;
#[path = "session.rs"]
pub mod session;

pub use file_check::FileCheck;
pub use session::AuthSession;