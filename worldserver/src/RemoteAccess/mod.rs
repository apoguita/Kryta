//! The SOAP endpoint, when `SOAP.Enabled` is set.
//!
//! Reference: `src/server/apps/worldserver/RemoteAccess/RASession.cpp` and `ACSoap/`.
//! Not ported yet.

/// The SOAP session.
///
/// Reference: `RASession`.
#[derive(Debug, Default)]
pub struct RaSession {
    _private: (),
}