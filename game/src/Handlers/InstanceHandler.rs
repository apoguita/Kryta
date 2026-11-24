//! The instance family: the three asks a map load answers - `GAME_CMSG_INSTANCE_LOAD_REQUEST_ITEMS`
//! (`0x8091`), `GAME_CMSG_INSTANCE_LOAD_REQUEST_SPAWN` (`0x8088`) and
//! `GAME_CMSG_INSTANCE_LOAD_REQUEST_PLAYERS` (`0x8090`).
//!
//! Reference: `src/server/game/Handlers/`, whose members read a message and call into the nouns. The
//! behaviours are ASURA's: `ASURA/crates/asura-world/src/map.rs:13-58` (the items answer and the entry
//! point) and `ASURA/crates/asura-world/src/player.rs` (`handle_request_players` -> `player_block`).
//!
//! What a load decided before the first ask - the map, the character's row, its inventory, where it
//! stands and the player it is told as - is the session's `MapLoad`; the frames are `agents::Player`'s.
//! Each handler is an `impl WorldSession` method the session's dispatch calls by header.

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::agents::player::{PLAYER_TEAM_TOKEN, UNLOCKED_MAP_ZEROS};
use crate::server::packets::{item_packets, misc_packets, title_packets};
use crate::server::world_session::{Connection, MapLoad, WorldSession};

impl WorldSession {
    /// The items ask: the inventory stream and the map's record, measured in one block. The manifest
    /// pair that ends it names the map in its second form.
    pub(crate) async fn answer_load_request_items(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        load: &MapLoad<'_>,
    ) {
        let frames = load.state.items_answer(&load.inventory, load.map);
        self.send(io, peer, crypt, &frames, "the inventory and the map's record")
            .await;
    }

    /// The spawn request: the entry point chosen before the first ask - a point of this map's own
    /// list, scattered (R13), a doorway's landing or the saved position.
    ///
    /// Answers whether the connection goes on: a map that cannot be entered ends it.
    pub(crate) async fn answer_load_request_spawn(
        &self,
        state: &mut Connection,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        load: &MapLoad<'_>,
    ) -> bool {
        let map = load.map;
        let player = &load.state;
        let point = load.entry;
        let frames = match point {
            Some(point) => {
                self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: the character stands at {:.2}, {:.2} (plane {}) on map {}.",
                        point.0, point.1, point.2, map.id
                    ),
                );
                state.place_agent(crate::agents::Agent::new(
                    state.player_id(),
                    (point.0, point.1),
                    point.2,
                    // A doorway crossing hands its arrival's facing over, and
                    // every other entry faces the direction its spawn point
                    // states (`Standing::at_spawn`, `agent.rs:460-476`; a point
                    // with no angle keeps `(1, 0)`, `map_spawns.rs:44-53`).
                    point.3,
                    self.world.settings().agent_speed,
                ));
                state.start_portal_position((point.0, point.1));
                vec![player.spawn_answer(map)]
            }
            None => {
                // A map with no entry spawn and no doorway arrival cannot be entered, and
                // the working implementation says so by refusing the load rather than
                // leaving the client on a loading screen
                // (`ASURA/crates/asura-world/src/agent.rs:513-527`). The connection ends
                // with the reason, exactly as there.
                self.say(
                    LogLevel::Error,
                    format!(
                        "Gameplay connection from {peer}: map {} '{}' has no configured entry spawn or doorway arrival; the load is refused.",
                        map.id, map.name
                    ),
                );
                return false;
            }
        };
        // The answer is the entry point and nothing else: `spawn_block` in the working
        // implementation is exactly one `0x0196` frame
        // (`ASURA/crates/asura-world/src/map.rs:13-20`), and the simulation's ticks
        // come from the ticker, not from this answer: a `0x001E` appended here would
        // be one message the working implementation never sends.
        self.send(io, peer, crypt, &frames, "the entry position").await;
        true
    }

    /// The players request: the whole block GWPS answers it with, frame for frame and in its own
    /// order (`GameSrv_HandleInstanceLoadRequestPlayers`, ported from
    /// `ASURA/crates/asura-world/src/player.rs`, `player_block`). It is one block, not a menu: the PvP
    /// unlocks and the unlockable-item table, the account features, the unlocked lists and the team
    /// token, the skills and the progress record, the visibility and player flags, the player and the
    /// agent it is drawn as, the agent's own properties and look, the party, and the message that
    /// ends the load.
    pub(crate) async fn answer_load_request_players(
        &self,
        state: &mut Connection,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        load: &MapLoad<'_>,
    ) {
        let map = load.map;
        let sheet = load.sheet;
        let inventory = &load.inventory;
        let told = &load.state;
        let (agent, player) = (told.agent, told.player);
        let (position, plane) = (told.position, told.plane);
        let (appearance, equipped) = (told.appearance, told.equipped);
        // The instance this character is now standing in is **reached**, so it is
        // unlocked - the walk sets the bit and the bit never gates the walk
        // (`ASURA/crates/asura-world/src/player.rs:442-462`). Written before the bitmap
        // is read, so the list the client gets names the map it is in even on the first
        // arrival; the creation instance is not a place a character "reaches" and sets
        // nothing.
        let mut reached_here = false;
        // The instances the character has reached, as the client addresses them: bit
        // `n % 32` of word `n / 32`.
        let mut unlocked = UNLOCKED_MAP_ZEROS.to_vec();
        if let Some(pool) = self.characters.as_ref() {
            let characters = crate::characters::DatabaseCharacters::new(
                std::sync::Arc::clone(pool),
                self.log.clone(),
            );
            let uuid = crate::characters::uuid_text(&sheet.uuid);
            if !map.is_creation() {
                match characters.unlock_map(&uuid, map.id) {
                    Ok(true) => {
                        reached_here = true;
                        self.say(
                            LogLevel::Debug,
                            format!(
                                "Gameplay connection from {peer}: map {} '{}' is unlocked - the character arrived in it (R12).",
                                map.id, map.name
                            ),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => self.say(
                        LogLevel::Warn,
                        format!(
                            "Gameplay connection from {peer}: the arrival in map {} cannot be recorded: {error}",
                            map.id
                        ),
                    ),
                }
            }
            match characters.unlocked_maps(&uuid) {
                Ok(maps) => {
                    for map_id in maps {
                        let word = (map_id / 32) as usize;
                        if let Some(slot) = unlocked.get_mut(word) {
                            *slot |= 1 << (map_id % 32);
                        }
                    }
                }
                Err(error) => self.say(
                    LogLevel::Warn,
                    format!(
                        "Gameplay connection from {peer}: the character's reached instances cannot be read ({error}); the list is sent with only what this load knows."
                    ),
                ),
            }
            // "a client told it is nowhere can offer nowhere": the map it is in is on
            // the list whether or not the read above worked (`player.rs:463-471`).
            if !map.is_creation() || reached_here {
                let word = (map.id / 32) as usize;
                if let Some(slot) = unlocked.get_mut(word) {
                    *slot |= 1 << (map.id % 32);
                }
            }
        }
        let load_ms = u32::try_from(load.load_started.elapsed().as_millis())
            .unwrap_or(u32::MAX);

        // Everyone already in this copy of the map is introduced first: the player, its
        // party size, its party and its agent, in that order
        // (`ASURA/crates/asura-world/src/player.rs:499-501`, `introduction`).
        let standing = crate::districts::Standing {
            position,
            plane,
            direction: (0.0, 1.0),
        };
        let me = crate::districts::PeerView {
            player_id: player,
            agent_id: agent,
            name: sheet.name.clone(),
            standing,
            equipped,
            displayed_rank: self.world.titles().initial_display_rank(),
            map_id: map.id,
            district: state.district(),
        };
        let (sending, receiving) = std::sync::mpsc::channel();
        let others = match self.world.peers().lock() {
            Ok(mut peers) => peers.join(me.clone(), sending),
            Err(_) => Vec::new(),
        };
        state.open_outbox(receiving);
        let mut frames = Vec::with_capacity(40);
        for peer in &others {
            frames.extend(crate::districts::introduction(
                peer,
                appearance,
                self.world.settings().agent_speed,
                PLAYER_TEAM_TOKEN,
            ));
        }
        frames.extend(told.load_block(
            &self.world.settings(),
            load_ms,
            &unlocked,
            self.world.titles(),
        ));
        // Send peer selections after this load has defined the corresponding ranks.
        frames.extend(
            others
                .iter()
                .filter(|other| other.displayed_rank != 0)
                .map(|other| {
                    title_packets::rank_display(
                        other.player_id as u16,
                        other.displayed_rank,
                    )
                }),
        );
        let count = frames.len();
        self.send(io, peer, crypt, &frames, "the players and the load's last message")
            .await;
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: map {} is loaded - the character '{}' is in it ({count} message(s), {} other player(s) introduced).",
                map.id,
                sheet.name,
                others.len()
            ),
        );
        // And the others are told this body is here: the introduction, then the items it
        // wears in the form a bag slot uses (`CREATE_UNNAMED_ITEM`, `GameSrv_BroadcastPlayerEquippedItems`,
        // `GmItem.c:129-160`), then what the agent is drawn wearing and its cape - the
        // reference's own order (`ASURA/crates/asura-world/src/player.rs:509-532`).
        let mut mine = crate::districts::introduction(
            &me,
            appearance,
            self.world.settings().agent_speed,
            PLAYER_TEAM_TOKEN,
        );
        if let Some(bag) = crate::creation::equipped_bag(self.world.creation()) {
            for item in inventory.items_in(bag.id) {
                mine.push(item_packets::create_unnamed_item(
                    &crate::creation::named_item(item),
                ));
            }
        }
        mine.push(misc_packets::update_agent_visual_equipment(agent, equipped));
        mine.push(misc_packets::agent_display_cape(agent, 1));
        if me.displayed_rank != 0 {
            mine.push(title_packets::rank_display(player as u16, me.displayed_rank));
        }
        if let Ok(peers) = self.world.peers().lock() {
            peers.broadcast(player, map.id, state.district(), &mine);
        }
    }
}
