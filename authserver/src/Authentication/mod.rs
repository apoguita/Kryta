//! The codes this program answers a login with.
//!
//! Reference: `src/server/apps/authserver/Authentication/AuthCodes.cpp`. One enum, one
//! function that turns it into the number the client reads. Not ported yet.

/// The result of an authentication attempt.
///
/// Reference: `AuthResult`. The numbers are the client's, so they are stated once and never
/// invented; they arrive with the login conversation, quoted from the reference's own file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthResult {
    /// The attempt succeeded.
    Success,
    /// The build does not match what this server serves.
    VersionMismatch,
    /// The account name or password is wrong.
    Failed,
}