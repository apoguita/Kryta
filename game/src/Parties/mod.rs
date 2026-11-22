//! `game/Parties/` - the reference's `src/server/game/Groups/` (7 files).
//!
//! Renamed: the reference calls this directory `Groups/`.
//!
//! Parties in one running copy of a map: who leads each one, who is in it, which invitations are open,
//! and what every client is told when that changes.
//!
//! **Ported from `ASURA/crates/asura-world/src/party.rs`** (2,222 lines, ASURA `071735e`). This round
//! carries lines 39-1206 and 1827-1946: the party table (`Party`, `Absence`, `Table`), the frames
//! (`member_stream`, `size_and_leader`, `load_section`, `in_map_notices`, `load_tail`), the outpost
//! requests (`handle_invite`, `handle_cancel`, `handle_refuse`, `handle_accept` with its merge,
//! `handle_leave`, `handle_kick`), the departure every one of them shares (`departure`), the two travel
//! leaves (`leave_to_travel`, `leave_to_travel_grey`) and a player leaving the map (`left_instance`,
//! without its explorable hold). **Ported thin**, and declared so in `docs/port/09-parity-audit.md`:
//! lines 1207-1826 - a party travelling as one (`arrive`, `announce_arrival`, `expire`, `begin_move`,
//! `start_countdown`, the database ledger), `/resign` and its defeat, and the explorable disconnect
//! hold - are not here yet; the rules they ask (`Rules.rs`) are.
//!
//! Every player is in a party from the moment it loads a map - a party of one, which its own load tells
//! the client about ([`load_section`]). An invitation is between two **parties**, and the messages that
//! carry it name the *other* party from the receiving client's point of view. What a live server sends
//! for each request is measured in ASURA `docs/44-party-invites.md`; each batch below names the capture
//! seq it copies, and the pieces no capture shows are marked **not captured** where they are sent.
//!
//! **This tree's arrangement.** ASURA keeps one table inside each `Instance` and its handlers send
//! through the instance. Here a running copy of a map is a district (`game/Districts/`), so [`Parties`]
//! holds one [`Table`] per `(map, district)`, and every request answers a [`Plan`] - which frames go to
//! which player - that the session delivers through `Peers::send_to` once this lock is released. The
//! lock order is ASURA's own, made strict: the party lock is never held while the peers' is taken.

#[path = "Rules.rs"]
pub mod rules;

use std::collections::{HashMap, HashSet};

use crate::server::packets::{character_packets, instance_packets, misc_packets, party_packets};

/// One frame, as every packet builder in this tree answers it.
pub type Frame = Vec<u8>;

/// Frames for some players, delivered after the table's lock is released
/// (`ASURA/crates/asura-world/src/party.rs:501-518`).
pub type Plan = Vec<(u32, Vec<Frame>)>;

/// What a request answers: the frames to deliver, or why it was refused. A refused request is answered
/// with nothing on the wire - what a live server does then is **not captured** - and the reason is the
/// log line (`party.rs:533-540`, `refused`).
pub type Answer = Result<Plan, String>;

/// The party state a load ends with, after the player's own party (`CHARACTER_UPDATE_PARTY`) and before
/// `INSTANCE_LOAD_FINISH` (`party.rs:172-198`). Every live load carries it, every value 0
/// (`titles-live-20261106-034507.jsonl` seq 847-861 for an outpost, `doors-20261031-095754.jsonl` seq
/// 322-324 for an explorable); a client given none of it keeps its own defaults, which drew the
/// difficulty toggle and the ready check in ASURA's Ascalon.
///
/// - an outpost: `0x01E0 {0}`, `0x01BF {0}`, `0x01BE {0}`, the party search adverts, `0x01E0 {0}`; this
///   server has no party search, so it sends no adverts;
/// - an explorable: `0x01BF {0}`, `0x01BE {0}`.
pub fn load_tail(in_outpost: bool) -> Vec<Frame> {
    if in_outpost {
        vec![
            party_packets::party_search_seek(0),
            party_packets::party_set_difficulty(0),
            party_packets::party_global_state_flags(0),
            party_packets::party_search_seek(0),
        ]
    } else {
        vec![
            party_packets::party_set_difficulty(0),
            party_packets::party_global_state_flags(0),
        ]
    }
}

/// Put [`load_tail`] into a built players block, right before its `INSTANCE_LOAD_FINISH`; a block with
/// none is left as it is (`party.rs:200-208`). ASURA's block ends with the finish; this tree's carries
/// the title display after it (`Agents/Player.rs`), so the finish is found rather than assumed last.
pub fn splice_load_tail(block: &mut Vec<Frame>, in_outpost: bool) {
    let Some(at) = block
        .iter()
        .rposition(|frame| header(frame) == instance_packets::INSTANCE_LOAD_FINISH)
    else {
        return;
    };
    block.splice(at..at, load_tail(in_outpost));
}

fn header(frame: &[u8]) -> u16 {
    match frame {
        [low, high, ..] => u16::from_le_bytes([*low, *high]),
        _ => 0,
    }
}

/// One party: its id, its leader and its members, all player ids. The leader is a member
/// (`party.rs:209-237`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Party {
    /// The id every party message carries.
    pub party_id: u16,
    /// The player who leads it.
    pub leader: u32,
    /// Its players, in the order they joined; the leader first. A member that is not in the map (held,
    /// or travelling in) is a member too, under the player id reserved for it.
    pub members: Vec<u32>,
    /// The members that are not in the map, by player id.
    pub absent: HashMap<u32, Absence>,
    /// A move is under way: its members are leaving, quietly, and nothing starts a second one.
    pub moving: bool,
    /// The ticket of a leader's countdown waiting to fire; a newer one, or a change of leader, voids it.
    pub countdown: Option<u64>,
    /// Members that have typed `/resign` and whose death is still due.
    pub resigned: HashSet<u32>,
    /// Members whose resigning death has been sent.
    pub dead: HashSet<u32>,
    /// The party has no member left standing; the leader's return button is now live.
    pub defeated: bool,
}

/// Why a member is not in the map (`party.rs:238-244`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Absence {
    /// Held after a dropped connection (`true`), or still travelling in (`false`).
    pub held: bool,
    /// Which expiry may remove it: a later absence of the same member carries a newer number.
    pub epoch: u64,
}

impl Party {
    /// The members standing in the map, in order.
    pub fn present(&self) -> Vec<u32> {
        self.members
            .iter()
            .copied()
            .filter(|member| !self.absent.contains_key(member))
            .collect()
    }

    /// Whether a member is in the map.
    pub fn is_present(&self, member: u32) -> bool {
        self.members.contains(&member) && !self.absent.contains_key(&member)
    }
}

/// The parties of one running copy of a map (`party.rs:262-366`).
#[derive(Debug, Default)]
pub struct Table {
    pub(crate) parties: HashMap<u16, Party>,
    of_player: HashMap<u32, u16>,
    /// Open invitations, `(inviting party, invited party)`.
    invites: HashSet<(u16, u16)>,
}

impl Table {
    /// The lowest free id. Live ids are arbitrary small numbers; the client keys its table by them and
    /// needs only that two parties never share one.
    fn free_id(&self) -> u16 {
        (1..=u16::MAX)
            .find(|id| !self.parties.contains_key(id))
            .unwrap_or(u16::MAX)
    }

    /// Put a player that has just loaded the map in a party of its own, and answer the party's id
    /// (`party.rs:284-297`).
    pub fn found(&mut self, player_id: u32) -> u16 {
        let party_id = self.free_id();
        self.parties.insert(
            party_id,
            Party {
                party_id,
                leader: player_id,
                members: vec![player_id],
                ..Party::default()
            },
        );
        self.of_player.insert(player_id, party_id);
        party_id
    }

    /// Take a party that crossed into this copy: its roster was carried by player id, so every member
    /// that has not arrived yet is marked absent. The arriving member is present immediately.
    pub fn adopt(&mut self, mut party: Party, arriving: u32) -> u16 {
        let party_id = if self.parties.contains_key(&party.party_id) {
            self.free_id()
        } else {
            party.party_id
        };
        party.party_id = party_id;
        party.moving = false;
        party.countdown = None;
        party.absent.clear();
        for member in &party.members {
            if *member != arriving {
                party.absent.insert(
                    *member,
                    Absence {
                        held: false,
                        epoch: 0,
                    },
                );
            }
        }
        for member in &party.members {
            self.of_player.insert(*member, party_id);
        }
        self.parties.insert(party_id, party);
        party_id
    }

    /// Remove a player from whatever party the table still remembers. A fresh login is a new
    /// session, not a reconnection into an old party unless a travel carried a roster.
    pub fn forget_member(&mut self, player_id: u32) {
        let Some(party_id) = self.of_player.remove(&player_id) else {
            return;
        };
        let gone = if let Some(party) = self.parties.get_mut(&party_id) {
            party.members.retain(|member| *member != player_id);
            party.absent.remove(&player_id);
            party.resigned.remove(&player_id);
            party.dead.remove(&player_id);
            if party.leader == player_id
                && let Some(next) = party.members.first()
            {
                party.leader = *next;
            }
            party.members.is_empty()
        } else {
            false
        };
        if gone {
            self.parties.remove(&party_id);
            self.close_invites_of(party_id);
        }
    }

    /// A member of an adopted party has arrived: it is no longer absent.
    pub fn arrive_member(&mut self, player_id: u32) {
        let Some(party_id) = self.of_player.get(&player_id).copied() else {
            return;
        };
        if let Some(party) = self.parties.get_mut(&party_id) {
            party.absent.remove(&player_id);
        }
    }

    /// The party a player is in.
    pub fn party_of(&self, player_id: u32) -> Option<&Party> {
        self.of_player
            .get(&player_id)
            .and_then(|party_id| self.parties.get(party_id))
    }

    /// Who leads a player's party, and how many players are in it: the two numbers `0x00B0` and
    /// `0x00B1` carry. A player the table does not know is a party of one (`party.rs:389-395`).
    pub fn leader_and_size(&self, player_id: u32) -> (u32, u32) {
        self.party_of(player_id)
            .map(|party| (party.leader, party.members.len() as u32))
            .unwrap_or((player_id, 1))
    }

    /// Whether no party is left, which is when the district's table can go.
    pub fn is_empty(&self) -> bool {
        self.parties.is_empty()
    }

    /// Close every open invitation that names `party_id`, either way round.
    fn close_invites_of(&mut self, party_id: u16) -> (Vec<u16>, Vec<u16>) {
        let mut invited: Vec<u16> = self
            .invites
            .iter()
            .filter(|(from, _)| *from == party_id)
            .map(|(_, to)| *to)
            .collect();
        let mut invited_by: Vec<u16> = self
            .invites
            .iter()
            .filter(|(_, to)| *to == party_id)
            .map(|(from, _)| *from)
            .collect();
        // A set has no order; the batches built from these go out in party order, so two runs of the
        // same requests send the same bytes.
        invited.sort_unstable();
        invited_by.sort_unstable();
        self.invites
            .retain(|(from, to)| *from != party_id && *to != party_id);
        (invited, invited_by)
    }

    fn members(&self, party_id: u16) -> Vec<u32> {
        self.parties
            .get(&party_id)
            .map(|party| party.members.clone())
            .unwrap_or_default()
    }

    /// Take a member out of its party without a word to anyone: what a member of a moving party is
    /// when its connection goes. The party goes with its last member.
    fn drop_quietly(&mut self, party_id: u16, member: u32) {
        let gone = match self.parties.get_mut(&party_id) {
            Some(party) => {
                party.members.retain(|m| *m != member);
                party.absent.remove(&member);
                if party.leader == member
                    && let Some(next) = party.members.first()
                {
                    party.leader = *next;
                }
                party.members.is_empty()
            }
            None => false,
        };
        if self.of_player.get(&member) == Some(&party_id) {
            self.of_player.remove(&member);
        }
        if gone {
            self.parties.remove(&party_id);
            self.close_invites_of(party_id);
        }
    }
}

/// Every running copy's parties, shared by every session of the world.
#[derive(Debug, Default)]
pub struct Parties {
    tables: HashMap<(u32, u32), Table>,
    next_ticket: u64,
}

impl Parties {
    /// No party anywhere, before anyone has loaded a map.
    pub fn new() -> Self {
        Self::default()
    }

    /// A number a countdown can be checked against; a newer one voids the older.
    pub fn ticket(&mut self) -> u64 {
        self.next_ticket += 1;
        self.next_ticket
    }

    /// Whether this countdown still stands, and use the ticket up. Same leader, same ticket.
    pub fn countdown_stands(
        &mut self,
        map_id: u32,
        district: u32,
        leader: u32,
        ticket: u64,
    ) -> bool {
        let Some(table) = self.tables.get_mut(&(map_id, district)) else {
            return false;
        };
        let Some(party_id) = table.party_of(leader).map(|party| party.party_id) else {
            return false;
        };
        let Some(party) = table.parties.get_mut(&party_id) else {
            return false;
        };
        let stands = party.leader == leader && party.countdown == Some(ticket);
        if party.countdown == Some(ticket) {
            party.countdown = None;
        }
        stands
    }

    /// The leader's cancel: clear the countdown and answer every present member with the unlock.
    pub fn cancel_countdown(
        &mut self,
        map_id: u32,
        district: u32,
        leader: u32,
    ) -> Option<(Vec<u32>, u64)> {
        let party_id = self.table(map_id, district).party_of(leader)?.party_id;
        let party = self
            .tables
            .get_mut(&(map_id, district))?
            .parties
            .get_mut(&party_id)?;
        if party.leader != leader {
            return None;
        }
        let ticket = party.countdown.take()?;
        Some((party.present(), ticket))
    }

    /// The table of one running copy, made on first use.
    pub fn table(&mut self, map_id: u32, district: u32) -> &mut Table {
        self.tables.entry((map_id, district)).or_default()
    }

    /// The table of one running copy, when anyone has a party there.
    pub fn get(&self, map_id: u32, district: u32) -> Option<&Table> {
        self.tables.get(&(map_id, district))
    }

    /// Forget a copy whose last party went, so an emptied district holds nothing.
    pub fn tidy(&mut self, map_id: u32, district: u32) {
        if self
            .tables
            .get(&(map_id, district))
            .is_some_and(Table::is_empty)
        {
            self.tables.remove(&(map_id, district));
        }
    }
}

/// Where a request is asked from: the player, whether the copy is an outpost, the party size its map
/// allows, and everyone loaded in the copy (the bystanders are read from it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Here {
    /// The player asking.
    pub me: u32,
    /// Whether the map is an outpost: parties change only there (`rules::may_change_party`).
    pub in_outpost: bool,
    /// The most players a party may hold on this map, or `None` when nothing says (`rules::may_fill`).
    pub max_size: Option<u32>,
    /// Every player loaded in this copy of the map, the asking one included.
    pub everyone: Vec<u32>,
}

// ---- the frames ---------------------------------------------------------------------------------

fn id(player_id: u32) -> u16 {
    player_id as u16
}

/// A party as a client is told it: create, one add per member, end (`party.rs:410-431`). The add's flag
/// is 1 for a member standing in the map (`titles-live-20261106-025940.jsonl` seq 2539-2541) and 0 for one
/// that is not (`multiplay/party.jsonl` seq 2667).
fn member_stream(party: &Party) -> Vec<Frame> {
    member_stream_where(party, |member| !party.absent.contains_key(&member))
}

/// [`member_stream`] with the flag decided by `in_map`.
fn member_stream_where(party: &Party, in_map: impl Fn(u32) -> bool) -> Vec<Frame> {
    let mut frames = vec![party_packets::party_create(party.party_id)];
    for member in &party.members {
        frames.push(party_packets::party_player_add(
            party.party_id,
            id(*member),
            u8::from(in_map(*member)),
        ));
    }
    frames.push(party_packets::party_member_stream_end(party.party_id));
    frames
}

/// The size and leader pair everyone in the map is told when a party changes: `0x00B0` names the leader
/// and the size, `0x00B1` names the player and its leader (`party.rs:433-441`).
fn size_and_leader(leader: u32, size: u32, player_id: u32) -> [Frame; 2] {
    [
        party_packets::player_update_party_size(leader, size),
        party_packets::player_update_party(player_id, leader),
    ]
}

/// The party part of a player's own load (`party.rs:443-465`): the leader and size, each member's
/// leader, the member stream, and `0x01B3` with 1 when the player leads it. A live load after a doorway
/// (`multiplay/party.jsonl` seq 2662-2669): `0x00B0 {1, 2}`, `0x00B1 {1, 1}`, `0x00B1 {2, 1}`, `0x01D3
/// {1}`, `0x01CC {1, 1, 1}`, `0x01CC {1, 2, 0}`, `0x01D4 {1}`, `0x01B3 {1, 1}`; a party of one is the
/// same shape (`multiplay/party interface check.jsonl` seq 588-591).
///
/// In its own load a client sees only itself in the map (flag 1). Every other member is sent with 0 and,
/// once its introduction is in the same block, with `0x01CF` ([`in_map_notices`]): a member sent with 1
/// before its `PLAYER_CREATE` is what dropped the second client of a party in ASURA's Barradin Estate.
pub fn load_section(party: &Party, me: u32) -> Vec<Frame> {
    let size = party.members.len() as u32;
    let mut frames = vec![party_packets::player_update_party_size(party.leader, size)];
    for member in &party.members {
        frames.push(party_packets::player_update_party(*member, party.leader));
    }
    frames.extend(member_stream_where(party, |member| member == me));
    frames.push(character_packets::character_update_party(
        party.party_id,
        u8::from(party.leader == me),
    ));
    frames
}

/// Replace the player block's own one-member party (`PARTY_CREATE` through `CHARACTER_UPDATE_PARTY`)
/// with this player's party as the table has it ([`load_section`]); `party.rs:467-488`. A block without
/// that run is left as it is.
pub fn splice_load_section(block: &mut Vec<Frame>, party: &Party, me: u32) {
    let Some(start) = block
        .iter()
        .position(|frame| header(frame) == party_packets::PARTY_CREATE)
    else {
        return;
    };
    let Some(end) = block[start..]
        .iter()
        .position(|frame| header(frame) == character_packets::CHARACTER_UPDATE_PARTY)
        .map(|offset| start + offset)
    else {
        return;
    };
    block.splice(start..=end, load_section(party, me));
}

/// What a loading client is told, after the introductions in its block, about the members of its party
/// already in the map: `0x01CF {party, member}` for each one it was introduced to (`entered`);
/// `party.rs:490-505`. **Not captured**; a client in ASURA's Barradin Estate took it without complaint.
pub fn in_map_notices(party: &Party, me: u32, entered: &[u32]) -> Vec<Frame> {
    party
        .present()
        .into_iter()
        .filter(|member| *member != me && entered.contains(member))
        .map(|member| party_packets::party_member_enter_world(party.party_id, id(member)))
        .collect()
}

fn to_all(players: &[u32], frames: &[Frame], plan: &mut Plan) {
    for player_id in players {
        plan.push((*player_id, frames.to_vec()));
    }
}

/// Everyone in the copy who is in none of `involved` (`party.rs:520-526`).
fn bystanders(here: &Here, involved: &[u32]) -> Vec<u32> {
    here.everyone
        .iter()
        .copied()
        .filter(|player_id| !involved.contains(player_id))
        .collect()
}

/// The outpost rule, asked first by every request (`party.rs:542-545`).
fn in_outpost(here: &Here) -> Result<(), String> {
    rules::may_change_party(here.in_outpost).map_err(|why| format!("{why:?}"))
}

fn why(refused: rules::Refused) -> String {
    format!("{refused:?}")
}

// ---- the requests -------------------------------------------------------------------------------

/// `GAME_CMSG_PARTY_INVITE_PLAYER` (`0x80A0`, one word: the invited player's id); `party.rs:547-596`.
///
/// The inviter is shown the invited party and then the open invitation (`032434` seq 380-384); the
/// invited is shown the inviting party and then the invitation (`025940` seq 2539-2542). The invited
/// party may hold more than one player: accepting is then a merge (experimental, not captured).
pub fn handle_invite(table: &mut Table, here: &Here, target: u32) -> Answer {
    in_outpost(here)?;
    let me = here.me;
    let (Some(mine), Some(theirs)) = (table.party_of(me).cloned(), table.party_of(target).cloned())
    else {
        return Err(format!("player {target} is not in this map"));
    };
    rules::may_invite(rules::Invitation {
        inviter_leads: mine.leader == me,
        same_party: mine.party_id == theirs.party_id,
        already_invited: table.invites.contains(&(mine.party_id, theirs.party_id)),
    })
    .map_err(why)?;
    rules::may_fill(mine.members.len() + theirs.members.len(), here.max_size).map_err(why)?;
    table.invites.insert((mine.party_id, theirs.party_id));
    let mut plan = Plan::new();
    let mut inviters = member_stream(&theirs);
    inviters.push(party_packets::party_invite_add(theirs.party_id));
    to_all(&mine.members, &inviters, &mut plan);
    let mut invited = member_stream(&mine);
    invited.push(party_packets::party_join_request(mine.party_id));
    to_all(&theirs.members, &invited, &mut plan);
    Ok(plan)
}

/// `GAME_CMSG_PARTY_ACCEPT_CANCEL` (`0x809D`, one word: the invited party). The inviter withdraws;
/// `party.rs:598-638`.
///
/// The inviter gets `0x01C7` alone (`032434` seq 1295-1300). The invited side's `0x01C8` is **not
/// captured**.
pub fn handle_cancel(table: &mut Table, here: &Here, invited: u16) -> Answer {
    in_outpost(here)?;
    let Some(mine) = table.party_of(here.me).cloned() else {
        return Err("no party".to_string());
    };
    let open = table.invites.contains(&(mine.party_id, invited));
    rules::may_answer(mine.leader == here.me, open).map_err(why)?;
    table.invites.remove(&(mine.party_id, invited));
    let mut plan = Plan::new();
    to_all(
        &mine.members,
        &[party_packets::party_invite_cancel(invited)],
        &mut plan,
    );
    to_all(
        &table.members(invited),
        &[party_packets::party_request_cancel(mine.party_id)],
        &mut plan,
    );
    Ok(plan)
}

/// `GAME_CMSG_PARTY_ACCEPT_REFUSE` (`0x809E`, one word: the inviting party). The invited declines;
/// `party.rs:640-695`.
///
/// The inviter gets `0x01CA` alone (`032434` seq 2058). The invited side's `0x01C9` alone is **not
/// captured**; it is the first message of the accept (`025940` seq 2622).
pub fn handle_refuse(table: &mut Table, here: &Here, inviting: u16) -> Answer {
    in_outpost(here)?;
    let Some(mine) = table.party_of(here.me).cloned() else {
        return Err("no party".to_string());
    };
    let open = table.invites.contains(&(inviting, mine.party_id));
    rules::may_answer(mine.leader == here.me, open).map_err(why)?;
    table.invites.remove(&(inviting, mine.party_id));
    let mut plan = Plan::new();
    to_all(
        &mine.members,
        &[party_packets::party_request_response(inviting)],
        &mut plan,
    );
    to_all(
        &table.members(inviting),
        &[party_packets::party_invite_response(mine.party_id)],
        &mut plan,
    );
    Ok(plan)
}

/// `GAME_CMSG_PARTY_ACCEPT_INVITE` (`0x809C`, one word: the inviting party). The invited party joins
/// it; `party.rs:697-840`.
///
/// A party of one joining (measured):
///
/// - the one joining: `0x01C9`, its add, the size and leader pair, its flags word, then its new party
///   (`0x01B3` with 0: not the leader), `0x01E0 {0}`, `0x01BF {0}` (`025940` seq 2622-2638);
/// - the party it joins: `0x01CA`, the add, the pair, the flags word (`multiplay/party.jsonl` seq
///   160-165);
/// - everyone else in the map: the pair, which every capture carries for parties the client is not in.
///
/// Two parties of more than one merging (experimental, **not captured**): the party whose leader leads
/// the merge survives (`rules::merge_leader`) and every member of the other moves into it. Other
/// invitations the joining party had open are closed first, the way `titles-live-20261106-030218.jsonl`
/// seq 203-204 closes two before an add. The flags word goes out with this server's value 0, as ASURA's
/// does; the live batches also carry `0x003C` for four or five other players, which are not sent.
pub fn handle_accept(table: &mut Table, here: &Here, inviting: u16) -> Answer {
    in_outpost(here)?;
    let me = here.me;
    let Some(mine) = table.party_of(me).cloned() else {
        return Err("no party".to_string());
    };
    let open = table.invites.contains(&(inviting, mine.party_id));
    rules::may_answer(mine.leader == me, open).map_err(why)?;
    let Some(theirs) = table.parties.get(&inviting).cloned() else {
        return Err("the inviting party is gone".to_string());
    };
    if theirs.moving || mine.moving {
        return Err("a party is leaving the map".to_string());
    }
    rules::may_fill(theirs.members.len() + mine.members.len(), here.max_size).map_err(why)?;
    let mut plan = Plan::new();
    // Every other invitation either party had open, either way round, is closed with it. The accepting
    // party's are closed by its own answer; the inviting party's other invitations stand.
    let (invited, invited_by) = table.close_invites_of(mine.party_id);
    for other in invited.into_iter().filter(|other| *other != inviting) {
        to_all(
            &mine.members,
            &[party_packets::party_invite_cancel(other)],
            &mut plan,
        );
        to_all(
            &table.members(other),
            &[party_packets::party_request_cancel(mine.party_id)],
            &mut plan,
        );
    }
    for other in invited_by.into_iter().filter(|other| *other != inviting) {
        to_all(
            &mine.members,
            &[party_packets::party_request_response(other)],
            &mut plan,
        );
        to_all(
            &table.members(other),
            &[party_packets::party_invite_response(mine.party_id)],
            &mut plan,
        );
    }
    table.invites.remove(&(inviting, mine.party_id));

    // The merge: who leads, and so which party survives.
    let (keep, moving_in, movers_answer, keepers_answer) =
        match rules::merge_leader(theirs.members.len(), mine.members.len()) {
            rules::MergeLeader::Inviting => (
                theirs,
                mine.clone(),
                party_packets::party_request_response(inviting),
                party_packets::party_invite_response(mine.party_id),
            ),
            rules::MergeLeader::Invited => (
                mine.clone(),
                theirs,
                party_packets::party_invite_response(mine.party_id),
                party_packets::party_request_response(inviting),
            ),
        };
    table.parties.remove(&moving_in.party_id);
    table.close_invites_of(moving_in.party_id);
    let Some(party) = table.parties.get_mut(&keep.party_id) else {
        return Err("the surviving party is gone".to_string());
    };
    for mover in &moving_in.members {
        party.members.push(*mover);
        if let Some(absence) = moving_in.absent.get(mover) {
            party.absent.insert(*mover, *absence);
        }
    }
    party.countdown = None;
    let party = party.clone();
    for mover in &moving_in.members {
        table.of_player.insert(*mover, keep.party_id);
    }

    let size = party.members.len() as u32;
    let movers = &moving_in.members;
    // The movers: the answer, then - for one player - its own add as the capture has it, or for a party
    // the whole surviving stream, then each mover's pair and flags word, then their new party.
    let mut moving_batch = vec![movers_answer];
    if movers.len() == 1 && keep.members.len() == 1 {
        moving_batch.push(party_packets::party_player_add(
            keep.party_id,
            id(movers[0]),
            1,
        ));
    } else {
        moving_batch.extend(member_stream(&party));
    }
    let mut keeping_batch = vec![keepers_answer];
    for mover in movers {
        let pair = size_and_leader(party.leader, size, *mover);
        let flags = misc_packets::player_update_flags(id(*mover), 0, 7);
        moving_batch.extend(pair.clone());
        moving_batch.push(flags.clone());
        keeping_batch.push(party_packets::party_player_add(
            keep.party_id,
            id(*mover),
            u8::from(!party.absent.contains_key(mover)),
        ));
        keeping_batch.extend(pair);
        keeping_batch.push(flags);
    }
    moving_batch.push(character_packets::character_update_party(keep.party_id, 0));
    moving_batch.push(party_packets::party_search_seek(0));
    moving_batch.push(party_packets::party_set_difficulty(0));
    to_all(movers, &moving_batch, &mut plan);
    to_all(&keep.members, &keeping_batch, &mut plan);

    let mut pairs = Vec::new();
    for mover in movers {
        pairs.extend(size_and_leader(party.leader, size, *mover));
    }
    to_all(&bystanders(here, &party.members), &pairs, &mut plan);
    Ok(plan)
}

/// `GAME_CMSG_PARTY_LEAVE_GROUP` (`0x80A2`, no fields); `party.rs:842-866`. A player leaves its party
/// and is put in a party of its own; a leader that leaves hands the party to the next member (`034507`
/// seq 3304-3346: party `0x29` goes on under player `0x0E`).
pub fn handle_leave(table: &mut Table, here: &Here) -> Answer {
    in_outpost(here)?;
    let Some(old) = table.party_of(here.me).cloned() else {
        return Err("no party".to_string());
    };
    rules::may_leave(old.members.len()).map_err(why)?;
    Ok(departure(table, here, &old, here.me, false))
}

/// A player leaves its party in order to travel alone (`rules::TravelRequest::LeavesParty`): the same
/// batch as [`handle_leave`] (`party.rs:868-896`). Answers nothing when it had no company to leave.
pub fn leave_to_travel(table: &mut Table, here: &Here) -> Option<Plan> {
    let old = table.party_of(here.me).cloned()?;
    if old.members.len() <= 1 {
        return None;
    }
    Some(departure(table, here, &old, here.me, false))
}

/// The world-map form of a member's leave (`0x80B1` with its confirmation flag 0); `party.rs:898-925`.
///
/// The departing player is still sent the full batch of a leave, because the client needs its new party
/// of one before it will act on the redirect. The members staying do **not** get the remove (`0x01D1`):
/// a live server keeps the name in their party window and greys it, with the departing player's leader
/// set to 0 (`0x00B1 {player, 0}` in `multiplay-live-20261106-200444.jsonl` seq 746).
pub fn leave_to_travel_grey(table: &mut Table, here: &Here) -> Option<Plan> {
    let old = table.party_of(here.me).cloned()?;
    if old.members.len() <= 1 {
        return None;
    }
    Some(departure(table, here, &old, here.me, true))
}

/// `GAME_CMSG_PARTY_KICK_PLAYER` (`0x80A9`, one word: the player); `party.rs:1037-1090`. The leader puts
/// a member out.
///
/// The leader's batch is measured (`034507` seq 2990-3001). The kicked player's is **not captured**; it
/// is sent the batch of a member that leaves.
pub fn handle_kick(table: &mut Table, here: &Here, target: u32) -> Answer {
    in_outpost(here)?;
    let Some(mine) = table.party_of(here.me).cloned() else {
        return Err("no party".to_string());
    };
    rules::may_kick(
        mine.leader == here.me,
        mine.members.contains(&target),
        target == here.me,
    )
    .map_err(why)?;
    Ok(departure(table, here, &mine, target, false))
}

/// One player goes from `old`: a leave, a kick, or a departure from the map. Answers every client's
/// frames; the one going is put in a new party of its own (`party.rs:1060-1206`).
///
/// - the one going (`025940` seq 2437-2458 for a member, `034507` seq 3309-3346 for the leader): the
///   remove, then - if it led the party - the handover to the next member (`0x00B0 {new, n}`,
///   `0x00B1 {it, new}`, `0x00B1 {new, new}`), the party's new size, its own pair twice around its flags
///   word, the stream of its new party, `0x01B3` with 1, `0x01E0 {0}`, `0x01BF {0}`;
/// - the members staying (`034507` seq 2994-3001, the leader's side of a kick): the remove, the handover
///   if there is one, the new size, then the one going's pair twice around its flags word; a new leader
///   then gets `0x01CB {1}` (`025940` seq 627-631);
/// - everyone else in the map: the pairs.
///
/// `grey_staying` is the world-map travel form: the staying members get no remove, only the new size,
/// the one going's leader 0, and `0x01CE` - which keeps the name in their party window, greyed.
fn departure(table: &mut Table, here: &Here, old: &Party, going: u32, grey_staying: bool) -> Plan {
    let remaining: Vec<u32> = old
        .members
        .iter()
        .copied()
        .filter(|m| *m != going)
        .collect();
    let size_before = old.members.len() as u32;
    let size = remaining.len() as u32;
    let new_leader = if old.leader == going {
        remaining
            .iter()
            .copied()
            .find(|m| !old.absent.contains_key(m))
            .or_else(|| remaining.first().copied())
    } else {
        Some(old.leader)
    };
    if let Some(party) = table.parties.get_mut(&old.party_id) {
        party.members.retain(|member| *member != going);
        party.absent.remove(&going);
        if let Some(leader) = new_leader {
            if party.leader != leader {
                party.countdown = None;
            }
            party.leader = leader;
            // The leader stands first, the order every handover reads.
            if let Some(at) = party.members.iter().position(|m| *m == leader) {
                let leader = party.members.remove(at);
                party.members.insert(0, leader);
            }
        }
    }
    if remaining.is_empty() {
        table.parties.remove(&old.party_id);
    }

    let fresh = table.found(going);
    let new_party = Party {
        party_id: fresh,
        leader: going,
        members: vec![going],
        ..Party::default()
    };

    let remove = party_packets::party_player_remove(old.party_id, id(going));
    let mut handover = Vec::new();
    if let (true, Some(leader)) = (old.leader == going, new_leader) {
        handover.push(party_packets::player_update_party_size(leader, size_before));
        handover.push(party_packets::player_update_party(going, leader));
        handover.push(party_packets::player_update_party(leader, leader));
    }
    let new_size = party_packets::player_update_party_size(new_leader.unwrap_or(going), size);
    let own = size_and_leader(going, 1, going);
    let flags = misc_packets::player_update_flags(id(going), 0, 7);

    let mut leaving = vec![remove.clone()];
    leaving.extend(handover.iter().cloned());
    leaving.push(new_size.clone());
    leaving.extend(own.clone());
    leaving.push(flags.clone());
    leaving.extend(own.clone());
    leaving.extend(member_stream(&new_party));
    leaving.push(character_packets::character_update_party(fresh, 1));
    leaving.push(party_packets::party_search_seek(0));
    leaving.push(party_packets::party_set_difficulty(0));

    let mut staying = Vec::new();
    if grey_staying {
        // The measured "this player left the map" message, before the grey flag. A travel keeps the
        // name, so the remove is the one part it does not send (ASURA `docs/44-party-invites.md`).
        staying.push(party_packets::party_member_leave_world(
            old.party_id,
            id(going),
        ));
    } else {
        // The live "left the map" pair: the world notice, then the row removed. The window kept a
        // shadow row when only the remove was sent.
        staying.push(party_packets::party_member_leave_world(
            old.party_id,
            id(going),
        ));
        staying.push(remove);
    }
    staying.extend(handover.iter().cloned());
    staying.push(new_size.clone());
    if grey_staying {
        staying.push(party_packets::player_update_party(going, 0));
        staying.push(party_packets::party_member_disconnect_soft(
            old.party_id,
            id(going),
        ));
    } else {
        staying.extend(own.clone());
        staying.push(flags);
        staying.extend(own.clone());
    }

    let mut plan = Plan::new();
    plan.push((going, leaving));
    to_all(&remaining, &staying, &mut plan);
    if let (true, Some(leader)) = (old.leader == going, new_leader) {
        plan.push((leader, vec![party_packets::party_you_are_leader(1)]));
    }
    let mut pairs = handover;
    pairs.push(new_size);
    if !grey_staying {
        pairs.extend(own);
    }
    to_all(&bystanders(here, &old.members), &pairs, &mut plan);
    plan
}

/// A player leaves the map (a logout, a dropped connection, or a travel), before the copy forgets it;
/// `party.rs:1852-1944`, without the explorable disconnect hold, which arrives with party travel.
///
/// Parties that invited it, or that it invited, are told it left the map, then that it is out of its
/// party, then that the invitation is closed: `0x01D0`, `0x01D1`, `0x01CA` (`034507` seq 2645-2650, an
/// invited player walking out). The live batch has the player's farewell between the first two; here,
/// as in ASURA, the farewell follows the party messages. For the party it had invited the closing message
/// is `0x01C8` (**not captured**); its own party, if it has company, sees the departure above (a leader's
/// hands the party on). Nothing is addressed to the player itself: its connection is closing.
pub fn left_instance(table: &mut Table, here: &Here) -> Plan {
    let me = here.me;
    let Some(mine) = table.party_of(me).cloned() else {
        return Plan::new();
    };
    if mine.moving {
        table.drop_quietly(mine.party_id, me);
        return Plan::new();
    }
    // An explorable keeps the member's place: the others see the shadow row, and the member is absent
    // until it returns or the copy ends. This is the soft-disconnect form ASURA keeps for an
    // explorable with company (`party.rs:1869-1895`, `holds_on_disconnect`).
    let others_present: Vec<u32> = mine
        .present()
        .into_iter()
        .filter(|member| *member != me)
        .collect();
    if !here.in_outpost && !others_present.is_empty() {
        if let Some(party) = table.parties.get_mut(&mine.party_id) {
            party.absent.insert(
                me,
                Absence {
                    held: true,
                    epoch: 0,
                },
            );
        }
        let mut plan = Plan::new();
        // The grey party row: the member's leader becomes 0, then the soft disconnect. Sending only
        // the soft disconnect left the client with no row at all.
        to_all(
            &others_present,
            &[
                party_packets::player_update_party(me, 0),
                party_packets::party_member_disconnect_soft(mine.party_id, id(me)),
            ],
            &mut plan,
        );
        return plan;
    }
    let mut plan = Plan::new();
    let (invited, invited_by) = table.close_invites_of(mine.party_id);
    let gone = [
        party_packets::party_member_leave_world(mine.party_id, id(me)),
        party_packets::party_player_remove(mine.party_id, id(me)),
    ];
    for other in invited_by {
        let mut frames = gone.to_vec();
        frames.push(party_packets::party_invite_response(mine.party_id));
        to_all(&table.members(other), &frames, &mut plan);
    }
    for other in invited {
        to_all(
            &table.members(other),
            &[party_packets::party_request_cancel(mine.party_id)],
            &mut plan,
        );
    }
    if mine.members.len() > 1 {
        plan.extend(departure(table, here, &mine, me, false));
    }
    // Whatever party it is in now goes with it.
    if let Some(last) = table.of_player.remove(&me) {
        table.parties.remove(&last);
        table.close_invites_of(last);
    }
    plan.into_iter().filter(|(to, _)| *to != me).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(frames: &[Frame]) -> Vec<u16> {
        frames.iter().map(|frame| header(frame)).collect()
    }

    fn body(frame: &[u8]) -> Vec<u8> {
        frame[2..].to_vec()
    }

    fn outpost(me: u32, everyone: &[u32]) -> Here {
        Here {
            me,
            in_outpost: true,
            max_size: Some(2),
            everyone: everyone.to_vec(),
        }
    }

    fn sent_to(plan: &Plan, player: u32) -> Vec<Frame> {
        plan.iter()
            .filter(|(to, _)| *to == player)
            .flat_map(|(_, frames)| frames.clone())
            .collect()
    }

    /// `party.rs:1981-2000`.
    #[test]
    fn the_load_ends_with_the_party_state_before_its_finish() {
        let mut block = vec![
            character_packets::character_update_party(1, 1),
            instance_packets::instance_load_finish(),
        ];
        splice_load_tail(&mut block, true);
        assert_eq!(
            headers(&block),
            vec![
                character_packets::CHARACTER_UPDATE_PARTY,
                party_packets::PARTY_SEARCH_SEEK,
                party_packets::PARTY_SET_DIFFICULTY,
                party_packets::PARTY_GLOBAL_STATE_FLAGS,
                party_packets::PARTY_SEARCH_SEEK,
                instance_packets::INSTANCE_LOAD_FINISH,
            ]
        );
    }

    /// `party.rs:2002-2013`.
    #[test]
    fn a_party_of_one_per_player_with_the_lowest_free_id() {
        let mut table = Table::default();
        assert_eq!(table.found(1), 1);
        assert_eq!(table.found(2), 2);
        assert_eq!(table.leader_and_size(2), (2, 1));
        assert_eq!(
            table.leader_and_size(9),
            (9, 1),
            "an unknown player is a party of one"
        );
    }

    /// `party.rs:2015-2033`.
    #[test]
    fn closing_a_partys_invitations_closes_both_directions() {
        let mut table = Table::default();
        let a = table.found(1);
        let b = table.found(2);
        let c = table.found(3);
        table.invites.insert((a, b));
        table.invites.insert((c, a));
        table.invites.insert((b, c));
        let (invited, invited_by) = table.close_invites_of(a);
        assert_eq!(invited, vec![b]);
        assert_eq!(invited_by, vec![c]);
        assert_eq!(
            table.invites.len(),
            1,
            "an invitation between two other parties stays"
        );
    }

    /// `multiplay/party.jsonl` seq 2662-2669, the load of a leader whose party of two has just come
    /// through a doorway, the second member not in the map yet (`party.rs:2035-2076`).
    #[test]
    fn a_party_arriving_loads_as_the_live_server_sends_it() {
        let mut party = Party {
            party_id: 1,
            leader: 1,
            members: vec![1, 2],
            ..Party::default()
        };
        party.absent.insert(
            2,
            Absence {
                held: false,
                epoch: 1,
            },
        );
        let section = load_section(&party, 1);
        let bodies: Vec<(u16, Vec<u8>)> = section
            .iter()
            .map(|frame| (header(frame), body(frame)))
            .collect();
        assert_eq!(
            bodies,
            vec![
                (party_packets::PLAYER_UPDATE_PARTY_SIZE, vec![1, 0, 2]),
                (party_packets::PLAYER_UPDATE_PARTY, vec![1, 0, 1, 0]),
                (party_packets::PLAYER_UPDATE_PARTY, vec![2, 0, 1, 0]),
                (party_packets::PARTY_CREATE, vec![1, 0]),
                (party_packets::PARTY_PLAYER_ADD, vec![1, 0, 1, 0, 1]),
                (party_packets::PARTY_PLAYER_ADD, vec![1, 0, 2, 0, 0]),
                (party_packets::PARTY_MEMBER_STREAM_END, vec![1, 0]),
                (character_packets::CHARACTER_UPDATE_PARTY, vec![1, 0, 1]),
            ]
        );
        assert_eq!(
            load_section(&party, 2).last().map(|frame| body(frame)),
            Some(vec![1, 0, 0]),
            "a member that does not lead is told 0"
        );
    }

    /// `party.rs:2078-2114`.
    #[test]
    fn the_section_replaces_the_blocks_own_party_and_keeps_what_is_in_front() {
        let mut block = vec![
            instance_packets::instance_load_finish(),
            party_packets::party_create(1),
            party_packets::party_player_add(1, 1, 1),
            party_packets::party_member_stream_end(1),
            character_packets::character_update_party(1, 1),
            instance_packets::instance_load_finish(),
        ];
        let party = Party {
            party_id: 4,
            leader: 1,
            members: vec![1],
            ..Party::default()
        };
        splice_load_section(&mut block, &party, 1);
        assert_eq!(
            headers(&block),
            vec![
                instance_packets::INSTANCE_LOAD_FINISH,
                party_packets::PLAYER_UPDATE_PARTY_SIZE,
                party_packets::PLAYER_UPDATE_PARTY,
                party_packets::PARTY_CREATE,
                party_packets::PARTY_PLAYER_ADD,
                party_packets::PARTY_MEMBER_STREAM_END,
                character_packets::CHARACTER_UPDATE_PARTY,
                instance_packets::INSTANCE_LOAD_FINISH,
            ]
        );
    }

    /// In its own load a client is in the map and every other member is not, whoever entered first
    /// (`party.rs:2116-2135`).
    #[test]
    fn a_load_shows_only_its_own_player_in_the_map() {
        let party = Party {
            party_id: 1,
            leader: 1,
            members: vec![1, 2],
            ..Party::default()
        };
        let flags = |me: u32| -> Vec<u8> {
            load_section(&party, me)
                .iter()
                .filter(|frame| header(frame) == party_packets::PARTY_PLAYER_ADD)
                .map(|frame| frame[6])
                .collect()
        };
        assert_eq!(flags(1), vec![1, 0]);
        assert_eq!(flags(2), vec![0, 1]);
        assert_eq!(
            headers(&in_map_notices(&party, 2, &[1])),
            vec![party_packets::PARTY_MEMBER_ENTER_WORLD]
        );
        assert!(in_map_notices(&party, 2, &[]).is_empty());
    }

    /// The world-map leave keeps the name on the staying side but still gives the leaver the full leave
    /// batch (`party.rs:2137-2181`).
    #[test]
    fn a_grey_travel_departure_keeps_the_name_and_sends_the_leaver_its_own_party() {
        let mut table = Table::default();
        let old = table.found(1);
        table.parties.get_mut(&old).unwrap().members.push(2);
        table.of_player.insert(2, old);
        let old = table.party_of(1).cloned().unwrap();

        let plan = departure(&mut table, &outpost(2, &[1, 2]), &old, 2, true);
        let staying = headers(&sent_to(&plan, 1));
        assert!(
            !staying.contains(&party_packets::PARTY_PLAYER_REMOVE),
            "the staying side is not told to remove the name"
        );
        assert!(staying.contains(&party_packets::PARTY_MEMBER_LEAVE_WORLD));
        assert!(staying.contains(&party_packets::PARTY_MEMBER_DISCONNECT_SOFT));
        assert!(staying.contains(&party_packets::PLAYER_UPDATE_PARTY));

        let leaving = headers(&sent_to(&plan, 2));
        assert!(
            leaving.contains(&party_packets::PARTY_PLAYER_REMOVE),
            "the leaver still gets its own leave batch"
        );
        assert!(
            leaving.contains(&party_packets::PARTY_CREATE),
            "and its new party of one"
        );
    }

    /// `party.rs:2183-2202`.
    #[test]
    fn a_party_knows_who_is_in_the_map() {
        let mut party = Party {
            party_id: 1,
            leader: 1,
            members: vec![1, 2, 3],
            ..Party::default()
        };
        party.absent.insert(
            3,
            Absence {
                held: true,
                epoch: 1,
            },
        );
        assert_eq!(party.present(), vec![1, 2]);
        assert!(party.is_present(2));
        assert!(!party.is_present(3));
        assert!(!party.is_present(9));
    }

    /// `party.rs:2204-2222`.
    #[test]
    fn a_quiet_drop_hands_the_party_on_and_ends_it_with_its_last_member() {
        let mut table = Table::default();
        let party_id = table.found(1);
        {
            let party = table.parties.get_mut(&party_id).expect("founded");
            party.members.push(2);
            party.moving = true;
        }
        table.of_player.insert(2, party_id);
        table.drop_quietly(party_id, 1);
        assert_eq!(
            table.parties.get(&party_id).map(|party| party.leader),
            Some(2)
        );
        table.drop_quietly(party_id, 2);
        assert!(table.parties.is_empty());
        assert!(table.of_player.is_empty());
    }

    /// The whole outpost conversation of two players, end to end through the table: an invitation, its
    /// accept, the leader's kick, and a leave - each batch in the order the captures carry it.
    #[test]
    fn an_invitation_accept_and_kick_run_through_the_table() {
        let mut table = Table::default();
        table.found(1);
        table.found(2);
        table.found(3);
        let here = |me| outpost(me, &[1, 2, 3]);

        // Player 1 invites player 2: the inviter sees party 2's stream and `0x01C5 {2}`, the invited
        // sees party 1's stream and `0x01C6 {1}` (`032434` seq 380-384, `025940` seq 2539-2542).
        let plan = handle_invite(&mut table, &here(1), 2).expect("an invitation");
        assert_eq!(
            headers(&sent_to(&plan, 1)),
            vec![0x01D3, 0x01CC, 0x01D4, party_packets::PARTY_INVITE_ADD]
        );
        assert_eq!(
            headers(&sent_to(&plan, 2)),
            vec![0x01D3, 0x01CC, 0x01D4, party_packets::PARTY_JOIN_REQUEST]
        );
        assert!(
            sent_to(&plan, 3).is_empty(),
            "a bystander hears nothing yet"
        );
        assert!(
            handle_invite(&mut table, &here(1), 2).is_err(),
            "the same invitation twice is refused"
        );

        // Player 2 accepts party 1's invitation.
        let plan = handle_accept(&mut table, &here(2), 1).expect("an accept");
        assert_eq!(
            headers(&sent_to(&plan, 2)),
            vec![
                party_packets::PARTY_REQUEST_RESPONSE,
                party_packets::PARTY_PLAYER_ADD,
                party_packets::PLAYER_UPDATE_PARTY_SIZE,
                party_packets::PLAYER_UPDATE_PARTY,
                misc_packets::PLAYER_UPDATE_FLAGS,
                character_packets::CHARACTER_UPDATE_PARTY,
                party_packets::PARTY_SEARCH_SEEK,
                party_packets::PARTY_SET_DIFFICULTY,
            ]
        );
        assert_eq!(
            headers(&sent_to(&plan, 1)),
            vec![
                party_packets::PARTY_INVITE_RESPONSE,
                party_packets::PARTY_PLAYER_ADD,
                party_packets::PLAYER_UPDATE_PARTY_SIZE,
                party_packets::PLAYER_UPDATE_PARTY,
                misc_packets::PLAYER_UPDATE_FLAGS,
            ]
        );
        assert_eq!(
            headers(&sent_to(&plan, 3)),
            vec![
                party_packets::PLAYER_UPDATE_PARTY_SIZE,
                party_packets::PLAYER_UPDATE_PARTY
            ],
            "a bystander is told the pair"
        );
        assert_eq!(table.leader_and_size(2), (1, 2));
        assert!(
            handle_invite(&mut table, &here(1), 3).is_err(),
            "pre-Searing's two is full"
        );

        // The leader kicks player 2, who is put in a party of its own.
        let plan = handle_kick(&mut table, &here(1), 2).expect("a kick");
        assert_eq!(
            headers(&sent_to(&plan, 2))[0],
            party_packets::PARTY_PLAYER_REMOVE
        );
        assert_eq!(table.leader_and_size(2), (2, 1));
        assert_eq!(table.leader_and_size(1), (1, 1));
        assert!(
            handle_leave(&mut table, &here(1)).is_err(),
            "a party of one has nobody to leave"
        );
    }

    /// A leader that leaves hands the party on: the next member is told `0x01CB {1}`.
    #[test]
    fn a_leader_that_leaves_hands_the_party_on() {
        let mut table = Table::default();
        table.found(1);
        table.found(2);
        let here = |me| Here {
            max_size: Some(4),
            ..outpost(me, &[1, 2])
        };
        handle_invite(&mut table, &here(1), 2).unwrap();
        handle_accept(&mut table, &here(2), 1).unwrap();
        let plan = handle_leave(&mut table, &here(1)).expect("a leave");
        assert_eq!(table.leader_and_size(2), (2, 1));
        assert!(
            headers(&sent_to(&plan, 2)).contains(&party_packets::PARTY_YOU_ARE_LEADER),
            "the new leader is told"
        );
    }

    /// Parties change only in outposts (`rules::may_change_party`).
    #[test]
    fn nothing_changes_outside_an_outpost() {
        let mut table = Table::default();
        table.found(1);
        table.found(2);
        let here = Here {
            in_outpost: false,
            ..outpost(1, &[1, 2])
        };
        assert!(handle_invite(&mut table, &here, 2).is_err());
    }

    /// A player walking out of the map closes the invitations it was in, either way round, and its party
    /// hears its departure; nothing is addressed to the player itself.
    #[test]
    fn leaving_the_map_closes_the_invitations_it_was_in() {
        let mut table = Table::default();
        table.found(1);
        table.found(2);
        handle_invite(&mut table, &outpost(1, &[1, 2]), 2).unwrap();
        let plan = left_instance(&mut table, &outpost(2, &[1, 2]));
        assert_eq!(
            headers(&sent_to(&plan, 1)),
            vec![
                party_packets::PARTY_MEMBER_LEAVE_WORLD,
                party_packets::PARTY_PLAYER_REMOVE,
                party_packets::PARTY_INVITE_RESPONSE
            ],
            "`034507` seq 2645-2650"
        );
        assert!(sent_to(&plan, 2).is_empty());
        assert!(table.party_of(2).is_none());
        assert_eq!(table.leader_and_size(1), (1, 1));
    }

    /// A member leaving an adopted party is removed from the other client's window, not left as a
    /// shadow. The remove pair is `0x01D0` then `0x01D1`.
    #[test]
    fn a_member_leaving_an_adopted_party_removes_its_row() {
        let mut table = Table::default();
        table.adopt(
            Party {
                party_id: 3,
                leader: 1,
                members: vec![1, 2],
                ..Party::default()
            },
            1,
        );
        table.arrive_member(2);
        let plan = left_instance(&mut table, &outpost(2, &[1, 2]));
        assert_eq!(
            &headers(&sent_to(&plan, 1))[..2],
            &[
                party_packets::PARTY_MEMBER_LEAVE_WORLD,
                party_packets::PARTY_PLAYER_REMOVE
            ]
        );
        assert!(table.party_of(2).is_none());
    }

    /// An explorable keeps the member as a shadow for the others: no remove, and the row stays.
    #[test]
    fn a_member_leaving_an_explorable_stays_as_a_shadow() {
        let mut table = Table::default();
        table.adopt(
            Party {
                party_id: 3,
                leader: 1,
                members: vec![1, 2],
                ..Party::default()
            },
            1,
        );
        table.arrive_member(2);
        let here = Here {
            me: 2,
            in_outpost: false,
            max_size: Some(4),
            everyone: vec![1, 2],
        };
        let plan = left_instance(&mut table, &here);
        assert_eq!(
            headers(&sent_to(&plan, 1)),
            vec![
                party_packets::PLAYER_UPDATE_PARTY,
                party_packets::PARTY_MEMBER_DISCONNECT_SOFT
            ]
        );
        assert!(table.party_of(1).unwrap().absent.contains_key(&2));
        assert!(table.party_of(2).is_some(), "the shadow keeps its place");
    }

    /// A crossing carries the roster; the first arrival adopts it and the others stop being absent as
    /// their own connections land.
    #[test]
    fn a_crossing_roster_is_adopted_and_its_members_arrive() {
        let mut table = Table::default();
        let party = Party {
            party_id: 7,
            leader: 1,
            members: vec![1, 2],
            ..Party::default()
        };
        assert_eq!(table.adopt(party, 1), 7);
        assert_eq!(table.party_of(1).unwrap().present(), vec![1]);
        assert_eq!(table.leader_and_size(1), (1, 2));
        table.arrive_member(2);
        assert_eq!(table.party_of(2).unwrap().present(), vec![1, 2]);
    }
}
