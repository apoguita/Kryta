//! The guild message family: the server-to-client messages `0x0118`-`0x0133` and the four client
//! requests this world answers (`0x8036`, `0x80B5`, `0x80BB`, `0x80BE`).
//!
//! Reference: `src/server/game/Server/Packets/GuildPackets.{h,cpp}` - one family, one file. Every
//! header, field, order and value here is ported 1:1 from
//! `ASURA/crates/asura-protocol/src/game/guild.rs` (665 lines, all carried: the constants `:17-98`, the
//! date encoding `:100-108`, the builders `:110-303`, the registration window `:305-329`, the readers
//! `:331-432` and the tests `:434-664`). What each message is, field by field, with the capture beside
//! it, is `ASURA/docs/41-guild-system-research.md`. This file only writes and reads the bytes: who may
//! create, invite, leave or announce is `game/Guilds/`, and which batch answers a request is
//! `game/Handlers/GuildHandler.rs`.
//!
//! Layouts are the client's own table (`game/src/Server/Protocol/Tables.rs`): a `String16(n)` is a
//! `u16` count and that many units, a `Blob(16)` is sixteen raw bytes. Names marked GUESS are ASURA's;
//! the packet table leaves those headers unnamed.

/// `GAME_SMSG_GUILD_PLAYER_ROLE` (`0x0118`): word local id, the guild key, byte role.
pub const GUILD_PLAYER_ROLE: u16 = 0x0118;
/// `0x0119`, header only. GUESS (ASURA): "this character has no guild". Measured once, right after the
/// leaver's own `0x0128` and `0x009F {30, agent, 0}` (leave file seq 731).
pub const GUILD_NONE: u16 = 0x0119;
/// `0x011C`, header only (`GAME_SMSG_TOWN_ALLIANCE_END` in the table). The last guild message of every
/// load and of the creation batch (`ASURA/crates/asura-protocol/src/game/guild.rs:22-27`).
pub const GUILD_STREAM_END: u16 = 0x011C;
/// `0x011F`, unnamed. GUESS (ASURA): one guild history line - a date dword and a coded string.
pub const GUILD_HISTORY_ENTRY: u16 = 0x011F;
/// `GAME_SMSG_GUILD_GENERAL_INFO` (`0x0121`): one guild's name, tag, cape and standing.
pub const GUILD_GENERAL_INFO: u16 = 0x0121;
/// `GAME_SMSG_GUILD_PLAYER_INFO` (`0x0127`): one roster row.
pub const GUILD_PLAYER_INFO: u16 = 0x0127;
/// `GAME_SMSG_GUILD_PLAYER_REMOVE` (`0x0128`): a roster row goes, by name.
pub const GUILD_PLAYER_REMOVE: u16 = 0x0128;
/// `0x012F`, unnamed. GUESS (ASURA): the guild announcement - text and author.
pub const GUILD_ANNOUNCEMENT: u16 = 0x012F;
/// `0x0131`, header only. GUESS (ASURA): opens the creation batch (seq 318).
pub const GUILD_STREAM_START: u16 = 0x0131;
/// `0x0132`, unnamed. GUESS (ASURA): the answer to a guild request - its request id and a result byte.
pub const GUILD_REQUEST_RESULT: u16 = 0x0132;
/// `0x0133`, one dword, unnamed. Sent once per guild change; see [`guild_revision`].
pub const GUILD_REVISION: u16 = 0x0133;
/// `GAME_SMSG_TRADER_TRANSACTION_DONE` (`0x00CC`): a vendor window's transaction is finished.
pub const TRADER_TRANSACTION_DONE: u16 = 0x00CC;
/// `GAME_SMSG_MERCHANT_WINDOW_UPDATE_OWNER` (`0x00C4`).
pub const MERCHANT_WINDOW_UPDATE_OWNER: u16 = 0x00C4;
/// `GAME_SMSG_WINDOW_ADD_ITEMS` (`0x0084`).
pub const WINDOW_ADD_ITEMS: u16 = 0x0084;
/// `0x00CA`, unnamed in the table: a byte and a dword.
pub const WINDOW_SCALE: u16 = 0x00CA;
/// `GAME_SMSG_MERCHANT_WINDOW_OPEN` (`0x00C3`).
pub const MERCHANT_WINDOW_OPEN: u16 = 0x00C3;

/// `0x8036`, unnamed. GUESS (ASURA): create a guild - dword, `String16(32)` name, `String16(6)` tag.
pub const CLIENT_GUILD_CREATE: u16 = 0x8036;
/// `0x80B5`, unnamed. GUESS (ASURA): invite a character - dword request id, `String16(32)` name, byte.
pub const CLIENT_GUILD_INVITE: u16 = 0x80B5;
/// `0x80BB`, unnamed. GUESS (ASURA): leave the guild - dword request id.
pub const CLIENT_GUILD_LEAVE: u16 = 0x80BB;
/// `0x80BE`, unnamed. GUESS (ASURA): set the announcement - dword request id, `String16(256)` text.
pub const CLIENT_GUILD_ANNOUNCEMENT: u16 = 0x80BE;

/// The units a guild name field holds (`0x0121`, `0x8036`: `String16(32)`).
pub const GUILD_NAME_UNITS: usize = 32;
/// The units a guild tag field holds (`String16(6)`).
pub const GUILD_TAG_UNITS: usize = 6;
/// The units a player-name field of the family holds (`0x0127`, `0x0128`, `0x012F`: `String16(20)`).
pub const GUILD_PLAYER_NAME_UNITS: usize = 20;
/// The units `0x0127`'s context field holds (`String16(64)`).
pub const GUILD_CONTEXT_UNITS: usize = 64;
/// The units a history line holds (`0x011F`: `String16(64)`).
pub const GUILD_HISTORY_UNITS: usize = 64;
/// The units the announcement field holds (`0x012F`, `0x80BE`: `String16(256)`).
pub const GUILD_ANNOUNCEMENT_UNITS: usize = 256;

/// `0x0132`'s result for a request the server carried out (seq 3350, 5587, 6082; leave seq 733).
pub const GUILD_RESULT_OK: u8 = 0;
/// `0x0132`'s result for four refused invitations (seq 722, 1108, 1512, 1839). GUESS (ASURA) that it
/// serves for every refusal: it is the only refusal code captured.
pub const GUILD_RESULT_REFUSED: u8 = 14;

/// `0x0118` role and `0x0127` member type of the guild's leader: the founder at creation (seq 321, 324).
pub const GUILD_MEMBER_LEADER: u8 = 5;
/// `0x0127` member type of an invited character that has not accepted (seq 3351).
pub const GUILD_MEMBER_INVITED: u8 = 1;

/// `0x0127` status of a member that is online (the founder, seq 324).
pub const GUILD_STATUS_ONLINE: u32 = 1;
/// `0x0127` status of a member that is not (the invitee row, seq 3351).
pub const GUILD_STATUS_OFFLINE: u32 = 0;

/// Agent int-property 30: the local id of the guild an agent belongs to, 0 for none
/// (`ASURA/crates/asura-protocol/src/game/guild.rs:83-87`).
pub const AGENT_PROPERTY_GUILD: u32 = 30;

/// The string id of the history line a new guild gets (seq 322). GUESS (ASURA) that it reads
/// "founded by".
pub const GUILD_HISTORY_FOUNDED: u16 = 0x0345;

/// `0x00CC` window kind of the Guild Registrar: the creation batch confirms the transaction with `{8}`
/// (seq 317).
pub const GUILD_REGISTRAR_WINDOW: u8 = 8;

/// The literal start and end a coded string wraps a name in
/// (`ASURA/crates/asura-protocol/src/game/mod.rs:379-381`).
const CODED_LITERAL_START: u16 = 0x0107;
const CODED_LITERAL_END: u16 = 0x0001;

/// Days from 1900-01-01 to 1970-01-01: the client's day count starts in 1900.
const DAYS_1900_TO_1970: i64 = 25_567;

/// The client's date dword: the day since 1900-01-01 (UTC) in the low word, the minute of that day in
/// the high word (`ASURA/crates/asura-protocol/src/game/guild.rs:100-108`).
pub fn guild_date(unix_seconds: i64) -> u32 {
    let days = DAYS_1900_TO_1970 + unix_seconds.div_euclid(86_400);
    let minute = unix_seconds.rem_euclid(86_400) / 60;
    let days = u32::try_from(days).unwrap_or(0) & 0xFFFF;
    days | ((minute as u32) << 16)
}

/// A frame: the `u16` header and then the body.
fn frame(header: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// A `u16` count and at most `capacity` units (ASURA's `Builder::units`, `game/mod.rs:604-612`).
fn put_units(body: &mut Vec<u8>, units: &[u16], capacity: usize) {
    let units = &units[..units.len().min(capacity)];
    body.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        body.extend_from_slice(&unit.to_le_bytes());
    }
}

/// A string as a `String16`, cut at `capacity` units (ASURA's `Builder::text`, `game/mod.rs:594-597`).
fn put_text(body: &mut Vec<u8>, text: &str, capacity: usize) {
    let units: Vec<u16> = text.encode_utf16().take(capacity).collect();
    put_units(body, &units, capacity);
}

/// A guild's cape as `0x0121` carries it. All zero for a guild that has none (a new guild, seq 319).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cape {
    /// The cape's background colour.
    pub background_color: u8,
    /// The colour of its detail.
    pub detail_color: u8,
    /// The colour of its emblem.
    pub emblem_color: u8,
    /// Its shape.
    pub shape: u8,
    /// Its detail pattern.
    pub detail: u8,
    /// Its emblem (a word on the wire).
    pub emblem: u16,
    /// Its trim.
    pub trim: u8,
}

/// `GAME_SMSG_GUILD_GENERAL_INFO`'s fields, in wire order (`ASURA/docs/41` section 2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildGeneral<'a> {
    /// The id this connection's messages name the guild by. Server-chosen and small.
    pub local_id: u16,
    /// The guild's sixteen-byte key; also `0x0118`'s.
    pub key: [u8; 16],
    /// The guild's name, at most [`GUILD_NAME_UNITS`] units.
    pub name: &'a str,
    /// The guild's tag, at most [`GUILD_TAG_UNITS`] units.
    pub tag: &'a str,
    /// 0 on a new guild, 20 or 40 on established ones; meaning not established.
    pub feature: u8,
    /// 2 on the founder's new guild, 0 on most others; meaning not established.
    pub territory: u8,
    /// The cape; all zero for none.
    pub cape: Cape,
    /// 4 on a new guild (seq 319).
    pub allegiance: u8,
    /// Faction points.
    pub faction: u32,
    /// Qualifier points.
    pub qualifier: u32,
    /// Ladder rating: 1000 on a new guild.
    pub rating: u32,
    /// Ladder position: 0 for an unranked guild.
    pub rank: u32,
}

/// `GAME_SMSG_GUILD_GENERAL_INFO`: one guild, for every guild a player in view belongs to
/// (`ASURA/crates/asura-protocol/src/game/guild.rs:159-184`).
pub fn guild_general_info(guild: &GuildGeneral<'_>) -> Vec<u8> {
    let cape = guild.cape;
    let mut body = Vec::with_capacity(130);
    body.extend_from_slice(&guild.local_id.to_le_bytes());
    body.extend_from_slice(&guild.key);
    put_text(&mut body, guild.name, GUILD_NAME_UNITS);
    put_text(&mut body, guild.tag, GUILD_TAG_UNITS);
    body.push(guild.feature);
    body.push(guild.territory);
    body.push(cape.background_color);
    body.push(cape.detail_color);
    body.push(cape.emblem_color);
    body.push(cape.shape);
    body.push(cape.detail);
    body.extend_from_slice(&cape.emblem.to_le_bytes());
    body.push(cape.trim);
    body.push(guild.allegiance);
    body.extend_from_slice(&guild.faction.to_le_bytes());
    body.extend_from_slice(&guild.qualifier.to_le_bytes());
    body.extend_from_slice(&guild.rating.to_le_bytes());
    body.extend_from_slice(&guild.rank.to_le_bytes());
    // The last byte is 0 in every capture (seq 319, 1148).
    body.push(0);
    frame(GUILD_GENERAL_INFO, &body)
}

/// `GAME_SMSG_GUILD_PLAYER_ROLE`: this character's place in the guild `local_id` names
/// (`guild.rs:186-193`).
pub fn guild_player_role(local_id: u16, key: [u8; 16], role: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(19);
    body.extend_from_slice(&local_id.to_le_bytes());
    body.extend_from_slice(&key);
    body.push(role);
    frame(GUILD_PLAYER_ROLE, &body)
}

/// `0x0119`, header only. See [`GUILD_NONE`].
pub fn guild_none() -> Vec<u8> {
    frame(GUILD_NONE, &[])
}

/// `0x011C`, header only. See [`GUILD_STREAM_END`].
pub fn guild_stream_end() -> Vec<u8> {
    frame(GUILD_STREAM_END, &[])
}

/// `0x0131`, header only. See [`GUILD_STREAM_START`].
pub fn guild_stream_start() -> Vec<u8> {
    frame(GUILD_STREAM_START, &[])
}

/// `0x011F`: one history line, `date` in [`guild_date`]'s encoding and `text` a coded string
/// (`guild.rs:210-216`).
pub fn guild_history_entry(date: u32, text: &[u16]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + 2 + text.len() * 2);
    body.extend_from_slice(&date.to_le_bytes());
    put_units(&mut body, text, GUILD_HISTORY_UNITS);
    frame(GUILD_HISTORY_ENTRY, &body)
}

/// The coded string of a new guild's history line (seq 322): [`GUILD_HISTORY_FOUNDED`], literal start,
/// the founder's name, literal end (`guild.rs:218-228`). One unit is left free: a coded string that
/// fills its field exactly is a hangup on the title family, and nothing says this family differs.
pub fn founded_line(founder: &str) -> Vec<u16> {
    let room = GUILD_HISTORY_UNITS - 4;
    let mut units = vec![GUILD_HISTORY_FOUNDED, CODED_LITERAL_START];
    units.extend(founder.encode_utf16().take(room));
    units.push(CODED_LITERAL_END);
    units
}

/// `0x012F`: the announcement and who set it. Empty both at creation (seq 323; `guild.rs:230-236`).
pub fn guild_announcement(text: &str, author: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + (text.len() + author.len()) * 2);
    put_text(&mut body, text, GUILD_ANNOUNCEMENT_UNITS);
    put_text(&mut body, author, GUILD_PLAYER_NAME_UNITS);
    frame(GUILD_ANNOUNCEMENT, &body)
}

/// `GAME_SMSG_GUILD_PLAYER_INFO`'s fields, in wire order (`ASURA/docs/41` section 2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildPlayer<'a> {
    /// The name the character was invited under; the founder's own name at creation.
    pub invited_name: &'a str,
    /// The character's current name: the founder's at creation, empty on an invitee row (seq 3351).
    pub current_name: &'a str,
    /// Who invited it: the inviter on an invitee row, the founder on its own.
    pub invited_by: &'a str,
    /// Empty in both captured rows.
    pub context: &'a str,
    /// 0 in both captured rows.
    pub minutes_since_login: u32,
    /// [`guild_date`]'s encoding.
    pub join_date: u32,
    /// [`GUILD_STATUS_ONLINE`] or [`GUILD_STATUS_OFFLINE`].
    pub status: u32,
    /// [`GUILD_MEMBER_LEADER`], [`GUILD_MEMBER_INVITED`], ...
    pub member_type: u8,
}

/// `GAME_SMSG_GUILD_PLAYER_INFO`: one roster row (`guild.rs:259-273`).
pub fn guild_player_info(row: &GuildPlayer<'_>) -> Vec<u8> {
    let mut body = Vec::with_capacity(64);
    put_text(&mut body, row.invited_name, GUILD_PLAYER_NAME_UNITS);
    put_text(&mut body, row.current_name, GUILD_PLAYER_NAME_UNITS);
    put_text(&mut body, row.invited_by, GUILD_PLAYER_NAME_UNITS);
    put_text(&mut body, row.context, GUILD_CONTEXT_UNITS);
    // The dword before minutes_since_login is 0 in both captured rows.
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&row.minutes_since_login.to_le_bytes());
    body.extend_from_slice(&row.join_date.to_le_bytes());
    body.extend_from_slice(&row.status.to_le_bytes());
    body.push(row.member_type);
    frame(GUILD_PLAYER_INFO, &body)
}

/// `GAME_SMSG_GUILD_PLAYER_REMOVE`: the roster row with this name goes (leave file seq 728;
/// `guild.rs:275-280`).
pub fn guild_player_remove(name: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(2 + name.len() * 2);
    put_text(&mut body, name, GUILD_PLAYER_NAME_UNITS);
    frame(GUILD_PLAYER_REMOVE, &body)
}

/// `0x0132`: the answer to the request that carried `request_id` (`guild.rs:282-288`).
pub fn guild_request_result(request_id: u32, result: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    body.extend_from_slice(&request_id.to_le_bytes());
    body.push(result);
    frame(GUILD_REQUEST_RESULT, &body)
}

/// `0x0133`, one dword, sent once per change of the guild (`guild.rs:290-295`). The value is the
/// caller's; this only writes it.
pub fn guild_revision(value: u32) -> Vec<u8> {
    frame(GUILD_REVISION, &value.to_le_bytes())
}

/// `GAME_SMSG_TRADER_TRANSACTION_DONE` (`0x00CC`): the creation batch sends
/// `{GUILD_REGISTRAR_WINDOW}` (seq 317; `guild.rs:297-303`).
pub fn trader_transaction_done(window_kind: u8) -> Vec<u8> {
    frame(TRADER_TRANSACTION_DONE, &[window_kind])
}

/// The Guild Registration window as a server opens it: owner, staged stock, opener, kind
/// (`ASURA/crates/asura-protocol/src/game/guild.rs:305-329`).
///
/// No live registrar has been captured. The order is the one a merchant window is opened with -
/// `0x00C4 {owner}`, `0x0084 {stock ids}`, `0x00CA {1, 1.0f}`, `0x00C3 {kind, 0}` - with an empty stock
/// list. GUESS (ASURA) that the client accepts that; the live client decides.
pub fn registration_window(owner_agent: u32) -> Vec<Vec<u8>> {
    let mut scale = vec![1u8];
    scale.extend_from_slice(&1.0f32.to_bits().to_le_bytes());
    let mut open = vec![GUILD_REGISTRAR_WINDOW];
    open.extend_from_slice(&0u32.to_le_bytes());
    vec![
        frame(MERCHANT_WINDOW_UPDATE_OWNER, &owner_agent.to_le_bytes()),
        // A `u16` count and no dwords.
        frame(WINDOW_ADD_ITEMS, &0u16.to_le_bytes()),
        frame(WINDOW_SCALE, &scale),
        frame(MERCHANT_WINDOW_OPEN, &open),
    ]
}

// ---- the requests -------------------------------------------------------------------------------

/// Reads the client's counted fields in order. `None` once the body runs out
/// (`guild.rs:333-378`).
struct Reader<'a> {
    body: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(body: &'a [u8]) -> Self {
        Reader { body, at: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let value = *self.body.get(self.at)?;
        self.at += 1;
        Some(value)
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes: [u8; 2] = self.body.get(self.at..self.at + 2)?.try_into().ok()?;
        self.at += 2;
        Some(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes: [u8; 4] = self.body.get(self.at..self.at + 4)?.try_into().ok()?;
        self.at += 4;
        Some(u32::from_le_bytes(bytes))
    }

    /// A `u16` count and that many units, decoded. Refused when the count exceeds the field.
    fn text(&mut self, capacity: usize) -> Option<String> {
        let count = usize::from(self.u16()?);
        if count > capacity {
            return None;
        }
        let mut units = Vec::with_capacity(count);
        for _ in 0..count {
            units.push(self.u16()?);
        }
        Some(
            char::decode_utf16(units)
                .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect(),
        )
    }
}

/// `0x8036`, the create request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRequest {
    /// 100 in the one capture, equal to the gold taken (seq 308, 313). GUESS (ASURA): the price the
    /// client was quoted. Read and logged; the server charges its own price.
    pub quoted: u32,
    /// The guild name asked for.
    pub name: String,
    /// The tag asked for.
    pub tag: String,
}

/// Read a `0x8036` body - the frame **without** its two header bytes (`guild.rs:392-400`).
pub fn read_create_request(body: &[u8]) -> Option<CreateRequest> {
    let mut reader = Reader::new(body);
    Some(CreateRequest {
        quoted: reader.u32()?,
        name: reader.text(GUILD_NAME_UNITS)?,
        tag: reader.text(GUILD_TAG_UNITS)?,
    })
}

/// `0x80B5`, the invite request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRequest {
    /// Counts up from 1 per session (seq 712 ... 3338); echoed in `0x0132`.
    pub request_id: u32,
    /// The name of the character to invite, as typed.
    pub name: String,
    /// 2 in all five captured requests; meaning not established.
    pub kind: u8,
}

/// Read a `0x80B5` body (`guild.rs:413-421`).
pub fn read_invite_request(body: &[u8]) -> Option<InviteRequest> {
    let mut reader = Reader::new(body);
    Some(InviteRequest {
        request_id: reader.u32()?,
        name: reader.text(GUILD_NAME_UNITS)?,
        kind: reader.u8()?,
    })
}

/// Read a `0x80BB` body: its request id (`guild.rs:423-426`).
pub fn read_leave_request(body: &[u8]) -> Option<u32> {
    Reader::new(body).u32()
}

/// Read a `0x80BE` body: its request id (0 in both captures, and echoed as 0) and the text
/// (`guild.rs:428-432`).
pub fn read_announcement_request(body: &[u8]) -> Option<(u32, String)> {
    let mut reader = Reader::new(body);
    Some((reader.u32()?, reader.text(GUILD_ANNOUNCEMENT_UNITS)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(text: &str) -> Vec<u8> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut bytes = (units.len() as u16).to_le_bytes().to_vec();
        for unit in units {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn the_date_is_days_since_1900_and_the_minute_of_the_day() {
        // 2026-11-06 00:35:30 UTC and 00:36:22 UTC: the founder's and the invitee's join dates.
        assert_eq!(guild_date(1_793_925_330), 0x0023_B4FA);
        assert_eq!(guild_date(1_793_925_382), 0x0024_B4FA);
    }

    /// The fixed messages are their declared sizes in the table (`Tables.rs`: `0x0118` 21, `0x0119` 2,
    /// `0x011C` 2, `0x0131` 2, `0x0132` 7, `0x0133` 6, `0x00CC` 3).
    #[test]
    fn the_fixed_messages_are_their_declared_sizes() {
        assert_eq!(
            guild_player_role(180, [7; 16], GUILD_MEMBER_LEADER).len(),
            21
        );
        assert_eq!(guild_none().len(), 2);
        assert_eq!(guild_stream_end().len(), 2);
        assert_eq!(guild_stream_start().len(), 2);
        assert_eq!(guild_request_result(5, GUILD_RESULT_OK).len(), 7);
        assert_eq!(guild_revision(0x0200).len(), 6);
        assert_eq!(trader_transaction_done(GUILD_REGISTRAR_WINDOW).len(), 3);
    }

    #[test]
    fn general_info_is_the_tables_layout_and_fits_its_maximum() {
        let blank = GuildGeneral {
            local_id: 180,
            key: [0xAB; 16],
            name: "Some Guild",
            tag: "tgne",
            feature: 0,
            territory: 2,
            cape: Cape::default(),
            allegiance: 4,
            faction: 0,
            qualifier: 0,
            rating: 1000,
            rank: 0,
        };
        let bytes = guild_general_info(&blank);
        assert_eq!(&bytes[0..2], &[0x21, 0x01]);
        assert_eq!(&bytes[2..4], &180u16.to_le_bytes());
        assert_eq!(&bytes[4..20], &[0xAB; 16]);
        // name (2 + 20), tag (2 + 8), seven bytes, a word, two bytes, four dwords, a byte.
        assert_eq!(bytes.len(), 2 + 2 + 16 + 22 + 10 + 7 + 2 + 2 + 16 + 1);
        let rating_at = bytes.len() - 1 - 8;
        assert_eq!(&bytes[rating_at..rating_at + 4], &1000u32.to_le_bytes());
        // At full name and tag it fits the table's declared maximum of 132.
        let name = "n".repeat(GUILD_NAME_UNITS);
        let tag = "t".repeat(GUILD_TAG_UNITS);
        let full = guild_general_info(&GuildGeneral {
            name: &name,
            tag: &tag,
            ..blank
        });
        assert!(full.len() <= 132);
    }

    #[test]
    fn a_roster_row_ends_with_its_four_dwords_and_the_type() {
        let bytes = guild_player_info(&GuildPlayer {
            invited_name: "Abc",
            current_name: "",
            invited_by: "De",
            context: "",
            minutes_since_login: 0,
            join_date: 0x0024_B4FA,
            status: GUILD_STATUS_OFFLINE,
            member_type: GUILD_MEMBER_INVITED,
        });
        assert_eq!(bytes.len(), 2 + (2 + 6) + 2 + (2 + 4) + 2 + 16 + 1);
        let tail = &bytes[bytes.len() - 17..];
        assert_eq!(&tail[0..4], &0u32.to_le_bytes());
        assert_eq!(&tail[8..12], &0x0024_B4FAu32.to_le_bytes());
        assert_eq!(tail[16], GUILD_MEMBER_INVITED);
        assert!(bytes.len() <= 283);
    }

    #[test]
    fn the_founded_line_is_the_id_a_literal_and_its_end() {
        assert_eq!(
            founded_line("Ab"),
            vec![
                GUILD_HISTORY_FOUNDED,
                0x0107,
                u16::from(b'A'),
                u16::from(b'b'),
                0x0001
            ]
        );
        assert!(founded_line(&"x".repeat(200)).len() < GUILD_HISTORY_UNITS);
    }

    #[test]
    fn the_requests_read_their_counted_fields() {
        let mut create = 100u32.to_le_bytes().to_vec();
        create.extend(counted("My Guild"));
        create.extend(counted("tgne"));
        assert_eq!(
            read_create_request(&create),
            Some(CreateRequest {
                quoted: 100,
                name: "My Guild".into(),
                tag: "tgne".into()
            })
        );

        let mut invite = 5u32.to_le_bytes().to_vec();
        invite.extend(counted("Someone"));
        invite.push(2);
        assert_eq!(
            read_invite_request(&invite),
            Some(InviteRequest {
                request_id: 5,
                name: "Someone".into(),
                kind: 2
            })
        );
        assert_eq!(read_invite_request(&invite[..invite.len() - 1]), None);

        assert_eq!(read_leave_request(&1u32.to_le_bytes()), Some(1));

        let mut announce = 0u32.to_le_bytes().to_vec();
        announce.extend(counted("hello"));
        assert_eq!(
            read_announcement_request(&announce),
            Some((0, "hello".into()))
        );

        // A count past the field is refused, not read into the next field.
        let mut long = 0u32.to_le_bytes().to_vec();
        long.extend_from_slice(&(GUILD_TAG_UNITS as u16 + 40).to_le_bytes());
        assert_eq!(read_create_request(&long), None);
    }

    /// Every header of the family is a row of the client's own table, in its direction
    /// (`ASURA/crates/asura-protocol/src/game/guild.rs:620-644`), and every fixed frame is no longer than
    /// the row declares.
    #[test]
    fn the_headers_match_the_table() {
        use crate::server::protocol::tables::lookup;
        for (header, from_client) in [
            (GUILD_PLAYER_ROLE, false),
            (GUILD_NONE, false),
            (GUILD_STREAM_END, false),
            (GUILD_HISTORY_ENTRY, false),
            (GUILD_GENERAL_INFO, false),
            (GUILD_PLAYER_INFO, false),
            (GUILD_PLAYER_REMOVE, false),
            (GUILD_ANNOUNCEMENT, false),
            (GUILD_STREAM_START, false),
            (GUILD_REQUEST_RESULT, false),
            (GUILD_REVISION, false),
            (TRADER_TRANSACTION_DONE, false),
            (CLIENT_GUILD_CREATE, true),
            (CLIENT_GUILD_INVITE, true),
            (CLIENT_GUILD_LEAVE, true),
            (CLIENT_GUILD_ANNOUNCEMENT, true),
        ] {
            assert!(
                lookup(header, from_client).is_some(),
                "{header:#06X} is not a game row"
            );
        }
        for frame in registration_window(1)
            .into_iter()
            .chain([guild_player_role(1, [0; 16], 5), guild_revision(1)])
        {
            let header = u16::from_le_bytes([frame[0], frame[1]]);
            let row = lookup(header, false).expect("a server row");
            assert!(
                frame.len() <= row.size as usize,
                "{header:#06X} is longer than its row"
            );
        }
    }

    #[test]
    fn the_registration_window_is_the_merchant_order_with_kind_8() {
        let frames = registration_window(0x2F9);
        assert_eq!(frames[0], vec![0xC4, 0x00, 0xF9, 0x02, 0x00, 0x00]);
        assert_eq!(frames[1], vec![0x84, 0x00, 0x00, 0x00]);
        assert_eq!(frames[2], vec![0xCA, 0x00, 0x01, 0x00, 0x00, 0x80, 0x3F]);
        assert_eq!(frames[3], vec![0xC3, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00]);
    }
}
