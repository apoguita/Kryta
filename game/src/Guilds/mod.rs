//! `game/Guilds/` - the reference's `src/server/game/Guilds/` (4 files): the guild, its members and its
//! rules, and the frames a load and an introduction carry for it.
//!
//! Ported 1:1 from ASURA, three files:
//!
//! - the rules: `ASURA/crates/asura-rules/src/guild.rs` (277 lines, all carried: `Refused` `:13-42`,
//!   `fits` `:44-54`, `Creation`/`may_create` `:56-82`, `Target`/`may_invite` `:84-125`,
//!   `Departure`/`may_leave` `:127-146`, `may_announce` `:148-157`, the four tests `:159-276`);
//! - the world half that is not a request: `ASURA/crates/asura-world/src/guild.rs:35-200` -
//!   `Membership`, `Badge`, the local ids, `badge_frames`, `load_frames`, `splice_into_player_block` -
//!   and its three tests `:749-817`. The four requests (`:202-710`) are `Handlers/GuildHandler.rs`;
//! - the settings: `ASURA/crates/asura-config/src/lib.rs:916-966`, `Guilds`, read here from
//!   `config/Server/guilds.json` (`Settings::load`).
//!
//! The rows are `Guilds/Repository.rs` (`ASURA/crates/asura-db/src/guilds.rs`).
//!
//! **Gold is not charged** (ASURA owner ruling, 2026-11-06: free for now, gold a placeholder). The two
//! prices in `guilds.json` are read and logged only, as ASURA reads them.

#[path = "Repository.rs"]
pub mod repository;

pub use repository::{Cape, DatabaseGuilds, Guild, Member, NewGuild};

use crate::server::packets::{agent_packets, guild_packets, party_packets};

// ---- the rules (`ASURA/crates/asura-rules/src/guild.rs`) -----------------------------------------

/// Why a guild request is refused. Which reason the live server gives is **not** captured: four refused
/// invitations all came back with the same code, so the reasons below are ASURA's, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The character is already in a guild, so it may not create one.
    InGuild,
    /// The character is in no guild, so there is nothing to invite to, leave or announce in.
    NotInGuild,
    /// Only the leader invites and sets the announcement (the only permitted type that is measured).
    NotLeader,
    /// A guild name or tag must have at least one unit.
    Empty,
    /// A name, tag or announcement that fills its field. Refused rather than cut.
    TooLong,
    /// No character has that name.
    UnknownCharacter,
    /// The character named is not online in this world.
    Offline,
    /// The character named is in a guild already.
    TargetInGuild,
    /// The character named has an open invitation from this guild already.
    AlreadyInvited,
    /// A character does not invite itself.
    InvitesSelf,
    /// A leader leaves only when no other member is left; it does not leave members leaderless.
    LeaderNotAlone,
}

/// May a field of `units` units be stored in a field that holds `capacity`? Empty is refused only where
/// the caller says so: an announcement may be cleared.
pub fn fits(units: usize, capacity: usize, may_be_empty: bool) -> Result<(), Refused> {
    if units == 0 && !may_be_empty {
        return Err(Refused::Empty);
    }
    if units >= capacity {
        return Err(Refused::TooLong);
    }
    Ok(())
}

/// What a creation is decided on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Creation {
    /// Whether the founder is in a guild now.
    pub in_guild: bool,
    /// The name's length in units.
    pub name_units: usize,
    /// The name field's capacity.
    pub name_capacity: usize,
    /// The tag's length in units.
    pub tag_units: usize,
    /// The tag field's capacity.
    pub tag_capacity: usize,
}

/// May this guild be created? A name already taken is the database's to refuse: it is the only place
/// that sees every guild.
pub fn may_create(creation: Creation) -> Result<(), Refused> {
    if creation.in_guild {
        return Err(Refused::InGuild);
    }
    fits(creation.name_units, creation.name_capacity, false)?;
    fits(creation.tag_units, creation.tag_capacity, false)?;
    Ok(())
}

/// What the invited character is, as the caller found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// No character has the name.
    Unknown,
    /// A character with the name.
    Found {
        /// It is the inviter.
        is_self: bool,
        /// It is connected to this world.
        online: bool,
        /// It is in a guild.
        in_guild: bool,
        /// The inviter's guild has invited it already.
        already_invited: bool,
    },
}

/// May this invitation be made? `inviter_leads` is `None` when the inviter is in no guild.
pub fn may_invite(inviter_leads: Option<bool>, target: Target) -> Result<(), Refused> {
    match inviter_leads {
        None => return Err(Refused::NotInGuild),
        Some(false) => return Err(Refused::NotLeader),
        Some(true) => {}
    }
    match target {
        Target::Unknown => Err(Refused::UnknownCharacter),
        Target::Found { is_self: true, .. } => Err(Refused::InvitesSelf),
        Target::Found { online: false, .. } => Err(Refused::Offline),
        Target::Found { in_guild: true, .. } => Err(Refused::TargetInGuild),
        Target::Found {
            already_invited: true,
            ..
        } => Err(Refused::AlreadyInvited),
        Target::Found { .. } => Ok(()),
    }
}

/// What a departure does to the guild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Departure {
    /// The member goes; the guild stays.
    Leaves,
    /// The leader was the last member: the guild goes with it, open invitations included.
    Dissolves,
}

/// May this character leave its guild, and what happens to the guild? `leads` is `None` when it is in
/// no guild; `others` counts the other members (open invitations are not members).
pub fn may_leave(leads: Option<bool>, others: usize) -> Result<Departure, Refused> {
    match leads {
        None => Err(Refused::NotInGuild),
        Some(false) => Ok(Departure::Leaves),
        Some(true) if others == 0 => Ok(Departure::Dissolves),
        Some(true) => Err(Refused::LeaderNotAlone),
    }
}

/// May this character set its guild's announcement to a text of `units` units?
pub fn may_announce(leads: Option<bool>, units: usize, capacity: usize) -> Result<(), Refused> {
    match leads {
        None => return Err(Refused::NotInGuild),
        Some(false) => return Err(Refused::NotLeader),
        Some(true) => {}
    }
    fits(units, capacity, true)
}

// ---- the settings (`ASURA/crates/asura-config/src/lib.rs:916-966`) --------------------------------

/// `config/Server/guilds.json`: what a new guild starts with, the `0x0133` counter's start and step, and
/// the two placeholder prices. Every key is ASURA's, with its `{"value", "doc"}` shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Gold a creation costs. A placeholder: not charged.
    pub creation_price: u32,
    /// Gold an invitation costs. A placeholder: not charged.
    pub invite_fee: u32,
    /// `0x0121`'s feature byte on a new guild.
    pub new_guild_feature: u8,
    /// `0x0121`'s territory byte on a new guild.
    pub new_guild_territory: u8,
    /// A new guild's allegiance.
    pub new_guild_allegiance: u8,
    /// A new guild's ladder rating.
    pub new_guild_rating: u32,
    /// A new guild's ladder position.
    pub new_guild_rank: u32,
    /// The `0x0133` value of the creation batch, and the counter's start.
    pub revision_on_create: u32,
    /// How far the counter moves per later change.
    pub revision_step: u32,
}

/// One `{"value": n, "doc": ...}` entry, as a number of at most `max`.
fn setting(document: &serde_json::Value, key: &str, max: u64) -> Result<u64, String> {
    let number = document
        .get(key)
        .and_then(|entry| entry.get("value"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{key}.value is missing or not a whole number"))?;
    if number > max {
        return Err(format!("{key}.value {number} is above {max}"));
    }
    Ok(number)
}

impl Settings {
    /// Parse the file's text (`Guilds::from_json`).
    pub fn parse(text: &str) -> Result<Self, String> {
        let document: serde_json::Value =
            serde_json::from_str(text).map_err(|error| error.to_string())?;
        let dword = u64::from(u32::MAX);
        let byte = u64::from(u8::MAX);
        Ok(Self {
            creation_price: setting(&document, "creation_price", dword)? as u32,
            invite_fee: setting(&document, "invite_fee", dword)? as u32,
            new_guild_feature: setting(&document, "new_guild_feature", byte)? as u8,
            new_guild_territory: setting(&document, "new_guild_territory", byte)? as u8,
            new_guild_allegiance: setting(&document, "new_guild_allegiance", byte)? as u8,
            new_guild_rating: setting(&document, "new_guild_rating", dword)? as u32,
            new_guild_rank: setting(&document, "new_guild_rank", dword)? as u32,
            revision_on_create: setting(&document, "revision_on_create", dword)? as u32,
            revision_step: setting(&document, "revision_step", dword)? as u32,
        })
    }

    /// Read the file. A missing or malformed file is an error, which stops the program (`AGENTS.md`,
    /// rule 7); ASURA refuses every guild request instead (`asura-world/src/guild.rs:86-95`).
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{} is not readable: {error}", path.display()))?;
        Self::parse(&text)
            .map_err(|error| format!("{} is not a valid guilds file: {error}", path.display()))
    }

    /// What a new guild's row starts with.
    pub fn new_guild(&self) -> NewGuild {
        NewGuild {
            feature: self.new_guild_feature,
            territory: self.new_guild_territory,
            allegiance: self.new_guild_allegiance,
            rating: self.new_guild_rating,
            rank: self.new_guild_rank,
            revision: self.revision_on_create,
        }
    }
}

// ---- the world half (`ASURA/crates/asura-world/src/guild.rs:35-200`) ------------------------------

/// This character's guild and its own member row, as read when the connection was built and as changed
/// by this connection since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    /// The guild.
    pub guild: Guild,
    /// The character's row in it.
    pub member: Member,
}

impl Membership {
    /// Whether the character leads the guild: the only permission that is measured.
    pub fn leads(&self) -> bool {
        self.member.member_type == guild_packets::GUILD_MEMBER_LEADER
    }
}

/// What another client in the copy is told about a player's guild: the guild, and the local id this
/// copy's messages name it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// The id `0x0121` and agent property 30 carry.
    pub local_id: u16,
    /// The guild.
    pub guild: Guild,
}

/// The local ids of the guilds one copy of a map has named (`guild.rs:62-84`, `LocalIds`). The live ids
/// are small server-chosen numbers that change from load to load; the client needs only that two guilds
/// in view never share one, so this hands out the lowest free id and keeps it for the copy's life.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LocalIds {
    ids: std::collections::HashMap<[u8; 16], u16>,
}

impl LocalIds {
    /// The local id of a guild in this copy, allocated on first use.
    pub fn of(&mut self, guild: [u8; 16]) -> u16 {
        if let Some(id) = self.ids.get(&guild) {
            return *id;
        }
        let id = (1..=u16::MAX)
            .find(|candidate| !self.ids.values().any(|taken| taken == candidate))
            .unwrap_or(u16::MAX);
        self.ids.insert(guild, id);
        id
    }
}

/// `0x0121` for one guild (`guild.rs:123-148`).
fn general_info(local_id: u16, guild: &Guild) -> Vec<u8> {
    let cape = guild.cape;
    guild_packets::guild_general_info(&guild_packets::GuildGeneral {
        local_id,
        key: guild.id,
        name: &guild.name,
        tag: &guild.tag,
        feature: guild.feature,
        territory: guild.territory,
        cape: guild_packets::Cape {
            background_color: cape.background_color,
            detail_color: cape.detail_color,
            emblem_color: cape.emblem_color,
            shape: cape.shape,
            detail: cape.detail,
            emblem: cape.emblem,
            trim: cape.trim,
        },
        allegiance: guild.allegiance,
        faction: guild.faction,
        qualifier: guild.qualifier,
        rating: guild.rating,
        rank: guild.rank,
    })
}

/// Agent property 30: the agent's guild, by local id; 0 for none (`guild.rs:150-153`).
pub fn guild_property(agent_id: u32, local_id: u16) -> Vec<u8> {
    agent_packets::agent_property_update_int(
        guild_packets::AGENT_PROPERTY_GUILD,
        agent_id,
        u32::from(local_id),
    )
}

/// A guilded player as another client is told it, right after its player create: `0x0121`, then
/// property 30 on its agent (`guild.rs:155-163`; `pre-searing-load-3.jsonl` seq 299-301).
pub fn badge_frames(badge: &Badge, agent_id: u32) -> [Vec<u8>; 2] {
    [
        general_info(badge.local_id, &badge.guild),
        guild_property(agent_id, badge.local_id),
    ]
}

/// The guild block of a character's own load, just before its party block (`guild.rs:165-187`):
///
/// - in a guild: `0x0121`, property 30 on its agent, `0x0118` with its role, then `0x011C`;
/// - in none: `0x011C` alone.
pub fn load_frames(
    badge: Option<&Badge>,
    membership: Option<&Membership>,
    agent_id: u32,
) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    if let (Some(badge), Some(membership)) = (badge, membership) {
        frames.extend(badge_frames(badge, agent_id));
        frames.push(guild_packets::guild_player_role(
            badge.local_id,
            badge.guild.id,
            membership.member.member_type,
        ));
    }
    frames.push(guild_packets::guild_stream_end());
    frames
}

fn header(frame: &[u8]) -> u16 {
    match frame {
        [low, high, ..] => u16::from_le_bytes([*low, *high]),
        _ => 0,
    }
}

/// Put the guild block in front of the player block's party (`PARTY_CREATE`), where a live load carries
/// it (`guild.rs:189-200`). `false`, and the block untouched, when there is no party message to anchor
/// on.
pub fn splice_into_player_block(block: &mut Vec<Vec<u8>>, guild: Vec<Vec<u8>>) -> bool {
    let Some(at) = block
        .iter()
        .position(|frame| header(frame) == party_packets::PARTY_CREATE)
    else {
        return false;
    };
    block.splice(at..at, guild);
    true
}

/// A fresh random key for a new guild: the sixteen bytes every guild message names it by. ASURA draws a
/// v4 uuid (`guild.rs:345`, `Uuid::new_v4`); this tree draws the same sixteen bytes from the operating
/// system's entropy, as the portal draws its tokens.
pub fn new_key() -> Result<[u8; 16], String> {
    let mut key = [0u8; 16];
    getrandom::getrandom(&mut key).map_err(|error| error.to_string())?;
    // The version and variant bits of a v4 uuid, so the key is what ASURA's would be.
    key[6] = (key[6] & 0x0F) | 0x40;
    key[8] = (key[8] & 0x3F) | 0x80;
    Ok(key)
}

/// The current time in seconds since 1970-01-01 UTC.
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::packets::{agent_packets::AGENT_PROPERTY_UPDATE_INT, character_packets};

    const FOUND: Target = Target::Found {
        is_self: false,
        online: true,
        in_guild: false,
        already_invited: false,
    };

    #[test]
    fn a_creation_needs_no_guild_and_a_name_and_tag_that_fit() {
        let ok = Creation {
            in_guild: false,
            name_units: 10,
            name_capacity: 32,
            tag_units: 4,
            tag_capacity: 6,
        };
        assert_eq!(may_create(ok), Ok(()));
        assert_eq!(
            may_create(Creation {
                in_guild: true,
                ..ok
            }),
            Err(Refused::InGuild)
        );
        assert_eq!(
            may_create(Creation { tag_units: 0, ..ok }),
            Err(Refused::Empty)
        );
        assert_eq!(
            may_create(Creation {
                name_units: 32,
                ..ok
            }),
            Err(Refused::TooLong)
        );
    }

    #[test]
    fn an_invitation_is_a_leaders_to_an_online_unguilded_other() {
        assert_eq!(may_invite(Some(true), FOUND), Ok(()));
        assert_eq!(may_invite(None, FOUND), Err(Refused::NotInGuild));
        assert_eq!(may_invite(Some(false), FOUND), Err(Refused::NotLeader));
        assert_eq!(
            may_invite(Some(true), Target::Unknown),
            Err(Refused::UnknownCharacter)
        );
        let with = |is_self, online, in_guild, already_invited| Target::Found {
            is_self,
            online,
            in_guild,
            already_invited,
        };
        assert_eq!(
            may_invite(Some(true), with(true, true, true, false)),
            Err(Refused::InvitesSelf)
        );
        assert_eq!(
            may_invite(Some(true), with(false, false, false, false)),
            Err(Refused::Offline)
        );
        assert_eq!(
            may_invite(Some(true), with(false, true, true, false)),
            Err(Refused::TargetInGuild)
        );
        assert_eq!(
            may_invite(Some(true), with(false, true, false, true)),
            Err(Refused::AlreadyInvited)
        );
    }

    #[test]
    fn a_leader_leaves_only_alone_and_takes_the_guild_with_it() {
        assert_eq!(may_leave(None, 0), Err(Refused::NotInGuild));
        assert_eq!(may_leave(Some(false), 3), Ok(Departure::Leaves));
        assert_eq!(may_leave(Some(true), 0), Ok(Departure::Dissolves));
        assert_eq!(may_leave(Some(true), 1), Err(Refused::LeaderNotAlone));
    }

    #[test]
    fn an_announcement_is_a_leaders_and_may_be_empty() {
        assert_eq!(may_announce(Some(true), 0, 256), Ok(()));
        assert_eq!(may_announce(Some(true), 256, 256), Err(Refused::TooLong));
        assert_eq!(may_announce(Some(false), 5, 256), Err(Refused::NotLeader));
        assert_eq!(may_announce(None, 5, 256), Err(Refused::NotInGuild));
    }

    #[test]
    fn the_shipped_settings_are_asuras() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/Server/guilds.json");
        let settings = Settings::load(&path).expect("the shipped guilds.json reads");
        assert_eq!(settings.creation_price, 0);
        assert_eq!(settings.invite_fee, 0);
        assert_eq!(settings.new_guild_feature, 0);
        assert_eq!(settings.new_guild_territory, 2);
        assert_eq!(settings.new_guild_allegiance, 4);
        assert_eq!(settings.new_guild_rating, 1000);
        assert_eq!(settings.new_guild_rank, 0);
        assert_eq!(settings.revision_on_create, 512);
        assert_eq!(settings.revision_step, 256);
        assert!(Settings::parse(r#"{"creation_price":{"value":0}}"#).is_err());
    }

    fn guild() -> Guild {
        Guild {
            id: [9; 16],
            name: "Guild".into(),
            tag: "TAG".into(),
            feature: 0,
            territory: 2,
            cape: Cape::default(),
            allegiance: 4,
            faction: 0,
            qualifier: 0,
            rating: 1000,
            rank: 0,
            announcement: String::new(),
            announcement_author: String::new(),
            revision: 0x0200,
        }
    }

    fn membership() -> Membership {
        Membership {
            guild: guild(),
            member: Member {
                member_type: guild_packets::GUILD_MEMBER_LEADER,
                invited_name: "Founder".into(),
                invited_by: "Founder".into(),
                joined_at: 0,
            },
        }
    }

    fn headers(frames: &[Vec<u8>]) -> Vec<u16> {
        frames.iter().map(|frame| header(frame)).collect()
    }

    #[test]
    fn a_guild_keeps_its_local_id_and_two_guilds_never_share_one() {
        let mut ids = LocalIds::default();
        assert_eq!(ids.of([1; 16]), 1);
        assert_eq!(ids.of([2; 16]), 2);
        assert_eq!(ids.of([1; 16]), 1);
    }

    #[test]
    fn an_unguilded_load_is_the_end_marker_alone() {
        assert_eq!(
            headers(&load_frames(None, None, 7)),
            vec![guild_packets::GUILD_STREAM_END]
        );
    }

    #[test]
    fn a_guilded_load_is_info_property_role_end() {
        let membership = membership();
        let badge = Badge {
            local_id: 22,
            guild: membership.guild.clone(),
        };
        let frames = load_frames(Some(&badge), Some(&membership), 7);
        assert_eq!(
            headers(&frames),
            vec![
                guild_packets::GUILD_GENERAL_INFO,
                AGENT_PROPERTY_UPDATE_INT,
                guild_packets::GUILD_PLAYER_ROLE,
                guild_packets::GUILD_STREAM_END,
            ]
        );
        // Property 30, agent 7, local id 22.
        let property = &frames[1][2..];
        assert_eq!(&property[0..4], &30u32.to_le_bytes());
        assert_eq!(&property[4..8], &7u32.to_le_bytes());
        assert_eq!(&property[8..12], &22u32.to_le_bytes());
        // The role names the same guild by the same key.
        assert_eq!(&frames[2][2 + 2..2 + 18], &[9; 16]);
        assert_eq!(frames[2][2 + 18], guild_packets::GUILD_MEMBER_LEADER);
    }

    #[test]
    fn the_guild_block_goes_before_the_party() {
        let mut block = vec![
            agent_packets::world_update_controlled_agent(1, 3),
            party_packets::party_create(1),
            party_packets::party_member_stream_end(1),
        ];
        assert!(splice_into_player_block(
            &mut block,
            load_frames(None, None, 1)
        ));
        assert_eq!(
            headers(&block),
            vec![
                header(&agent_packets::world_update_controlled_agent(1, 3)),
                guild_packets::GUILD_STREAM_END,
                party_packets::PARTY_CREATE,
                header(&party_packets::party_member_stream_end(1)),
            ]
        );
        let mut no_party = vec![character_packets::character_update_info("")];
        assert!(!splice_into_player_block(&mut no_party, Vec::new()));
    }

    #[test]
    fn a_new_key_is_a_version_4_uuid() {
        let key = new_key().expect("entropy");
        assert_eq!(key[6] >> 4, 4);
        assert_eq!(key[8] >> 6, 2);
    }
}
