//! The answer to a login: the five messages a client needs before it shows a character.
//!
//! Reference: the reference's `AuthSrv_HandlePortalAccountLogin`, whose reply this reproduces in order
//! and at the sizes the records give. The **layouts and literals are measured facts**, each marked
//! where it is used; the bytes a client compares come from it, so a field one byte out is a client
//! error rather than a log line.
//!
//! | # | message | size, with no characters |
//! |---|---|---|
//! | 1 | `0x0007` character info, once per character | 102 each, none here |
//! | 2 | `0x0016` account settings | 599 |
//! | 3 | `0x0014` friend stream end | 10 |
//! | 4 | `0x0011` account info | 81 |
//! | 5 | `0x0003` request response | 10 |
//!
//! The sizes are the reference's own wire trace (`ASURA/recordings/gwps/world/load-stream.log`, one
//! login): `0x0007` 102 bytes for a four-character name, `0x0016` 599, `0x0014` 10, `0x0011` 81,
//! `0x0003` 10. With no characters it is 700, and the client shows an empty character list rather than
//! failing.
//!
//! Nothing here is decided by this server: the two 8-byte blobs in message 4 and the feature list are
//! the reference's literals, and the 591-byte settings block is a copied payload in
//! `data/account_settings.conf`.

/// The status field of a `0x0003` that accepts the login.
pub const GM_ERROR_NONE: u32 = 0;

/// The status field of a `0x0003` that refuses it: the identity pair did not resolve.
pub const GM_ERROR_AUTH_ERROR: u32 = 11;

/// One character, as far as this reply is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyCharacter {
    /// Its 16-byte identifier, in the wire's little-endian UUID form.
    pub id: [u8; 16],
    /// The name the client shows.
    pub name: String,
    /// The 64-byte settings block that belongs to this character.
    pub settings: [u8; 64],
}

/// The five messages, each a whole wire frame: a `u16` header and then its body.
pub fn reply_frames(
    request: u32,
    account: [u8; 16],
    characters: &[ReplyCharacter],
    eula_accepted: u8,
    account_settings: &[u8],
) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();

    for character in characters {
        frames.push(character_info(request, character));
    }
    frames.push(account_settings_frame(request, account_settings));
    frames.push(friend_stream_end(request));
    frames.push(account_info(request, account, characters.first(), eula_accepted));
    frames.push(request_response(request, GM_ERROR_NONE));

    frames
}

/// A `0x0003` and nothing else, which is all a refusal sends.
pub fn refusal(request: u32, status: u32) -> Vec<u8> {
    request_response(request, status)
}

/// `AUTH_SMSG_CHARACTER_INFO` (`0x0007`): one character in the list.
///
/// Layout: `req_id`, the character's 16-byte id, a `u32` the reference leaves zero, a counted UTF-16
/// name in a 20-slot field, then the settings block as a counted byte string.
fn character_info(request: u32, character: &ReplyCharacter) -> Vec<u8> {
    let mut body = Vec::with_capacity(100);
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&character.id);
    body.extend_from_slice(&0u32.to_le_bytes());

    write_counted_utf16(&mut body, &character.name, 20);

    body.extend_from_slice(&(character.settings.len() as u16).to_le_bytes());
    body.extend_from_slice(&character.settings);

    frame(0x0007, body)
}

/// `AUTH_SMSG_ACCOUNT_SETTINGS` (`0x0016`): the 591-byte block, counted in bytes.
fn account_settings_frame(request: u32, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(6 + data.len());
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&(data.len() as u16).to_le_bytes());
    body.extend_from_slice(data);
    frame(0x0016, body)
}

/// `AUTH_SMSG_FRIEND_STREAM_END` (`0x0014`): the request id twice, and nothing else.
fn friend_stream_end(request: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&request.to_le_bytes());
    frame(0x0014, body)
}

/// `AUTH_SMSG_ACCOUNT_INFO` (`0x0011`): the account, the selected character, and the feature list.
///
/// Every literal is the reference's, and the last `u32` is on the wire - reading it as the next
/// message's header is what desynchronised a real client once.
fn account_info(
    request: u32,
    account: [u8; 16],
    selected: Option<&ReplyCharacter>,
    eula_accepted: u8,
) -> Vec<u8> {
    /// The reference's `h000E`.
    const H000E: [u8; 8] = [0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    /// The reference's `h0016`: which campaigns the account owns.
    const H0016: [u8; 8] = [0x80, 0x3F, 0x42, 0x00, 0x83, 0x00, 0x0C, 0x02];
    /// The reference's feature list; its third `u16` is the account's character slots.
    const FEATURES: [u8; 8] = [0x01, 0x00, 0x04, 0x00, 0x57, 0x00, 0x01, 0x00];

    let mut body = Vec::with_capacity(79);
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes()); // h0006
    body.extend_from_slice(&4u32.to_le_bytes()); // h000A
    body.extend_from_slice(&H000E);
    body.extend_from_slice(&H0016);
    body.extend_from_slice(&account);
    body.extend_from_slice(&selected.map(|character| character.id).unwrap_or([0u8; 16]));
    body.extend_from_slice(&8u32.to_le_bytes()); // h003E
    body.extend_from_slice(&(FEATURES.len() as u16).to_le_bytes());
    body.extend_from_slice(&FEATURES);
    body.push(eula_accepted);
    body.extend_from_slice(&3u32.to_le_bytes()); // unk5

    frame(0x0011, body)
}

/// `AUTH_SMSG_REQUEST_RESPONSE` (`0x0003`): the request id and how it went.
fn request_response(request: u32, status: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&status.to_le_bytes());
    frame(0x0003, body)
}

/// A frame: the `u16` header, then the body. No length field - that is the whole protocol's rule.
fn frame(header: u16, body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// A counted UTF-16 string in the **server's** form: a `u16` count and the units, with no padding out
/// to the field's capacity.
fn write_counted_utf16(out: &mut Vec<u8>, text: &str, capacity: usize) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let units = &units[..units.len().min(capacity)];
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> [u8; 16] {
        let mut id = [0u8; 16];
        id[0] = 0x11;
        id
    }

    fn settings() -> Vec<u8> {
        crate::server::account_settings::load("../data/account_settings.conf")
            .expect("the shipped settings block")
    }

    #[test]
    fn no_characters_answers_five_messages_of_the_recorded_sizes() {
        let frames = reply_frames(7, account(), &[], 0, &settings());

        // Nothing per character, then settings, friend stream end, account info, response.
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].len(), 599, "account settings, as the trace shows");
        assert_eq!(frames[1].len(), 10, "friend stream end");
        assert_eq!(frames[2].len(), 81, "account info");
        assert_eq!(frames[3].len(), 10, "request response");

        let total: usize = frames.iter().map(Vec::len).sum();
        assert_eq!(total, 700, "599 + 10 + 81 + 10");
    }

    #[test]
    fn the_headers_are_the_recorded_ones_in_order() {
        let frames = reply_frames(7, account(), &[], 0, &settings());
        let headers: Vec<u16> = frames
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert_eq!(headers, vec![0x0016, 0x0014, 0x0011, 0x0003]);
    }

    #[test]
    fn a_character_with_a_four_unit_name_is_the_recorded_hundred_and_two_bytes() {
        // The trace's character info is 102 bytes, and the name in it is four units long:
        // header 2 + request 4 + id 16 + zero 4 + count and units (2 + 8) + settings count 2 + 64.
        let character = ReplyCharacter {
            id: [0x22; 16],
            name: "Asur".to_string(),
            settings: [0u8; 64],
        };
        let frames = reply_frames(7, account(), &[character], 0, &settings());
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0][..2], [0x07, 0x00], "character info first");
        assert_eq!(frames[0].len(), 102, "the trace's own size");

        let total: usize = frames.iter().map(Vec::len).sum();
        assert_eq!(total, 802, "102 + 599 + 10 + 81 + 10");
    }

    #[test]
    fn the_request_id_is_echoed_in_every_message() {
        let character = ReplyCharacter {
            id: [0x22; 16],
            name: "A".to_string(),
            settings: [0u8; 64],
        };
        for frame in reply_frames(0xDEAD_BEEF, account(), &[character], 0, &settings()) {
            let request = u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]);
            assert_eq!(request, 0xDEAD_BEEF);
        }
    }

    #[test]
    fn a_refusal_is_one_message() {
        let frame = refusal(9, GM_ERROR_AUTH_ERROR);
        assert_eq!(frame.len(), 10);
        assert_eq!(&frame[..2], &[0x03, 0x00]);
        let status = u32::from_le_bytes([frame[6], frame[7], frame[8], frame[9]]);
        assert_eq!(status, GM_ERROR_AUTH_ERROR);
    }

    #[test]
    fn a_name_is_counted_and_not_padded_to_its_field() {
        let mut body = Vec::new();
        write_counted_utf16(&mut body, "Ab", 20);
        assert_eq!(body.len(), 2 + 4, "a count and two units, not twenty slots");

        let mut long = Vec::new();
        write_counted_utf16(&mut long, &"x".repeat(40), 20);
        assert_eq!(long.len(), 2 + 40, "a name over the field is cut to the field");
    }
}
