//! The misc family: the small messages that belong to no larger subject.
//!
//! Reference: `src/server/game/Server/Packets/MiscPackets.{h,cpp}`, which is where the reference
//! keeps exactly this kind of message. Here that is the ping pair and the tick, the hard-mode flag,
//! the current-map notice - every one of them a message a working server sent on a load
//! (`recordings/gwps/session/login-travel-session.log`, `recordings/gwps/world/load-stream.log`) -
//! and the PvP halves of the creation screen, because a creation account has unlocked nothing and
//! an empty list is what says so.

/// `GAME_SMSG_PING_REQUEST` (`0x000C`): the header alone.
pub const PING_REQUEST: u16 = 0x000C;
/// `GAME_SMSG_PING_REPLY` (`0x000D`): one dword - the tick it answers.
pub const PING_REPLY: u16 = 0x000D;
/// `GAME_SMSG_WORLD_SIMULATION_TICK` (`0x001E`): one dword - the tick the simulation is on.
pub const WORLD_SIMULATION_TICK: u16 = 0x001E;
/// `GAME_SMSG_HARD_MODE_UNLOCKED` (`0x0070`): one byte.
pub const HARD_MODE_UNLOCKED: u16 = 0x0070;
/// `GAME_SMSG_MAP_UPDATE_CURRENT` (`0x0099`): the map, and one byte.
pub const MAP_UPDATE_CURRENT: u16 = 0x0099;
/// `GAME_SMSG_TRANSFER_GAME_SERVER_INFO` (`0x01A6`): where a travelling client reconnects.
pub const TRANSFER_GAME_SERVER_INFO: u16 = 0x01A6;
/// `GAME_SMSG_PVP_UPDATE_UNLOCKED_SKILLS`: a counted list of unlocked PvP skills.
pub const PVP_UPDATE_UNLOCKED_SKILLS: u16 = 0x001D;
/// `GAME_SMSG_PVP_UPDATE_UNLOCKED_HEROES`: a counted bitmap of unlocked PvP heroes.
pub const PVP_UPDATE_UNLOCKED_HEROES: u16 = 0x0018;
/// `GAME_SMSG_PVP_ITEM_STREAM_END`: the PvP item stream is over.
pub const PVP_ITEM_STREAM_END: u16 = 0x001B;
/// `GAME_SMSG_PVP_ITEM_STREAM_ADD` (`0x001A`): one unlockable PvP item.
pub const PVP_ITEM_STREAM_ADD: u16 = 0x001A;
/// `GAME_SMSG_ACCOUNT_FEATURE` (`0x000F`): one account entitlement.
pub const ACCOUNT_FEATURE: u16 = 0x000F;
/// `GAME_SMSG_MAP_UPDATE_UNLOCKED_LIST` (`0x0094`): five counted lists of dwords.
pub const MAP_UPDATE_UNLOCKED_LIST: u16 = 0x0094;
/// `GAME_SMSG_INSTANCE_LOADED` (`0x00F2`): the player's team token.
pub const INSTANCE_LOADED: u16 = 0x00F2;
/// `GAME_SMSG_CHAR_VISIBILITY_FLAGS` (`0x00EF`): a value and the mask it applies under.
pub const CHAR_VISIBILITY_FLAGS: u16 = 0x00EF;
/// `GAME_SMSG_PLAYER_UPDATE_FLAGS` (`0x003C`): a player's flags word, as a masked write.
pub const PLAYER_UPDATE_FLAGS: u16 = 0x003C;
/// `GAME_SMSG_WORLD_UPDATE_LOAD_TIME` (`0x001F`): how long the load took.
pub const WORLD_UPDATE_LOAD_TIME: u16 = 0x001F;
/// `GAME_SMSG_UPDATE_AGENT_VISUAL_EQUIPMENT` (`0x006E`): the nine items an agent is drawn wearing.
pub const UPDATE_AGENT_VISUAL_EQUIPMENT: u16 = 0x006E;
/// `GAME_SMSG_AGENT_DISPLAY_CAPE` (`0x0048`): whether an agent's cape is shown.
pub const AGENT_DISPLAY_CAPE: u16 = 0x0048;

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

/// `0x000C`: the header alone. A load sends it once, unasked.
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x000C 2 bytes : 0C 00`, right after the
/// load head block and before the item stream.
pub fn ping_request() -> Vec<u8> {
    frame(PING_REQUEST, &[])
}

/// `0x000D`: the dword the reference answers the ping with.
///
/// Source: the same trace - `SEND 0x000D 6 bytes : 0D 00 14 00 00 00` (20).
pub fn ping_reply(tick: u32) -> Vec<u8> {
    frame(PING_REPLY, &tick.to_le_bytes())
}

/// `0x001E`: the simulation tick. The reference sent 500 once the entry point was sent.
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x001E 6 bytes : 1E 00 F4 01 00 00`.
pub fn world_simulation_tick(tick: u32) -> Vec<u8> {
    frame(WORLD_SIMULATION_TICK, &tick.to_le_bytes())
}

/// `0x0070`: the hard-mode flag, one byte (1 = unlocked).
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x0070 3 bytes : 70 00 01`.
pub fn hard_mode_unlocked(unlocked: u8) -> Vec<u8> {
    frame(HARD_MODE_UNLOCKED, &[unlocked])
}

/// `GAME_SMSG_TRANSFER_GAME_SERVER_INFO` (`0x01A6`): where the client should reconnect, and with what.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:839`, `transfer_game_server_info`, a faithful port of
/// the reference's own struct (`GameMsg.h:659`): the host as a Winsock `sockaddr_in` - family `2` little
/// endian, the port **big** endian, four address bytes and sixteen zero - then the world's id, a region,
/// the player token, whether the destination is an explorable, and the map id last. Fifty-two bytes
/// before the header. The working implementation's notes on the two shapes that failed: the captured
/// order in a 46-byte frame made the client read the map id out of the token slot (`007`), and a
/// 52-byte frame with a four-byte header made it read `00 00` as the address family (`005`).
pub fn transfer_game_server_info(
    host: [u8; 4],
    port: u16,
    map_token: u32,
    region: u32,
    player_token: u32,
    is_explorable: u32,
    map_id: u32,
) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(44);
    body.extend_from_slice(&2u16.to_le_bytes()); // AF_INET, little endian
    body.extend_from_slice(&port.to_be_bytes());
    body.extend_from_slice(&host);
    body.extend_from_slice(&[0u8; 16]); // the rest of the sockaddr_in
    body.extend_from_slice(&map_token.to_le_bytes());
    body.extend_from_slice(&region.to_le_bytes());
    body.extend_from_slice(&player_token.to_le_bytes());
    body.extend_from_slice(&is_explorable.to_le_bytes());
    body.extend_from_slice(&map_id.to_le_bytes());
    (
        TRANSFER_GAME_SERVER_INFO,
        frame(TRANSFER_GAME_SERVER_INFO, &body),
    )
}

/// `GAME_SMSG_MAP_UPDATE_CURRENT` in the form that **follows a redirect**: twelve bytes, where the login
/// batch's copy is five (`ASURA/crates/asura-protocol/src/game/mod.rs:808`,
/// `map_update_current_redirect`).
pub fn map_update_current_redirect(map_id: u32) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(10);
    body.extend_from_slice(&0u16.to_le_bytes()); // the upper half of the four-byte game header
    body.extend_from_slice(&map_id.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    (MAP_UPDATE_CURRENT, frame(MAP_UPDATE_CURRENT, &body))
}

/// `0x0099`: the map the character is now in, and one byte (0 in every capture).
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x0099 5 bytes : 99 00 92 00 00` (map 146).
pub fn map_update_current(map_id: u16, unknown: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    body.extend_from_slice(&map_id.to_le_bytes());
    body.push(unknown);
    frame(MAP_UPDATE_CURRENT, &body)
}

/// `0x001D`: a counted list of unlocked skills.
pub fn pvp_update_unlocked_skills(skills: &[u32]) -> Vec<u8> {
    frame(PVP_UPDATE_UNLOCKED_SKILLS, &dwords(skills))
}

/// `0x0018`: a counted bitmap of unlocked heroes.
pub fn pvp_update_unlocked_heroes(bitmap: &[u32]) -> Vec<u8> {
    frame(PVP_UPDATE_UNLOCKED_HEROES, &dwords(bitmap))
}

/// `0x001B`: the header alone, which ends the stream.
pub fn pvp_item_stream_end() -> Vec<u8> {
    frame(PVP_ITEM_STREAM_END, &[])
}

/// `0x001A`: one unlockable PvP item - its id and the parameters the client pairs with it.
///
/// The parameter count is a **byte**, not the `u16` every other counted field uses: the reference's
/// layout declares this one as a nested struct. Evidence: `ASURA/crates/asura-protocol/src/game/mod.rs`,
/// `pvp_item_stream_add`, whose own log shows an entry with three parameters at 17 bytes (`2 + 2 + 1 + 12`).
pub fn pvp_item_stream_add(item_id: u16, params: &[u32]) -> Vec<u8> {
    let mut body = Vec::with_capacity(3 + params.len() * 4);
    body.extend_from_slice(&item_id.to_le_bytes());
    body.push(params.len() as u8);
    for param in params {
        body.extend_from_slice(&param.to_le_bytes());
    }
    frame(PVP_ITEM_STREAM_ADD, &body)
}

/// `0x000F`: one account entitlement - a feature id and two parameters.
pub fn account_feature(feature_id: u16, param1: u16, param2: u16) -> Vec<u8> {
    let mut body = Vec::with_capacity(6);
    body.extend_from_slice(&feature_id.to_le_bytes());
    body.extend_from_slice(&param1.to_le_bytes());
    body.extend_from_slice(&param2.to_le_bytes());
    frame(ACCOUNT_FEATURE, &body)
}

/// `0x0094`: five lists of dwords - missions completed, their bonuses, the same two in hard mode, and
/// the instances a character has reached. Each list is a `u16` count and then its dwords.
///
/// The two hard-mode lists go out **full length and zero-filled**, as the reference sends them for a
/// character with no progress (`ASURA/crates/asura-world/src/player.rs`, `UNLOCKED_MAP_ZEROS`, 31
/// words each): an empty list is not the same message, it is an array the client can index past the end
/// of. The last list is the instance bitmap: instance `n` is bit `n % 32` of word `n / 32`.
pub fn map_update_unlocked_list(lists: [&[u32]; 5]) -> Vec<u8> {
    let mut body = Vec::new();
    for list in lists {
        body.extend_from_slice(&(list.len() as u16).to_le_bytes());
        for value in list {
            body.extend_from_slice(&value.to_le_bytes());
        }
    }
    frame(MAP_UPDATE_UNLOCKED_LIST, &body)
}

/// `0x00F2`: the player's team token, once per load.
pub fn instance_loaded(player_team_token: u32) -> Vec<u8> {
    frame(INSTANCE_LOADED, &player_team_token.to_le_bytes())
}

/// `0x00EF`: two dwords, a value and the mask it applies under. A live load sends
/// `FFFFFFFF, FFFFFFFF` once, immediately after the faction record; uninitialised, the inventory
/// panel's per-slot display mode stays blank for every slot (`ASURA/crates/asura-protocol/src/game/mod.rs`,
/// `char_visibility_flags`).
pub fn char_visibility_flags(value: u32, mask: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&value.to_le_bytes());
    body.extend_from_slice(&mask.to_le_bytes());
    frame(CHAR_VISIBILITY_FLAGS, &body)
}

/// `0x003C`: one player's flags word as a masked read-modify-write - a `u16` player id, the value and
/// the mask. A live load sends it once per player with mask 7, right after the visibility flags
/// (`ASURA/crates/asura-protocol/src/game/mod.rs`, `player_update_flags`).
pub fn player_update_flags(player_id: u16, value: u32, mask: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(10);
    body.extend_from_slice(&player_id.to_le_bytes());
    body.extend_from_slice(&value.to_le_bytes());
    body.extend_from_slice(&mask.to_le_bytes());
    frame(PLAYER_UPDATE_FLAGS, &body)
}

/// `0x001F`: how long the load took, in milliseconds.
pub fn world_update_load_time(load_time: u32) -> Vec<u8> {
    frame(WORLD_UPDATE_LOAD_TIME, &load_time.to_le_bytes())
}

/// `0x006E`: the nine item ids the agent is drawn wearing - the equipped bag, in its own order.
pub fn update_agent_visual_equipment(agent: u32, items: [u32; 9]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + items.len() * 4);
    body.extend_from_slice(&agent.to_le_bytes());
    for item in items {
        body.extend_from_slice(&item.to_le_bytes());
    }
    frame(UPDATE_AGENT_VISUAL_EQUIPMENT, &body)
}

/// `0x0048`: whether an agent's cape is shown.
pub fn agent_display_cape(agent: u32, status: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    body.extend_from_slice(&agent.to_le_bytes());
    body.push(status);
    frame(AGENT_DISPLAY_CAPE, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_pvp_half_is_a_header_and_a_zero_count() {
        assert_eq!(pvp_update_unlocked_skills(&[]), vec![0x1D, 0x00, 0, 0]);
        assert_eq!(pvp_update_unlocked_heroes(&[]), vec![0x18, 0x00, 0, 0]);
        assert_eq!(pvp_item_stream_end(), vec![0x1B, 0x00]);
    }

    #[test]
    fn a_pvp_item_counts_its_parameters_in_a_byte() {
        let frame = pvp_item_stream_add(1, &[0x2408_0081, 0x2530_0102, 0x24B8_0300]);
        assert_eq!(frame.len(), 2 + 2 + 1 + 12);
        assert_eq!(&frame[..2], &[0x1A, 0x00]);
        assert_eq!(&frame[2..4], &1u16.to_le_bytes(), "the item");
        assert_eq!(frame[4], 3, "three parameters, counted by a byte");
    }

    #[test]
    fn the_account_feature_is_a_id_and_two_parameters() {
        assert_eq!(
            account_feature(1, 10, 0),
            vec![0x0F, 0x00, 1, 0, 10, 0, 0, 0]
        );
    }

    #[test]
    fn the_unlocked_lists_carry_their_own_counts() {
        let frame = map_update_unlocked_list([&[], &[], &[0u32; 2], &[0u32; 31], &[5]]);
        assert_eq!(&frame[..2], &[0x94, 0x00]);
        // Two empty lists, one of two words, one of thirty-one, one of one.
        let expected = 4 + (2 + 8) + (2 + 124) + (2 + 4);
        assert_eq!(frame.len(), 2 + expected);
    }

    #[test]
    fn the_player_flags_are_a_word_a_value_and_a_mask() {
        assert_eq!(
            player_update_flags(1, 0, 7),
            vec![0x3C, 0x00, 1, 0, 0, 0, 0, 0, 7, 0, 0, 0]
        );
    }

    #[test]
    fn the_visual_equipment_is_the_agent_and_nine_items() {
        let frame = update_agent_visual_equipment(1, [2, 3, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(frame.len(), 2 + 4 + 36);
        assert_eq!(&frame[..2], &[0x6E, 0x00]);
    }

    #[test]
    fn the_ping_and_the_tick_are_the_recording_s_bytes() {
        assert_eq!(ping_request(), vec![0x0C, 0x00]);
        assert_eq!(ping_reply(20), vec![0x0D, 0x00, 0x14, 0x00, 0x00, 0x00]);
        assert_eq!(
            world_simulation_tick(500),
            vec![0x1E, 0x00, 0xF4, 0x01, 0x00, 0x00]
        );
        assert_eq!(hard_mode_unlocked(1), vec![0x70, 0x00, 0x01]);
        assert_eq!(
            map_update_current(146, 0),
            vec![0x99, 0x00, 0x92, 0x00, 0x00]
        );
    }
}
