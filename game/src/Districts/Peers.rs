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

use crate::server::packets::{
    agent_packets, character_packets, item_packets, misc_packets, title_packets,
};

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
    /// The character's uuid, which the unlock table names.
    pub character: [u8; 16],
    /// The agent the player drives.
    pub agent_id: u32,
    /// The character's name, as the client shows it.
    pub name: String,
    /// Where the body stands.
    pub standing: Standing,
    /// The nine item ids the body is drawn wearing, by slot.
    pub equipped: [u32; 9],
    /// The look the client builds this body's model from - this player's own, not the newcomer's.
    pub appearance: u32,
    /// The primary profession the agent is drawn and fought as.
    pub profession: u8,
    /// Its secondary profession; zero until one is chosen.
    pub secondary_profession: u8,
    /// The level the party and target windows draw.
    pub level: u8,
    /// The worn items in the form an introduction declares (`0x015E`), one per piece.
    pub wearing: Vec<item_packets::NamedItem>,
    /// The single rank the player displays, or zero when hidden.
    pub displayed_rank: u32,
    /// Which map this body is in.
    pub map_id: u32,
    /// Which district of it.
    pub district: u32,
    /// The guild another client is told this player is in, with this copy's local id for it; `None`
    /// for a player in no guild (`ASURA/crates/asura-world/src/instance.rs:55-57`, `guild`).
    pub guild: Option<crate::guilds::Badge>,
}

/// One loaded player: the view a peer gets, and the channel its own session reads.
struct Slot {
    view: PeerView,
    outbox: Sender<Vec<u8>>,
    /// Which session filled this slot. A travel reconnects with the same player id, so an old
    /// connection's tail must not remove the new connection's slot.
    session: u64,
}

/// The population of every running copy, shared by every session.
#[derive(Default)]
pub struct Peers {
    slots: HashMap<u32, Slot>,
    /// The next player id to hand out. The reference allocates one per connection from a pool of its
    /// own; this world counts from the first id, so two sessions can never name each other's player.
    next: u32,
    /// The id a character already has. A doorway crossing reconnects with the same character, and the
    /// party roster is keyed by player id, so the id must survive the hand-off.
    characters: HashMap<[u8; 16], u32>,
    /// The next session number, so a slot can tell the connection that filled it from one that did not.
    next_session: u64,
    /// The local ids each running copy names guilds by, keyed by map and district
    /// (`ASURA/crates/asura-world/src/instance.rs:154-155`, `guild_ids`: one table per instance).
    guild_ids: HashMap<(u32, u32), crate::guilds::LocalIds>,
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

    /// The id this character already has, or a new one. The travel hand-off carries the party roster
    /// by player id, so a character that crosses a doorway keeps its number.
    pub fn allocate_for(&mut self, character: [u8; 16]) -> u32 {
        if let Some(id) = self.characters.get(&character) {
            eprintln!("world: peers: character {character:02X?} keeps player id {id}");
            return *id;
        }
        let id = self.allocate();
        self.characters.insert(character, id);
        eprintln!("world: peers: character {character:02X?} is drawn player id {id}");
        id
    }

    /// Register a loaded player and answer **everyone else in that map and district**, which is who it
    /// has to be told about (`ASURA/crates/asura-world/src/instance.rs:257-264`, `others`).
    pub fn join(&mut self, view: PeerView, outbox: Sender<Vec<u8>>) -> (Vec<PeerView>, u64) {
        let others = self.others(&view);
        self.next_session += 1;
        let session = self.next_session;
        self.slots.insert(
            view.player_id,
            Slot {
                view,
                outbox,
                session,
            },
        );
        (others, session)
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

    /// One player's view, when it is loaded.
    pub fn view(&self, player_id: u32) -> Option<PeerView> {
        self.slots.get(&player_id).map(|slot| slot.view.clone())
    }

    /// Take a player back out, answering its view when it was there.
    pub fn leave(&mut self, player_id: u32) -> Option<PeerView> {
        self.slots.remove(&player_id).map(|slot| slot.view)
    }

    /// Take a player back out only when the slot still belongs to this session. A travel reconnects
    /// with the same player id, so the old connection's tail must not remove the new connection's slot.
    pub fn leave_if(&mut self, player_id: u32, session: u64) -> Option<PeerView> {
        if self
            .slots
            .get(&player_id)
            .is_some_and(|slot| slot.session == session)
        {
            self.leave(player_id)
        } else {
            None
        }
    }

    /// Record where a player's body stands now, so a later introduction is the truth
    /// (`ASURA/crates/asura-world/src/instance.rs:249-255`, `standing`).
    pub fn moved(&mut self, player_id: u32, standing: Standing) {
        if let Some(slot) = self.slots.get_mut(&player_id) {
            slot.view.standing = standing;
        }
    }

    /// The local id a guild is named by in one copy of a map, allocated on first use
    /// (`ASURA/crates/asura-world/src/guild.rs:109-115`, `badge`).
    pub fn guild_local_id(&mut self, map_id: u32, district: u32, guild: [u8; 16]) -> u16 {
        self.guild_ids
            .entry((map_id, district))
            .or_default()
            .of(guild)
    }

    /// Record the guild a player is in, for the introductions that follow
    /// (`ASURA/crates/asura-world/src/instance.rs:398-401`, `wear_guild`).
    pub fn wear_guild(&mut self, player_id: u32, badge: Option<crate::guilds::Badge>) {
        if let Some(slot) = self.slots.get_mut(&player_id) {
            slot.view.guild = badge;
        }
    }

    /// Whether a character with this name is loaded anywhere in this world, matched without regard to
    /// case: the "online" a guild invitation asks about (`ASURA/crates/asura-world/src/instance.rs:404-414`
    /// and `:1172-1179`, `is_online`). A character in another world process reads as offline.
    pub fn is_online(&self, name: &str) -> bool {
        self.slots
            .values()
            .any(|slot| slot.view.name.eq_ignore_ascii_case(name))
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

    /// Push frames to one player's own session (`ASURA/crates/asura-world/src/instance.rs:584-590`,
    /// `send_to`): how a party batch reaches the members a request names. A player that has gone is
    /// silently skipped, as [`Self::broadcast`] skips one.
    pub fn send_to(&self, player_id: u32, frames: &[Vec<u8>]) {
        if let Some(slot) = self.slots.get(&player_id) {
            for frame in frames {
                let _ = slot.outbox.send(frame.clone());
            }
        }
    }

    /// Every player loaded in one map and district, by id: who a party change's bystanders are read
    /// from (`ASURA/crates/asura-world/src/party.rs:520-526`, `bystanders`).
    pub fn everyone(&self, map_id: u32, district: u32) -> Vec<u32> {
        let mut found: Vec<u32> = self
            .slots
            .values()
            .filter(|slot| slot.view.map_id == map_id && slot.view.district == district)
            .map(|slot| slot.view.player_id)
            .collect();
        found.sort_unstable();
        found
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
/// agent (`ASURA/crates/asura-world/src/instance.rs:1031-1100`, `introduction`).
///
/// `GmAgent.c` sends `BroadcastAddPlayerToPlayerParty` rather than the unicast sender, because a
/// joining player's party is news to everyone already there; here it is the same frame either way.
///
/// `speed` is the world's own `World.AgentSpeed`, so the introduction states the same walking speed the
/// instance advances the same body by, and `team_token` is the one a load gives the client.
///
/// `party` is the peer's party as the copy's table has it - its leader and its size - read by the caller
/// after the peers' lock is released (`ASURA/crates/asura-world/src/instance.rs:561-572`, `others`): the
/// leader and its party's size, then this player and its leader, a live load's order for every other
/// player (`titles-live-20261106-025940.jsonl` seq 2088-2091: player 6 in a party of three led by player
/// 0x40 is `0x00B0 {0x40, 3}`, `0x00B1 {6, 0x40}`; `instance.rs:1052-1057`).
///
/// The whole burst is retail's, not only the player and its agent: the peer's own appearance in
/// `0x0059`, its guild (`0x0121` and agent property 30) when it has one, its professions (`0x00A6`)
/// and level (`0x009F` property 36), its flags or displayed rank,
/// its initial effects, then the items it wears (`0x015E`), the nine visual slots (`0x006E`) and its
/// cape (`0x0048`). `pre-searing-load-3.jsonl` seq 210-228 is one, in this order.
pub fn introduction(
    peer: &PeerView,
    party: (u32, u32),
    speed: f32,
    team_token: u32,
) -> Vec<Vec<u8>> {
    let (leader, size) = party;
    let mut frames = vec![crate::server::packets::instance_packets::player_create(
        peer.player_id,
        peer.agent_id,
        peer.appearance,
        &peer.name,
    )];
    // A guilded player's guild and its agent's guild property, between its player create and its
    // party pair (`ASURA/crates/asura-world/src/instance.rs:1047-1051`; `pre-searing-load-3.jsonl` seq
    // 299-302).
    if let Some(badge) = &peer.guild {
        frames.extend(crate::guilds::badge_frames(badge, peer.agent_id));
    }
    frames.extend([
        crate::server::packets::party_packets::player_update_party_size(leader, size),
        crate::server::packets::party_packets::player_update_party(peer.player_id, leader),
        agent_packets::agent_update_profession(
            peer.agent_id,
            peer.profession,
            peer.secondary_profession,
        ),
        agent_packets::agent_property_update_int(
            agent_packets::AGENT_PROPERTY_PUBLIC_LEVEL,
            peer.agent_id,
            u32::from(peer.level),
        ),
    ]);
    // Retail binds a displayed rank here, before the agent; a peer with no title gets the same flags
    // word every introduction carries (`pre-searing-load-3.jsonl` seq 217-220, 306-307). ASURA's
    // binding also sends the rank record because it does not track what a peer already has; this
    // tree's load sends every configured record in the player's own block, so the binding is enough.
    if peer.displayed_rank != 0 {
        frames.push(title_packets::rank_display(
            peer.player_id as u16,
            peer.displayed_rank,
        ));
    }
    frames.push(misc_packets::player_update_flags(
        peer.player_id as u16,
        0,
        7,
    ));
    frames.push(agent_packets::agent_initial_effects(peer.agent_id, 0));
    frames.push(agent_packets::world_create_agent(
        &agent_packets::CreateAgent {
            agent_id: peer.agent_id,
            model_id: agent_packets::CHAR_CLASS_PLAYER_BASE | peer.player_id,
            agent_type: agent_packets::AGENT_TYPE_LIVING,
            position: peer.standing.position,
            plane: peer.standing.plane,
            direction: peer.standing.direction,
            speed_base: speed,
            player_team_token: team_token,
        },
    ));
    for item in &peer.wearing {
        frames.push(item_packets::item_low_detail(item));
    }
    frames.push(misc_packets::update_agent_visual_equipment(
        peer.agent_id,
        peer.equipped,
    ));
    frames.push(misc_packets::agent_display_cape(peer.agent_id, 1));
    frames
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
            character: [0u8; 16],
            agent_id: player_id,
            name: format!("player {player_id}"),
            standing: Standing {
                position: (1.0, 2.0),
                plane: 0,
                direction: (0.0, 1.0),
            },
            equipped: [0u32; 9],
            appearance: 0x0310_0000,
            profession: 1,
            secondary_profession: 0,
            level: 1,
            wearing: Vec::new(),
            displayed_rank: 0,
            map_id,
            district,
            guild: None,
        }
    }

    #[test]
    fn a_join_answers_everyone_else_in_the_same_map_and_district() {
        let mut peers = Peers::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        assert!(
            peers.join(view(1, 148, 1), tx.clone()).0.is_empty(),
            "the first body has no peers"
        );
        assert!(
            peers.join(view(2, 147, 1), tx.clone()).0.is_empty(),
            "another map is another place"
        );
        assert_eq!(
            peers.join(view(3, 148, 1), tx).0,
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
        assert_eq!(
            peers.leave(7),
            None,
            "leaving twice is not an error, and not a second farewell"
        );
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
        assert!(
            rx1.try_recv().is_err(),
            "the sender is not told its own news"
        );
        assert_eq!(rx2.try_recv(), Ok(vec![0x3C, 0x00]), "the other player is");
        assert!(rx3.try_recv().is_err(), "another district is another place");
    }

    /// The introduction is the reference's whole burst in its order: the player, its party pair, its
    /// professions and level, its flags, its effects, the agent, what it wears, and its cape.
    #[test]
    fn an_introduction_is_the_references_whole_burst_in_order() {
        let frames = introduction(&view(4, 148, 1), (4, 1), 288.0, 0xBAAD_F00D);
        let headers: Vec<u16> = frames
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert_eq!(
            headers,
            vec![
                0x0059, 0x00B0, 0x00B1, 0x00A6, 0x009F, 0x003C, 0x00F0, 0x0020, 0x006E, 0x0048
            ]
        );
    }

    /// A guilded peer's guild goes between its player create and its party pair
    /// (`ASURA/crates/asura-world/src/instance.rs:1047-1051`).
    #[test]
    fn a_guilded_peer_is_introduced_with_its_badge() {
        let mut peer = view(4, 148, 1);
        peer.guild = Some(crate::guilds::Badge {
            local_id: 3,
            guild: crate::guilds::Guild {
                id: [7; 16],
                name: "Guild".into(),
                tag: "TAG".into(),
                feature: 0,
                territory: 2,
                cape: crate::guilds::Cape::default(),
                allegiance: 4,
                faction: 0,
                qualifier: 0,
                rating: 1000,
                rank: 0,
                announcement: String::new(),
                announcement_author: String::new(),
                revision: 0x200,
            },
        });
        let headers: Vec<u16> = introduction(&peer, (4, 1), 288.0, 0xBAAD_F00D)
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect();
        assert_eq!(&headers[..4], &[0x0059, 0x0121, 0x009F, 0x00B0]);
    }

    #[test]
    fn a_guild_keeps_its_local_id_per_copy_and_online_ignores_case() {
        let mut peers = Peers::new();
        assert_eq!(peers.guild_local_id(148, 1, [1; 16]), 1);
        assert_eq!(peers.guild_local_id(148, 1, [2; 16]), 2);
        assert_eq!(
            peers.guild_local_id(148, 2, [2; 16]),
            1,
            "another copy, another table"
        );
        assert_eq!(peers.guild_local_id(148, 1, [1; 16]), 1);
        let (tx, _rx) = std::sync::mpsc::channel();
        peers.join(view(5, 148, 1), tx);
        assert!(peers.is_online("PLAYER 5"));
        assert!(!peers.is_online("player 6"));
    }

    /// Each peer is drawn with its own look, not with the joining player's: the bug this guards was
    /// every introduction carrying the newcomer's appearance for every peer.
    #[test]
    fn an_introduction_uses_the_peers_own_look_and_wear() {
        let mut peer = view(4, 148, 1);
        peer.appearance = 0x1234_5678;
        peer.profession = 3;
        peer.secondary_profession = 5;
        peer.level = 17;
        peer.wearing.push(item_packets::NamedItem {
            id: 7,
            ..item_packets::NamedItem::default()
        });
        let frames = introduction(&peer, (4, 1), 288.0, 0xBAAD_F00D);
        let create = frames
            .iter()
            .find(|frame| frame[0] == 0x59 && frame[1] == 0x00)
            .expect("a player create");
        assert_eq!(
            &create[10..14],
            &0x1234_5678u32.to_le_bytes(),
            "its own appearance"
        );
        let profession = frames
            .iter()
            .find(|frame| frame[0] == 0xA6 && frame[1] == 0x00)
            .expect("a profession");
        assert_eq!(&profession[6..], &[3, 5], "its own two professions");
        let level = frames
            .iter()
            .find(|frame| frame[0] == 0x9F && frame[1] == 0x00)
            .expect("a level");
        assert_eq!(
            &level[2..6],
            &36u32.to_le_bytes(),
            "the public-level property"
        );
        assert_eq!(&level[10..14], &17u32.to_le_bytes(), "its own level");
        assert!(
            frames.iter().any(|frame| frame[0] == 0x5E
                && frame[1] == 0x01
                && frame[2..6] == 7u32.to_le_bytes()),
            "each worn item as a low-detail record"
        );
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
