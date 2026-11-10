//! What a connection is, and the codes this program answers a login with.
//!
//! Reference for the shape: `src/server/apps/authserver/Authentication/` and its
//! `AuthCodes.{h,cpp}`, plus the four-byte classification every one of the reference's programmes
//! does first.
//!
//! The **numbers** in this file are not the reference's. They are this client's, measured, and each
//! one names the document that recorded it. That is the line `AGENTS.md` draws: the reference says
//! how a program is arranged, and a measured capture says what this client sends.

/// What the first four bytes of a connection say it is.
///
/// Evidence: `ASURA/docs/reference/client-protocol.md`, section 2, where the three constants and
/// their sizes are recorded from captures:
///
/// | header | constant | message | size |
/// |---|---|---|---|
/// | `0x000C0400` | `AUTH_CMSG_VERSION_HEADER` | auth: `{header, version, h0008, h000C}` | 16 |
/// | `0x000C0500` | `GAME_CMSG_VERSION_HEADER` | game: the auth shape plus map/player tokens and both UUIDs | 64 |
/// | `0x000C0FFF` | `CTRL_CMSG_VERSION_HEADER` | control: the header alone | 4 |
///
/// The first four bytes are a **bare little-endian `u32`**, not a framed message: they carry no
/// two-byte header, so decoding them as one would read `0x0400` as a message id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionKind {
    /// A login connection. Its first message is 16 bytes.
    Auth,
    /// A gameplay connection. Its first message is 64 bytes.
    Game,
    /// A control connection. Its first message is the header alone.
    Control,
}

/// The four bytes of an auth connection.
///
/// Evidence: `ASURA/docs/reference/client-protocol.md` section 2, the table above.
pub const AUTH_VERSION_HEADER: u32 = 0x000C_0400;
/// The four bytes of a gameplay connection.
pub const GAME_VERSION_HEADER: u32 = 0x000C_0500;
/// The header of the client's login message, the first thing it sends once the exchange is done.
///
/// Evidence: `ASURA/docs/reference/client-protocol.md` section 4, which records it as
/// `AUTH_CMSG_PORTAL_ACCOUNT_LOGIN` (`0x8038`, 150 bytes on the wire) carrying the portal's
/// `user_id` and `session_id`, a character name and a type.
pub const PORTAL_ACCOUNT_LOGIN: u16 = 0x8038;

/// The four bytes of a control connection.
pub const CONTROL_VERSION_HEADER: u32 = 0x000C_0FFF;

impl ConnectionKind {
    /// Read the first four bytes of a connection.
    ///
    /// A value this table does not name is refused rather than guessed, which is what the reference
    /// does with a connection byte it does not recognise.
    pub fn classify(first_four: [u8; 4]) -> Option<Self> {
        match u32::from_le_bytes(first_four) {
            AUTH_VERSION_HEADER => Some(Self::Auth),
            GAME_VERSION_HEADER => Some(Self::Game),
            CONTROL_VERSION_HEADER => Some(Self::Control),
            _ => None,
        }
    }

    /// What this kind of connection is called in a log line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Game => "game",
            Self::Control => "control",
        }
    }

    /// How many bytes this kind's first message is.
    ///
    /// Evidence: the same table. The size is what a reader must not guess: the auth message is 16
    /// bytes and the game one 64, and neither is a framed message with a length in front.
    pub fn first_message_size(self) -> usize {
        match self {
            Self::Auth => 16,
            Self::Game => 64,
            Self::Control => 4,
        }
    }
}

// The result of an authentication attempt is not declared here yet.
//
// Reference: `AuthResult` in `AuthCodes.{h,cpp}`. Its numbers are the client's, and a code with no
// caller is a guess about what the conversation will answer; it arrives with the conversation that
// sends it.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_headers_are_recognised() {
        assert_eq!(
            ConnectionKind::classify(0x000C_0400u32.to_le_bytes()),
            Some(ConnectionKind::Auth)
        );
        assert_eq!(
            ConnectionKind::classify(0x000C_0500u32.to_le_bytes()),
            Some(ConnectionKind::Game)
        );
        assert_eq!(
            ConnectionKind::classify(0x000C_0FFFu32.to_le_bytes()),
            Some(ConnectionKind::Control)
        );
    }

    #[test]
    fn the_bytes_are_little_endian() {
        // The wire bytes for the auth header, as a capture would show them.
        assert_eq!(
            ConnectionKind::classify([0x00, 0x04, 0x0C, 0x00]),
            Some(ConnectionKind::Auth)
        );
        // The same four bytes the other way round are not a message this server knows.
        assert_eq!(ConnectionKind::classify([0x04, 0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn an_unknown_header_is_refused_rather_than_guessed() {
        assert_eq!(ConnectionKind::classify([0, 0, 0, 0]), None);
        assert_eq!(ConnectionKind::classify([0xFF, 0xFF, 0xFF, 0xFF]), None);
    }

    #[test]
    fn the_first_message_sizes_are_the_measured_ones() {
        assert_eq!(ConnectionKind::Auth.first_message_size(), 16);
        assert_eq!(ConnectionKind::Game.first_message_size(), 64);
        assert_eq!(ConnectionKind::Control.first_message_size(), 4);
    }
}
