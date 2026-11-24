//! Who is in a running copy of a map, and the two sequences that say so.
//!
//! Ported from `ASURA/crates/asura-world/src/instance.rs:406-473` - `PeerView`, `introduction` and
//! `farewell` - and from the block a load builds with them (`player.rs:499-533`).
//!
//! A running copy of a map is a place other players can be in, and the client learns about them from
//! the server alone: `GameSrv_SendWorldAgents` (`GmAgent.c:310-331`) tells a joining player about each
//! body already there - the player, their party size, their party, then the agent itself - and
//! `GameSrv_BroadcastPlayerDestroy` (`GmPlayer.c:314`) with `GameSrv_BroadcastAgentRemove`
//! (`GmAgent.c:60`) takes one back out when it leaves.
//!
//! The registry holds one slot per loaded player, keyed by player id, and each slot carries the channel
//! its own session reads. Frames travel **plaintext** through it: every session scrambles what it writes
//! with its own cipher, so only the session that owns a socket can put bytes on it.

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use crate::server::packets::{agent_packets, character_packets};

/// Where a body stands, as the messages that carry it want it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Standing {
    /// The point, in the game's own units.
    pub position: (f32, f32),
    /// Which plane it stands on.
    pub plane: u16,
    /// Which way it faces, as the unit vector `WORLD_CREATE_AGENT` carries.
    pub direction: (f32, f32),
}

/// A peer as it is copied out of a running copy: everything a joining player needs to be told.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerView {
    /// The player's id, which the player messages name.
    pub player_id: u32,
    /// The agent the player drives.
    pub agent_id: u32,
    /// The character's name, as the client shows it.
    pub name: String,
    /// Where the body stands.
    pub standing: Standing,
    /// The nine item ids the body is drawn wearing, by slot.
    pub equipped: [u32; 9],
    /// The single rank the player displays, or zero when hidden.
    pub displayed_rank: u32,
    /// Which map this body is in.
    pub map_id: u32,
    /// Which district of it.
    pub district: u32,
}

/// One loaded player: the view a peer gets, and the channel its own session reads.
struct Slot {
    view: PeerView,
    outbox: Sender<Vec<u8>>,
}

/// The population of every running copy, shared by every session.
#[derive(Default)]
pub struct Peers {
    slots: HashMap<u32, Slot>,
    /// The next player id to hand out. The reference allocates one per connection from a pool of its
    /// own; this world counts from the first id, so two sessions can never name each other's player.
    next: u32,
}

impl Peers {
    /// An empty world, before anyone has loaded a map.
    pub fn new() -> Self {
        Self::default()
    }

    /// The next player id, for a connection that has just opened.
    ///
    /// The working implementation reads one from its player pool; this world counts, so the first id is
    /// `CREATION_AGENT`'s number and every connection after it gets the next.
    pub fn allocate(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// Register a loaded player and answer **everyone else in that map and district**, which is who it
    /// has to be told about (`ASURA/crates/asura-world/src/instance.rs:257-264`, `others`).
    pub fn join(&mut self, view: PeerView, outbox: Sender<Vec<u8>>) -> Vec<PeerView> {
        let others = self.others(&view);
        self.slots.insert(view.player_id, Slot { view, outbox });
        others
    }

    /// Everyone in the same map and district as `view`, except `view`'s own player.
    pub fn others(&self, view: &PeerView) -> Vec<PeerView> {
        let mut found: Vec<PeerView> = self
            .slots
            .values()
            .filter(|slot| {
                slot.view.player_id != view.player_id
                    && slot.view.map_id == view.map_id
                    && slot.view.district == view.district
            })
            .map(|slot| slot.view.clone())
            .collect();
        found.sort_by_key(|peer| peer.player_id);
        found
    }

    /// Take a player back out, answering its view when it was there.
    pub fn leave(&mut self, player_id: u32) -> Option<PeerView> {
        self.slots.remove(&player_id).map(|slot| slot.view)
    }

    /// Record where a player's body stands now, so a later introduction is the truth
    /// (`ASURA/crates/asura-world/src/instance.rs:249-255`, `standing`).
    pub fn moved(&mut self, player_id: u32, standing: Standing) {
        if let Some(slot) = self.slots.get_mut(&player_id) {
            slot.view.standing = standing;
        }
    }

    /// Remember a new displayed title for players who join this district later.
    pub fn set_displayed_rank(&mut self, player_id: u32, rank: u32) {
        if let Some(slot) = self.slots.get_mut(&player_id) {
            slot.view.displayed_rank = rank;
        }
    }

    /// Push frames to everyone in the same map and district, except one player
    /// (`ASURA/crates/asura-world/src/instance.rs:266-276`, `broadcast`). A closed channel is a session
    /// that has gone: its own task removes it, so a failed send is not an error here.
    pub fn broadcast(&self, except: u32, map_id: u32, district: u32, frames: &[Vec<u8>]) {
        for slot in self.slots.values().filter(|slot| {
            slot.view.player_id != except
                && slot.view.map_id == map_id
                && slot.view.district == district
        }) {
            for frame in frames {
                let _ = slot.outbox.send(frame.clone());
            }
        }
    }

    /// How many players are loaded in this world.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether no player is loaded.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// The frames that introduce one player to another: `GameSrv_SendWorldAgents`'s order for a player
/// agent (`ASURA/crates/asura-world/src/instance.rs:432-464`, `introduction`).
///
/// `GmAgent.c` sends `BroadcastAddPlayerToPlayerParty` rather than the unicast sender, because a
/// joining player's party is news to everyone already there; here it is the same frame either way.
///
/// `speed` is the world's own `World.AgentSpeed`, so the introduction states the same walking speed the
/// instance advances the same body by, and `team_token` is the one a load gives the client.
pub fn introduction(peer: &PeerView, appearance: u32, speed: f32, team_token: u32) -> Vec<Vec<u8>> {
    vec![
        crate::server::packets::instance_packets::player_create(
            peer.player_id,
            peer.agent_id,
            appearance,
            &peer.name,
        ),
        crate::server::packets::party_packets::player_update_party_size(peer.player_id, 1),
        crate::server::packets::party_packets::player_update_party(
            peer.player_id,
            peer.player_id,
        ),
        agent_packets::world_create_agent(&agent_packets::CreateAgent {
            agent_id: peer.agent_id,
            model_id: agent_packets::CHAR_CLASS_PLAYER_BASE | peer.player_id,
            agent_type: agent_packets::AGENT_TYPE_LIVING,
            position: peer.standing.position,
            plane: peer.standing.plane,
            direction: peer.standing.direction,
            speed_base: speed,
            player_team_token: team_token,
        }),
    ]
}

/// The frames that take one player back out of another's world: the player destroyed, then its agent
/// removed, in that order (`ASURA/crates/asura-world/src/instance.rs:466-473`, `farewell`).
pub fn farewell(player_id: u32, agent_id: u32) -> Vec<Vec<u8>> {
    vec![
        character_packets::player_destroy(player_id).1,
        agent_packets::world_remove_agent(agent_id).1,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(player_id: u32, map_id: u32, district: u32) -> PeerView {
        PeerView {
            player_id,
            agent_id: player_id,
            name: format!("player {player_id}"),
            standing: Standing {
                position: (1.0, 2.0),
                plane: 0,
                direction: (0.0, 1.0),
            },
            equipped: [0u32; 9],
            displayed_rank: 0,
            map_id,
            district,
        }
    }

    #[test]
    fn a_join_answers_everyone_else_in_the_same_map_and_district() {
        let mut peers = Peers::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(peers.join(view(1, 148, 1), tx.clone()).is_empty(), "the first body has no peers");
        assert!(peers.join(view(2, 147, 1), tx.clone()).is_empty(), "another map is another place");
        assert_eq!(
            peers.join(view(3, 148, 1), tx),
            vec![view(1, 148, 1)],
            "the same map and district answers its population"
        );
        assert_eq!(peers.len(), 3);
    }

    #[test]
    fn a_leave_names_what_left_and_takes_it_out() {
        let mut peers = Peers::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        peers.join(view(7, 148, 1), tx);
        assert_eq!(peers.leave(7).map(|peer| peer.player_id), Some(7));
        assert!(peers.is_empty());
        assert_eq!(peers.leave(7), None, "leaving twice is not an error, and not a second farewell");
    }

    #[test]
    fn a_broadcast_reaches_the_others_and_never_the_sender() {
        let mut peers = Peers::new();
        let (tx1, rx1) = std::sync::mpsc::channel();
        let (tx2, rx2) = std::sync::mpsc::channel();
        let (tx3, rx3) = std::sync::mpsc::channel();
        peers.join(view(1, 148, 1), tx1);
        peers.join(view(2, 148, 1), tx2);
        peers.join(view(3, 148, 2), tx3);
        peers.broadcast(1, 148, 1, &[vec![0x3C, 0x00]]);
        assert!(rx1.try_recv().is_err(), "the sender is not told its own news");
        assert_eq!(rx2.try_recv(), Ok(vec![0x3C, 0x00]), "the other player is");
        assert!(rx3.try_recv().is_err(), "another district is another place");
    }

    /// The introduction is OpenTyria's four messages in its order: the player, its party size, its
    /// party, and the agent itself.
    #[test]
    fn an_introduction_is_four_messages_in_the_references_order() {
        let frames = introduction(&view(4, 148, 1), 0x0310_0000, 288.0, 0xBAAD_F00D);
        let headers: Vec<u16> = frames
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert_eq!(headers, vec![0x0059, 0x00B0, 0x00B1, 0x0020]);
    }

    #[test]
    fn a_farewell_is_the_player_then_its_agent() {
        let frames = farewell(4, 4);
        let headers: Vec<u16> = frames
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert_eq!(headers, vec![0x005A, 0x0021]);
    }
}
