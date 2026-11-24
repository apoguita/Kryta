//! The player a load draws: the reference's own `Player` object, whose
//! `SendInitialPacketsBeforeAddToMap` and `SendInitialPacketsAfterAddToMap`
//! (`src/server/game/Entities/Player/Player.cpp`) are the load's message burst - not a message
//! handler's work and not the session's. This tree's `Agents/` is the reference's `Entities/`,
//! which is why the player entity's frames live beside the agent's body.
//!
//! Reference for every frame and its order: `ASURA/crates/asura-world/src/player.rs` (540 lines),
//! `handle_request_players` -> `player_block`. The reads that decide *what* the block says - the
//! character's row, R12's unlock, the entry point - stay where they belong (the session and the
//! nouns it asks); this file is the frames.

use crate::server::packets::{
    agent_packets, character_packets, instance_packets, misc_packets, party_packets,
};

/// The team token a load gives the client. Evidence: GWPS's own trace carried `0D F0 AD BA`
/// in `0x00F2` (`recordings/gwps/world/load-stream.log`), which is `ASURA`'s `PLAYER_TEAM_TOKEN`.
pub(crate) const PLAYER_TEAM_TOKEN: u32 = 0xBAAD_F00D;

/// The two dword lists a load sends at full length whether or not a character has used them: the
/// hard-mode mission and bonus bitmaps. An empty list is not the same message - it is an array the
/// client can index past the end of (`ASURA/crates/asura-world/src/player.rs`, `UNLOCKED_MAP_ZEROS`,
/// copied from GWPS's own send log: 31 zero words each).
pub(crate) const UNLOCKED_MAP_ZEROS: &[u32] = &[0u32; 31];

/// Full morale, which a fresh agent has (`ASURA/crates/asura-world/src/player.rs`, `MORALE_FULL`).
const MORALE_FULL: u32 = 100;

/// The character a load is told as: the two ids it is named by, where it stands, what it
/// wears, the instances it has reached, and what its progress record says about it.
pub(crate) struct Player {
    pub agent: u32,
    pub player: u32,
    pub position: (f32, f32),
    pub plane: u16,
    pub direction: (f32, f32),
    pub appearance: u32,
    /// The item ids the agent is drawn wearing, from the equipped bag, in its own order. A slot
    /// the character wears nothing in is zero.
    pub equipped: [u32; 9],
    /// The gold the character carries (R45: gold is per character).
    pub gold: u32,
    pub profession: u8,
    pub unlocked_professions: u32,
    pub level: u8,
    pub name: String,
}

impl Player {
    /// The `0x8091` answer: the inventory stream and the map's record, measured in one block. The
    /// manifest pair that ends it names the map in its second form
    /// (`ASURA/crates/asura-world/src/map.rs:22-58`; ASURA answers it in `item.rs`,
    /// `handle_request_items`, and the reference in `Player::SendItems`).
    pub(crate) fn items_answer(
        &self,
        inventory: &crate::creation::Inventory,
        map: &crate::world::MapEntry,
    ) -> Vec<Vec<u8>> {
        let mut frames = crate::creation::load_item_stream(inventory, self.gold);
        frames.push(misc_packets::hard_mode_unlocked(1));
        frames.push(character_packets::character_update_info(""));
        frames.push(misc_packets::map_update_current(map.id as u16, 0));
        frames.push(instance_packets::ready_for_map_spawn(0));
        frames.push(instance_packets::instance_manifest_phase(0));
        frames.push(instance_packets::instance_manifest_phase(1));
        frames.push(instance_packets::instance_manifest_done(2, 877, 0));
        frames.push(instance_packets::instance_manifest_phase(0));
        frames.push(instance_packets::instance_manifest_phase(1));
        frames.push(instance_packets::instance_manifest_done(
            0,
            map.id as u16,
            0,
        ));
        frames
    }

    /// The `0x8088` answer: the entry point and nothing else - `spawn_block` in the working
    /// implementation is exactly one `0x0196` frame (`ASURA/crates/asura-world/src/map.rs:13-20`),
    /// and the simulation's ticks come from the ticker, not from this answer. ASURA answers it in
    /// `agent::handle_request_spawn`.
    pub(crate) fn spawn_answer(&self, map: &crate::world::MapEntry) -> Vec<u8> {
        instance_packets::instance_load_spawn_point(
            map.file_id,
            self.position.0,
            self.position.1,
            self.plane,
        )
    }

    /// The `0x8090` answer: the whole block the client's players request is answered with, frame
    /// for frame and in GWPS's own order (`GameSrv_HandleInstanceLoadRequestPlayers`,
    /// `ASURA/crates/asura-world/src/player.rs`, `player_block`). It is one block, not a menu: the
    /// PvP unlocks and the unlockable-item table, the account features, the unlocked lists and the
    /// team token, the skills and the progress record, the visibility and player flags, the player
    /// and the agent it is drawn as, the agent's own properties and look, the party, and the
    /// message that ends the load.
    ///
    /// The players already in this copy of the map are introduced ahead of it, and that is the
    /// session's to do: it is the district that knows who else is there. So is the bitmap of
    /// reached instances - it is the character's rows, read with the arrival's own R12 write, and
    /// handed here as the list the block names.
    pub(crate) fn load_block(
        &self,
        settings: &crate::world::world_config::WorldConfig,
        load_ms: u32,
        unlocked: &[u32],
        titles: &crate::characters::titles::VanillaTitles,
        preferences: Option<&[u8]>,
    ) -> Vec<Vec<u8>> {
        let agent = self.agent;
        let player = self.player;
        let position = self.position;
        let plane = self.plane;
        // The entry point's own facing (its spawn point's angle, or a doorway arrival's), and
        // `(1, 0)` when it states none - `map_spawns.rs:44-53`.
        let direction = self.direction;
        let appearance = self.appearance;
        let equipped = self.equipped;
        let mut progress = [0u32; character_packets::FACTIONS_DWORDS];
        // Experience has no column yet; level and morale do, and the client shows 0 for a level
        // it is not told (`ASURA/crates/asura-world/src/player.rs`).
        progress[9] = u32::from(self.level);
        progress[10] = MORALE_FULL;
        let mut frames = Vec::with_capacity(40);
        frames.push(misc_packets::pvp_update_unlocked_skills(&[]));
        frames.push(misc_packets::pvp_update_unlocked_heroes(&[0xFFFF_FFFF; 8]));
        for item in crate::data_stores::pvp_items::PVP_ITEMS {
            frames.push(misc_packets::pvp_item_stream_add(item.id, item.params));
        }
        frames.push(misc_packets::pvp_item_stream_end());
        for (feature, param1, param2) in [
            (1u16, 10u16, 0u16),
            (99, 3, 0),
            (100, 5, 0),
            (101, 5, 5),
            (102, 2, 2),
            (111, 1, 0),
            (124, 1, 0),
            (125, 1, 0),
            (131, 1, 0),
        ] {
            frames.push(misc_packets::account_feature(feature, param1, param2));
        }
        frames.push(misc_packets::map_update_unlocked_list([
            &[],
            &[],
            UNLOCKED_MAP_ZEROS,
            UNLOCKED_MAP_ZEROS,
            unlocked,
        ]));
        frames.push(misc_packets::instance_loaded(PLAYER_TEAM_TOKEN));
        frames.push(agent_packets::agent_update_attribute_points(agent, 50, 50));
        frames.push(character_packets::player_update_profession(
            agent,
            self.profession,
            0,
            0,
        ));
        frames.push(character_packets::player_update_unlocked_professions(
            agent,
            self.unlocked_professions,
        ));
        frames.push(character_packets::skillbar_update(
            agent, &[0u32; 8], &[0u32; 8], 1,
        ));
        // The character's own preferences, when it has uploaded some (`0x8045`), directly before
        // the unlocked skills: that is where the retail server sends them on every load
        // (`recordings/client/session/login-to-spawn-20261124-180308.jsonl` seq 1253, between the
        // exploration grid's `0x008A` at 1252 and `0x00DB` at 1254). A character that never
        // uploaded is sent nothing here, as before; what the retail server sends such a character
        // is not recorded (`docs/port/09-parity-audit.md`, row `0x00B5`).
        if let Some(preferences) = preferences {
            frames.push(character_packets::preferences_blob(preferences));
        }
        frames.push(character_packets::update_unlocked_skills(&[]));
        frames.push(character_packets::character_update_factions(&progress));
        frames.push(misc_packets::char_visibility_flags(
            0xFFFF_FFFF,
            0xFFFF_FFFF,
        ));
        frames.push(misc_packets::player_update_flags(player as u16, 0, 7));
        // ASURA crates/asura-protocol/src/packets.rs:771-774 defines the packets
        // without a title sender. Build 39167 Northlands, seq 534-582:
        // definitions follow 0x00E9, 0x00EF and 0x003C, before load finish.
        frames.extend(crate::characters::titles::load_frames(titles));
        frames.push(agent_packets::agent_update_attributes(agent, &[0u32; 42]));
        frames.push(misc_packets::world_update_load_time(load_ms));
        frames.push(instance_packets::player_create(
            player, agent, appearance, &self.name,
        ));
        frames.push(character_packets::player_update_profession(
            agent,
            self.profession,
            0,
            0,
        ));
        frames.push(agent_packets::world_create_agent(
            &agent_packets::CreateAgent {
                agent_id: agent,
                model_id: agent_packets::CHAR_CLASS_PLAYER_BASE | player,
                agent_type: agent_packets::AGENT_TYPE_LIVING,
                position,
                plane,
                // The direction the entry point states - the spawn point's own angle, or
                // a doorway arrival's, or `(1, 0)` when the row states neither
                // (`ASURA/crates/asura-world/src/map_spawns.rs:44-53`).
                direction,
                speed_base: settings.agent_speed,
                player_team_token: PLAYER_TEAM_TOKEN,
            },
        ));
        frames.push(agent_packets::agent_update_morale(agent, MORALE_FULL));
        frames.push(agent_packets::agent_property_update_int(
            agent_packets::AGENT_PROPERTY_ENERGY,
            agent,
            settings.agent_energy,
        ));
        frames.push(agent_packets::agent_property_update_int(
            agent_packets::AGENT_PROPERTY_HEALTH,
            agent,
            settings.agent_health,
        ));
        frames.push(agent_packets::agent_property_update_float(
            agent_packets::AGENT_PROPERTY_ENERGY_REGEN,
            agent,
            settings.agent_energy_regen,
        ));
        frames.push(agent_packets::agent_property_update_int(
            agent_packets::AGENT_PROPERTY_PUBLIC_LEVEL,
            agent,
            u32::from(self.level),
        ));
        frames.push(agent_packets::agent_initial_effects(agent, 0));
        frames.push(misc_packets::update_agent_visual_equipment(agent, equipped));
        frames.push(misc_packets::agent_display_cape(agent, 1));
        frames.push(agent_packets::world_update_controlled_agent(agent, 3));
        frames.push(party_packets::party_create(party_packets::PARTY_ID));
        frames.push(party_packets::party_player_add(
            party_packets::PARTY_ID,
            player as u16,
            1,
        ));
        frames.push(party_packets::party_member_stream_end(
            party_packets::PARTY_ID,
        ));
        frames.push(character_packets::character_update_party(
            party_packets::PARTY_ID,
            1,
        ));
        frames.push(instance_packets::instance_load_finish());
        frames.push(crate::server::packets::title_packets::rank_display(
            player as u16,
            titles.initial_display_rank(),
        ));
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Player {
        Player {
            agent: 1,
            player: 1,
            position: (0.0, 0.0),
            plane: 0,
            direction: (1.0, 0.0),
            appearance: 0,
            equipped: [0; 9],
            gold: 0,
            profession: 1,
            unlocked_professions: 2,
            level: 1,
            name: "Prefs One".to_string(),
        }
    }

    fn headers(frames: &[Vec<u8>]) -> Vec<u16> {
        frames
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect()
    }

    #[test]
    fn the_preferences_go_directly_before_the_unlocked_skills() {
        // The retail order: `0x00B5` at seq 1253, `0x00DB` at seq 1254
        // (`recordings/client/session/login-to-spawn-20261124-180308.jsonl`).
        let settings = crate::world::world_config::WorldConfig::default();
        let titles = crate::characters::titles::VanillaTitles::load(
            "../config/World/Vanilla/titles.json",
        )
        .expect("the titles file");
        let blob = [0x05u8, 0x00, 0x66, 0x11];
        let with = player().load_block(&settings, 0, &[], &titles, Some(&blob));
        let sent = headers(&with);
        let at = sent
            .iter()
            .position(|&header| header == character_packets::PREFERENCES_BLOB)
            .expect("the block carries the preferences");
        assert_eq!(sent[at + 1], character_packets::UPDATE_UNLOCKED_SKILLS);
        assert_eq!(with[at], character_packets::preferences_blob(&blob));

        // A character that never uploaded is sent the block without it, and nothing else moves.
        let without = player().load_block(&settings, 0, &[], &titles, None);
        assert!(!headers(&without).contains(&character_packets::PREFERENCES_BLOB));
        let mut expected = with.clone();
        expected.remove(at);
        assert_eq!(without, expected);
    }
}
