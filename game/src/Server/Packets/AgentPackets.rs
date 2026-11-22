//! The agent message family: an agent's creation, its properties and its attributes.
//!
//! Reference: `src/server/game/Server/Packets/MiscPackets` and the agent handlers in
//! `PlayerHandler`/`WorldSession`. A body in this game is an *agent* - the client's word for
//! anything that can be seen, a player's own included - so a player's health and energy arrive as
//! agent properties. One file per family, as this tree does it.
//!
//! Every layout here is this client's, measured; each names where its bytes came from. The creation
//! frame (`GAME_SMSG_WORLD_CREATE_AGENT`, 99 bytes) is ported from the reference's own builder as
//! `ASURA` transcribes it; the map's content - which agents a map has - arrives with plan step 6.

/// `GAME_SMSG_AGENT_PROPERTY_UPDATE_INT`: one integer property of one agent.
pub const AGENT_PROPERTY_UPDATE_INT: u16 = 0x009F;
/// `GAME_SMSG_AGENT_UPDATE_ATTRIBUTE_POINTS`: attribute points spent and unspent.
pub const AGENT_UPDATE_ATTRIBUTE_POINTS: u16 = 0x0037;
/// `GAME_SMSG_AGENT_UPDATE_ATTRIBUTES`: the agent's attribute ranks, one dword each.
pub const AGENT_UPDATE_ATTRIBUTES: u16 = 0x003A;
/// `GAME_SMSG_AGENT_UPDATE_EFFECTS` (`0x00F1`): an agent's effect word, updated.
pub const AGENT_UPDATE_EFFECTS: u16 = 0x00F1;
/// `GAME_SMSG_WORLD_CREATE_AGENT` (`0x0020`): one agent created, 99 bytes fixed.
pub const WORLD_CREATE_AGENT: u16 = 0x0020;
/// `GAME_SMSG_AGENT_PROPERTY_UPDATE_FLOAT` (`0x00A2`): one float property of one agent.
pub const AGENT_PROPERTY_UPDATE_FLOAT: u16 = 0x00A2;
/// `GAME_SMSG_AGENT_UPDATE_MORALE` (`0x009C`): one agent's morale as a percentage.
pub const AGENT_UPDATE_MORALE: u16 = 0x009C;
/// `GAME_SMSG_AGENT_INITIAL_EFFECTS` (`0x00F0`): the effect bitmask an agent starts with.
pub const AGENT_INITIAL_EFFECTS: u16 = 0x00F0;
/// `GAME_SMSG_AGENT_UPDATE_PROFESSION` (`0x00A6`): an agent's primary and secondary profession.
pub const AGENT_UPDATE_PROFESSION: u16 = 0x00A6;
/// `GAME_SMSG_WORLD_UPDATE_CONTROLLED_AGENT` (`0x0022`): which agent this client drives.
pub const WORLD_UPDATE_CONTROLLED_AGENT: u16 = 0x0022;
/// `GAME_SMSG_AGENT_PLAYER_DIE` (`0x002D`): the agent is dead.
pub const AGENT_PLAYER_DIE: u16 = 0x002D;
/// `GAME_SMSG_AGENT_UPDATE_FLAGS` (`0x0026`): the agent's flags word.
pub const AGENT_UPDATE_FLAGS: u16 = 0x0026;
/// `GAME_SMSG_AGENT_UPDATE_DIRECTION` (`0x0025`): an agent's facing, as a unit vector.
pub const AGENT_UPDATE_DIRECTION: u16 = 0x0025;
/// `GAME_SMSG_AGENT_STOP_MOVING` (`0x0028`): the agent is no longer walking.
pub const AGENT_STOP_MOVING: u16 = 0x0028;
/// `GAME_SMSG_AGENT_MOVE_TO_POINT` (`0x0029`): the agent is walking to a destination.
pub const AGENT_MOVE_TO_POINT: u16 = 0x0029;
/// `GAME_SMSG_AGENT_UPDATE_SPEED` (`0x002B`): the agent's movement type and speed factor.
pub const AGENT_UPDATE_SPEED: u16 = 0x002B;
/// `GAME_SMSG_WORLD_REMOVE_AGENT` (`0x0021`): an agent has left the map.
pub const WORLD_REMOVE_AGENT: u16 = 0x0021;
/// `GAME_SMSG_WORLD_UPDATE_CONTROLLED_AGENT` (`0x0022`): which agent this client drives.
/// `GAME_SMSG_AGENT_UPDATE_POSITION` (`0x002C`): where an agent is, sent to everyone else.
pub const AGENT_UPDATE_POSITION: u16 = 0x002C;

/// The property an agent's energy is reported through.
///
/// Evidence: `ASURA`'s `agent.rs`, which records the reference's own numbers: energy 41, health 42,
/// and 64 for the third property the creation screen is sent.
pub const AGENT_PROPERTY_ENERGY: u32 = 41;
/// The property an agent's current energy is reported through, as a fraction of its maximum.
///
/// Preforged `src/runtime/gw_vitals.h:64-78`, `packets_2022`: the capacity precedes the fraction,
/// and the order is `41`, `42`, `33`, `34`. Without the two fractions the client has maxima but no
/// current pool, which is a bar with no fill.
pub const AGENT_PROPERTY_ENERGY_FRACTION: u32 = 33;
/// The property an agent's current health is reported through, as a fraction of its maximum.
///
/// See [`AGENT_PROPERTY_ENERGY_FRACTION`]; Preforged sends both after the maxima.
pub const AGENT_PROPERTY_HEALTH_FRACTION: u32 = 34;
/// The property an agent's health is reported through.
pub const AGENT_PROPERTY_HEALTH: u32 = 42;
/// The third property the creation screen is sent, always zero there.
pub const AGENT_PROPERTY_VALUE64: u32 = 64;
/// The property an agent's public level is reported through.
///
/// The party window draws it, and it is not a field of the create message: `ASURA` reads the reference's
/// own property 36 as `PublicLevel` and the client stores it in its agent record
/// (`crates/asura-world/src/player.rs`, `AGENT_PROPERTY_PUBLIC_LEVEL`).
pub const AGENT_PROPERTY_PUBLIC_LEVEL: u32 = 36;
/// The property an agent's energy regeneration is reported through, as a float.
pub const AGENT_PROPERTY_ENERGY_REGEN: u32 = 43;
/// The model word a player agent is drawn with: this base or'd with the player's id.
///
/// Evidence: `ASURA/crates/asura-world/src/player.rs` (`CHAR_CLASS_PLAYER_BASE`), and the reference's
/// own create message in this tree's capture (`recordings/client/feature/maps/pre-searing-load-3.jsonl`
/// seq 221: agent 40 is drawn with model `0x30000035`).
pub const CHAR_CLASS_PLAYER_BASE: u32 = 0x3000_0000;
/// `1` for a living body, `4` for a dropped item.
pub const AGENT_TYPE_LIVING: u8 = 1;

/// One agent, in the field order `GAME_SMSG_WORLD_CREATE_AGENT` writes it.
///
/// Source: `ASURA`'s `crates/asura-protocol/src/game/mod.rs`, its `CreateAgent` struct and
/// `world_create_agent` builder, which transcribe the reference's `GameSrv_BuildCreateAgentMsg`
/// field for field. The one field that differs between a player and an item on the ground is
/// `agent_type`; the rest is what the reference fills from its `GmAgent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreateAgent {
    /// The id the rest of the stream refers to this body by.
    pub agent_id: u32,
    /// What the client draws: a character class word for a player, an item model for a drop.
    pub model_id: u32,
    /// `1` for a living body, `4` for a dropped item.
    pub agent_type: u8,
    /// Where the body stands, in the map's own units.
    pub position: (f32, f32),
    /// The pathing plane, not a height.
    pub plane: u16,
    /// Which way it faces, as a unit vector.
    pub direction: (f32, f32),
    /// Walking speed the client uses until a `0x002B` says otherwise.
    pub speed_base: f32,
    /// The team the body belongs to; 0 for a neutral body.
    pub player_team_token: u32,
}

/// `0x0020`: one agent created, 99 bytes fixed - the frame that puts a body in a map.
///
/// The field order and the five constants are the reference's own (`GameSrv_BuildCreateAgentMsg` as
/// ASURA transcribes it): the byte after the type is 5, the byte after the direction is 1, the float
/// after the speed is 1.0, and the dword after it is `0x41400000` - "12.0 reinterpreted", as the
/// reference writes it. The two `f32::INFINITY` pairs are what it sends for the unmodelled vectors.
/// They are carried as measured rather than reasoned about.
pub fn world_create_agent(agent: &CreateAgent) -> Vec<u8> {
    let mut body = Vec::with_capacity(97);
    body.extend_from_slice(&agent.agent_id.to_le_bytes());
    body.extend_from_slice(&agent.model_id.to_le_bytes());
    body.push(agent.agent_type);
    body.push(5); // h000B
    body.extend_from_slice(&agent.position.0.to_le_bytes());
    body.extend_from_slice(&agent.position.1.to_le_bytes());
    body.extend_from_slice(&agent.plane.to_le_bytes());
    body.extend_from_slice(&agent.direction.0.to_le_bytes());
    body.extend_from_slice(&agent.direction.1.to_le_bytes());
    body.push(1); // h001E
    body.extend_from_slice(&agent.speed_base.to_le_bytes());
    body.extend_from_slice(&1.0f32.to_le_bytes()); // h0023
    body.extend_from_slice(&0x4140_0000u32.to_le_bytes()); // h0027
    body.extend_from_slice(&agent.player_team_token.to_le_bytes());
    for _ in 0..5 {
        body.extend_from_slice(&0u32.to_le_bytes()); // h002F..h003F
    }
    body.extend_from_slice(&0f32.to_le_bytes()); // h0043.x
    body.extend_from_slice(&0f32.to_le_bytes()); // h0043.y
    body.extend_from_slice(&f32::INFINITY.to_le_bytes()); // h004B.x
    body.extend_from_slice(&f32::INFINITY.to_le_bytes()); // h004B.y
    body.extend_from_slice(&0u16.to_le_bytes()); // h0053
    body.extend_from_slice(&0u32.to_le_bytes()); // h0055
    body.extend_from_slice(&f32::INFINITY.to_le_bytes()); // h0059.x
    body.extend_from_slice(&f32::INFINITY.to_le_bytes()); // h0059.y
    body.extend_from_slice(&0u16.to_le_bytes()); // h0061
    frame(WORLD_CREATE_AGENT, &body)
}

/// A frame: the `u16` header and then the body.
fn frame(header: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// A `u16` count followed by that many dwords - the counted field a list is sent as.
fn dwords(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + values.len() * 4);
    out.extend_from_slice(&(values.len() as u16).to_le_bytes());
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// `0x009F`: property id, agent id, value - three dwords, fourteen bytes.
pub fn agent_property_update_int(property: u32, agent: u32, value: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&property.to_le_bytes());
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&value.to_le_bytes());
    frame(AGENT_PROPERTY_UPDATE_INT, &body)
}

/// `0x0037`: the agent, then the unspent and the spent points, one byte each.
pub fn agent_update_attribute_points(agent: u32, unused: u8, used: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(6);
    body.extend_from_slice(&agent.to_le_bytes());
    body.push(unused);
    body.push(used);
    frame(AGENT_UPDATE_ATTRIBUTE_POINTS, &body)
}

/// `0x003A`: the agent, then a counted list of its attribute ranks.
pub fn agent_update_attributes(agent: u32, attributes: &[u32]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + 2 + attributes.len() * 4);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&dwords(attributes));
    frame(AGENT_UPDATE_ATTRIBUTES, &body)
}

/// `0x00A2`: one float property of an agent - the property, the agent, the value.
///
/// Source: the game table's row (`GAME_SMSG_AGENT_PROPERTY_UPDATE_FLOAT`, 14 bytes) and `ASURA`'s
/// `agent_property_update_float` builder, field for field.
pub fn agent_property_update_float(property: u32, agent: u32, value: f32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&property.to_le_bytes());
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&value.to_le_bytes());
    frame(AGENT_PROPERTY_UPDATE_FLOAT, &body)
}

/// `0x009C`: one agent's morale as a percentage, 100 being the unmodified value.
///
/// Source: `ASURA`'s `agent_update_morale` builder and the trace it cites - a live server sends one
/// per body at the load (`9C 00 | 1F 00 00 00 | 64 00 00 00`, agent 0x1F at 100).
pub fn agent_update_morale(agent: u32, morale: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&morale.to_le_bytes());
    frame(AGENT_UPDATE_MORALE, &body)
}

/// `0x00F0`: the effect bitmask an agent starts with.
///
/// Source: the game table's row (`GAME_SMSG_AGENT_INITIAL_EFFECTS`, 10 bytes) and `ASURA`'s
/// `agent_initial_effects` builder: the agent and the effects, two dwords.
pub fn agent_initial_effects(agent: u32, effects: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&effects.to_le_bytes());
    frame(AGENT_INITIAL_EFFECTS, &body)
}

/// `0x00A6`: an agent's primary and secondary profession, ahead of its create.
///
/// Reference: `ASURA/crates/asura-world/src/instance.rs`, `introduction`, and its builder
/// `agent_update_profession` (an agent dword and two bytes). A live introduction sends it for every
/// other player: `pre-searing-load-3.jsonl` seq 215 carries agent 0x28, primary 2, secondary 0 before
/// that agent's `0x0020`.
pub fn agent_update_profession(agent: u32, primary: u8, secondary: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(6);
    body.extend_from_slice(&agent.to_le_bytes());
    body.push(primary);
    body.push(secondary);
    frame(AGENT_UPDATE_PROFESSION, &body)
}

/// `0x0022`: which agent this client drives - the agent, and the constant the reference sends.
///
/// Source: the game table's row (`GAME_SMSG_WORLD_UPDATE_CONTROLLED_AGENT`, 10 bytes) and `ASURA`'s
/// `world_update_controlled_agent` builder, whose own words are: "the reference sends 3 as the
/// second field and nothing reads it back". A load sends it once, for the player's own agent.
pub fn world_update_controlled_agent(agent: u32, constant: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&constant.to_le_bytes());
    frame(WORLD_UPDATE_CONTROLLED_AGENT, &body)
}

/// `GAME_SMSG_AGENT_UPDATE_DIRECTION` (`0x0025`): 15 bytes - the agent, a unit vector, a facing byte.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1391`, `agent_update_direction`.
pub fn agent_update_direction(agent: u32, direction: (f32, f32), facing: u8) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(13);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&direction.0.to_le_bytes());
    body.extend_from_slice(&direction.1.to_le_bytes());
    body.push(facing);
    (AGENT_UPDATE_DIRECTION, frame(AGENT_UPDATE_DIRECTION, &body))
}

/// `GAME_SMSG_AGENT_UPDATE_SPEED` (`0x002B`): 11 bytes - the agent, a speed factor, a facing byte.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1405`, `agent_update_speed`. The factor is the
/// movement type's own: 0.66 for 4-6, 0.75 for 7-8, 1.0 otherwise (`agent.rs:234`).
pub fn agent_update_speed(agent: u32, speed: f32, facing: u8) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(9);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&speed.to_le_bytes());
    body.push(facing);
    (AGENT_UPDATE_SPEED, frame(AGENT_UPDATE_SPEED, &body))
}

/// `GAME_SMSG_AGENT_STOP_MOVING` (`0x0028`): 6 bytes - the agent alone.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1475`, `agent_stop_moving`.
pub fn agent_stop_moving(agent: u32) -> (u16, Vec<u8>) {
    (
        AGENT_STOP_MOVING,
        frame(AGENT_STOP_MOVING, &agent.to_le_bytes()),
    )
}

/// `GAME_SMSG_AGENT_MOVE_TO_POINT` (`0x0029`) in the two-byte-header form: 18 bytes - the agent, the
/// destination, the destination's plane and the plane it is walking from.
///
/// `GameSrv_MoveAgentToPoint`; `ASURA/crates/asura-protocol/src/game/mod.rs:1375`,
/// `agent_move_to_point`. Its `_live` form widens the last two fields to dwords and carries the
/// waypoint's trap id instead; the working implementation defines that form and sends this one
/// (`agent.rs:256`, `:271`, `:829`, `instance.rs:376`).
pub fn agent_move_to_point(
    agent: u32,
    destination: (f32, f32),
    plane: u16,
    current_plane: u16,
) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(16);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&destination.0.to_le_bytes());
    body.extend_from_slice(&destination.1.to_le_bytes());
    body.extend_from_slice(&plane.to_le_bytes());
    body.extend_from_slice(&current_plane.to_le_bytes());
    (AGENT_MOVE_TO_POINT, frame(AGENT_MOVE_TO_POINT, &body))
}

/// `GAME_SMSG_AGENT_MOVE_TO_POINT`, the live form: 24 bytes, `29 00 00 00 | agent | x | y | trap |
/// plane`.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1422`, `agent_move_to_point_live`. Measured from the
/// live capture: the upper two bytes of the four-byte game header are zero, then the agent, the
/// position, the destination waypoint's trap id (zero when the point is not on a trap) and the plane
/// as a dword. The working implementation defines it and does not send it; so does this tree.
pub fn agent_move_to_point_live(
    agent: u32,
    destination: (f32, f32),
    trap_id: u32,
    plane: u32,
) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(22);
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&destination.0.to_le_bytes());
    body.extend_from_slice(&destination.1.to_le_bytes());
    body.extend_from_slice(&trap_id.to_le_bytes());
    body.extend_from_slice(&plane.to_le_bytes());
    (AGENT_MOVE_TO_POINT, frame(AGENT_MOVE_TO_POINT, &body))
}

/// `GAME_SMSG_AGENT_UPDATE_DIRECTION`, the live form: 20 bytes, `25 00 00 00 | agent | dx | dy | 1`.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1441`, `agent_update_direction_live`.
pub fn agent_update_direction_live(agent: u32, direction: (f32, f32)) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(18);
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&direction.0.to_le_bytes());
    body.extend_from_slice(&direction.1.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    (AGENT_UPDATE_DIRECTION, frame(AGENT_UPDATE_DIRECTION, &body))
}

/// `GAME_SMSG_AGENT_UPDATE_SPEED`, the live form: 16 bytes, `2b 00 00 00 | agent | 1.0 | mode`.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1454`, `agent_update_speed_live`. The capture shows
/// 1.0 in every instance, mode 1 when a walk starts and mode 9 when it is cancelled.
pub fn agent_update_speed_live(agent: u32, speed: f32, mode: u32) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(14);
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&speed.to_le_bytes());
    body.extend_from_slice(&mode.to_le_bytes());
    (AGENT_UPDATE_SPEED, frame(AGENT_UPDATE_SPEED, &body))
}

/// `GAME_SMSG_AGENT_UPDATE_EFFECTS` (`0x00F1`): 10 bytes - the agent and an effect word.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs:1255`, `agent_update_effects`. A live server sends it
/// for the travelling player immediately before a redirect, with the word `0x100`
/// (`ASURA/crates/asura-world/src/travel.rs:19`, `TRAVEL_EFFECT`).
pub fn agent_update_effects(agent: u32, effects: u32) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&effects.to_le_bytes());
    (AGENT_UPDATE_EFFECTS, frame(AGENT_UPDATE_EFFECTS, &body))
}

/// `GAME_SMSG_WORLD_REMOVE_AGENT` (`0x0021`): **6 bytes** - a two-byte header and the agent
/// (Headquarter's `GAME_SERVER_0033`).
///
/// `GameSrv_BroadcastAgentRemove`; ported from `ASURA/crates/asura-protocol/src/game/mod.rs:2181-2186`
/// (ASURA `071735e`). The `21 00 00 00 | <agent id>` capture this used to copy as eight bytes is the
/// struct layer.
pub fn world_remove_agent(agent: u32) -> (u16, Vec<u8>) {
    (
        WORLD_REMOVE_AGENT,
        frame(WORLD_REMOVE_AGENT, &agent.to_le_bytes()),
    )
}

/// `0x002D`: the agent is dead. ASURA `agent_player_die`.
pub fn agent_player_die(agent: u32) -> Vec<u8> {
    frame(AGENT_PLAYER_DIE, &agent.to_le_bytes())
}

/// `0x0026`: the agent's flags word. A resign death sends `4` right after `0x002D`.
pub fn agent_update_flags(agent: u32, flags: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    body.extend_from_slice(&agent.to_le_bytes());
    body.push(flags);
    frame(AGENT_UPDATE_FLAGS, &body)
}

/// `GAME_SMSG_AGENT_UPDATE_POSITION` (`0x002C`): 16 bytes - the agent, where it is, and the plane.
///
/// `GameSrv_BroadcastAgentPosition`; `ASURA/crates/asura-protocol/src/game/mod.rs:1481`,
/// `agent_update_position`. Sent to everyone *else*: the client that reported the position already
/// knows it. The working implementation also records that a wrong `0x002C` disconnected the client
/// with `007` once and teleported it into a wall once, which is why its own walks are answered by
/// sending nothing when the path is refused.
pub fn agent_update_position(agent: u32, position: (f32, f32), plane: u16) -> (u16, Vec<u8>) {
    let mut body = Vec::with_capacity(14);
    body.extend_from_slice(&agent.to_le_bytes());
    body.extend_from_slice(&position.0.to_le_bytes());
    body.extend_from_slice(&position.1.to_le_bytes());
    body.extend_from_slice(&plane.to_le_bytes());
    (AGENT_UPDATE_POSITION, frame(AGENT_UPDATE_POSITION, &body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six bytes, the client's table (`ASURA/crates/asura-protocol/src/game/mod.rs:2181-2186`).
    #[test]
    fn an_agent_is_removed_in_six_bytes() {
        assert_eq!(
            world_remove_agent(0x1234).1,
            vec![0x21, 0x00, 0x34, 0x12, 0, 0]
        );
    }

    #[test]
    fn a_property_is_three_dwords_in_the_measured_order() {
        // `property`, `agent`, `value` - the order the reference's own builder writes them.
        assert_eq!(
            agent_property_update_int(AGENT_PROPERTY_HEALTH, 1, 100),
            vec![0x9F, 0x00, 42, 0, 0, 0, 1, 0, 0, 0, 100, 0, 0, 0]
        );
    }

    #[test]
    fn a_profession_is_the_agent_and_two_bytes() {
        assert_eq!(
            agent_update_profession(0x28, 2, 0),
            vec![0xA6, 0x00, 0x28, 0, 0, 0, 2, 0],
            "eight bytes, the table's declared size"
        );
    }

    #[test]
    fn the_attribute_points_are_two_bytes_after_the_agent() {
        assert_eq!(
            agent_update_attribute_points(1, 50, 50),
            vec![0x37, 0x00, 1, 0, 0, 0, 50, 50]
        );
    }

    #[test]
    fn the_attributes_are_a_counted_list() {
        let frame = agent_update_attributes(1, &[0u32; 42]);
        assert_eq!(frame.len(), 2 + 4 + 2 + 42 * 4);
        assert_eq!(&frame[..6], &[0x3A, 0x00, 1, 0, 0, 0]);
        assert_eq!(&frame[6..8], &42u16.to_le_bytes());
    }

    #[test]
    fn a_float_property_is_the_property_the_agent_and_the_value() {
        let frame = agent_property_update_float(0x2B, 1, 0.5);
        assert_eq!(frame.len(), 14, "the table's own size");
        assert_eq!(&frame[..2], &[0xA2, 0x00]);
        assert_eq!(&frame[2..6], &0x2Bu32.to_le_bytes(), "the property");
        assert_eq!(&frame[6..10], &1u32.to_le_bytes(), "the agent");
        assert_eq!(&frame[10..14], &0.5f32.to_le_bytes());
    }

    #[test]
    fn a_morale_is_the_agent_and_its_percentage() {
        // The trace's own frame: `9C 00 | 1F 00 00 00 | 64 00 00 00` - agent 0x1F, 100.
        assert_eq!(
            agent_update_morale(0x1F, 100),
            vec![0x9C, 0x00, 0x1F, 0x00, 0x00, 0x00, 100, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn the_initial_effects_are_the_agent_and_the_bitmask() {
        let frame = agent_initial_effects(2, 0x400);
        assert_eq!(frame.len(), 10, "the table's own size");
        assert_eq!(&frame[..2], &[0xF0, 0x00]);
        assert_eq!(&frame[2..6], &2u32.to_le_bytes(), "the agent");
        assert_eq!(&frame[6..10], &0x400u32.to_le_bytes(), "the effects");
    }

    #[test]
    fn the_controlled_agent_is_the_agent_and_the_reference_s_constant() {
        // The reference sends 3 as the second field (`ASURA`'s builder says so in its own words).
        assert_eq!(
            world_update_controlled_agent(1, 3),
            vec![0x22, 0x00, 1, 0, 0, 0, 3, 0, 0, 0]
        );
    }

    #[test]
    fn an_agent_is_the_reference_s_ninety_nine_bytes() {
        // The reference's `GameSrv_BuildCreateAgentMsg` order, constant by constant: the id and the
        // model, the type and the 5, the position, the plane, the direction, the 1 and the speed,
        // the 1.0, the 0x41400000, the team, five zero dwords, the two zero floats, the infinite
        // vectors, and the closing words.
        let agent = CreateAgent {
            agent_id: 7,
            model_id: 0x2710,
            agent_type: 1,
            position: (9541.87, 4796.46),
            plane: 0,
            direction: (0.0, 1.0),
            speed_base: 288.0,
            player_team_token: 2,
        };
        let frame = world_create_agent(&agent);
        assert_eq!(frame.len(), 99, "the table's own size");
        assert_eq!(&frame[..2], &[0x20, 0x00]);
        assert_eq!(&frame[2..6], &7u32.to_le_bytes(), "the agent");
        assert_eq!(&frame[6..10], &0x2710u32.to_le_bytes(), "the model");
        assert_eq!(frame[10], 1, "the type");
        assert_eq!(frame[11], 5, "h000B");
        assert_eq!(&frame[12..16], &9541.87f32.to_le_bytes());
        assert_eq!(&frame[16..20], &4796.46f32.to_le_bytes());
        assert_eq!(&frame[20..22], &0u16.to_le_bytes(), "the plane");
        assert_eq!(&frame[22..26], &0f32.to_le_bytes());
        assert_eq!(&frame[26..30], &1f32.to_le_bytes());
        assert_eq!(frame[30], 1, "h001E");
        assert_eq!(&frame[31..35], &288f32.to_le_bytes(), "the walking speed");
        assert_eq!(&frame[35..39], &1f32.to_le_bytes(), "h0023");
        assert_eq!(&frame[39..43], &0x4140_0000u32.to_le_bytes(), "h0027");
        assert_eq!(&frame[43..47], &2u32.to_le_bytes(), "the team");
        for offset in [47usize, 51, 55, 59, 63] {
            assert_eq!(
                &frame[offset..offset + 4],
                &0u32.to_le_bytes(),
                "h002F..h003F"
            );
        }
        for offset in [67usize, 71] {
            assert_eq!(&frame[offset..offset + 4], &0f32.to_le_bytes());
        }
        for offset in [75usize, 79, 89, 93] {
            assert_eq!(
                f32::from_le_bytes(frame[offset..offset + 4].try_into().unwrap()),
                f32::INFINITY
            );
        }
        assert_eq!(&frame[83..85], &0u16.to_le_bytes());
        assert_eq!(&frame[85..89], &0u32.to_le_bytes());
        assert_eq!(&frame[97..99], &0u16.to_le_bytes());
    }
}
