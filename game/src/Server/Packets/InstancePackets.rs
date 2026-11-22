//! The instance message family: what a world says when it puts a client inside a map.
//!
//! Reference: `src/server/game/Server/Packets/InstancePackets.{h,cpp}`. One file per family is this
//! tree's rule (`AGENTS.md`), and this is the first family to have content - the messages a
//! character-creation connection is answered with.
//!
//! Sizes and layouts are this client's, measured from a server that works with it; each builder
//! names where its bytes came from. A message here is a whole wire frame: the `u16` header and then
//! its fields, with no length anywhere.

/// `GAME_SMSG_INSTANCE_LOAD_HEAD`: the load begins.
///
/// Two bytes of flags and two zero bytes. Evidence: the reference's own send log,
/// `SEND 0x017C 6 bytes : 7C 01 3F 3F 00 00`, and a working server's builder that writes exactly
/// those four bytes after the header (`instance_load_head(0x3F, 0x3F)`).
pub const INSTANCE_LOAD_HEAD: u16 = 0x017C;

/// `GAME_SMSG_INSTANCE_PLAYER_DATA_START`: the player-data stream opens.
pub const INSTANCE_PLAYER_DATA_START: u16 = 0x0186;

/// `GAME_SMSG_INSTANCE_PLAYER_DATA_DONE`: the player-data stream is over.
///
/// Two bytes, the header only. Its only caller is the character-creation flow - a map load never
/// carries it - which is why it sits beside the creation start here.
pub const INSTANCE_PLAYER_DATA_DONE: u16 = 0x018A;

/// `GAME_SMSG_CHAR_CREATION_START`: the creation instance may show its screen.
///
/// Evidence: the reference's own send log; two bytes, the header only. A creation connection is
/// answered with the load head and this, and nothing else - the map's own block (player name, load
/// info, the agent) belongs to a map that has geometry, and the creation instance has none.
pub const CHAR_CREATION_START: u16 = 0x0189;

/// `GAME_SMSG_INSTANCE_LOAD_PLAYER_NAME` (`0x017D`): the character's name, as a counted UTF-16 string.
pub const INSTANCE_LOAD_PLAYER_NAME: u16 = 0x017D;

/// `GAME_SMSG_INSTANCE_LOAD_INFO` (`0x019A`): which instance this is, and which agent the client
/// controls in it.
pub const INSTANCE_LOAD_INFO: u16 = 0x019A;

/// `GAME_SMSG_INSTANCE_LOAD_SPAWN_POINT` (`0x0196`): where the character stands, and which map file to
/// load.
///
/// Evidence for the layout and the numbers: a working server's own recording,
/// `recordings/asura/feature/travel/first-crossing-20261031.jsonl` seq 125 - a 26-byte frame whose
/// fields are, after the header, `map file id`, an x/y pair of floats, the plane, two bytes and eight
/// zero bytes. That frame's own values are map 148's: file id `0x0001B97D`, x `9541.87`, y `4796.46`,
/// plane 0.
pub const INSTANCE_LOAD_SPAWN_POINT: u16 = 0x0196;

/// `GAME_CMSG_INSTANCE_LOAD_REQUEST_SPAWN` (`0x8088`): the client asks where it stands.
pub const CLIENT_LOAD_REQUEST_SPAWN: u16 = 0x8088;

/// `GAME_CMSG_INSTANCE_LOAD_REQUEST_PLAYERS` (`0x8090`): the client asks for the bodies in the map,
/// and the load ends with the answer.
pub const CLIENT_LOAD_REQUEST_PLAYERS: u16 = 0x8090;

/// `GAME_CMSG_INSTANCE_LOAD_REQUEST_ITEMS` (`0x8091`): the client asks for its inventory, and the
/// answer is the item stream and the map's record.
///
/// Source: `recordings/gwps/world/load-stream.log` - `RECV 0x8091 4 bytes : 91 80 00 00`, answered
/// with `0x0144` and the rest of the stream.
pub const CLIENT_LOAD_REQUEST_ITEMS: u16 = 0x8091;

/// `GAME_SMSG_INSTANCE_LOAD_FINISH` (`0x018E`): the last message of the load; the client stops loading
/// on it.
pub const INSTANCE_LOAD_FINISH: u16 = 0x018E;

/// `GAME_SMSG_INSTANCE_MANIFEST_PHASE` (`0x0199`): one byte - a step of the map's manifest.
pub const INSTANCE_MANIFEST_PHASE: u16 = 0x0199;

/// `GAME_SMSG_INSTANCE_MANIFEST_DONE` (`0x0198`): a byte, a word and a dword - the end of one manifest.
pub const INSTANCE_MANIFEST_DONE: u16 = 0x0198;

/// `GAME_SMSG_READY_FOR_MAP_SPAWN` (`0x01AC`): one dword - the world is ready for the client's spawn
/// request.
pub const READY_FOR_MAP_SPAWN: u16 = 0x01AC;

/// `GAME_SMSG_PLAYER_CREATE` (`0x0059`): one body the client is told about - for a load of our own, the
/// player's own agent.
pub const PLAYER_CREATE: u16 = 0x0059;

/// `GAME_SMSG_MAP_INIT_START` (`0x008B`, 14 bytes): the exploration grid's dimensions and how many
/// compressed bytes the `0x008A` messages that follow carry.
///
/// Unnamed in the generated table (it records three dwords and nothing else); the name is the client's
/// own handler, `OnMsgSrvMapInitStart`, and the shape is measured - `{dims.x, dims.y, compressed byte
/// count}` (`ASURA/crates/asura-protocol/src/game/mod.rs:90-96`, `map_init_start`).
pub const MAP_INIT_START: u16 = 0x008B;

/// `GAME_SMSG_CARTOGRAPHY_DATA` (`0x008A`, 262 bytes): one slice of the exploration grid - a `u16` count
/// and the fixed 64-dword array (`ASURA/crates/asura-protocol/src/game/mod.rs:97-112`).
///
/// The shape is the one this client frames to, and it is not the one a capture shows: a live capture
/// reports the same message at 132 bytes with a `u32` count, and a server that sent *that* shape got
/// `007` at map load from this client, measured 2026-10-30
/// (`ASURA/crates/asura-protocol/src/game/mod.rs:99-106`). The table's own field list settles it: the
/// row's `Array32` is 64 long, so the message is a `u16` count and 256 bytes.
pub const CARTOGRAPHY_DATA: u16 = 0x008A;

/// The dwords one [`CARTOGRAPHY_DATA`] message carries: the packet table's own `Array32` length
/// (`ASURA/crates/asura-protocol/src/game/mod.rs:110`).
pub const CARTOGRAPHY_DATA_SLOTS: usize = 64;

/// The blob bytes one [`CARTOGRAPHY_DATA`] message carries.
pub const CARTOGRAPHY_DATA_BYTES: usize = CARTOGRAPHY_DATA_SLOTS * 4;

/// `GAME_SMSG_INSTANCE_TABLE` (`0x01A0`): one batch of the destinations a client can resolve - a `u8`
/// count and that many `{u16 instance, u32 value}` pairs.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:864-878`, `instance_table`. A load sends it in batches
/// during the load block, and the working implementation's value for this shape is the one a live
/// client's own load carried for map 148 (`ASURA/crates/asura-world/src/instance_table.rs:154-173`).
pub const INSTANCE_TABLE: u16 = 0x01A0;

/// A frame: the `u16` header and then the body.
fn frame(header: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// `0x017C`: the reference's own flags - both unlock bits set, then two zero bytes.
pub fn instance_load_head() -> Vec<u8> {
    frame(INSTANCE_LOAD_HEAD, &[0x3F, 0x3F, 0x00, 0x00])
}

/// `0x0189`: the header and nothing else.
pub fn char_creation_start() -> Vec<u8> {
    frame(CHAR_CREATION_START, &[])
}

/// `0x0186`: the header and nothing else. It opens the block a creation screen's player is drawn
/// from, and `instance_player_data_done` closes it.
pub fn instance_player_data_start() -> Vec<u8> {
    frame(INSTANCE_PLAYER_DATA_START, &[])
}

/// `0x018A`: the header and nothing else.
pub fn instance_player_data_done() -> Vec<u8> {
    frame(INSTANCE_PLAYER_DATA_DONE, &[])
}

/// `0x017D`: the character's name, as a counted string of at most twenty units.
pub fn instance_load_player_name(name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().take(20).collect();
    let mut body = Vec::with_capacity(2 + units.len() * 2);
    body.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in &units {
        body.extend_from_slice(&unit.to_le_bytes());
    }
    frame(INSTANCE_LOAD_PLAYER_NAME, &body)
}

/// `0x019A`: the agent the client controls, the map it is in, whether the client runs an explorable's
/// load, the district, the language and two bytes.
///
/// The explorable byte is **1 for every load**, which is what the reference sends: it hardcodes 1 with
/// the comment `// what to put here?` (`ASURA/crates/asura-protocol/src/game/mod.rs:530-534`,
/// `instance_load_info`; its only caller passes 1, `ASURA/crates/asura-world/src/map.rs:34-42`). It is
/// a parameter here for the same reason it is one there, and every call passes 1.
pub fn instance_load_info(
    agent: u32,
    map_id: u16,
    is_explorable: u8,
    district: u32,
    language: u8,
    is_observer: u8,
) -> Vec<u8> {
    let mut body = Vec::with_capacity(14);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&map_id.to_le_bytes());
    body.push(is_explorable);
    body.extend_from_slice(&district.to_le_bytes());
    body.push(language);
    body.push(is_observer);
    body.push(0);
    frame(INSTANCE_LOAD_INFO, &body)
}

/// `0x0196`: the map file to load, where the character stands, the plane, two bytes and eight zeros.
pub fn instance_load_spawn_point(map_file_id: u32, x: f32, y: f32, plane: u16) -> Vec<u8> {
    let mut body = Vec::with_capacity(24);
    body.extend_from_slice(&map_file_id.to_le_bytes());
    body.extend_from_slice(&x.to_le_bytes());
    body.extend_from_slice(&y.to_le_bytes());
    body.extend_from_slice(&plane.to_le_bytes());
    body.push(0); // unk0
    body.push(0); // is_cinematic
    body.extend_from_slice(&[0u8; 8]);
    frame(INSTANCE_LOAD_SPAWN_POINT, &body)
}

/// `0x018E`: the load is over - the header alone.
pub fn instance_load_finish() -> Vec<u8> {
    frame(INSTANCE_LOAD_FINISH, &[])
}

/// `0x0199`: one byte of a map's manifest phase.
///
/// Source: `recordings/gwps/world/load-stream.log` - `SEND 0x0199 3 bytes : 99 01 00`, then
/// `99 01 01`, before each manifest's end.
pub fn instance_manifest_phase(phase: u8) -> Vec<u8> {
    frame(INSTANCE_MANIFEST_PHASE, &[phase])
}

/// `0x0198`: the end of one manifest - a byte, a word and a dword.
///
/// Source: the same trace - `SEND 0x0198 9 bytes : 98 01 02 6D 03 00 00 00 00` (byte 2, word 877)
/// and `98 01 00 92 00 00 00 00 00` (byte 0, the map). Both are repeated once per load; what the
/// word names is not measured (`docs/port/06-world-plan.md`, "Where the ledger says we do not know").
pub fn instance_manifest_done(phase: u8, word: u16, dword: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(7);
    body.push(phase);
    body.extend_from_slice(&word.to_le_bytes());
    body.extend_from_slice(&dword.to_le_bytes());
    frame(INSTANCE_MANIFEST_DONE, &body)
}

/// `0x01AC`: the world is ready for the client's spawn request - one dword.
///
/// Source: the same trace - `SEND 0x01AC 6 bytes : AC 01 00 00 00 00`.
pub fn ready_for_map_spawn(dword: u32) -> Vec<u8> {
    frame(READY_FOR_MAP_SPAWN, &dword.to_le_bytes())
}

/// `0x0059`: the player's own agent, as the client is told about it at a load.
///
/// The field set is the ported row's: the player, the agent, the packed appearance word, a byte, two
/// dwords and the name. The third dword is the appearance, not a constant: `0x03100000` is what every
/// capture carries because that is the reference's own default look - bit 20 is profession 1 (Warrior)
/// and bits 24-25 are hair style 3 (`docs/game-rules.md`, E74, packs sex, height, skin, hair colour,
/// face, profession, hair style and campaign into that word). A character's own appearance comes from
/// its row; `ASURA/crates/asura-protocol/src/game/mod.rs`, `player_create`, writes it from the row too.
pub fn player_create(player: u32, agent: u32, appearance: u32, name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().take(20).collect();
    let mut body = Vec::with_capacity(21 + units.len() * 2);
    body.extend_from_slice(&player.to_le_bytes()); // the id the client keys the body by
    body.extend_from_slice(&agent.to_le_bytes()); // the agent
    body.extend_from_slice(&appearance.to_le_bytes()); // the packed look the model is built from
    body.push(0);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in &units {
        body.extend_from_slice(&unit.to_le_bytes());
    }
    frame(PLAYER_CREATE, &body)
}

/// What a character-creation connection is answered with, in the reference's order.
pub fn creation_load_block() -> Vec<u8> {
    let mut block = instance_load_head();
    block.extend_from_slice(&char_creation_start());
    block
}

/// `0x008B`: the exploration grid's dimensions and the byte count the `0x008A` slices add up to.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:904`, `map_init_start`: three dwords after the header.
pub fn map_init_start(dims_x: u32, dims_y: u32, byte_count: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&dims_x.to_le_bytes());
    body.extend_from_slice(&dims_y.to_le_bytes());
    body.extend_from_slice(&byte_count.to_le_bytes());
    frame(MAP_INIT_START, &body)
}

/// `0x008A`: one slice of the compressed grid - a `u16` count of the dwords that arrived, then the
/// fixed 64-dword array, zero past the slice.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:914`, `cartography_data`, which is the shape this
/// client frames to (see the constant above).
pub fn cartography_data(blob: &[u8]) -> Vec<u8> {
    let bytes = &blob[..blob.len().min(CARTOGRAPHY_DATA_BYTES)];
    let mut body = Vec::with_capacity(2 + CARTOGRAPHY_DATA_BYTES);
    body.extend_from_slice(&(bytes.len().div_ceil(4) as u16).to_le_bytes());
    for slot in 0..CARTOGRAPHY_DATA_SLOTS {
        let at = slot * 4;
        let mut word = [0u8; 4];
        for (index, byte) in word.iter_mut().enumerate() {
            if let Some(source) = bytes.get(at + index) {
                *byte = *source;
            }
        }
        body.extend_from_slice(&word);
    }
    frame(CARTOGRAPHY_DATA, &body)
}

/// `0x01A0`: one batch of the instance table - a `u8` count and that many `{u16, u32}` pairs.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:872`, `instance_table`.
pub fn instance_table(batch: &[(u16, u32)]) -> Vec<u8> {
    let mut body = Vec::with_capacity(1 + batch.len() * 6);
    body.push(batch.len().min(usize::from(u8::MAX)) as u8);
    for (instance, value) in batch.iter().take(usize::from(u8::MAX)) {
        body.extend_from_slice(&instance.to_le_bytes());
        body.extend_from_slice(&value.to_le_bytes());
    }
    frame(INSTANCE_TABLE, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spawn_point_is_the_recordings_twenty_six_bytes() {
        // Map 148's own frame, from `recordings/asura/feature/travel/first-crossing-20261031.jsonl`:
        // `96 01 | 7d b9 01 00 | 7e 17 15 46 | a6 e3 95 45 | 00 00 | 00 00 | 8 zeros`.
        let frame = instance_load_spawn_point(0x0001_B97D, 9541.87, 4796.46, 0);
        assert_eq!(frame.len(), 26, "the recording's size");
        assert_eq!(&frame[..2], &[0x96, 0x01]);
        assert_eq!(
            &frame[2..6],
            &0x0001_B97Du32.to_le_bytes(),
            "the map file id"
        );
        assert_eq!(&frame[6..10], &9541.87f32.to_le_bytes());
        assert_eq!(&frame[10..14], &4796.46f32.to_le_bytes());
        assert_eq!(&frame[14..16], &0u16.to_le_bytes(), "the plane");
        assert_eq!(&frame[16..], &[0u8; 10], "two bytes and eight zeros");
    }

    #[test]
    fn the_load_info_is_sixteen_bytes_and_names_the_map_and_the_district() {
        let frame = instance_load_info(1, 148, 0, 1, 0, 0);
        assert_eq!(frame.len(), 16, "the table's declared size");
        assert_eq!(&frame[2..6], &1u32.to_le_bytes(), "the agent");
        assert_eq!(&frame[6..8], &148u16.to_le_bytes(), "the map");
        assert_eq!(frame[8], 0, "an outpost runs the outpost's load");
        assert_eq!(&frame[9..13], &1u32.to_le_bytes(), "the district");
    }

    #[test]
    fn the_player_name_is_counted_and_the_finish_is_bare() {
        let frame = instance_load_player_name("Aeris");
        assert_eq!(&frame[..2], &[0x7D, 0x01]);
        assert_eq!(&frame[2..4], &5u16.to_le_bytes());
        assert_eq!(frame.len(), 4 + 10);
        assert_eq!(instance_load_finish(), vec![0x8E, 0x01]);
    }

    #[test]
    fn a_player_create_carries_the_agent_and_a_counted_name() {
        let frame = player_create(1, 1, 0x0310_0000, "Aeris");
        assert_eq!(&frame[..2], &[0x59, 0x00]);
        assert_eq!(&frame[2..6], &1u32.to_le_bytes(), "the id");
        assert_eq!(&frame[6..10], &1u32.to_le_bytes(), "the agent");
        assert_eq!(
            &frame[10..14],
            &0x0310_0000u32.to_le_bytes(),
            "the measured dword, on two maps and two names"
        );
        assert_eq!(&frame[14], &0u8, "the byte");
        assert_eq!(frame.len(), 25 + 10, "25 + 2n for a five-unit name");
    }

    #[test]
    fn the_load_head_is_the_traced_six_bytes() {
        // `7C 01 3F 3F 00 00` - the reference's own send log, one line per message.
        assert_eq!(
            instance_load_head(),
            vec![0x7C, 0x01, 0x3F, 0x3F, 0x00, 0x00]
        );
    }

    #[test]
    fn the_creation_start_is_the_header_alone() {
        assert_eq!(char_creation_start(), vec![0x89, 0x01]);
    }

    #[test]
    fn the_creation_block_is_the_two_messages_in_that_order() {
        assert_eq!(
            creation_load_block(),
            vec![0x7C, 0x01, 0x3F, 0x3F, 0x00, 0x00, 0x89, 0x01]
        );
    }

    #[test]
    fn the_grid_is_announced_in_fourteen_bytes() {
        let frame = map_init_start(64, 128, 16);
        assert_eq!(frame.len(), 14, "the table's declared size");
        assert_eq!(&frame[..2], &[0x8B, 0x00]);
        assert_eq!(&frame[2..6], &64u32.to_le_bytes(), "the width");
        assert_eq!(&frame[6..10], &128u32.to_le_bytes(), "the height");
        assert_eq!(&frame[10..14], &16u32.to_le_bytes(), "the byte count");
    }

    /// A slice is the `u16` count, the fixed 64-dword array and nothing else: 260 bytes, which is what
    /// the working implementation's own test asserts (`ASURA/crates/asura-world/src/cartography.rs:198-217`).
    #[test]
    fn a_grid_slice_is_the_count_and_the_fixed_array() {
        let blob = vec![0u8; 16];
        let frame = cartography_data(&blob);
        assert_eq!(frame.len(), 2 + 2 + CARTOGRAPHY_DATA_BYTES);
        assert_eq!(&frame[..2], &[0x8A, 0x00]);
        assert_eq!(
            u16::from_le_bytes([frame[2], frame[3]]),
            4,
            "four dwords arrived"
        );
        assert_eq!(&frame[4..4 + 16], &blob[..], "the slice");
        assert!(
            frame[4 + 16..].iter().all(|byte| *byte == 0),
            "the rest of the fixed array is zero"
        );
    }

    /// A slice longer than the array is cut to it, and one that does not end on a dword still counts
    /// the dword it starts.
    #[test]
    fn a_slice_is_cut_to_the_array_and_counted_in_dwords() {
        let frame = cartography_data(&vec![0xFFu8; CARTOGRAPHY_DATA_BYTES + 32]);
        assert_eq!(frame.len(), 2 + 2 + CARTOGRAPHY_DATA_BYTES);
        assert_eq!(u16::from_le_bytes([frame[2], frame[3]]), 64);
        let odd = cartography_data(&[1, 2, 3]);
        assert_eq!(
            u16::from_le_bytes([odd[2], odd[3]]),
            1,
            "three bytes, one dword"
        );
        assert_eq!(&odd[4..8], &[1, 2, 3, 0], "padded inside the dword");
    }

    /// The batch is the count and the pairs: map 148's own value is the one a live load carried
    /// (`recordings/client/feature/maps/pre-searing-load-3.jsonl`, seq 751).
    #[test]
    fn a_table_batch_is_a_count_and_its_pairs() {
        let frame = instance_table(&[(148, 0xBB14_F539), (145, 0xF15F_BD2A)]);
        assert_eq!(frame.len(), 2 + 1 + 12);
        assert_eq!(&frame[..2], &[0xA0, 0x01]);
        assert_eq!(frame[2], 2, "two pairs");
        assert_eq!(&frame[3..5], &148u16.to_le_bytes());
        assert_eq!(&frame[5..9], &0xBB14_F539u32.to_le_bytes());
        assert_eq!(&frame[9..11], &145u16.to_le_bytes());
        assert_eq!(&frame[11..15], &0xF15F_BD2Au32.to_le_bytes());
    }
}
