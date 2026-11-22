//! The chat message family: a line spoken, and who spoke it.
//!
//! Reference: `src/server/game/Server/Packets/ChatPackets.{h,cpp}` - one file pair per message family,
//! and this is the family that carries chat. Nothing of its *contents* crosses: these headers, these
//! layouts and these channel numbers are this client's. They come from the GameServer the working
//! implementation ports (`research/OpenTyria/code/OpenTyria/GmChat.c`, `GmChat.h:3-20`) and from the
//! measured wire in `research/Rurik/studies/chat/FINDINGS.md`, and `docs/decisions/0012-chat-text-is-sent.md`
//! records why a measurement outranks the guessed table this file used to carry.
//!
//! **A line is a pair, never a lone body.** One or more `0x005D` body fragments, then exactly one commit
//! tag: `0x0061 [sender player id, channel]` for a player's line, `0x005E [subject player id, channel]`
//! for a line the server composed. A bare `0x005D` renders nothing and is silently held by the client
//! (`Rurik/studies/isle/FINDINGS.md:393-398`), and every one of the 225 tags in that corpus sits directly
//! after a body (`studies/chat/FINDINGS.md:60-61`).
//!
//! **The body is a coded string.** For an All-chat line it is `0x0108` (the All-chat body wrapper), the
//! literal mark `0x0107`, the text, and the terminator `0x0001` - the framing on 3 of 3 observed
//! channel-3 bodies, and the eight units the one confirmed render carried: `5d00 0800 0108 0107 h e l l o
//! 0001` (`studies/chat/FINDINGS.md:263`). Two independent witnesses write those exact bytes: the
//! measurement, and the GameServer's own builder `GmText_BuildLiteral`
//! (`research/OpenTyria/code/OpenTyria/GmText.c:43-57`).

/// `GAME_SMSG_CHAT_MESSAGE_CORE` (`0x005D`): one fragment of a body, `2 + 2 + 2n`.
pub const CHAT_MESSAGE_CORE: u16 = 0x005D;
/// `GAME_SMSG_CHAT_MESSAGE_SERVER` (`0x005E`): 5 bytes - the server itself speaking, and who it is about.
pub const CHAT_MESSAGE_SERVER: u16 = 0x005E;
/// `GAME_SMSG_CHAT_MESSAGE_LOCAL` (`0x0061`): 5 bytes - which player spoke, on which channel.
pub const CHAT_MESSAGE_LOCAL: u16 = 0x0061;

/// The declared capacity of `0x005D`'s `String16`, in units: 122.
pub const CHAT_MESSAGE_CORE_UNITS: usize = 122;

/// Where a line is split, in units: **121**, one less than the capacity.
///
/// Measured, not derived: both multi-part lines in the corpus split their coded string at exactly
/// 121 = declared(122) - 1, the same exclusive-cap rule `0x00F3`'s `string16(8)` shows
/// (`studies/chat/FINDINGS.md:68-69`). A cap read off the field width would say 122, which is what the
/// GameServer does and what put the `/maps` answer past what one frame carries.
pub const CHAT_MESSAGE_FRAGMENT_UNITS: usize = 121;

/// The leading word of an All-chat body: string id 8, the wrapper the client unwraps
/// (`studies/chat/FINDINGS.md:238-244`).
pub const ALL_BODY_WRAPPER: u16 = 0x0108;
/// The literal-run opening mark: what follows is text, not an archive string id.
pub const LITERAL_MARK: u16 = 0x0107;
/// The coded string's terminator: `TERM_INTERMEDIATE` (`GmText.c:4`).
pub const LITERAL_END: u16 = 0x0001;
/// `TERM_FINAL` (`GmText.c:3`): the other end-of-run code, which a literal run never carries.
pub const TERM_FINAL: u16 = 0x0000;

/// Whether a player's text may be echoed: the reference refuses a line that carries either terminator.
///
/// Reference: `GmText_ValidateUserMessage` (`research/OpenTyria/code/OpenTyria/GmText.c:32-41`) - a
/// `0x0000` or `0x0001` in what a player typed would end the literal run early and have the client read
/// the rest as codes, so the line is refused rather than sent (`GmChat.c:166-169`).
pub fn valid_user_message(units: &[u16]) -> bool {
    !units
        .iter()
        .any(|&unit| unit == TERM_FINAL || unit == LITERAL_END)
}

/// `Channel_All`, in the client's own enum (`GmChat.h:7`): the channel `!` speaks on.
pub const CHANNEL_ALL: u8 = 3;
/// `Channel_Emote` (`GmChat.h:10`): what a `/bow` reply is committed on.
pub const CHANNEL_EMOTE: u8 = 6;
/// `Channel_Warning` (`GmChat.h:11`): the middle-of-the-screen panel that draws the `TxtError` frame and
/// does not parse `<c>` tags.
pub const CHANNEL_WARNING: u8 = 7;
/// `Channel_Global` (`GmChat.h:14`): the channel the reference answers a command on
/// (`GmChat.c:140`, `GameSrv_SendInvalidCommand`).
pub const CHANNEL_GLOBAL: u8 = 10;
/// `Channel_Group` (`GmChat.h:15`): the channel `#` speaks on.
pub const CHANNEL_GROUP: u8 = 11;
/// `Channel_Trade` (`GmChat.h:16`): the channel `$` speaks on.
pub const CHANNEL_TRADE: u8 = 12;

/// A frame: the `u16` header and then the body.
fn frame(header: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// A `u16` count followed by that many UTF-16 code units, which is the wire's `String16`.
fn units_field(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + units.len() * 2);
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// The coded-string body of one All-chat line: wrapper, literal mark, the text, the terminator.
///
/// The text is the line **without** the sigil the player typed - the confirmed render's body carries
/// `hello`, not `!hello` (`studies/chat/FINDINGS.md:263`). Characters outside the Basic Multilingual
/// Plane arrive as two UTF-16 units, which the field carries as any other pair.
pub fn all_chat_body(text: &str) -> Vec<u16> {
    let mut body = Vec::with_capacity(text.encode_utf16().count() + 3);
    body.push(ALL_BODY_WRAPPER);
    body.push(LITERAL_MARK);
    body.extend(text.encode_utf16());
    body.push(LITERAL_END);
    body
}

/// The body split into fragments of at most [`CHAT_MESSAGE_FRAGMENT_UNITS`], in order.
///
/// Joining the fragments reproduces the body exactly, which is the property the corpus's own splits
/// have and what the test below asserts rather than trusting the slicing.
pub fn fragments(body: &[u16]) -> Vec<&[u16]> {
    body.chunks(CHAT_MESSAGE_FRAGMENT_UNITS).collect()
}

/// `0x005D`: one fragment of a coded body.
pub fn chat_message_core(units: &[u16]) -> Vec<u8> {
    debug_assert!(
        units.len() <= CHAT_MESSAGE_CORE_UNITS,
        "a fragment is at most the field's capacity"
    );
    frame(CHAT_MESSAGE_CORE, &units_field(units))
}

/// Every `0x005D` frame for one All-chat line, in order: the body, wrapped and split.
pub fn all_chat_frames(text: &str) -> Vec<Vec<u8>> {
    let body = all_chat_body(text);
    fragments(&body)
        .into_iter()
        .map(chat_message_core)
        .collect()
}

/// `0x0061`: who is speaking, and on which channel - `[sender player id, channel]`, five bytes.
///
/// `studies/chat/FINDINGS.md:42`, and the confirmed bytes `6100 0100 03` for player 1 on channel 3.
/// The id is the **per-instance player id** the sender is known by, which the client resolves through its
/// own player table (`:78-80`), not an agent id.
pub fn chat_message_local(player: u16, channel: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    body.extend_from_slice(&player.to_le_bytes());
    body.push(channel);
    frame(CHAT_MESSAGE_LOCAL, &body)
}

/// `0x005E`: the server itself speaking - `[subject player id, channel]`, five bytes.
///
/// `studies/chat/FINDINGS.md:39`, and the confirmed bytes `5e00 0100 06`. The word is **the player the
/// line is about** (125 of 133 sightings are the operator's own id, 7 are 0 for a district-wide
/// announcement, 1 is another player), and no name renders in this tag: `overrides.json:3073`, "sender is
/// the system".
pub fn chat_message_server(player: u16, channel: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    body.extend_from_slice(&player.to_le_bytes());
    body.push(channel);
    frame(CHAT_MESSAGE_SERVER, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The confirmed render's own bytes, unit for unit: `5d00 0800 0108 0107 h e l l o 0001`
    /// (`Rurik/studies/chat/FINDINGS.md:263`).
    #[test]
    fn the_confirmed_all_chat_body_is_the_wrapper_a_mark_the_text_and_a_terminator() {
        let body = all_chat_body("hello");
        assert_eq!(
            body,
            vec![
                0x0108, 0x0107, 0x0068, 0x0065, 0x006C, 0x006C, 0x006F, 0x0001
            ]
        );
        assert_eq!(body.len(), 8, "the eight units the render carried");

        let frame = chat_message_core(&body);
        assert_eq!(
            frame,
            vec![
                0x5D, 0x00, // header
                0x08, 0x00, // eight units
                0x08, 0x01, 0x07, 0x01, // wrapper, literal mark
                0x68, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F, 0x00, // hello
                0x01, 0x00, // terminator
            ]
        );
    }

    /// A body longer than one fragment is split, and the fragments rejoin to the body.
    #[test]
    fn a_long_body_is_fragmented_at_121_units_and_rejoins() {
        let text = "x".repeat(300);
        let body = all_chat_body(&text);
        let pieces = fragments(&body);
        assert!(pieces.len() > 1, "300 units do not fit one fragment");
        for piece in &pieces {
            assert!(piece.len() <= CHAT_MESSAGE_FRAGMENT_UNITS);
        }
        assert_eq!(pieces[0].len(), CHAT_MESSAGE_FRAGMENT_UNITS);
        assert_eq!(pieces.concat(), body);
        assert_eq!(all_chat_frames(&text).len(), pieces.len());
    }

    /// The declared capacity is 122 and the split is 121: a body of exactly 121 units is one frame, and
    /// one of 122 is two.
    #[test]
    fn the_split_is_one_less_than_the_field_capacity() {
        // 3 marks + text + ... : a body of exactly the fragment size.
        let text = "y".repeat(CHAT_MESSAGE_FRAGMENT_UNITS - 3);
        assert_eq!(all_chat_body(&text).len(), CHAT_MESSAGE_FRAGMENT_UNITS);
        assert_eq!(fragments(&all_chat_body(&text)).len(), 1);

        let text = "y".repeat(CHAT_MESSAGE_FRAGMENT_UNITS - 2);
        assert_eq!(all_chat_body(&text).len(), CHAT_MESSAGE_FRAGMENT_UNITS + 1);
        assert_eq!(fragments(&all_chat_body(&text)).len(), 2);
    }

    /// `6100 0100 03`: player 1, channel 3, five bytes - the confirmed pair for the `!hello` render
    /// (`Rurik/studies/chat/FINDINGS.md:263`).
    #[test]
    fn a_local_line_is_the_player_and_the_channel_in_five_bytes() {
        assert_eq!(
            chat_message_local(1, CHANNEL_ALL),
            vec![0x61, 0x00, 0x01, 0x00, 0x03]
        );
        assert_eq!(chat_message_local(1, CHANNEL_ALL).len(), 5);
        assert_eq!(
            chat_message_local(1, CHANNEL_GROUP),
            vec![0x61, 0x00, 0x01, 0x00, 0x0B]
        );
    }

    /// `5e00 0100 06`: player 1, channel 6, five bytes - the `/bow` reply
    /// (`Rurik/studies/chat/FINDINGS.md:264`).
    #[test]
    fn the_server_speaks_the_player_and_the_channel_in_five_bytes() {
        assert_eq!(
            chat_message_server(1, CHANNEL_EMOTE),
            vec![0x5E, 0x00, 0x01, 0x00, 0x06]
        );
        assert_eq!(chat_message_server(1, CHANNEL_EMOTE).len(), 5);
        assert_eq!(
            chat_message_server(1, CHANNEL_GLOBAL),
            vec![0x5E, 0x00, 0x01, 0x00, 0x0A]
        );
    }

    /// The channel numbers are the client's own (`GmChat.h:3-20`), corroborated by GWCA's enum and
    /// measured from line content before either was consulted (`studies/chat/FINDINGS.md:145-158`).
    #[test]
    fn the_channels_are_the_clients_own_numbers() {
        assert_eq!(CHANNEL_ALL, 3);
        assert_eq!(CHANNEL_EMOTE, 6);
        assert_eq!(CHANNEL_WARNING, 7);
        assert_eq!(CHANNEL_GLOBAL, 10);
        assert_eq!(CHANNEL_GROUP, 11);
        assert_eq!(CHANNEL_TRADE, 12);
    }
}
