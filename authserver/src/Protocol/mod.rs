//! The login connection's protocol: the rows, and the one entry point that frames with them.
//!
//! Reference: `src/server/apps/authserver/AuthSocket.cpp` serves this connection and its messages are
//! this program's, not the world's - which is why the rows are here and not in the game library. The
//! walk itself is `shared::packets::Frame`, the mechanism both connections use; what differs between
//! the two is the vocabulary, and each program owns its half of it.
//!
//! Source of the rows: `ASURA/crates/asura-protocol/src/packets.rs`, split out of the merged table by
//! `tools/split-protocol-table.py`. `docs/port/02-wire.md` says how a stream is split with it.

// The whole family is ported, including the parts a dispatcher will use later: the rows, the masks
// and the counters. A warning for each of those would be noise about a table that is deliberately
// complete.
#[allow(dead_code)]
#[path = "Tables.rs"]
pub mod tables;

use shared::packets::{Frame, FrameError, HEADER_LEN};

/// Frame one message of the login connection: the row is this program's own table.
///
/// The walk is the shared mechanism and the rows are `tables` beside this file, because the login
/// connection is what this program serves.
pub fn decode_login_message(bytes: &[u8], from_client: bool) -> Result<(Frame, usize), FrameError> {
    let Some(header) = bytes
        .first_chunk::<2>()
        .map(|pair| u16::from_le_bytes(*pair))
    else {
        return Err(FrameError::Incomplete { needed: HEADER_LEN });
    };
    let Some(row) = tables::lookup(header, from_client) else {
        return Err(FrameError::UnknownHeader { header });
    };
    Frame::decode_with(header, row.fields, bytes)
}

/// The table's name for a header of this connection, for a log line.
#[allow(dead_code)]
pub fn label(header: u16) -> String {
    tables::lookup(header, true)
        .map(|row| row.label())
        .unwrap_or_else(|| format!("{header:#06X}"))
}
