//! The session on a connected client, and the exchange that starts it.
//!
//! Reference: `src/server/apps/authserver/Server/AuthSocket.{h,cpp}`, which is the object a
//! connection becomes and where the reference's own key exchange lives.

#[path = "handshake.rs"]
pub mod handshake;

/// One connected client, from the first byte to disconnect.
///
/// Reference: `AuthSession : Socket<AuthSession>`. The socket layer is `shared/Network/`, which
/// supplies the connection; this object is what it is handed to.
#[derive(Debug, Default)]
pub struct AuthSession {
    _private: (),
}