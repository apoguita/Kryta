//! The cryptography this client and this server share.
//!
//! Reference: `src/common/Cryptography/` in the reference's tree, which holds its hash and key
//! exchange primitives for its own protocol. This module holds this game's, and every algorithm in
//! it is either public (RC4 is textbook) or stated with the evidence that fixed it.

#[path = "Arc4.rs"]
pub mod arc4;

pub use arc4::Arc4;