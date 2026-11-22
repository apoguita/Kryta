# The wire, as this tree knows it

A statement of today: which of this client's wire facts this server uses, where each came from, and
what is not written. It is not a plan and not a specification of its own - when it and a recording
disagree, the recording is right and this file is wrong.

Two rules govern every line here, and both come from `AGENTS.md`:

- The **arrangement** is the reference's: the byte buffer in `shared/src/Packets/`, the opcode table in
  the game library's `Protocol/`, one file per message family in `Packets/`.
- The **contents** are this client's, measured, and each fact names the document or capture that
  recorded it. None of it is Azeroth's protocol.

## The frame

After the handshake every message is a `u16` header and then its fields, with **no length field**:
a header whose bit 15 is set came from the client, and the header selects the field layout.

- Source: `docs/evidence/client-protocol.md` section 1, which corrects an earlier claim of a
  length field and names the three reference functions that show it.
- Why it matters: a port that writes a length the client does not expect gets error `007` and
  nothing else.
- The header is **two bytes on both connections**; the client's opcodes carry the `0x8000` direction
  bit (`0x8089`, `0x808B`). Measured, not assumed: a session from `ASURA`'s own world records the
  client's first frame as `header 32778`, which is `0x800A`, and `recordings/gwps/world/load-stream.log`
  prints the server's own `SEND 0x017C   6 bytes : 7C 01 3F 3F 00 00`.

### Three kinds of capture, and which layer each holds

`recordings/` holds three different things, made by three different tools, and **only two of them are
the wire**. A size or a header taken from the wrong one is the wrong number, so a citation says which
file it came from and every capture says its own layer in its header (`layer`, `header_len`).

| directory | who made it | what a record holds | the header |
|---|---|---|---|
| `recordings/gwps/` | the reference server's own send trace | the packed frame it wrote | two bytes |
| `recordings/asura/` | a working server, recording its own socket | the packed frame, both directions | two bytes |
| `recordings/client/` | the client sniffer (`tools/client/sniff-capture.py`), hooking the client's handler table | the client's message **struct**: every field widened to four bytes, zero padding out to the struct's size | four bytes |

- The packet table declares a size or upper bound. Counted fields give variable-length frames;
  the descriptor and captured wire determine the actual length. Fixed examples in the send trace
  include `SEND 0x017C 6 bytes` and `SEND 0x0186 2 bytes`.
- A struct is **two bytes wider** for a fixed message and far wider where it pads - the recorded table
  says it in as many words (`docs/evidence/packet-coverage.md`: `0x001E` 8 against 6, `0x0186` 4
  against 2, `0x000A` 240 against 140).
- **Why this is written down here and not in the tool:** reading `recordings/client/` as wire bytes
  turns a two-byte header into four, a 6-byte load head into 20 and a 58-byte confirm into 56. This
  tree did exactly that, broke a layer that was right, and undid it the next commit (`d9f0849`). The
  fact belongs to the project, not to the sniffer's docstring.

## The tables, and the walk that splits a stream

**The whole protocol is in this tree, split the way the reference splits it.** 782 rows, ported from
`ASURA/crates/asura-protocol/src/packets.rs`:

| where | holds | is |
|---|---|---|
| `game/src/Server/Protocol/Tables.rs` | 682 rows | the world connection's, in the game library beside the socket layer that serves it |
| `authserver/src/Protocol/Tables.rs` | 100 rows | the login connection's, in the program that serves it |
| `shared/src/Packets/Types.rs` | - | the vocabulary both write rows in: `Packet`, `Field`, `FieldKind`, `Conn`, `Status` |
| `shared/src/Packets/Frame.rs` | - | the walk both frame with |

The opcode tables belong to their respective connections. Auth currently links the game library for
the portal and other uses (`docs/decisions/0003-portal-host.md`, `docs/port/10-separation-guide.md`).
What is shared is the framing mechanism, which is what
`shared/Packets/` is for; each program owns the vocabulary of its own connection and calls the walk
with the row it found. (`tools/split-protocol-table.py` and `tools/extract-protocol-types.py` are the
record of how the merged file was separated again.)

A message carries no length, so a row is what says where one ends: the walk follows a message's field
layout and stops where the layout stops. A counted field (`String16`, `Array8`,
`Array16`, `Array32`) carries its own `u16` count and consumes exactly that many units, and a nested
struct is the same idea one level down (a `u8` count and that many repeats of the fields after it).
The table's `size` column is an upper bound, never a length.

- Proved against the wire: `the_wire_recordings_frame_to_their_own_sizes` in the game library frames every
  client message of a recorded session (`recordings/asura/world/live-client-session-20261027-223528.jsonl`)
  and holds it to the size that session recorded.
- One row was believed short and is not. `0x808B`'s row lists `String16(20)`, `Blob(8)` and a dword -
  twelve bytes after the name - and that is what the client sends: **sixteen bytes plus twice the
  name** (`docs/game-rules.md`, E73). A framer that waited for the `56`-byte figure of the *struct*
  capture (`recordings/client/`, which holds struct dumps and never the wire) never saw a confirm at
  all, and the client answered the silence with its own connection error. The framer now takes the
  row's walk and nothing else (`a_creation_confirm_frames_to_sixteen_bytes_plus_twice_its_name`).

## The creation screen's conversation

The frames, with the reference's own wire sizes:

| # | direction | message | wire | is |
|---|---|---|---|---|
| 1 | server -> client | `0x017C` | 6 | the load begins: `7C 01 3F 3F 00 00` |
| 2 | server -> client | `0x0189` | 2 | the creation screen may show |
| 3 | client -> server | `0x8089` | 2 | it asks for its player's data |
| 4 | server -> client | `0x0186` | 2 | the player-data stream opens |
| 4a | server -> client | `0x0144` | 5 | the item stream opens on stream 1 |
| 4b | server -> client | `0x013F` | 13 | one bag: kind, model, id, slot count, the item that is the bag |
| 4c | server -> client | `0x0161` | 31 + 2 + 2n + 1 + 4m | one item: its ids, kind, dyes, flags, name (`n` units), modifiers (`m`, counted by a **byte**) |
| 4d | server -> client | `0x013E` | 11 | where that item sits: stream, item, bag, slot |
| 4e | server -> client | `0x0148`, `0x0147`, `0x0141` | 5, 13, 8 | the active weapon set, four empty weapon sets, the gold |
| 4f | client -> server | `0x808A` | 2 | it asks for its armoury: the creation screen's own PvP half |
| 4g | server -> client | `0x001D`, `0x0018`, `0x001B` | 4, 4, 2 | the creation screen's PvP half: no unlocked skills, no unlocked heroes, and the stream ends |
| 5 | client -> server | `0x8060` | 4 | campaign and profession, chosen |
| 6 | server -> client | `0x014D` x5 | 8 each | the previous set comes off: the reference removes the worn pieces before it sends new ones, and a client still holding them asserts on the slot (`ItCliInv.cpp(105)`) |
| 7 | server -> client | `0x00DB`, `0x00B7` | 4, 9 | the empty unlocked-skill list, then the agent and the new professions |
| 8 | server -> client | `0x0161` + `0x015A` + `0x013E` + `0x014B` per item | variable | the new set: each piece created, the profession it was made for, the slot it moves into, the location it changes to |
| 9 | client -> server | `0x808B` | 16 + 2n | the confirm: the header, a counted name of `n` units, the packed appearance word and eight more bytes - 32 for a name of eight units, 30 for seven |
| 10 | server -> client | `0x0188` | 102 for a name of eight units | written: the header, the id, the name, a **map id**, the settings block. The u16 is a map id, not the `0x2211` OpenTyria fills it with: the creation capture carries `416`, the creation instance, there |
| 11 | server -> client | `0x0099` | 5 | and where the character now is: the map it starts in. Without it the client dials the creation instance back and waits there |
| 12 | server -> client | `0x018B` | 6 | refused: the header and the error code, which the client turns into text - 25 "you already have a character with the name", 29 "already in use by another player", 31 "not permitted", anything else its generic network error |
| 13 | client -> server | `0x0084` | 4 per piece | the colour of one armour slot: `u8 equipSlot, u8 colour`. Seven follow **every** `0x8060`, in the fixed slot order **4, 2, 3, 6, 5, 0, 1**, carrying the five armour-piece colours and the two hand slots' - the creation screen's dye choice, not a field of `0x008B` |

- The live client struct capture `ASURA/recordings/client/feature/character_creation/
  character_creation-live-necro_factions.jsonl` records five `0x014D` followed by `0x00B7`, `0x00DB`
  at records 314-320. Its next change also has `0x00B7`, `0x00DB` at records 364-365. This capture
  establishes observed message order, not wire sizes.
- What this tree sends on `0x8060` follows ASURA's order: the skill list comes before the profession
  and each item is created and placed only. The live capture includes additional item messages:
  `ASURA/crates/asura-world/src/creation.rs`, `handle_change_profession`. It omits the reference's
  `0x015A` and `0x014B`, and a client that has drawn the creation screen from it does not mind.
  Adopting those two is open, not done.

- The two answers whose length follows a counted field (`0x0188`'s name and settings) are sized from
  what they carry; the rest are fixed by the reference's table.
- The inventory is the reference's own: six bags, the items it creates before a profession is chosen, and the chosen profession's starting set, imported from its rows by `tools/import-creation-inventory.py` into `config/World/Vanilla/creation/inventory.json` and `config/World/Vanilla/creation/equipment.json`. A creation screen is sent about 180 item messages in all for a profession's set; what this tree sends
  today is the reference's six bags, its pre-choice items and the chosen set, which is enough for the
  client to have an inventory model to draw.
- The screens are the client's, and the wire carries their outcome: the campaign and profession go out as
  `0x8060` - `u8 chapter, u8 profession`, chapter `0` for a PvP character and `1`-`3` for Prophecies,
  Factions, Nightfall - as the choice changes, and what the appearance and name screens collect arrives
  in the confirm: the counted name, the packed word holding sex, height, skin, hair colour, face,
  profession and campaign, then 8 bytes of which only the first 4 are read, then the three flags
  (Reforged `0x1`, Dhuum `0x2`, Melandru `0x4`). The owner's description of the screen order is
  `docs/game-rules.md` R59-R67, and what the client's own code says about each screen is E221-E229.
- **A colour is not a field of the confirm.** The five armour-piece colours leave as seven `0x0084`
  after every `0x8060`, in the fixed slot order `4, 2, 3, 6, 5, 0, 1`; this tree answers the preview but
  never reads those, so a created character's armour keeps the colours in `config/World/Vanilla/creation/equipment.json`
  (`docs/game-rules.md` E226-E227).
- In this tree: `game/src/Server/Packets/`, the size table in `WorldSession`, and the row the confirm
  writes (`docs/decisions/0007-character-schema.md`).

## The load of a real map

Once the character has been chosen - or when a connection names a map it is not creating a character
on - the same connection runs the load. Measured from `recordings/gwps/world/load-stream.log`, a
working server's own trace of one login into map 146 (build 39112/39167), message by message:

| # | direction | message | wire | is |
|---|---|---|---|---|
| 1 | server -> client | `0x017C` | 6 | the load begins |
| 2 | server -> client | `0x0186` | 2 | the player-data stream opens |
| 3 | server -> client | `0x017D` | 4 + 2n | the character's name, a counted UTF-16 string: 14 bytes for "Appok" (5 units) |
| 4 | server -> client | `0x019A` | 16 | the instance: the agent, the map id, one flag byte (0 for an outpost, 1 for an explorable), the district, the language and two bytes |
| 5 | server -> client | `0x000C` | 2 | the ping request, unasked |
| 6 | server -> client | `0x000D` | 6 | its reply: 20 |
| 7 | client -> server | `0x8091` | 4 | it asks for its items (after `0x800A`, `0x800B` and `0x8009`, which is the machine information and are answered with nothing) |
| 8 | server -> client | `0x0144`, `0x0148`, the item stream, `0x0147` x4, `0x0141`, `0x00EA`-`0x00ED` | 300 bytes for a fresh character | the inventory: the stream, the active set, the bags and items, the four weapon sets, the gold, the four faction caps |
| 9 | server -> client | `0x0070`, `0x0030`, `0x0099`, `0x01AC` | 3, 28, 5, 6 | the hard-mode flag (1 only when this character has finished a campaign), the character summary, the current map, ready for a spawn |
| 10 | server -> client | `0x0199` 0, `0x0199` 1, `0x0198` (2, 877, 0), `0x0199` 0, `0x0199` 1, `0x0198` (0, map, 0) | 3, 3, 9, 3, 3, 9 | the map manifest: two phases and its end, twice. What the second word names is not measured |
| 11 | client -> server | `0x8088` | 2 | it asks where it stands |
| 12 | server -> client | `0x0196` | 26 | the answer: the map's file id, x, y, plane, and 8 inert bytes |
| 13 | server -> client | `0x001E` | 6 | the first simulation tick: the reference's dword alternates between 500 (`F4 01`) and 516/515 (`04 02`/`03 02`) across the ticks in both traces - not a counter, and what it is is not measured. This tree sends 500, the value the world's own log shows first |
| 14 | client -> server | `0x8090` | 19 | it asks for the map's players: a 16-byte blob and one byte |
| 15 | server -> client | `0x001D`, `0x0018` | 4, 36 | the PvP halves: no unlocked skills, and the unlocked-hero bitmap (eight dwords) |
| 16 | server -> client | the whole load block - see the list below | 4 to 1 + 4m each | the players answer: the PvP unlocks and the unlockable-item table, the account features, the unlocked lists and the team token, the skills and the progress record, the flags, the player and the agent it is drawn as, the agent's own properties and look, the party, and `0x018E` |

### The players answer, frame for frame

One block, in this order, ported from the working implementation's own
(`ASURA/crates/asura-world/src/player.rs`, `player_block`), which is
`GameSrv_HandleInstanceLoadRequestPlayers`:

| # | header | is |
|---|---|---|
| 1 | `0x001D` | no unlocked PvP skills |
| 2 | `0x0018` | the unlocked-hero bitmap, eight dwords of ones |
| 3 | `0x001A` x387 | the unlockable-PvP-item table, one frame per entry (`game/src/DataStores/PvpItems.rs`) |
| 4 | `0x001B` | the item stream ends |
| 5 | `0x000F` x9 | the account features: (1,10), (99,3), (100,5), (101,5,5), (102,2,2), (111,1), (124,1), (125,1), (131,1) |
| 6 | `0x0094` | five counted lists: missions, bonuses, the same two in hard mode at their full 31 zero words, and the reached instances |
| 7 | `0x00F2` | the team token, `0xBAADF00D` |
| 8 | `0x0037` | attribute points: 50 unspent, 50 spent |
| 9 | `0x00B7` | the agent and its professions |
| 10 | `0x00B6` | the unlocked-profession bitmap |
| 11 | `0x00DA` | the skillbar: eight empty slots |
| 12 | `0x00DB` | no unlocked skills |
| 13 | `0x00E9` | the progress record, **fifteen** dwords |
| 14 | `0x00EF` | the visibility flags, `FFFFFFFF FFFFFFFF` |
| 15 | `0x003C` | the player's flags word, mask 7 |
| 16 | `0x003A` | the agent's 42 attribute ranks, zero |
| 17 | `0x001F` | how long the load took |
| 18 | `0x0059` | the player: its id, the agent, the packed appearance and the name |
| 19 | `0x00B7` | **the profession again** - the second one is what gives the new player the model's profession |
| 20 | `0x0020` | the agent itself, 99 bytes: the point the entry answer named, speed 288 |
| 21 | `0x009C` | its morale, 100 |
| 22-27 | `0x009F` x2, `0x00A2` x3, `0x009F` | energy max 25, health max `80 + 20 * level`, energy fraction 1.0, health fraction 1.0, energy regeneration 0.0, public level |
| 28 | `0x00F0` | its initial effects, none |
| 29 | `0x006E` | the nine item ids it is drawn wearing, from the equipped bag |
| 30 | `0x0048` | its cape, shown |
| 31 | `0x0022` | the agent the client drives |
| 32-35 | `0x01D3`, `0x01CC`, `0x01D4`, `0x01B3` | the party: created, the player added, the member stream ended, and which party the character is in |
| 36 | `0x018E` | the load is over |

- Before 2026-11-12 this tree sent frames 16 to 18 only, and no party at all: the client hung on
  the loading screen (`m_partyClient`, `PyCliParty.cpp(650)`), and the agent it is played as was
  never created.
- **Outpost and explorable are the map's own type, not one constant.** A type-0 map is an outpost:
  `0x019A` carries 0, party changes are allowed, and the copy is one district of many. A type-1 map
  is an explorable: `0x019A` carries 1, the party is fixed, and the copy is that party's own.
- **`0x019A` must name the same agent as the load's own `0x0020`.** It is the message that tells the
  client which body it controls. Until 2026-11-22 the player id was drawn after the load head, so
  `0x019A` always said agent 1 while a second connection's own agent was 2 (and a travel's was 3):
  its health and energy updated a body it was not controlling, and its own bars stayed empty. The
  first connection worked only because its id happened to be 1.
- **Other players are introduced after frame 36, never before it.** Before 2026-11-22 the second
  player's answer opened with everyone else's `0x0059`/`0x0020`, which the client created before it
  had read its own team token (`0x00F2`, frame 7) and therefore drew as hostile. ASURA's order is the
  block first, then each introduction (`ASURA/crates/asura-world/src/player.rs:784-822`).
- The appearance word is the character's own from its row, or `World.Appearance` when the row has
  none - `0x03100000`, profession 1 and hair style 3, which is what every capture carries because it
  is the reference's own default (E74 packs the word's fields).
- The agent's numbers are settings (`AgentHealthBase`, `AgentHealthPerLevel`, `AgentEnergy`,
  `AgentEnergyRegen`, `AgentSpeed`), read from the world's file and logged at start-up. Maximum
  health is `AgentHealthBase + AgentHealthPerLevel * level` (ASURA `combat.json`): a level 1
  character is 100, the value the client draws its own bar against.

- **The party is four frames and the order is the party object.** `0x01D3` creates it, `0x01CC` adds
  the player, `0x01D4` ends the member stream, and only then does `0x01B3` say which party the
  character is in. A client given `0x01B3` alone asserts `m_partyClient` in `PyCliParty.cpp(650)` -
  measured 2026-11-12 on the load of map 148. The four, with their numbers, are
  `recordings/client/feature/travel/doors-20261031-095754.jsonl` seq 319-322 (`d3 01 | 01 00`,
  `cc 01 | 01 00 | 01 00 | 01`, `d4 01 | 01 00`, `b3 01 | 01 00 | 01`) and
  `game/src/Server/Packets/PartyPackets.rs`.
- The position is a point of the map's own list, scattered per R13 (`docs/game-rules.md`), so
  `0x0196`'s x and y differ from run to run; the file id and plane are the map's, from its row.
- **The file id is the one thing in this block the client cannot be wrong about.** It is an entry in
  its own `Gw.dat`: a number that names no map file there makes it log `Map '0x...' is not a valid riff
  type`, `Creating default map`, and then assert in `Map.cpp(1762)`. Measured 2026-11-12 with map 148's
  row carrying 112509 (`0x0001B77D`); the value a real load of that map carried is **113021**
  (`0x0001B97D`, `recordings/client/feature/maps/pre-searing-load-3.jsonl` seq 137, the fourth byte of
  the `0x0196` frame onward, and ASURA's own catalogue row for map 148). `0006-map-148-file-id.sql`
  corrects the row.
- `0x017D`'s name is the character row's `name`, as the creation confirm wrote it; the campaign and
  profession of the same row choose which starting set the item stream carries.
- The party stream is `game/Parties/`: a load's own section is `load_section`, and a player that is
  already in a party has it spliced into the block in place of the four-frame party of one
  (`splice_load_section`, before `0x018E`). The agent stream (row 16) is the reference's map content -
  one `0x001A` per agent in the map - and it arrives with `game/Agents/` (plan step 6); the world sends
  no NPC agents yet.
- **After the load, the party's frames are the measured ones.** A resign sends `0x002D` and
  `0x0026 {4}` after `World.ResignSecs` (`multiplay-live-20261106-200638.jsonl` seq 52-55), and
  `0x01D9` when every present member resigned (seq 42). A leader's world-map travel sends
  `0x01E7 {4}` or `{5}` when it must ask, then `0x01DA {2, 1}` and `0x01BC` to every member;
  `0x80A3` cancels it with `0x01DA {0, 0}`. The builders are `Server/Packets/PartyPackets.rs`, and the
  steps are `Handlers/TravelHandler.rs` and `Handlers/ChatHandler.rs`.
- In this tree: the builders are `game/src/Server/Packets/` (`InstancePackets`, `ItemPackets`,
  `CharacterPackets`, `MiscPackets`), the order and the answers are `WorldSession`'s `load_map`, and
  the rows are `game/Maps/`'s (`0009-world-schema.md`).
- The builders' shapes are corroborated by ASURA's protocol crate
  (`crates/asura-protocol/src/game/mod.rs`), written against the same client: the manifest phase is a
  byte, the manifest end a byte, a word and a dword, the item stream a word and a byte, a bag a type,
  a model and the bag's own item - the same headers and field orders this tree sends, which is why a
  frame ported from the reference's trace and a frame ported from ASURA's records agree.
- `0x0059`'s third dword is the character's packed appearance, not a constant: `0x03100000` in every
  capture because every captured character wore the reference's own default look - profession 1 and
  hair style 3 (E74). A character's own appearance comes from its row, and `World.Appearance` is the
  fallback for one that records none (`InstancePackets::player_create`).

### What the reference's own trace shows, and what this tree sends

The reference's own log for a load of map 146 (`recordings/gwps/world/load-stream.log`) answers the
client's `0x8090` with the block above, and then with the map's own content: one `0x001A` per agent in
the map, hundreds of them in that trace, interleaved with the frames that describe them.

This tree sends the block for the player itself - all thirty-four frames listed above - and **not yet**
the map's own agents: which agents a map has is the map's content, and it arrives with plan step 6
(`game/Agents/`). An outpost will therefore be empty, and the block itself is what says so.

The position of the content follows the client's asks - the same block appears between `0x0196` and the
end in `recordings/gwps/session/login-travel-session.log`, where that client asked `0x8088` first - so
the answer is ask-driven, not a fixed script.

## The file check

Before the version message, on the same socket, the client asks which game files it should have:
**21 bytes in, 32 bytes out**. It compares the whole answer, and treats a wrong or missing one as
fatal.

| | |
|---|---|
| the question | 21 bytes, `01 00 00 00 00 F1 00 10 00 01` then eleven zeros |
| the answer | 32 bytes: header `0x02F1`, length `0x0020`, then six file ids in tight consecutive order with a zero among them |

- Source of the shape: this client's protocol, recorded in `docs/evidence/client-protocol.md`
  section 7. The reference server has no equivalent - its client does not ask this - which is why
  `game/FileCheck/` exists in this tree's naming.
- In this tree: both byte strings are constants in `authserver/src/Server/file_check.rs`, because they
  are a **payload** rather than a setting - nobody chooses them and the client compares
  them exactly (`docs/port/04-configuration.md`). Refreshing them is a new build: capture the reply a
  live client accepts and replace the literal.
- Proved by: the constants' lengths (21 and 32) and header; a real socket sending the question
  and then the preamble on the same connection; and a question that only starts like it, which is
  refused rather than answered.
- What the ids are not: a rule. They are a client-build fact, refreshed by capturing the reply a
  live client accepts - never by arithmetic on the previous set.

A socket that does not answer this leaves the client at "Connecting" with nothing to report, which
is exactly what this server did to the client before the check existed.
## The portal, before the login message

The client is launched with `-portal 127.0.0.1`, speaks plain HTTP to `localhost:6601`, and takes the
pair its login message will carry from four endpoints. Between "the client reached the auth port" and
"the client can log in" this is the step, and it is a different protocol on a different port.

| # | request | answer |
|---|---|---|
| 1 | `GET /Spawned/WebGate/session/create.xml` | `<Reply><Session>UUID</Session></Reply>` |
| 2 | `POST /Spawned/WebGate/users/login.xml` | the `UserId`/`LoginName`/`Provider` block |
| 3 | `POST /Spawned/WebGate/my_account/game_accounts.xml` | a one-row array for the game code |
| 4 | `POST /Spawned/WebGate/my_account/token.xml` | `<Reply><Token>UUID</Token></Reply>` |

The session comes from call 1 and travels in `Authorization: Arena <session>`. The refusals are the
reference's own: a missing or unknown session is `401`, a provider that is not `Portal` is
`400 Unsupported provider`, and a login name nobody knows is `400 who are you?`.

**What the portal does not check: a password.** The evidence for call 2 names the
`UserId`/`LoginName`/`Provider` block and no password field, and no capture shows the client sending
one - ASURA's own web gate, the implementation this client's four calls were measured through
(`crates/asura-webgate/src/lib.rs`), resolves `LoginName` only and answers `400 who are you?` for an
unknown email, with no password in the request. So this tree answers the login name and hands out the
pair, and the auth program then trusts the pair the login message presents. A refusal for a wrong
password is therefore not written: the client's flow has no password to refuse. What an identity
check would be is the owner's, and it is named here rather than silently absent.

- Source: `docs/evidence/client-protocol.md` section 4, measured with a real client at build
  39112, where all four answered `200` and the pair they returned is what the login message carried.
- In this tree: `game/src/Portal/`, which is one of the directories the naming adds because the
  reference has no equivalent. The arrangement is a request-to-response function with no socket in
  it, so the same code answers a test and a program.
- Who an account is: the login database's `account` table, read by the portal's caller - the auth
  program hosts the portal beside its own listener (`docs/decisions/0003-portal-host.md`), and
  `docs/decisions/0006-accounts-are-rows.md` places the identity in a row. The listener on 6601
  exists and the auth program owns it.

## The answer to the login

After the exchange and a `0x8038`, the client is owed five messages in this order, and each is sent
whatever the account holds:

| # | message | size |
|---|---|---|
| 1 | `0x0007` character info, once per character | 102 for a four-unit name |
| 2 | `0x0016` account settings | 599 |
| 3 | `0x0014` friend stream end | 10 |
| 4 | `0x0011` account info | 81 |
| 5 | `0x0003` request response | 10 |

With no characters that is 700 bytes, and the client shows an empty character list; with one it is 802.
A refusal sends only `0x0003`, with status `11`.

- Source: `recordings/gwps/world/load-stream.log`, the reference's own send trace of one login,
  which prints each message with its header, its size and its first 24 bytes. That trace is the
  authority for the sizes, and it is what settles a disagreement in the older notes: their arithmetic
  sums an extra leading 10-byte message that the trace does not carry.
- In this tree: the builders are `authserver/src/Server/login_reply.rs`, and the 591-byte block is a
  constant in `authserver/src/Server/account_settings.rs` - a payload, not a setting, so
  refreshing it is a new build rather than an edit. It is the answer for an account that has never
  uploaded settings; once the client uploads its own (`0x8021` then `0x8020`), the next login carries
  that blob instead, from `account.settings` (`docs/evidence/ui-settings-persistence.md`). The 599 in
  the table is the reference trace's size for the default block; a stored blob sets its own length.
- **Checked byte for byte against that trace**, without a client: five tests compare each builder's
  output - all ten bytes of `0x0003` and `0x0014`, and the first 24 of `0x0016`, `0x0011` and `0x0007`
  - with the bytes the reference printed. Every one matches.
- What the trace does not show, and a client alone can judge: the fields inside a message past its
  first 24 bytes, and the `eula_accepted` byte of `0x0011`.

## Splitting the stream: the table is the protocol

No message carries its length. The framer walks the selected table row's field layout, including
counts, to find the end. A single read can therefore hold several messages.

| header | size | what it is |
|---|---|---|
| `0x8001` | counted | computer info: two UTF-16 strings, each with a `u16` count |
| `0x8002` | 22 | computer hash, and the build number in its first dword |
| `0x8000` | 6 | heartbeat |
| `0x8023` | 6 | unnamed; the reference logs it and ignores it |
| `0x8038` | counted | portal login: request id, two 16-byte ids, then two counted strings |
| `0x8026` | 3 | accept EULA, one version byte |
| `0x8035` | 6 | ask server response |
| `0x800E` | 6 | set player status |
| `0x8029` | 26 | request game instance, six dwords |

- Source: `recordings/gwps/world/load-stream.log`, the same trace, one line per message either
  direction, with its size. The two counted messages are walked rather than measured because their
  length is on the wire: a `u16` count of UTF-16 units, not a count of bytes and not padded out to the
  field the reference's struct reserves.
- In this tree: `authserver/src/Server/stream.rs`, which answers one whole message, "not yet", or the
  header it cannot size.
- Why it is a rule rather than an optimisation: a run against this server before the table existed
  logged a single `0x8026` with nine bytes. The client had sent the EULA (three) and its ask-response
  (six) in one write; both were read as one body, the ask-response was never answered, and the client
  waited and then reset the connection.
- A header with no known size **stops the reading** and is reported with the bytes behind it. A length
  invented for it would desynchronise every message after it, and the log would blame the wrong one.

## The request for a map, and the address it is given

| # | direction | header | size |
|---|---|---|---|
| 1 | client -> server | `0x8026` accept EULA | 3 |
| 2 | server | nothing - the reference sends nothing for it | - |
| 3 | client -> server | `0x8035` ask server response | 6 |
| 4 | server -> client | `0x0003` request response, status 0 | 10 |
| 5 | client -> server | `0x800E` set player status | 6 |
| 6 | server | nothing - the reference only logs it | - |
| 7 | client -> server | `0x8029` request game instance | 26 |
| 8 | server -> client | `0x0009` game server info | 42 |
| 9 | server -> client | `0x0003` request response, status 0 | 10 |

The `0x0009` carries `req_id`, `map_token` (the world's own id), `map_id`, a 24-byte host field and a
player token drawn per travel. The host field is Winsock's shape rather than a string: two bytes of
address family little-endian (`2` for IPv4, `23` for IPv6 - Windows' values, not Linux's), then the
**port big-endian**, then the address bytes, then zero padding. The trace shows
`02 00 | 17 E0 | 7F 00 00 01`: family 2, port 6112, 127.0.0.1.

A request this server cannot place is answered with the response alone and status `2`, and with no
address: the client then shows an error it can describe, where an address nothing listens on is a
connect timeout with no message.

**The map in the answer is not always the map the client asked for.** A client has no business knowing
which outpost its character stands in, so the character's own row decides the destination
(`AuthSrv_HandleRequestGameInstance`; the rule is `game/src/Maps/Placement.rs`, ported from
`ASURA/crates/asura-rules/src/lib.rs`, `placement`):

| what the client names | where it is sent |
|---|---|
| `CreationMap` **with** the creation map type (11) | the creation instance: a creation visit is its own destination |
| `CreationMap` as a place - which is what this client sends when a character is clicked | the map the character's row names, then its last outpost, then `FallbackOutpost` |
| any other map | that map |

- Measured, 2026-11-12, build 39167: after a character was created and the list clicked, the client
  sent `0x8029` with `req_id` 6, **map 0, type 0, district 0 and language `-1`** - the creation map
  named as a place. A login that echoed it back handed the client the creation instance again and the
  client waited on a loading screen that never ended. Both numbers the rule needs are settings:
  `CreationMap` and `FallbackOutpost` (`docs/port/04-configuration.md`).
- The same request is the first one to carry a language of `-1`, which no district record can hold; the
  world reads anything above `0xFF` as "none named" and uses 0.

- Source: `recordings/gwps/world/load-stream.log` for the order and the bytes, and
  `docs/evidence/client-protocol.md` section 5a for the two layouts and the refusal rule.
- In this tree: `authserver/src/Server/login_reply.rs` (`game_server_info`, `host_blob`,
  `GM_ERROR_NETWORK_ERROR`) and `authserver/src/Server/session.rs`, which answers six dwords with an
  address taken from the realm list's own row.
- **Measured, 2026-11-10, build 39167: the client dials the address it was launched with, not the
  one it is handed.** This server named `127.0.0.1:6113` - the world program's own port, a port no
  program is listening on - and a listener on 6113 accepted nothing at all. The gameplay connection
  (`00 05 0C 00`, the 64-byte version message) arrived on **6112**, the auth port the client was
  launched against, 35 ms after the `0x0009` naming 6113 was sent. The client's own request, in the
  same session, was for `map 0` with `map_type 11` - the character-creation instance.
- What that settles: the address in `0x0009` is not how a client finds its game server here. The
  reference never had to know because its `0x0009` names its own auth address and it hands the
  connection to a game server over its control channel; this port must do the same, which is
  `docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`. Naming the launch address
  instead and answering the game protocol on the auth port was rejected: it fuses the two halves.
- In this tree: `authserver/src/Server/front_door.rs`. It reads the client's 64 bytes only for
  `map_token` (offset 12), forwards those bytes whole and unmodified to the world the id names, and
  copies bytes both ways until either side ends. It does not decode the game protocol and does not
  decide who may play: the world reads the same 64 bytes, compares the token pair against the travel
  the login recorded, and closes the connection on a mismatch. A world that does not answer closes
  the client in 3 s.

## The gameplay connection's 64-byte version message

The first thing a client sends on a gameplay connection, and the whole of what the front door reads:

| off | size | field |
|---|---|---|
| 0 | 4 | the header, `0x000C0500` |
| 4 | 4 | the client's build - never compared |
| 8 | 4 | `h0008`, observed 1 |
| 12 | 4 | `map_token` - the world's id from the answer to `0x8029` |
| 16 | 4 | `map_id` - the map that answer named |
| 20 | 4 | `player_token` - drawn per travel by the login |
| 24 | 16 | the account, little-endian UUID |
| 40 | 16 | the character, the same |
| 56 | 8 | `h0038`, `h003C`, both observed 0 |

- Source: `docs/evidence/client-protocol.md` section 11.1, which records the reference reading
  the same fields in the same order (`Connection_ReadVersion`).
- In this tree: `game/src/Server/Protocol/` - the first message of the game library's protocol, with
  the offsets spelled out in a test so a reordering fails a test rather than a client.
- What it is used for: the token pair (`map_token` with `player_token`) is what the world compares
  against the travel the login recorded, and the record's `map_id` - not this message's - is the map
  that gets loaded. A connection whose pair claims no record keeps the map named here, which is how
  the character-creation instance arrives. The login records the row when it answers `0x8029`
  (`data/sql/base/db_auth/travel.sql`), and the world claims it once (`game/src/Maps/Travels.rs`,
  `docs/port/06-world-plan.md`, "The three seams").

## The first four bytes of a connection

The client's very first bytes are a bare little-endian `u32`, not a framed message. Decoding them as
one would read `0x0400` as a message id.

| header | what it is | first message size |
|---|---|---|
| `0x000C0400` | an auth connection | 16 |
| `0x000C0500` | a gameplay connection | 64 |
| `0x000C0FFF` | a control connection | 4 |

- Source: `docs/evidence/client-protocol.md` section 2.
- In this tree: `authserver/src/Authentication/`, whose `ConnectionKind::classify` answers one of the
  three and refuses anything else rather than guessing.
- Proved by: a real socket sending `0x000C0400` and being told what it is, and one sending
  `[EF, BE, AD, DE]` and being refused with its own bytes logged.

## The exchange

```text
client -> server   header 0x4200, then 64 bytes: the client's public value, little endian,
                   read as a 512-bit integer                            (66 bytes total)
server             shared = client_public ^ server_private mod prime   (512-bit prime, 511-bit
                   exponent; neither is chosen by the server)
server             20 random bytes: the master secret
server -> client   header 0x1601, then master[i] XOR shared_le[i]      (22 bytes total)
both               key = the client's own 20-byte digest of the master
both               RC4 keyed with those 20 bytes, one instance per direction, both from offset 0
```

- Source: `docs/evidence/client-protocol.md` section 3, whose sizes and constants come from
  the reference's handshake and the client's own arithmetic.
- In this tree: `authserver/src/Server/handshake.rs` (the sizes, the two constants as decimal
  digits, `shared_secret`, `server_seed`) and `common/src/Cryptography/Arc4.rs` (the cipher).
- Proved by: three published RC4 vectors; the constants' own bit widths; and a test that plays the
  client against a real listener and recovers the master out of the XOR.

Two deviations, both recorded where they are taken:

- a public value congruent to zero modulo the prime is **refused**, because the shared secret would
  be zero and the exchange would have exchanged nothing. The reference carries on with the zero; the
  real client cannot present one, so the refusal costs nothing.
- the master secret comes from the operating system's entropy, and a machine without any logs that
  instead of silently using zeros.

## The digest the connection is scrambled with, and what will prove it

After the server seed, both sides derive a 20-byte key from the master and run RC4 with it, one
instance per direction, both from offset zero.

- Source of the shape: recorded behaviour - five rounds over the five little-endian words of the
  master, from SHA-1's initial state, with its first two round constants and two f functions, and a
  digest that adds the working state back onto the input words. Recorded in
  `docs/evidence/client-protocol.md` section 3 and implemented there.
- In this tree: `authserver/src/Server/handshake.rs` (`derive_key`) and `common/src/Cryptography/`
  (`Arc4`). It is the one wire fact here that is **transcribed rather than derived**, because there
  is no other record of it and nothing in this repository can prove it: no capture holds a master
  and its digest, there is no published vector, and no other path would fail loudly if a rotation
  constant were wrong.
- What *is* checkable here: that it maps 20 bytes to 20, deterministically, and that a changed
  master changes the digest; and that a message encrypted with the derived key is read back through
  it as its own id.

**The client is the proof, and the log now says what it says.** Every message the client sends after
the exchange is decrypted, named by its header, and answered for itself:

| the log line | what it means |
|---|---|
| `login request N, user <uuid> session <uuid> character '<name>' type '<type>'` | the derivation is right and the framing is right, because a wrong key reads the login message as noise and a wrong frame reads it as the wrong size |
| `message N is 0xXXXX with ... bytes, which this server does not answer` | a message this server has no behaviour for; its header and size are in the line |
| `a message with header 0xXXXX has no size this server knows` | the stream cannot be split any further; every byte received is printed beside it |
| `auth.trace ... 0xXXXX body: ...` | at `auth.trace` = 6: the decrypted body of each message, which the raw byte trace cannot show |

That is the whole reason both traces exist: a wrong digest, an unknown length and a message that
never arrived are three different failures, and each one now names itself.

## Where a fact goes when it is learned

A new message's header, size and layout: this file and the code that uses it, in the same commit,
with the capture or the document section named beside it. A fact with no evidence is a guess wearing
a constant's clothes, and the reference's own history has a list of what that costs.
