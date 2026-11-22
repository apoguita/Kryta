//! The party message family: the party a character is in, and who is in it.
//!
//! Reference: `src/server/game/Server/Packets/GroupPackets.{h,cpp}` - one file pair per message family,
//! and this tree's noun for `Groups` is `Parties`. Nothing of its *contents* crosses: these headers are
//! this client's.
//!
//! The family and its order are measured twice over, and both agree:
//!
//! * the reference's own capture of a visit to another district
//!   (`recordings/client/feature/travel/doors-20261031-095754.jsonl`, seq 319-322, and the same four
//!   again at 756-759, 2832-2835 and every other entry): `0x01D3` with the party id, `0x01CC` with the
//!   party, the player and `1`, `0x01D4` with the party id, then `0x01B3` with the party id and `1`.
//!   A second entry carries party 3 where the first carries party 1, so the dwords are the party, the
//!   player and a loaded byte in that order;
//! * the working implementation's builders, which send the same four before the load's last message
//!   (`ASURA/crates/asura-world/src/player.rs:382-388`, `ASURA/crates/asura-protocol/src/game/mod.rs`,
//!   `party_create`, `party_player_add`, `party_member_stream_end`).
//!
//! The order is not decoration: the client's party object is what `0x01B3` talks about, and a client
//! told about a party it was never given asserts (`Assertion: m_partyClient`,
//! `P:\Code\Gw\Party\Cli\PyCliParty.cpp(650)`, measured 2026-11-12 on the load of map 148).

/// `GAME_SMSG_PARTY_CREATE` (`0x01D3`): 4 bytes - the header and the party's id.
pub const PARTY_CREATE: u16 = 0x01D3;
/// `GAME_SMSG_PARTY_PLAYER_ADD` (`0x01CC`): 7 bytes - the party, the player, and whether it is loaded.
pub const PARTY_PLAYER_ADD: u16 = 0x01CC;
/// `GAME_SMSG_PARTY_MEMBER_STREAM_END` (`0x01D4`): 4 bytes - the header and the party's id.
pub const PARTY_MEMBER_STREAM_END: u16 = 0x01D4;
/// `GAME_SMSG_PLAYER_UPDATE_PARTY_SIZE` (`0x00B0`): a player's party size.
pub const PLAYER_UPDATE_PARTY_SIZE: u16 = 0x00B0;
/// `GAME_SMSG_PLAYER_UPDATE_PARTY` (`0x00B1`): the leader of a player's party.
pub const PLAYER_UPDATE_PARTY: u16 = 0x00B1;

/// The id a solo connection's party has.
///
/// Measured: every entry of the reference's capture but one names party `1`; the other names `3` for a
/// party of two, so the number is the party's own and `1` is what a fresh one gets.
/// `ASURA/crates/asura-world/src/player.rs:132` (`PARTY_ID`).
pub const PARTY_ID: u16 = 1;

/// A frame: the `u16` header and then its fields.
fn frame(header: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// `0x01D3`: the party exists.
pub fn party_create(party_id: u16) -> Vec<u8> {
    frame(PARTY_CREATE, &party_id.to_le_bytes())
}

/// `0x01CC`: one member of the party - which party, which player, and whether it is loaded into it.
pub fn party_player_add(party_id: u16, player_id: u16, is_loaded: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    body.extend_from_slice(&party_id.to_le_bytes());
    body.extend_from_slice(&player_id.to_le_bytes());
    body.push(is_loaded);
    frame(PARTY_PLAYER_ADD, &body)
}

/// `GAME_SMSG_PLAYER_UPDATE_PARTY_SIZE` (`0x00B0`): **five bytes** - a two-byte header, a `u16` player id
/// and a `u8` size (the client's table, `MsgHeader`, `Word`, `Byte`; Headquarter's `GAME_SERVER_0176`).
///
/// Ported from `ASURA/crates/asura-protocol/src/game/mod.rs:2146-2161` (ASURA `071735e`). This builder
/// wrote twelve, from a capture that reads `b0 00 00 00 | 01 00 00 00 | 01 00 00 00`; that capture is the
/// *struct* layer (`docs/port/02-wire.md`, "Three kinds of capture"), which widens the header to four
/// bytes and every field to four. Sent as twelve, a live client (build 39167) decoded it as player 0,
/// size 1, framed the remaining seven bytes as a message `0x0000`, and dropped with error 7 (ASURA
/// `recordings/client/feature/multiplay/asdasd.jsonl` seq 258-260).
pub fn player_update_party_size(player: u32, party_size: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    body.extend_from_slice(&(player as u16).to_le_bytes());
    body.push(party_size as u8);
    frame(PLAYER_UPDATE_PARTY_SIZE, &body)
}

/// `GAME_SMSG_PLAYER_UPDATE_PARTY` (`0x00B1`): which player leads the party this player is in. **Six
/// bytes**: a two-byte header, a `u16` player id and a `u16` leader player id (the client's table and
/// Headquarter's `GAME_SERVER_0177`). Ported from `ASURA/crates/asura-protocol/src/game/mod.rs:2163-2171`
/// (ASURA `071735e`); see [`player_update_party_size`] for why it is not twelve.
pub fn player_update_party(player: u32, party_leader: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(4);
    body.extend_from_slice(&(player as u16).to_le_bytes());
    body.extend_from_slice(&(party_leader as u16).to_le_bytes());
    frame(PLAYER_UPDATE_PARTY, &body)
}

/// `0x01D4`: the party's members have all been sent.
pub fn party_member_stream_end(party_id: u16) -> Vec<u8> {
    frame(PARTY_MEMBER_STREAM_END, &party_id.to_le_bytes())
}

// ---- party invitations, answers and departures -------------------------------------------------
//
// Ported from `ASURA/crates/asura-protocol/src/game/mod.rs:1827-1965` and `2105-2148` (ASURA `071735e`):
// six one-word messages, each naming **the other party** from the point of view of the client that
// receives it, the remove, the leader word, the four member-state messages `0x01CD`-`0x01D0`, and the
// party state a load and a change of party end with. Layouts are the client's table (`MsgHeader`, `Word`
// or `Byte`); names and meanings are measured in ASURA `docs/44-party-invites.md`.

/// `GAME_SMSG_PARTY_INVITE_ADD` (`0x01C5`): this client's party has invited the named party.
pub const PARTY_INVITE_ADD: u16 = 0x01C5;
/// `GAME_SMSG_PARTY_JOIN_REQUEST` (`0x01C6`): the named party invites this client's party.
pub const PARTY_JOIN_REQUEST: u16 = 0x01C6;
/// `GAME_SMSG_PARTY_INVITE_CANCEL` (`0x01C7`): this client's invitation of the named party is withdrawn.
pub const PARTY_INVITE_CANCEL: u16 = 0x01C7;
/// `GAME_SMSG_PARTY_REQUEST_CANCEL` (`0x01C8`): the named party withdrew its invitation of this client.
pub const PARTY_REQUEST_CANCEL: u16 = 0x01C8;
/// `GAME_SMSG_PARTY_REQUEST_RESPONSE` (`0x01C9`): this client answered the named party's invitation.
pub const PARTY_REQUEST_RESPONSE: u16 = 0x01C9;
/// `GAME_SMSG_PARTY_INVITE_RESPONSE` (`0x01CA`): the named party answered this client's invitation.
pub const PARTY_INVITE_RESPONSE: u16 = 0x01CA;
/// `GAME_SMSG_PARTY_YOU_ARE_LEADER` (`0x01CB`): this client now leads its party.
pub const PARTY_YOU_ARE_LEADER: u16 = 0x01CB;
/// `0x01CD`: a member is connected again (ledger E38 `MemberConnectSoft`). **Not captured.**
pub const PARTY_MEMBER_CONNECT_SOFT: u16 = 0x01CD;
/// `0x01CE`: a member lost its connection and is held; the party window draws it grey (ledger E38
/// `MemberDisconnectSoft`). **Not captured.**
pub const PARTY_MEMBER_DISCONNECT_SOFT: u16 = 0x01CE;
/// `0x01CF`: a member is in the map now (ledger E38 `MemberEnterWorld`). **Not captured.**
pub const PARTY_MEMBER_ENTER_WORLD: u16 = 0x01CF;
/// `0x01D0`, unnamed in the table: a member left the map (ledger E38 `MemberLeaveWorld`), measured first
/// in the batch for an invited player who walked out (`titles-live-20261106-034507.jsonl` seq 2645-2650).
pub const PARTY_MEMBER_LEAVE_WORLD: u16 = 0x01D0;
/// `GAME_SMSG_PARTY_PLAYER_REMOVE` (`0x01D1`): a player is no longer in the party.
pub const PARTY_PLAYER_REMOVE: u16 = 0x01D1;
/// `0x01BE`, unnamed in the table: one dword, always `{0}` (ledger E43's `GlobalStateFlags`).
pub const PARTY_GLOBAL_STATE_FLAGS: u16 = 0x01BE;
/// `GAME_SMSG_PARTY_SET_DIFFICULTY` (`0x01BF`): one byte, always `{0}` - normal mode, by the table's name.
pub const PARTY_SET_DIFFICULTY: u16 = 0x01BF;
/// `GAME_SMSG_PARTY_SEARCH_SEEK` (`0x01E0`): one word, always `{0}` after a client's own party changes.
pub const PARTY_SEARCH_SEEK: u16 = 0x01E0;

/// `GAME_CMSG_PARTY_ACCEPT_INVITE` (`0x809C`, one word: the inviting party).
pub const CLIENT_PARTY_ACCEPT_INVITE: u16 = 0x809C;
/// `GAME_CMSG_PARTY_ACCEPT_CANCEL` (`0x809D`, one word: the invited party). The inviter withdraws.
pub const CLIENT_PARTY_ACCEPT_CANCEL: u16 = 0x809D;
/// `GAME_CMSG_PARTY_ACCEPT_REFUSE` (`0x809E`, one word: the inviting party). The invited declines.
pub const CLIENT_PARTY_ACCEPT_REFUSE: u16 = 0x809E;
/// `GAME_CMSG_PARTY_INVITE_PLAYER` (`0x80A0`, one word: the invited player's id).
pub const CLIENT_PARTY_INVITE_PLAYER: u16 = 0x80A0;
/// `GAME_CMSG_PARTY_RETURN_TO_OUTPOST` (`0x80A7`): the leader's return after a defeat.
pub const CLIENT_PARTY_RETURN_TO_OUTPOST: u16 = 0x80A7;
/// `GAME_CMSG_PARTY_CANCEL_ENTER_CHALLENGE` (`0x80A3`): the leader cancels a countdown.
pub const CLIENT_PARTY_CANCEL_ENTER_CHALLENGE: u16 = 0x80A3;
/// `GAME_CMSG_PARTY_LEAVE_GROUP` (`0x80A2`, no fields).
pub const CLIENT_PARTY_LEAVE_GROUP: u16 = 0x80A2;
/// `GAME_CMSG_PARTY_KICK_PLAYER` (`0x80A9`, one word: the player).
pub const CLIENT_PARTY_KICK_PLAYER: u16 = 0x80A9;

fn word(header: u16, value: u16) -> Vec<u8> {
    frame(header, &value.to_le_bytes())
}

fn two_words(header: u16, first: u16, second: u16) -> Vec<u8> {
    let mut body = Vec::with_capacity(4);
    body.extend_from_slice(&first.to_le_bytes());
    body.extend_from_slice(&second.to_le_bytes());
    frame(header, &body)
}

/// `0x01C5`: sent to the inviter after the invited party's member stream
/// (`titles-live-20261106-032434.jsonl` seq 380-384).
pub fn party_invite_add(party_id: u16) -> Vec<u8> {
    word(PARTY_INVITE_ADD, party_id)
}

/// `0x01C6`: sent to the invited after the inviting party's member stream
/// (`titles-live-20261106-025940.jsonl` seq 2539-2542).
pub fn party_join_request(party_id: u16) -> Vec<u8> {
    word(PARTY_JOIN_REQUEST, party_id)
}

/// `0x01C7`: the inviter's answer to its own cancel (`titles-live-20261106-032434.jsonl` seq 1295-1300).
pub fn party_invite_cancel(party_id: u16) -> Vec<u8> {
    word(PARTY_INVITE_CANCEL, party_id)
}

/// `0x01C8`: the invited side of a cancel. **Not captured**; inferred from the inviter's `0x01C7`.
pub fn party_request_cancel(party_id: u16) -> Vec<u8> {
    word(PARTY_REQUEST_CANCEL, party_id)
}

/// `0x01C9`: the first message of the invited side's accept (`titles-live-20261106-025940.jsonl` seq
/// 2622), and the whole of its decline (`titles-live-20261106-034507.jsonl` seq 2620-2626).
pub fn party_request_response(party_id: u16) -> Vec<u8> {
    word(PARTY_REQUEST_RESPONSE, party_id)
}

/// `0x01CA`: alone for a decline (`titles-live-20261106-032434.jsonl` seq 2058), first of the accept's
/// batch (`multiplay/party.jsonl` seq 160).
pub fn party_invite_response(party_id: u16) -> Vec<u8> {
    word(PARTY_INVITE_RESPONSE, party_id)
}

/// `0x01D1`: the first message of a leave (`titles-live-20261106-025940.jsonl` seq 2437).
pub fn party_player_remove(party_id: u16, player_id: u16) -> Vec<u8> {
    two_words(PARTY_PLAYER_REMOVE, party_id, player_id)
}

/// `0x01CB {1}` to the member that takes over when the leader goes, after the size and leader pairs
/// (`titles-live-20261106-025940.jsonl` seq 627-631).
pub fn party_you_are_leader(leads: u8) -> Vec<u8> {
    frame(PARTY_YOU_ARE_LEADER, &[leads])
}

/// `0x01D0`: a member left the map.
pub fn party_member_leave_world(party_id: u16, player_id: u16) -> Vec<u8> {
    two_words(PARTY_MEMBER_LEAVE_WORLD, party_id, player_id)
}

/// `0x01CD`: a member is connected again. **Not captured.**
pub fn party_member_connect_soft(party_id: u16, player_id: u16) -> Vec<u8> {
    two_words(PARTY_MEMBER_CONNECT_SOFT, party_id, player_id)
}

/// `0x01CE`: a member lost its connection and is held. **Not captured.**
pub fn party_member_disconnect_soft(party_id: u16, player_id: u16) -> Vec<u8> {
    two_words(PARTY_MEMBER_DISCONNECT_SOFT, party_id, player_id)
}

/// `0x01CF`: a member is in the map now. **Not captured.**
pub fn party_member_enter_world(party_id: u16, player_id: u16) -> Vec<u8> {
    two_words(PARTY_MEMBER_ENTER_WORLD, party_id, player_id)
}

/// `0x01E0 {0}` after `CHARACTER_UPDATE_PARTY` whenever a client's own party changes
/// (`titles-live-20261106-025940.jsonl` seq 2457, 2637).
pub fn party_search_seek(value: u16) -> Vec<u8> {
    word(PARTY_SEARCH_SEEK, value)
}

/// `0x01BF {0}`: follows `0x01E0` in the same two batches.
pub fn party_set_difficulty(mode: u8) -> Vec<u8> {
    frame(PARTY_SET_DIFFICULTY, &[mode])
}

/// `0x01D9`: the party has no member left standing.
pub const PARTY_DEFEATED: u16 = 0x01D9;
/// `0x01DA`: the party travel lock; kind 2, initiator 1, empty text.
pub const PARTY_LOCK: u16 = 0x01DA;
/// `0x01E7`: the client's travel confirmation question (4 = would leave party, 5 = members left).
pub const PARTY_TRAVEL_CONFIRMATION: u16 = 0x01E7;
/// `0x01BC`: the party travel countdown window.
pub const INSTANCE_TRAVEL_TIMER: u16 = 0x01BC;

/// `0x01D9`, bare.
pub fn party_defeated() -> Vec<u8> {
    frame(PARTY_DEFEATED, &[])
}

/// `0x01DA {kind, initiator, ""}`: six bytes.
pub fn party_lock(kind: u8, initiator: u8) -> Vec<u8> {
    frame(PARTY_LOCK, &[kind, initiator, 0, 0])
}

/// `0x01E7 {reason}`.
pub fn party_travel_confirmation(reason: u8) -> Vec<u8> {
    frame(PARTY_TRAVEL_CONFIRMATION, &[reason])
}

/// The client's encoded form of a string-table id, as ASURA `encoded_string_id` writes it.
pub fn encoded_string_id(mut id: u32) -> Vec<u16> {
    const BASE: u32 = 0x100;
    const RANGE: u32 = 0x7F00;
    let mut units = Vec::new();
    loop {
        units.push((BASE + id % RANGE) as u16);
        id /= RANGE;
        if id == 0 {
            break;
        }
    }
    units.reverse();
    units
}

/// `0x01BC {seconds, initiator, map name}`: the countdown window. The map name is the client's
/// encoded string id, from the map's `basic.json` `name_id`.
pub fn instance_travel_timer(seconds: u16, initiator: u8, name: &[u16]) -> Vec<u8> {
    let mut body = Vec::with_capacity(3 + 2 + name.len() * 2);
    body.extend_from_slice(&seconds.to_le_bytes());
    body.push(initiator);
    body.extend_from_slice(&(name.len() as u16).to_le_bytes());
    for unit in name {
        body.extend_from_slice(&unit.to_le_bytes());
    }
    frame(INSTANCE_TRAVEL_TIMER, &body)
}

/// `0x01BE {0}`: every live load sends it right after `0x01BF` (`titles-live-20261106-034507.jsonl`
/// seq 850, `multiplay/party.jsonl` seq 2671).
pub fn party_global_state_flags(flags: u32) -> Vec<u8> {
    frame(PARTY_GLOBAL_STATE_FLAGS, &flags.to_le_bytes())
}

/// The one word a party request carries: a player id for `0x80A0` and `0x80A9`, a party id for
/// `0x809C`, `0x809D` and `0x809E` (`ASURA/crates/asura-protocol/src/game/mod.rs:2139-2148`).
pub fn read_party_word(body: &[u8]) -> Option<u16> {
    let bytes: [u8; 2] = body.get(..2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_party_is_created_in_four_bytes() {
        assert_eq!(party_create(PARTY_ID), vec![0xD3, 0x01, 1, 0]);
    }

    #[test]
    fn a_member_is_the_party_the_player_and_one_byte() {
        assert_eq!(
            party_player_add(PARTY_ID, 1, 1),
            vec![0xCC, 0x01, 1, 0, 1, 0, 1]
        );
    }

    /// The client's table, not the struct-layer capture: five and six bytes
    /// (`ASURA/crates/asura-protocol/src/game/mod.rs:2146-2171`).
    #[test]
    fn the_party_size_and_leader_are_the_clients_five_and_six_bytes() {
        assert_eq!(player_update_party_size(7, 1), vec![0xB0, 0x00, 7, 0, 1]);
        assert_eq!(player_update_party(7, 7), vec![0xB1, 0x00, 7, 0, 7, 0]);
    }

    /// Every invitation message is a header and one word; the member-state messages two words; the
    /// load's party state a word, a byte and a dword (the client's table rows `0x01C5`-`0x01D1`,
    /// `0x01BE`, `0x01BF`, `0x01E0`).
    #[test]
    fn the_party_messages_are_the_tables_sizes() {
        for frame in [
            party_invite_add(3),
            party_join_request(3),
            party_invite_cancel(3),
            party_request_cancel(3),
            party_request_response(3),
            party_invite_response(3),
        ] {
            assert_eq!(&frame[2..], &[3, 0]);
        }
        assert_eq!(party_search_seek(0), vec![0xE0, 0x01, 0, 0]);
        assert_eq!(party_player_remove(3, 7), vec![0xD1, 0x01, 3, 0, 7, 0]);
        assert_eq!(party_member_leave_world(3, 7), vec![0xD0, 0x01, 3, 0, 7, 0]);
        assert_eq!(party_member_enter_world(3, 7), vec![0xCF, 0x01, 3, 0, 7, 0]);
        assert_eq!(
            party_member_disconnect_soft(3, 7),
            vec![0xCE, 0x01, 3, 0, 7, 0]
        );
        assert_eq!(
            party_member_connect_soft(3, 7),
            vec![0xCD, 0x01, 3, 0, 7, 0]
        );
        assert_eq!(party_you_are_leader(1), vec![0xCB, 0x01, 1]);
        assert_eq!(party_set_difficulty(0), vec![0xBF, 0x01, 0]);
        assert_eq!(party_global_state_flags(0), vec![0xBE, 0x01, 0, 0, 0, 0]);
        assert_eq!(read_party_word(&[0x34, 0x12]), Some(0x1234));
        assert_eq!(read_party_word(&[0x34]), None);
    }

    #[test]
    fn the_member_stream_ends_in_four_bytes() {
        assert_eq!(party_member_stream_end(PARTY_ID), vec![0xD4, 0x01, 1, 0]);
    }

    /// The reference's capture, as the wire: a four-byte header in the dump is a two-byte one here,
    /// and the dwords it holds become the word and the byte the frames carry.
    ///
    /// `doors-20261031-095754.jsonl` seq 319-322: `d3010000 01000000`, `cc010000 01000000 01000000
    /// 01000000`, `d4010000 01000000`, `b3010000 01000000 01000000`.
    #[test]
    fn the_family_is_the_one_the_capture_carries() {
        let header = |frame: &[u8]| u16::from_le_bytes([frame[0], frame[1]]);
        assert_eq!(header(&party_create(1)), 0x01D3);
        assert_eq!(header(&party_player_add(1, 1, 1)), 0x01CC);
        assert_eq!(header(&party_member_stream_end(1)), 0x01D4);
        assert_eq!(party_create(1).len(), 4);
        assert_eq!(party_player_add(1, 1, 1).len(), 7);
        assert_eq!(party_member_stream_end(1).len(), 4);
    }
}
