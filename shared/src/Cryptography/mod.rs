//! `shared/Cryptography/`: the key exchange and the cipher both conversations run on.
//!
//! Reference: `src/server/shared/Cryptography/`, whose `AuthCrypt` is the object a socket holds once
//! the seed exchange is done - one key, one RC4 per direction. The primitives stay in `common`
//! (`Arc4` is `common::cryptography`), the same split the reference has: `common` holds the cipher,
//! `shared` holds the thing a connection runs on it.

#[path = "AuthCrypt.rs"]
pub mod auth_crypt;
#[path = "Handshake.rs"]
pub mod handshake;

pub use auth_crypt::AuthCrypt;
