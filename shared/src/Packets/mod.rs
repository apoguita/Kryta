//! The wire: the byte buffer, and - as they land - the opcode table and one module per message
//! family.
//!
//! Reference: `src/server/shared/Packets/` holds the reference's `ByteBuffer`, and the rest of
//! its wire lives in the game library: `game/Server/Protocol/` for the opcode table and
//! `game/Server/Packets/` for one file pair per family. This module is the shared half.

#[path = "ByteBuffer.rs"]
pub mod byte_buffer;

pub use byte_buffer::{ByteBuffer, ByteBufferError};