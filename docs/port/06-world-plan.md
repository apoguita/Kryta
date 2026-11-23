# Porting the world: what the functionality is, where it is described, and the order the nouns come in

The infrastructure and world are in progress (`docs/port/01-structure.md`). The world
is not invented here: **every in-game system is described in detail on this machine already.** This file
is the plan for it: where each system's behaviour is described, which noun it becomes, what it needs from
the schema and the settings, and the order the chain allows.

## The source of functionality: the owner's game-rules ledger

How the game works is described in **`docs/game-rules.md`** - the owner's own ledger: 67 numbered rules
(R1-R67, with R27b correcting R27) and 234 evidenced facts, grouped by subject - travel, districts and
instances; party, heroes and henchmen; items and commerce; quests and missions; characters, skills and movement; chat and social wire;
death, resurrection and morale; guilds and alliance; aggro and monster AI; titles and character slots;
combat numbers and skill mechanics; merchants, traders, trade and crafting. It also holds what is
contradicted, what is suspect, and what the sources do not answer.

That file is authoritative for *what the game does*. It is not a port order and not an architecture: this
file maps each subject to the noun that owns it and the order the chain forces.

ASURA was the source of truth until parity with it was reached. Since parity every local project is a
source for the features it implements (`AGENTS.md`, Rule 0):
`docs/port/17-research-code-feature-inventory.md` records the code-derived feature paths found in their
code. Their architecture, language and adapter do not cross into Kryta; their proven packets, ordering
and client completion conditions are what is ported, from the source whose protocol for the feature is
the most complete. No source authorizes an invented response: where no project and no recording has
something, stop for the owner.

## Feature delivery model

Kryta is the compatible server foundation, not a copy of any research project's architecture. Every
new feature is delivered in this order:

1. **Establish the complete compatibility contract.** Start with the research feature and Kryta's
   current boundary in `docs/port/17-research-code-feature-inventory.md` and
   `docs/port/18-kryta-code-feature-inventory.md`. Record the client trigger, every packet and its
   order, required state change and accepted client result. Before client-facing implementation, open
   the sources that implement it, port from the most complete one, and use the parity audit. Where no
   project and no recording has it, stop for the owner (Rule 0).
2. **Give the feature a Kryta-owned state and content model.** Accounts, characters, progress,
   inventory and other authoritative mutable world state are database rows owned by their nouns.
   Shared editable definitions are content: JSON files read and validated at start-up and held in
   memory, never imported into rows (`AGENTS.md` rule 7) - or JSON settings when they are operator
   choices rather than world content. The classification
   test is `docs/port/04-configuration.md`; no feature inherits a research project's storage layout.
3. **Add behaviour only above working mechanics.** Rust owns validation, authoritative state and
   packet construction. Lua, once its runtime and bindings exist, may author quest logic, events, NPC
   behaviour and other rules through supported Rust operations. Lua cannot make an unfinished mechanic
   or packet contract complete.
4. **Expose supported control through the existing operator surfaces.** The manager, API, standalone
   CLI and in-game command surface call the shared registry; they do not gain a parallel execution
   path. A feature becomes editable or operable only through its own validated repository, asset
   reader or operation.
5. **Prove the full workflow.** Run the relevant server checks and verify the client-visible result
   with a real client. A handler, a packet table entry, an editor page, or a successful build alone is
   not feature completion.

This is the route for externally creating accounts and characters, defining titles, loot, skills,
quests and other content. It intentionally does not decide a feature's schema, JSON shape, Lua API or
operator command until that feature's contract and owning noun are selected.

## The nouns, and which part of the ledger describes each

| system | ledger rows that describe it (`docs/game-rules.md`) | noun | reference counterpart |
|---|---|---|---|
| a map is a definition, an instance is a running copy; outposts shared, explorables per party, districts, teardown | R1-R5, R10, R16, R29, R32-R40, R57-R58 | `game/Maps/`, `game/Districts/` | `Maps/`, `Districts/` <- `Instances/` |
| where a character enters and faces when it arrives | R13, R14, R21, R22 | `game/Maps/` (spawn and arrival points) | spawn points on the map record |
| what is in a map before anyone arrives, and what respawns | R29-R31, R34 | `game/Agents/`, `game/Behaviour/` | `Agents/` <- `Entities/`, `Behaviour/` <- `AI/` |
| the travel unit: party, leader, heroes, henchmen, caps and truncation | R6-R11, R15, R17-R28 | `game/Parties/`, `game/Heroes/` | `Parties/` <- `Groups/` |
| unlocks and campaign history gating travel | R12, R47 | `game/Maps/` (unlocks per character) | `Quests/` progression |
| aggro, the compass bubble, monster behaviour | R30, R43, E11 | `game/Behaviour/`, `game/Agents/` | `Behaviour/` <- `AI/` |
| combat numbers, death penalty, morale, resurrection | R44, E10-E15 | `game/Combat/`, `game/Agents/` | `Combat/` |
| attributes, level, experience, skill points | R46, R48, R51, E13, E14 | `game/Characters/` (a character's progression) | `Characters/` |
| skills: learning, capture, the skillbar | R50, E19 | `game/Skills/` | `Skills/` <- `Spells/` |
| items: what a character holds, dyes, storage, drops | R45, R49, E9, E16-E20 | `game/DataStores/` (definitions) + `game/Characters/` (what a character holds) | `DataStores/`, `Entities/Item` |
| merchants, traders, crafting, collectors | R54, E16-E18, E20 | `game/Trader/` <- `AuctionHouse/`, `game/Agents/` (NPCs) | `Trader/` |
| quests and missions | R52, R55-R56, E15, E21 | `game/Quests/`, `game/Districts/` (missions) | `Quests/` |
| chat and the social wire | the "Chat and social wire" section | `game/Chat/` | `Chat/` |
| guilds, friends, alliance | the "Guilds, friends, and alliance" section | `game/Guilds/` | `Guilds/` |
| titles and character slots | E22, E23 | `game/Characters/` | `Achievements/` (the reference's nearest) |
| heroes as scripted behaviour | R23-R27, and ASURA `docs/research/heroai-server-port.md` | `game/Heroes/` + `scripts/` | none - `Heroes/` is this tree's own noun |

Content profiles and the Lua/Rust behaviour boundary are in `docs/port/11-content-and-behaviour.md`.

Two subjects the ledger does not describe, and where their facts come from instead: the **archive's
file format** (`Gw.dat`) and the **wire shapes** of every message. The first is read from ASURA's archive
reader as a fact about the client's files; the second from the protocol table and the captures
(`docs/port/02-wire.md`).
## The next steps, in the ledger's order

Two things the ledger settles before any stage starts:

- **R53: every number and threshold is configurable, with the game's value as the default.** So the world's
  numbers are **settings**, not constants, and they come before the code that uses them. The rows that
  name a number: R5 (district allotment, starts at 100), R15 (party size: 8 common, 12 and 25 exist), R13
  (`spawn_scatter`, 250 units - the ledger names it as a config key), R33 (whether an instance ticks), R34
  (compass range, 10,000 units), R37 (15-minute teardown), R56 (whether the mission button is presented).
- **R1-R5 and R32-R40: the instance is the unit of work, not the map.** A map is a definition; an instance
  is one running copy; outposts are shared, explorables are per party; a district is a numbered copy of an
  outpost; an instance lives while a party is inside it and is disposed when they leave (with the one
  15-minute exception for a network disconnect).

| step | what it is | ledger rows | wire | data | proves |
|---|---|---|---|---|---|
| 0 | **the world's settings** (done) - `game/World/WorldConfig.rs`, the World section of `worldserver.json`, logged at start-up | R5, R13, R15, R33, R34, R37, R53, R56 | - | a settings file per program; content is the pack, and rows hold what the world writes | the program logs each number it read, and a changed value changes behaviour |
| 1 | **(done)** `game/Pack/` - the archive: the index (`Gwdat.rs`: the header, the file table and the table of contents, so a file id becomes an entry - this machine's `Gw.dat` holds 156,315 of them) and the decoder (`FaCompress.rs`, `FaCompress.c`'s `FaDecompress` with its six tables) | none: a client fact | - | - | a file id is read back out of the archive's own table and an id nobody put there answers `None`, and three real map blobs decode to exactly the size they announce and start with the RIFF signature `ffna` (`the_index_finds_the_blobs_the_map_catalogue_names`, `a_map_blob_decompresses_to_the_containers_own_signature`) |
| 2 | **(in progress)** `game/Maps/` + the world pack - a map as a **definition**: id, type, name, spawn points, doorway arches, arrivals | R1, R13, R14, R21, R22, R29 | the load, measured message by message: `0x017C`, `0x0186`, `0x017D`, `0x019A`, the ping pair, `0x0144`+inventory on `0x8091`, the record block and the manifest, `0x0196` on `0x8088` (the ticker sends `0x001E`), the PvP halves and `0x0059`+`0x018E` on `0x8090` (`docs/port/02-wire.md`, "The load of a real map") | the world pack's per-map files, `config/World/Vanilla/maps/<campaign>/<area>/<kind>/<map id>/{basic,spawns,portals}.json`, read at start-up (`game/src/Maps/Pack.rs`, `docs/decisions/0014-the-world-content-is-a-pack.md`); the item stream from the character row's campaign and profession | the gameplay connection of a client naming map 148 loads it the way GWPS's own trace did, message for message up to the end of the load (`an_outpost_connection_is_loaded_with_its_entry_point`), and the program itself answers it (`worldserver/tests/end_to_end.rs`, which spawns the binary and drives the same conversation, including a doorway crossing from 148 into 146); the load sends the player's own agent and its four party frames, the map's other agents arrive with step 6, and the party as a travel unit with step 7 |
| 3 | **(in progress)** `game/Districts/` - an **instance**: one running copy, its lifecycle, its district number | R2, R3, R4, R5, R16, R32, R35-R40, R57, R58 | `CtoS 177 TravelMission` (9 bytes, district 0 = unset, E220, read and logged); the district counts on the login connection (its `0x800B` -> its `0x001A`, one district in language 0, R16; ASURA answers neither); the redirect a travel sends (`0x01A6` then `0x0099`'s twelve-byte form) is ported from `ASURA/crates/asura-world/src/travel.rs:480-518` | `instance` row in `kryta_world` (`0003-instances.sql`): map, district number, language, kind, opened at, empty since | the district a connection enters is the ledger's - named (R57), first with room (R22), next when full (R5, R16), refused when full (R58) - and `0x019A` reports it; a copy is a row, held 15 minutes after it empties (R37) and disposed by the world's tick (R40); the login answers the client's district list with the one district a world begins with |
| 4 | **the travel pipeline** (in progress) - the login records a travel when it answers `0x8029`, the world claims it once when the version message presents the token pair, and R13 places the character; a doorway crossing hands over the receiving map's arrival (world pack `portals.json`, keyed by the map it came from), and a map that states no arrival and no spawn list falls back to nothing - it refuses the load only when it has neither (`Standing::arrival`); the current load unlocks the map at arrival; the scope of travel gating remains unresolved in `docs/game-rules.md` | R9, R12, R13, R14, R21, R22, R47, R57 | `AUTH_CMSG_REQUEST_GAME_INSTANCE` (26 bytes, `map_id`/district/language its own), `AUTH_SMSG_GAME_SERVER_INFO` (42), the gameplay version message's token pair | `travel` row in `data/sql/base/db_auth/`, claimed once, carrying the crossing's landing as its four columns; the receiving map's own entry point or arrival; `character_unlock` rows, the starting one written at the confirm (R12); arrival writes the map unlock; R58's full-district refusal wire is not captured | the login's record decides the map and the district even when the version message names another map, and the connection is answered at a scattered point of it (`a_travel_record_decides_the_map_and_the_district`); the login's own answer records the row (`a_map_request_after_a_login_records_the_travel`), which a second claim cannot take; the character starts with one map unlocked (R12), and the load writes it when the rows are absent; a step through Ascalon City's arch crosses into Lakeside County and stands on that map's stated arrival (`a_step_through_ascalon_citys_arch_crosses_to_lakeside_county`) |
| 5 | **(in progress)** `game/Terrain/` - the map parser, graph dump, path tables and pathfinder are present; `docs/port/01-structure.md` records their source and the archive checks | R34, R43 | movement messages | map blob | remaining parity and client acceptance follow `docs/port/09-parity-audit.md` |
| 6 | `game/Agents/` + `game/Movement/` - what is in a map, and the player's own agent | R29-R31, R34, R43 | agent families | the pack's spawn content, agent state | a spawn from the map's own content appears and moves. The player's own agent is created today (`AgentPackets::world_create_agent`, 99 bytes, ASURA's builder; `docs/port/02-wire.md`, "The players answer, frame for frame"). The stream that creates a map's other agents is sent by neither GWPS nor ASURA (`docs/port/09-parity-audit.md`, "What is not ported"); the retail client's captures under `recordings/client/raw/` record it (some 46 `0x0020` per login, struct layer), so this step has evidence and no ASURA counterpart - a stop for the owner under Rule 0 |
| 7 | `game/Parties/` + `game/Heroes/` - the travel unit | R6-R11, R15, R17-R28, R45 | party messages | `party`, `party_member` rows; hero/henchman state | a party travels as one unit and a smaller cap truncates it leader-first |
| 8 | **(in progress)** items: definitions and what a character holds | R45, R49, E9, E16-E20 | the creation stream's shapes, extended by capture | item definitions (done - the world pack's `config/World/Vanilla/creation/*.json`, held in memory; nothing is seeded, and `item_template` is empty by design, `0013-content-is-the-pack.sql`) and `item_instance` rows (done - the confirm writes a character's starting items, each row carrying the whole item, `0005-item-instance-definition.sql`; the load draws them); loot as a setting (next) | a definition is content, and a character's own items are rows: the rows draw the same item stream the creation set does, byte for byte (`a_creation_confirm_writes_a_character_row`, `a_travel_record_decides_the_map_and_the_district`) |
| 9 | skills, casting and combat | R43, R48, R50-R51, E10-E15, E19 | skill and combat messages | skill rows, effect state | a skill is cast on an agent and its effect is what the ledger says |
| 10 | quests, chat, guilds, titles (chat is done: `game/Chat/`'s command table and `/maps`, and the connection's `0x8064` handler; titles are partial: `game/Characters/Titles.rs` sends the pack's stock title rows and the patched client fetches custom text, `docs/port/19-title-research.md`) | R52, R55-R56, E15, E21-E23 | their families | their rows | each per its own rows |
| 11 | heroes and scripted behaviour | R23-R27 | - | content in `scripts/` and `modules/` | scripted behaviour out of tree |

Steps 0-4 are the spine: without them a client can only sit in the creation instance, and each of them is
**behaviour the ledger states**, not a guess. Steps 5-11 follow, each in its own rows.

## Where the ledger says we do not know

Two open items, and they are not ours to invent:

- **how a refused travel reports itself** (R58: a full or closing district cannot be entered) - the wire
  shape of the refusal is not captured.
- **the first word of `409 MapData`** - measured but not named.

Both go to the owner or to a capture, in that order of preference.

The client's in-game travel request is no longer an open item: `GAME_CMSG_PARTY_TRAVEL` (`0x80B1`, nine
bytes: the instance, then three bytes it does not interpret) is answered with the eight frames of
`ASURA/crates/asura-world/src/travel.rs:480-518`, in that order, off a hand-off row and a freshly drawn
token.

What to do with a real client in the meantime, and what the server must say at each step, is
`docs/port/07-manual-test.md`; where each rule of the ledger stands is `docs/port/08-rules-coverage.md`.

## The three seams

Everything above fits inside one noun. These cross a boundary and each has one answer:

1. **Travel, login to world.** The login records a travel when it answers `0x8029` - the account, the
   world, the map, the district, the language and the player token - and the world claims it once, by
   reading and deleting it in one statement, when the gameplay connection's version message presents
   the same world, account and token. In this tree the record is a row: `travel` in the login database
   beside `account` and `realmlist` (`data/sql/base/db_auth/travel.sql`), written by the auth program,
   read by the world through `game/Maps/Travels.rs`. That is the reference's own arrangement (its
   worldserver reads its login database) and it keeps the two programs from talking to each other. A
   connection with no record - the character-creation instance - keeps the map its version message
   names, which is how the screen worked before the record existed.
2. **Characters and their parts.** The character row is written at the creation confirm and read by the
   login's list (`docs/decisions/0007-character-schema.md`). Every later stage that adds a part - a bag, a
   skill, a quest state, a last outpost - adds a table or a column, never a second repository.
3. **What is a row and what is a file.** What a world is - its content - is the world pack's files
   (`config/World/Vanilla/`, `docs/decisions/0014-the-world-content-is-a-pack.md`); a setting is a
   program's file; what a server writes is rows; what the client dictates is a constant
   (`docs/port/04-configuration.md`, "Where each thing lives"). A port places its data by that table -
   never by where another tree happened to keep it.

## What is already in place

- The **message families** and the walk that frames anything (`shared/src/Packets/`, `docs/port/02-wire.md`).
- The **socket layer** and the world listener (`game/Server/`), so a client can arrive.
- The **creation flow**, which ends in a character row.
- The **administration tool** (`tools/manage/`): its `/content` page lists the world pack's maps, entry
  points, arrivals and item definitions, read only; editing the pack is not built.
- The **protocol table**, which already holds the rows of every family above - a row is not a noun, but it
  is the framing a noun needs.

## What this file does not decide

- The names of the tables and columns a later stage writes: those arrive with that stage. What a world
  contains is not a schema at all - it is the pack's files
  (`docs/decisions/0014-the-world-content-is-a-pack.md`, which supersedes `0009-world-schema.md`).
- Which of the documents above is authoritative where two disagree: the client is the judge, and the
  disagreement is reported (`AGENTS.md`, rule 3).
