//! The auth conversation and the manager that owns its listener.
//!
//! Reference: `src/server/apps/authserver/Server/AuthSession.{h,cpp}` and `AuthSocketMgr.h`.
//! `AuthSocketMgr` is `SocketMgr<AuthSession>` and nothing else: the socket layer is
//! `shared/Network/`, and this program supplies only the session type. Not ported yet.

/// One connected client, from the first byte to disconnect.
///
/// Reference: `AuthSession : Socket<AuthSession>`.
#[derive(Debug, Default)]
pub struct AuthSession {
    _private: (),
}