# 0012 - Chat text is sent, and its frames come from the measured recipe

Status: accepted, on the owner's instruction that chat functionality be enabled and ported. It changes
three client-facing frames and adds an answer path; the code change lands with it, and it corrects
`docs/port/09-parity-audit.md` in the same commit, because the audit is the ledger of what is ported.

## The question

A chat line is read and commands are answered, but every answer the server generates is printed to the
log and nothing goes on the wire. The three frames that would carry text - `0x005D` the body, `0x0061`
who spoke, `0x005E` the server speaking - are written today from ASURA's table, and ASURA's own document
says what they are worth: `0x005D`, `0x005E` and `0x0061` "appear in no capture, so the layouts are
guesses", and sending the `/maps` answer crashed a live client
(`ASURA/docs/reference/client-protocol.md:933-934`). What makes text appear on the client's screen, and
where does that behaviour live?

## The decision

**Four parts. Parts 1-3 correct the frames; part 4 sends the answers.**

1. **A line is a pair, never a lone body.** One or more `0x005D` body fragments, then exactly one commit
   tag: `0x0061 [sender player id, channel]` for a player's line, `0x005E [subject player id, channel]`
   for a line the server composed. A bare `0x005D` renders nothing and is silently held by the client
   (`research/Rurik/studies/isle/FINDINGS.md:393-398`); in a corpus of 225 tags every one sits directly
   after a `0x005D`, and no body is left orphaned (`studies/chat/FINDINGS.md:60-61`).
2. **The body is a coded string, not bare text.** For an All-chat line it is
   `0x0108` (the All-chat body wrapper) + `0x0107` (the literal mark) + the text + `0x0001` (the
   terminator) - the framing on 3 of 3 observed channel-3 bodies, and the eight units the confirmed
   render carried: `5d00 0800 0108 0107 h e l l o 0001` (`studies/chat/FINDINGS.md:263`). It is
   **fragmented at 121 units**, not 122: both multi-part lines in the corpus split at exactly
   121 = declared(122) - 1 (`:68-69`).
3. **The tags are five bytes, `[u16, u8]`.** `0x0061` is `[sender player id, channel]` and `0x005E` is
   `[subject player id, channel]`, wire maximum 5 bytes each, fixed (`studies/chat/FINDINGS.md:39`,
   `:42`; the confirmed bytes `6100 0100 03` and `5e00 0100 06`). The channel byte is the whole
   selector: 3 All, 6 Emote, 7 the on-screen warning panel, 10 the server's own notices, 11 Group,
   12 Trade (`studies/chat/FINDINGS.md:145-153`; the same numbers in `GmChat.h:3-20`).
4. **Answers are sent, as GWPS sends them.** GWPS's own invalid-command path builds
   the text body and sends `0x005D` plus `0x005E` on `Channel_Global` (10) - `GmChat.c:116-143`,
   `Channel_Global = 10` at `GmChat.h:14`. So an answer the server generates goes on the wire as the
   body pair, not to the log. A player's line is echoed the same way: `0x005D`* then
   `0x0061 [player id, channel]`, because the client renders no local copy of a typed line - the
   `#test` arm sent without a server echo rendered nothing (`studies/chat/FINDINGS.md:265-268`).

## Why ASURA printed instead, and why that is not the authority here

ASURA's frames were guesses, its `0x0061` and `0x005E` are models of eight body bytes where the wire has
three, and its `/maps` text exceeded what one `0x005D` can carry because it truncated at 122 instead of
fragmenting - which is the shape of a client crash, and it is why that tree prints
(`ASURA/crates/asura-world/src/chat.rs:288-324`). `AGENTS.md` gives ASURA the protocol, but it also says
the client is the judge and that a capture is a source. Here the client is the stronger witness: a
composition of these exact bytes was replayed to a real client and an operator read the text on screen,
twice - `Test Warrior: hello` and the `/bow` emote line - with a negative control that rendered nothing
(`studies/chat/FINDINGS.md:255-268`). A guess that crashes and a measurement that renders are not
neighbours; the measurement wins, and this record says so.

## What it does not decide, and what stays marked

- **Authored sentences on `0x005E`: settled on a live client.** Every `0x005E` body in the corpus is an
  archive string id or a template with literal arguments, so arbitrary text on that channel was
  unproven when this record was written. The owner then ran this tree's server against the real client,
  typed `/maps`, and read the answer in chat: the composed sentences rendered. That is the first
  authored text observed on `0x005E`, and it is what licenses `answer_line` rather than a string id.
  The log line is kept beside the send.
- **The player's own line (`0x005D`* + `0x0061`): settled on a live client.** The owner connected the
  real client to this tree's world and typed a line in chat; it rendered, and `/maps` rendered its
  answer. The frames the world sent for that line were captured (`CaptureFile`) and read back with
  `tools/capture/read.py`: the body is twenty bytes, `5d 00 | 08 00 | 08 01 | 07 01 | hello as UTF-16 |
  01 00`, and the tag is five, `61 00 | 01 00 | 03` - `[u16 player, u8 channel]`, on `Channel_All`. The
  count is eight because it counts the two wrapper units, the five of "hello" and the terminator, which
  is what this record's measured bytes already said.
- **Channels 11 and 12 use the same body framing as channel 3.** That framing is measured on channel 3
  only; no group or trade body was ever captured, and Rurik's own server drops those sigils rather than
  guess. This tree keeps accepting them, with the framing stated as an assumption rather than a
  measurement.
- **Nothing is invented for text the client cannot render.** `0x005F` (agent/NPC speech) is not used:
  it is medium confidence, it has never rendered on record, and sending it with a bare record crashed
  the client (`studies/isle/FINDINGS.md:399-411`).
- **The command surface itself.** What the commands are, and which refusals exist, is
  `0011-the-command-surface-is-content.md`; this record only says answers get sent.
- **The Lua binding and the runtime.** An answer a script composed travels the same path this record
  opens; the platform is `docs/port/11-content-and-behaviour.md`.

## What it costs

- `game/src/Server/Packets/ChatPackets.rs`: the two tags become five bytes with the fields in the
  reference's order, the body becomes a coded string, and one `0x005D` becomes a fragment list.
- `game/src/Handlers/ChatHandler.rs`: every generated answer is sent as well as printed.
- The player id must fit the tag's `u16`. This tree allocates ids from one upward per map
  (`game/src/Districts/Peers.rs`, `allocate`), so the bound holds; the builder refuses rather than
  truncates if it ever does not.
- `docs/port/09-parity-audit.md`: `0x005D`, `0x005E` and `0x0061` move from "ported" to ported with
  measured layouts, and the chat sequence row states the answer path.

## How the claim is proved

1. A unit test per builder pins the confirmed bytes: `chat_message_core` over a fragment, the two
   five-byte tags, and the All-chat body `0108 0107 h e l l o 0001`.
2. A test drives the handler and asserts the frame order a player's line produces: every `0x005D`
   fragment first, then one `0x0061`, with the channel byte and the sender id.
3. A live client: `/maps` and an unknown command display their text; a `!` line displays as
   `name: text`. This is the only step that establishes client acceptance, and no test above substitutes
   for it. **Done, by the owner's runs:** `/maps` was typed on a live client and its text was read in
   chat, and then a line typed in chat rendered the player's own message and `/maps` rendered a list of
   maps. The frames of the latter were captured and read back (see above), so the acceptance is recorded
   with its bytes.
