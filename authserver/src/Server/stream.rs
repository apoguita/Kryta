//! Where one message ends on this wire: a two-byte header and no length anywhere.
//!
//! Reference for the shape: the reference's `Connection_ReadMessage` and the message table it reads
//! sizes from (`proto.h`), which is the only thing that can tell it where a message stops. This
//! client's wire carries no length field: a `u16` header, then the packed fields, then the next
//! message's header, which is why the table is not an optimisation here but the protocol.
//!
//! Getting it wrong looks like a protocol bug and is not one. This server once logged a single
//! message `0x8026` with nine bytes, when the client had sent `0x8026` (three bytes) and `0x8035`
//! (six) in one TCP segment: the whole read was treated as one body, the nine bytes decoded as a
//! login that was not there, and the client, owed an answer to `0x8035`, waited and then reset.
//!
//! The sizes come from the login connection's own table (`authserver/src/Protocol/Tables.rs`), walked
//! by the shared frame walk: a fixed message is its row's size, and a counted message - the computer
//! info `0x8001`, the portal login `0x8038` - is walked field by field, because its length is on the
//! wire. The tests below hold the walk to the sizes GWPS's own trace of one working login prints
//! (`recordings/gwps/world/load-stream.log`): `0x8001` 42, `0x8002` 22, `0x8000` 6, `0x8038` 54,
//! `0x8026` 3, `0x8035` 6, `0x800E` 6, `0x8029` 26 - and `0x800D` 6, the disconnect, which was learned
//! from a client that went back to the character list (`docs/evidence/client-protocol.md`).

use shared::packets::FrameError;

/// One message, taken off the front of the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message<'a> {
    /// The two-byte message id.
    pub header: u16,
    /// The whole frame, header included.
    pub frame: &'a [u8],
}

impl<'a> Message<'a> {
    /// The fields after the header.
    pub fn body(&self) -> &'a [u8] {
        &self.frame[2..]
    }
}

/// What reading the front of a buffer found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Take<'a> {
    /// One whole message, and how many bytes it used.
    Message(Message<'a>),
    /// A message that has not fully arrived. The caller reads more and asks again.
    Incomplete,
    /// A header no size is known for, so the stream cannot be split any further.
    Unknown {
        /// The header that stopped the reading.
        header: u16,
    },
}

/// Take the message at the front of `buffer`, if it is all there.
///
/// `Incomplete` is not an error: a message arrives in however many segments the network chose, and
/// the caller's answer to it is to read more and ask again.
pub fn take(buffer: &[u8]) -> Take<'_> {
    if buffer.len() < 2 {
        return Take::Incomplete;
    }
    let header = u16::from_le_bytes([buffer[0], buffer[1]]);

    // The table decides, and the walk decides with it: a login message carries counted fields exactly
    // as a gameplay one does, so a size read off a hand-kept list stops at the first message this tree
    // did not think of - which is what a character click sends (`0x800A`) and what the settings
    // uploads send (`0x8009`, `0x8020`, `0x8021`). See `docs/port/02-wire.md`.
    match crate::protocol::decode_login_message(buffer, true) {
        Ok((_, used)) => Take::Message(Message {
            header,
            frame: &buffer[..used],
        }),
        Err(FrameError::Incomplete { .. }) => Take::Incomplete,
        Err(_) => Take::Unknown { header },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four messages of the reference's own post-login half, as its trace prints them.
    ///
    /// `recordings/gwps/world/load-stream.log`, 2026-10-26: the client's EULA, its ask-response,
    /// its player status, and its request for a map.
    const TRACED_ACCEPT_EULA: &str = "26 80 1A";
    const TRACED_ASK_SERVER: &str = "35 80 02 00 00 00";
    const TRACED_PLAYER_STATUS: &str = "0E 80 01 00 00 00";
    /// The trace prints the first twenty-four bytes of a message, and this one is twenty-six: the
    /// six dwords are all there in the header's own record of the size, so the last two bytes of the
    /// language field are its own.
    const TRACED_REQUEST_INSTANCE: &str =
        "29 80 03 00 00 00 01 00 00 00 92 00 00 00 00 00 00 00 02 00 00 00 00 00 00 00";

    fn bytes(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
    }

    #[test]
    fn the_traced_messages_are_the_sizes_the_table_holds() {
        for (text, size) in [
            (TRACED_ACCEPT_EULA, 3),
            (TRACED_ASK_SERVER, 6),
            (TRACED_PLAYER_STATUS, 6),
            (TRACED_REQUEST_INSTANCE, 26),
            ("0D 80 00 00 00 00", 6),
        ] {
            let message = bytes(text);
            assert_eq!(message.len(), size);
            assert!(
                matches!(take(&message), Take::Message(Message { frame, .. }) if frame.len() == size),
                "{text}"
            );
        }
    }

    #[test]
    fn two_messages_in_one_segment_are_two_messages() {
        // The failure this module exists for: the client sent the EULA and its ask-response in one
        // nine-byte write, and the whole nine bytes were once read as one message.
        let mut stream = bytes(TRACED_ACCEPT_EULA);
        stream.extend_from_slice(&bytes(TRACED_ASK_SERVER));
        assert_eq!(stream.len(), 9);

        let Take::Message(first) = take(&stream) else {
            panic!("the first message is all there");
        };
        assert_eq!(first.header, 0x8026);
        assert_eq!(first.frame.len(), 3);
        assert_eq!(first.body(), &[0x1A]);

        let Take::Message(second) = take(&stream[3..]) else {
            panic!("the second message is all there");
        };
        assert_eq!(second.header, 0x8035);
        assert_eq!(second.frame.len(), 6);
        assert_eq!(second.body(), &[0x02, 0x00, 0x00, 0x00], "request 2");
    }

    #[test]
    fn the_computer_info_is_walked_because_its_strings_are_counted() {
        // The reference's own capture, as the protocol document prints it: "Apo" (3 units) and
        // "DESKTOP-DURJ7N0" (15), then the 22-byte hash message behind them.
        let mut stream = bytes(
            "01 80 03 00 41 00 70 00 6F 00 0F 00 44 00 45 00 53 00 4B 00 54 00 4F 00 50 00 2D 00 \
             44 00 55 00 52 00 4A 00 37 00 4E 00 30 00",
        );
        assert_eq!(stream.len(), 42);
        stream.extend_from_slice(&bytes(
            "02 80 C8 98 00 00 F5 E6 06 66 71 A9 87 45 B8 FC 3A A6 B3 C8 55 25",
        ));
        assert_eq!(stream.len(), 64, "what one read of the preamble holds");

        let Take::Message(info) = take(&stream) else {
            panic!("the computer info is all there");
        };
        assert_eq!(info.header, 0x8001);
        assert_eq!(info.frame.len(), 42);

        let Take::Message(hash) = take(&stream[42..]) else {
            panic!("the hash follows it");
        };
        assert_eq!(hash.header, 0x8002);
        assert_eq!(hash.frame.len(), 22);
        assert_eq!(&hash.body()[..4], &[0xC8, 0x98, 0x00, 0x00], "build 39112");
    }

    #[test]
    fn the_portal_login_is_walked_too() {
        // The traced login: request 1, two identifiers, no character name, the type "Portal".
        let mut message = vec![0x38, 0x80, 0x01, 0x00, 0x00, 0x00];
        message.extend_from_slice(&[0x11u8; 16]);
        message.extend_from_slice(&[0x22u8; 16]);
        message.extend_from_slice(&0u16.to_le_bytes()); // no character name
        message.extend_from_slice(&6u16.to_le_bytes()); // "Portal", six units
        for unit in "Portal".encode_utf16() {
            message.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(message.len(), 54, "the trace's own size");

        let Take::Message(login) = take(&message) else {
            panic!("the login is all there");
        };
        assert_eq!(login.header, 0x8038);
        assert_eq!(login.frame.len(), 54);
    }

    #[test]
    fn a_message_that_has_not_arrived_asks_for_more_rather_than_guessing() {
        let request = bytes(TRACED_REQUEST_INSTANCE);
        for cut in [0usize, 1, 2, 13, 25] {
            assert_eq!(take(&request[..cut]), Take::Incomplete, "cut at {cut}");
        }

        // And a counted message is incomplete in the middle of its strings. "Apo", then "AB".
        let mut info = vec![0x01, 0x80];
        info.extend_from_slice(&3u16.to_le_bytes());
        info.extend_from_slice(&[0x41, 0x00, 0x70]);
        assert_eq!(take(&info), Take::Incomplete, "half a first string");
        info.extend_from_slice(&[0x00, 0x6F, 0x00]);
        assert_eq!(take(&info), Take::Incomplete, "no second count yet");
        info.extend_from_slice(&2u16.to_le_bytes());
        assert_eq!(take(&info), Take::Incomplete, "no second string yet");
        info.extend_from_slice(&[0x41, 0x00, 0x42]);
        assert_eq!(take(&info), Take::Incomplete, "half of the second string");
        info.extend_from_slice(&[0x00]);
        assert!(matches!(take(&info), Take::Message(_)), "the whole of it");
    }

    #[test]
    fn a_header_with_no_known_size_stops_the_reading() {
        // This is a refusal rather than a guess, and it says which header stopped it: a length
        // invented for an unknown message would desynchronise every message behind it.
        assert_eq!(take(&[0x99, 0x80]), Take::Unknown { header: 0x8099 });
        assert_eq!(take(&[0x00, 0x00, 0x00]), Take::Unknown { header: 0x0000 });
    }
}
