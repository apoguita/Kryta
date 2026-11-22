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
//! The sizes are the reference's own wire trace (`recordings/gwps/world/load-stream.log`, one
//! login): `0x0007` 102 bytes for a five-unit name, `0x0016` 599, `0x0014` 10, `0x0011` 81,
//! `0x0003` 10. With no characters it is 700, and the client shows an empty character list rather than
//! failing.
//!
//! Nothing here is decided by this server: the two 8-byte blobs in message 4 and the feature list are
//! the reference's literals, and the 591-byte settings block is a copied payload in
//! the account settings payload.

/// The status field of a `0x0003` that accepts the login.
pub const GM_ERROR_NONE: u32 = 0;

/// The status field of a `0x0003` that refuses it: the identity pair did not resolve.
pub const GM_ERROR_AUTH_ERROR: u32 = 11;

/// The status field of a `0x0003` that refuses a request this server cannot place - a map nobody is
/// serving.
///
/// Evidence: `docs/evidence/client-protocol.md` section 5a, which records that a travel the
/// auth service cannot place is refused with the ordinary request response and status 2 **and no
/// info message**, because the client then shows an error it can describe. An info message naming a
/// port nothing listens on is worse than either: the client leaves the character list and dies on a
/// connect timeout.
pub const GM_ERROR_NETWORK_ERROR: u32 = 2;

/// `GM_ERROR_INVALID_CHARNAME`: what the reference answers a character name it does not hold - recorded
/// by the working implementation at `ASURA/crates/asura-auth/src/lib.rs:426-429`
/// (`messages::STATUS_INVALID_CHARNAME`).
pub const GM_ERROR_INVALID_CHARNAME: u32 = 23;

/// The `host` field of `0x0009`, in bytes: room for either address family, which is why the
/// reference's struct reserves more than an IPv4 address needs.
pub const GAME_SERVER_HOST_LEN: usize = 24;

/// Winsock's `AF_INET`, which is what this client reads.
///
/// The value is written down rather than taken from the host platform: `AF_INET6` is 23 on Windows
/// and 10 on Linux, and the reader of this field is a Windows client.
pub const ADDRESS_FAMILY_INET: u16 = 2;

/// Winsock's `AF_INET6`, for the same reason.
pub const ADDRESS_FAMILY_INET6: u16 = 23;

/// One character, as far as this reply is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyCharacter {
    /// Its 16-byte identifier, in the wire's little-endian UUID form.
    pub id: [u8; 16],
    /// The name the client shows.
    pub name: String,
    /// The character's settings block, which the list carries as a counted byte string.
    ///
    /// It is `CharacterSettings`' own 62 bytes (`game/characters`), not this module's: the reference's
    /// `CharacterSettings_FromDbCharacter` builds the same record, and a character list is what reads
    /// it. The reference's own output settles the size - `05 00 "Appok" | 3e 00 <62 bytes>`, ninety
    /// of its 102 bytes - which is why this is a slice and not a fixed width.
    pub settings: Vec<u8>,
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
    frames.push(account_info(
        request,
        account,
        characters.first(),
        eula_accepted,
    ));
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

/// `AUTH_SMSG_ACCOUNT_SETTINGS` (`0x0016`): the settings blob (stored or committed), counted in bytes.
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
pub fn request_response(request: u32, status: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&status.to_le_bytes());
    frame(0x0003, body)
}

/// `AUTH_SMSG_REPORT_INSTANCE_COUNT2` (`0x001A`): the transaction and its district counts.
///
/// Layout: the request's transaction, a one-byte count, then that many `(index, count)` pairs - each
/// a language index and how many districts the outpost has for it. The shape is
/// `docs/game-rules.md` E4's (`{u32 trans, rep(11){u8 index, u8 count}}`) and E29's reading of the
/// pairs; the client asks with `0x800B` (`QueryInstanceCount`, E1) when its district list opens.
///
/// The counts are the world's state, and this program does not hold it: the answer is the one
/// district a world opens to begin with (R16), in language 0, and none for the languages it does not
/// run - so a client opening the map sees "district 1" rather than nothing.
pub fn instance_counts(trans: u32, counts: &[(u8, u8)]) -> Vec<u8> {
    let mut body = Vec::with_capacity(5 + counts.len() * 2);
    body.extend_from_slice(&trans.to_le_bytes());
    body.push(counts.len() as u8);
    for (index, count) in counts {
        body.push(*index);
        body.push(*count);
    }
    frame(0x001A, body)
}

/// `AUTH_SMSG_GAME_SERVER_INFO` (`0x0009`): where the client should play, and what it presents there.
///
/// Layout (`AuthSrv_GameServerInfo`): `req_id`, `map_token`, `map_id`, `host[24]`, `player_token` -
/// 42 bytes, fixed. The reference fills `map_token` with the world's own id and `player_token` with a
/// number drawn per travel; the message names a *server*, never a map file, because the world owns
/// that decision.
///
/// It is always sent with an `0x0003` behind it, and never alone.
pub fn game_server_info(
    request: u32,
    map_token: u32,
    map_id: u32,
    host: &[u8; GAME_SERVER_HOST_LEN],
    player_token: u32,
) -> Vec<u8> {
    let mut body = Vec::with_capacity(40);
    body.extend_from_slice(&request.to_le_bytes());
    body.extend_from_slice(&map_token.to_le_bytes());
    body.extend_from_slice(&map_id.to_le_bytes());
    body.extend_from_slice(host);
    body.extend_from_slice(&player_token.to_le_bytes());
    frame(0x0009, body)
}

/// The `host` field for an address: the family, the port **big-endian**, then the address bytes.
///
/// The asymmetry is the reference's own (`AuthSrv_SendGameServerInfo` writes the family with the
/// host's byte order and the port with the network's), and the trace shows it for IPv4:
/// `02 00 | 17 E0 | 7F 00 00 01` - family 2 little-endian, port 6112 big-endian, 127.0.0.1.
pub fn host_blob(address: std::net::SocketAddr) -> [u8; GAME_SERVER_HOST_LEN] {
    let mut host = [0u8; GAME_SERVER_HOST_LEN];
    match address.ip() {
        std::net::IpAddr::V4(ipv4) => {
            host[0..2].copy_from_slice(&ADDRESS_FAMILY_INET.to_le_bytes());
            host[4..8].copy_from_slice(&ipv4.octets());
        }
        std::net::IpAddr::V6(ipv6) => {
            host[0..2].copy_from_slice(&ADDRESS_FAMILY_INET6.to_le_bytes());
            host[4..20].copy_from_slice(&ipv6.octets());
        }
    }
    host[2..4].copy_from_slice(&address.port().to_be_bytes());
    host
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

/// `AUTH_SMSG_SESSION_INFO` (`0x0001`): the server salt, sent straight after the seed.
///
/// The reference sends this before anything else on the connection, and a client that does not get it
/// waits: a run against a server that answered only the seed sat for forty seconds and then reset the
/// connection. Size and shape from the trace: `01 00 | salt | 00 00 00 00`, ten bytes.
pub fn session_info(salt: u32, unknown: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&salt.to_le_bytes());
    body.extend_from_slice(&unknown.to_le_bytes());
    frame(0x0001, body)
}

/// `AUTH_SMSG_HEARTBEAT` (`0x0000`): the tick, which is a constant rather than a clock.
///
/// Size and shape from the trace: `00 00 | 10 00 00 00`, six bytes. The reference always sends 16.
pub fn heartbeat(tick: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(4);
    body.extend_from_slice(&tick.to_le_bytes());
    frame(0x0000, body)
}

/// What the server sends once the seed is answered, before the client asks for anything.
///
/// Two messages, in the trace's order: the session info and the heartbeat. Both are scrambled with the
/// same outbound cipher the rest of the conversation uses.
pub fn greeting(salt: u32) -> Vec<u8> {
    let mut out = session_info(salt, 0);
    out.extend_from_slice(&heartbeat(16));
    out
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
        let block = crate::server::account_settings::block();
        assert_eq!(block.len(), crate::server::account_settings::LEN);
        block
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
    fn a_character_with_a_five_unit_name_is_the_recorded_hundred_and_two_bytes() {
        // The reference's own output settles both widths: its character info for a five-unit name
        // and a 62-byte settings block is 102 bytes - header 2, request 4, id 16, zero 4, name
        // count 2 + 10, settings count 2 + 62.
        let character = ReplyCharacter {
            id: [0x22; 16],
            name: "Asura".to_string(),
            settings: vec![0u8; game::characters::CHARACTER_SETTINGS_LEN],
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
            settings: vec![0u8; game::characters::CHARACTER_SETTINGS_LEN],
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
        assert_eq!(
            long.len(),
            2 + 40,
            "a name over the field is cut to the field"
        );
    }
    /// The reference's own bytes, from the send trace of one login.
    ///
    /// `recordings/gwps/world/load-stream.log` prints each reply's header, its size and its first
    /// 24 bytes. Everything the trace shows, these builders must produce exactly - which is the only
    /// check of the packet stream that needs no client.
    mod against_the_reference_trace {
        use super::*;

        /// The request id the traced login used.
        const REQUEST: u32 = 1;

        /// The character id the trace shows: `A6 D0 EA 4E 01 6A 0B 43 30 88 13 51 59 9A EB BA`.
        const TRACED_CHARACTER: [u8; 16] = [
            0xA6, 0xD0, 0xEA, 0x4E, 0x01, 0x6A, 0x0B, 0x43, 0x30, 0x88, 0x13, 0x51, 0x59, 0x9A,
            0xEB, 0xBA,
        ];

        fn hex(bytes: &[u8]) -> String {
            bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        }

        fn settings() -> Vec<u8> {
            crate::server::account_settings::block()
        }

        #[test]
        fn the_request_response_is_the_traced_ten_bytes() {
            // 03 00 01 00 00 00 00 00 00 00
            assert_eq!(
                hex(&request_response(REQUEST, GM_ERROR_NONE)),
                "03 00 01 00 00 00 00 00 00 00"
            );
        }

        #[test]
        fn the_friend_stream_end_is_the_traced_ten_bytes() {
            // 14 00 01 00 00 00 01 00 00 00
            assert_eq!(
                hex(&friend_stream_end(REQUEST)),
                "14 00 01 00 00 00 01 00 00 00"
            );
        }

        #[test]
        fn the_account_settings_are_the_traced_599_bytes() {
            // 16 00 | 01 00 00 00 | 4F 02 | the first 16 bytes of the block
            let frame = account_settings_frame(REQUEST, &settings());
            assert_eq!(frame.len(), 599);
            assert_eq!(
                hex(&frame[..24]),
                "16 00 01 00 00 00 4F 02 08 00 66 21 00 10 38 43 7F DE 68 8E F5 93 FF FF"
            );
        }

        #[test]
        fn the_account_info_starts_with_the_traced_twenty_four_bytes() {
            // 11 00 | 01 00 00 00 | 00 00 00 00 | 04 00 00 00 | 3F 00 00 00 00 00 00 00 | 80 3F
            let frame = account_info(REQUEST, [0x11; 16], None, 0);
            assert_eq!(frame.len(), 81);
            assert_eq!(
                hex(&frame[..24]),
                "11 00 01 00 00 00 00 00 00 00 04 00 00 00 3F 00 00 00 00 00 00 00 80 3F"
            );
        }

        #[test]
        fn the_character_info_starts_with_the_traced_twenty_four_bytes() {
            // 07 00 | 01 00 00 00 | the character id | 00 00 ...
            let character = ReplyCharacter {
                id: TRACED_CHARACTER,
                name: "Asura".to_string(),
                settings: vec![0u8; game::characters::CHARACTER_SETTINGS_LEN],
            };
            let frame = character_info(REQUEST, &character);
            // Five units of name and a 62-byte settings block: the reference's own 102 bytes.
            assert_eq!(frame.len(), 102);
            assert_eq!(
                hex(&frame[..24]),
                "07 00 01 00 00 00 A6 D0 EA 4E 01 6A 0B 43 30 88 13 51 59 9A EB BA 00 00"
            );
        }
        #[test]
        fn the_greeting_after_the_seed_is_the_traced_sixteen_bytes() {
            // 01 00 | salt | 00 00 00 00 then 00 00 | 10 00 00 00
            let greeting = greeting(0xE94B_2D84);
            assert_eq!(greeting.len(), 16);
            assert_eq!(
                hex(&greeting),
                "01 00 84 2D 4B E9 00 00 00 00 00 00 10 00 00 00"
            );
        }

        #[test]
        fn the_game_server_info_is_the_traced_forty_two_bytes() {
            // 09 00 | 03 00 00 00 | 37 59 9E 7D | 92 00 00 00 | 02 00 | 17 E0 | 7F 00 00 01 ...
            //
            // The trace prints the first twenty-four bytes; the host's ten bytes here carry the
            // family, the port and the address, and the rest of the field is zero padding.
            let host = host_blob("127.0.0.1:6112".parse().unwrap());
            let frame = game_server_info(3, 0x7D9E_5937, 146, &host, 0x1122_3344);
            assert_eq!(frame.len(), 42, "the table's 42 and the trace's 42 agree");

            assert_eq!(
                hex(&frame[..24]),
                "09 00 03 00 00 00 37 59 9E 7D 92 00 00 00 02 00 17 E0 7F 00 00 01 00 00"
            );
            assert_eq!(&frame[24..38], &[0u8; 14], "the host field is zero-padded");
            assert_eq!(
                &frame[38..],
                &0x1122_3344u32.to_le_bytes(),
                "the player token is the last field"
            );
        }

        #[test]
        fn the_host_field_is_the_family_the_big_endian_port_and_the_address() {
            // The reference's asymmetry, spelled out: family little-endian, port big-endian.
            assert_eq!(
                hex(&host_blob("127.0.0.1:6112".parse().unwrap())),
                "02 00 17 E0 7F 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00"
            );
            // An address family this client reads, whatever platform wrote it: 23, not 10.
            assert_eq!(
                &host_blob("[::1]:6113".parse().unwrap())[..2],
                &[0x17, 0x00]
            );
            assert_eq!(&host_blob("[::1]:6113".parse().unwrap())[4..20], &{
                let mut v6 = [0u8; 16];
                v6[15] = 1;
                v6
            });
        }
    }
}
