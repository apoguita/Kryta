//! The character message family: a character's profession, skills, skillbar and faction standing.
//!
//! Reference: `src/server/game/Server/Packets/CharacterPackets.{h,cpp}`. The creation screen is the
//! first caller: a character being made has a profession, a skillbar of nothing, and an empty
//! faction record, and the client wants all four before it draws the screen.
//!
//! Every header and layout here is this client's, measured from a working server; each one names
//! where its bytes came from.

/// `GAME_SMSG_PLAYER_UPDATE_PROFESSION`: the primary and secondary profession of an agent.
pub const PLAYER_UPDATE_PROFESSION: u16 = 0x00B7;
/// `GAME_SMSG_PLAYER_UPDATE_UNLOCKED_PROFESSIONS`: which professions the account may choose.
pub const PLAYER_UPDATE_UNLOCKED_PROFESSIONS: u16 = 0x00B6;
/// `GAME_SMSG_UPDATE_UNLOCKED_SKILLS`: a counted list of unlocked skills.
pub const UPDATE_UNLOCKED_SKILLS: u16 = 0x00DB;
/// `GAME_SMSG_SKILLBAR_UPDATE`: the eight slots of the skillbar and their PvP masks.
pub const SKILLBAR_UPDATE: u16 = 0x00DA;
/// `GAME_SMSG_CHARACTER_UPDATE_FACTIONS`: a character's progress record.
pub const CHARACTER_UPDATE_FACTIONS: u16 = 0x00E9;
/// `GAME_SMSG_CHARACTER_UPDATE_INFO` (`0x0030`): a character summary - a counted string and six dwords.
pub const CHARACTER_UPDATE_INFO: u16 = 0x0030;
/// `GAME_SMSG_PLAYER_DESTROY` (`0x005A`): a player has left the map.
pub const PLAYER_DESTROY: u16 = 0x005A;
/// `GAME_SMSG_CHARACTER_UDPATE_PARTY` (`0x01B3`, the table's own spelling): a word and a byte.
pub const CHARACTER_UPDATE_PARTY: u16 = 0x01B3;
/// `GAME_SMSG_CHAR_CREATION_SUCCESS`: the character was written.
pub const CHAR_CREATION_SUCCESS: u16 = 0x0188;
/// `GAME_SMSG_CHAR_CREATION_ERROR`: it was not, and the code says why.
pub const CHAR_CREATION_ERROR: u16 = 0x018B;
/// The error code a taken name is answered with.
///
/// Evidence: `ASURA`'s `GM_ERROR_CHARACTER_NAME_ALREADY_EXIST`, read from GWPS's
/// `GmError.h` (`GameSrv.c:1939`).
pub const GM_ERROR_CHARACTER_NAME_ALREADY_EXIST: u32 = 0x0106;
/// The cap on the Kurzick faction.
pub const FACTION_MAX_KURZICK: u16 = 0x00EA;
/// The cap on the Luxon faction.
pub const FACTION_MAX_LUXON: u16 = 0x00EB;
/// The cap on the Balthazar faction.
pub const FACTION_MAX_BALTHAZAR: u16 = 0x00EC;
/// The cap on the Imperial faction.
pub const FACTION_MAX_IMPERIAL: u16 = 0x00ED;

/// The profession a fresh creation screen starts on: Warrior.
///
/// Evidence: `ASURA`'s `DEFAULT_PRIMARY_PROFESSION`, recorded from GWPS's
/// `HandleCharCreationRequestPlayer`, which sets Warrior and nothing else until the client says so.
pub const DEFAULT_PROFESSION: u8 = 1;

/// The unlocked-profession bitmap the creation screen is told before any character exists.
///
/// Evidence: ASURA's `config/Server/defaults.json`, `creation_screen_unlocked_professions: 2` - bit
/// 1, Warrior. The reference hardcodes the same value.
pub const CREATION_UNLOCKED_PROFESSIONS: u32 = 2;

/// A frame: the `u16` header and then the body.
fn frame(header: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// A `u16` count followed by that many dwords.
fn dwords(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + values.len() * 4);
    out.extend_from_slice(&(values.len() as u16).to_le_bytes());
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// `0x0188`: the character was written - its id, its name, the reference's own constant and the
/// settings block the list will carry.
///
/// Layout from the reference's message table: header, a sixteen-byte blob, a `String16` whose field
/// holds twenty units, a word, and a counted byte array (`ASURA`,
/// `crates/asura-protocol/src/packets.rs`, the row whose fields are `MsgHeader`, `Blob(16)`,
/// `String16(20)`, `Word`, `Array8(1024)`). The constant `0x2211` is GWPS's own and
/// unexplained there too (`GameSrv.c:1951`); it is carried because the message carries it.
pub fn char_creation_success(character: [u8; 16], name: &str, settings: &[u8]) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().take(20).collect();
    let mut body = Vec::with_capacity(16 + 2 + units.len() * 2 + 2 + 2 + settings.len());
    body.extend_from_slice(&character);
    body.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in &units {
        body.extend_from_slice(&unit.to_le_bytes());
    }
    body.extend_from_slice(&0x2211u16.to_le_bytes());
    body.extend_from_slice(&(settings.len() as u16).to_le_bytes());
    body.extend_from_slice(settings);
    frame(CHAR_CREATION_SUCCESS, &body)
}

/// `0x018B`: the creation was refused with this code - the header and one dword, six bytes.
pub fn char_creation_error(code: u32) -> Vec<u8> {
    frame(CHAR_CREATION_ERROR, &code.to_le_bytes())
}

/// `0x00B7`: the agent, then primary, secondary and the PvP flag.
pub fn player_update_profession(agent: u32, primary: u8, secondary: u8, is_pvp: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(7);
    body.extend_from_slice(&agent.to_le_bytes());
    body.push(primary);
    body.push(secondary);
    body.push(is_pvp);
    frame(PLAYER_UPDATE_PROFESSION, &body)
}

/// `0x00B6`: the agent and the unlocked-profession bitmap.
pub fn player_update_unlocked_professions(agent: u32, unlocked: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&unlocked.to_le_bytes());
    frame(PLAYER_UPDATE_UNLOCKED_PROFESSIONS, &body)
}

/// `GAME_SMSG_PREFERENCES_BLOB`: the character's own preferences, as the client last uploaded them.
pub const PREFERENCES_BLOB: u16 = 0x00B5;

/// The most bytes a preferences blob holds: the packet table's own `Array8(256)` for `0x00B5` and for
/// the client's `0x8045` (`game/src/Server/Protocol/Tables.rs`).
pub const PREFERENCES_BYTES: usize = 256;

/// `0x00B5`: a `u16` count and that many bytes - the blob a character's `0x8045` stored, handed back
/// on a load. Measured on the retail client: a count of 50 and the 50 bytes the client had uploaded
/// (`recordings/client/session/login-to-spawn-20261124-180308.jsonl` seq 1253, where the capture
/// holds the client's struct: the header, the count as a dword, then the bytes).
pub fn preferences_blob(preferences: &[u8]) -> Vec<u8> {
    let preferences = &preferences[..preferences.len().min(PREFERENCES_BYTES)];
    let mut body = Vec::with_capacity(2 + preferences.len());
    body.extend_from_slice(&(preferences.len() as u16).to_le_bytes());
    body.extend_from_slice(preferences);
    frame(PREFERENCES_BLOB, &body)
}

/// `0x00DB`: a counted list of skill ids.
pub fn update_unlocked_skills(skills: &[u32]) -> Vec<u8> {
    frame(UPDATE_UNLOCKED_SKILLS, &dwords(skills))
}

/// `0x00DA`: the agent, eight slots, eight PvP masks, one trailing byte.
pub fn skillbar_update(agent: u32, skills: &[u32], pvp_masks: &[u32], unknown: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(9 + 2 + 2 + skills.len() * 4 + pvp_masks.len() * 4);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&dwords(skills));
    body.extend_from_slice(&dwords(pvp_masks));
    body.push(unknown);
    frame(SKILLBAR_UPDATE, &body)
}

/// How many dwords a character's progress record is.
///
/// Evidence: `ASURA/crates/asura-protocol/src/game/mod.rs`, `character_update_factions`, which writes
/// all fifteen fields of `CharacterProgress` (`GameSrv_UpdatePlayerFactions`).
pub const FACTIONS_DWORDS: usize = 15;

/// `0x00E9`: a character's progress record - fifteen dwords, in the reference's own order.
///
/// | # | field |
/// |---|---|
/// | 0 | experience |
/// | 1, 2 | the Kurzick faction, held and earned |
/// | 3, 4 | the Luxon faction, held and earned |
/// | 5, 6 | the Imperial faction, held and earned |
/// | 7, 8 | two faction words the reference sends zero |
/// | 9 | level |
/// | 10 | morale |
/// | 11, 12 | the Balthazar faction, held and earned |
/// | 13, 14 | skill points, held and earned |
pub fn character_update_factions(progress: &[u32; FACTIONS_DWORDS]) -> Vec<u8> {
    let mut body = Vec::with_capacity(progress.len() * 4);
    for value in progress {
        body.extend_from_slice(&value.to_le_bytes());
    }
    frame(CHARACTER_UPDATE_FACTIONS, &body)
}

/// `GAME_SMSG_PLAYER_DESTROY` (`0x005A`): a player has left the map - eight bytes, a four-byte game
/// header and the player id.
///
/// `GameSrv_BroadcastPlayerDestroy` (`GmPlayer.c:314`); `ASURA/crates/asura-protocol/src/game/mod.rs:1352`,
/// `player_destroy`. The client's own sniffer calls `0x5A` `AGENT_DESTROY_PLAYER`, and the capture
/// carries `5a 00 00 00 | 0b 00 00 00`.
pub fn player_destroy(player: u32) -> (u16, Vec<u8>) {
    let mut body = vec![0u8; 6];
    body[2..6].copy_from_slice(&player.to_le_bytes());
    (PLAYER_DESTROY, frame(PLAYER_DESTROY, &body))
}

/// One of the four faction caps: the header names the faction, then the cap.
pub fn faction_max(header: u16, cap: u32) -> Vec<u8> {
    frame(header, &cap.to_le_bytes())
}

/// `0x0030`: the character summary a load sends - a counted string and six dwords.
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x0030 28 bytes :
/// 30 00 00 00 00 00 00 00 00 00 00 00 E8 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00`: an empty
/// counted string (`00 00`) and the dwords 0, 0, 1000, 0, 0, 0. What the dword of 1000 names is not
/// measured; a load repeats these values, so they are carried as the capture has them.
///
/// The name is a parameter, as it is in the working implementation's builder: the reference sets it to
/// empty, which is what this world passes, and the shape is `28 + 2n` because of the counted string
/// (`ASURA/crates/asura-protocol/src/game/mod.rs:775-788`).
pub fn character_update_info(name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().take(32).collect();
    let mut body = Vec::with_capacity(2 + units.len() * 2 + 24);
    body.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in &units {
        body.extend_from_slice(&unit.to_le_bytes());
    }
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&1000u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    frame(CHARACTER_UPDATE_INFO, &body)
}

/// `0x01B3`: a character's party membership, which a load sends before its end.
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x01B3 5 bytes : B3 01 01 00 01`: the
/// header, the word 1 and the byte 1. What the two fields name is not measured; the frame is carried
/// as the capture has it, and a one-member party is what a solo connection is (R3).
pub fn character_update_party(member: u16, flag: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    body.extend_from_slice(&member.to_le_bytes());
    body.push(flag);
    frame(CHARACTER_UPDATE_PARTY, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profession_is_the_agent_and_three_bytes() {
        assert_eq!(
            player_update_profession(1, DEFAULT_PROFESSION, 0, 0),
            vec![0xB7, 0x00, 1, 0, 0, 0, 1, 0, 0]
        );
    }

    #[test]
    fn the_unlocked_professions_are_the_agent_and_a_bitmap() {
        assert_eq!(
            player_update_unlocked_professions(1, CREATION_UNLOCKED_PROFESSIONS),
            vec![0xB6, 0x00, 1, 0, 0, 0, 2, 0, 0, 0]
        );
    }

    #[test]
    fn the_preferences_blob_is_a_counted_byte_string() {
        // The 50 bytes the retail server sends a character with every per-character setting at its
        // usual state (`recordings/client/session/login-to-spawn-20261124-180308.jsonl` seq 1253).
        let blob: Vec<u8> = (0..50u8).collect();
        let frame = preferences_blob(&blob);
        assert_eq!(&frame[..4], &[0xB5, 0x00, 50, 0]);
        assert_eq!(&frame[4..], &blob[..]);
        // The message declares 256 bytes; a longer blob is cut to what it can carry.
        assert_eq!(preferences_blob(&[7u8; 300]).len(), 4 + PREFERENCES_BYTES);
    }

    #[test]
    fn an_empty_skill_list_is_a_zero_count() {
        assert_eq!(update_unlocked_skills(&[]), vec![0xDB, 0x00, 0, 0]);
    }

    #[test]
    fn the_skillbar_is_two_counted_lists_and_a_byte() {
        let frame = skillbar_update(1, &[0u32; 8], &[0u32; 8], 1);
        // header 2, agent 4, count 2 + eight slots, count 2 + eight masks, one byte.
        assert_eq!(frame.len(), 2 + 4 + 34 + 34 + 1);
        assert_eq!(&frame[6..8], &8u16.to_le_bytes(), "eight slots");
        assert_eq!(&frame[40..42], &8u16.to_le_bytes(), "eight masks");
        assert_eq!(*frame.last().unwrap(), 1, "the trailing byte");
    }

    #[test]
    fn the_faction_record_is_fifteen_dwords() {
        let frame = character_update_factions(&[0u32; FACTIONS_DWORDS]);
        assert_eq!(frame.len(), 2 + 15 * 4, "fifteen dwords, as the reference sends");
    }

    #[test]
    fn a_faction_cap_is_its_header_and_one_dword() {
        assert_eq!(
            faction_max(FACTION_MAX_KURZICK, 10_000),
            vec![0xEA, 0x00, 0x10, 0x27, 0, 0]
        );
    }
}
