# The function, as ASURA measured it, and where each layer lands here

ASURA works. A real client logged in, entered an outpost, walked, chatted, travelled and left; the
recordings under `recordings/` are that run. What failed was the arrangement: one process answered
login and relayed gameplay, one library held the whole world, and the reference's boundary between
server and game was not there to be found afterwards. Two attempts were made to fix it in place -
the store split and the world program - and the last plan reached phase 4 of 6 without a single
phase proven by a client before the owner stopped it.

So the function is not copied and it is not redesigned. Each layer below is a question the server
has to answer and a set of facts a client established. The answer is written again in the
reference's shape; ASURA's files stay where they are, and so do its arrangements, because the
arrangements are what failed.

## The 18 layers, in dependency order

Dependency order, not the order a client meets them: nothing in a layer may depend on a later
one. The counts and the file names are ASURA's own measurement of its code
(`docs/work/layer-rework-checklist.md`, refreshed 2026-11-08).

| # | layer | ASURA's files | here | half |
|---|---|---|---|---|
| 1 | wire and codec | `asura-protocol`, 14 files, 8,415 lines, and the whole protocol in **one** table: `packets.rs` 2,106 lines, 782 rows | `shared/Packets/` for the byte buffer, `game/Server/Protocol/` for the opcode table, `game/Server/Packets/` for one file pair per message family | structure |
| 2 | storage and schema | `asura-store` (identity, state, placement, catalogue) + `asura-db` (schema, 27 migrations) | `database/Database/` and `database/Updater/`; the three stores are the reference's three logical databases | structure |
| 3 | rules | `asura-rules`, 1 file, 283 lines, 9 tests: pure placement decisions, no value of its own | `shared/Worlds/` for the list; the decision that picks one belongs to `authserver/` | structure |
| 4 | auth service | `asura-auth`, 6 files, 2,664 lines: the login conversation | `authserver/`, a program, as the reference has it | structure |
| 5 | socket layer | `asura-gateway`, 3 files, 534 lines: listener, file check, four-byte classification, relay | `shared/Network/` for the machinery, and `authserver/`'s front door for the one client-visible address | structure |
| 6 | web gate | `asura-webgate`, 2 files, 696 lines: four HTTP answers plus health | `game/Portal/` or a program of its own - **open**, see below | structure |
| 7 | sessions | `connection.rs` 572, `dispatch.rs` 210, `world.rs` 89, `map.rs` 137, `catalogue.rs` 103 | `game/Server/` and `game/Handlers/` | structure |
| 8 | instance and simulation | `instance.rs` 378, `agent.rs` 1,158, `player.rs` 305 | `game/Districts/`, `game/Agents/`, `game/Maps/` | mechanics |
| 9 | travel | `travel.rs` 522, `portals.rs` 232, `travel_targets.rs` 164 | `game/Maps/`, `game/Handlers/` | mechanics |
| 10 | character | `creation.rs` 482, plus `default_equipment.rs` 2,957 still unfiled | `game/Agents/`, `game/Handlers/`; the equipment table is content, so `data/` | mechanics |
| 11 | items | `item.rs` 525, `pvp_items.rs` 1,571 | `game/Agents/`, `game/Drops/`, `game/Trader/` | mechanics |
| 12 | chat and commands | `chat.rs` 349, one `COMMANDS` table | `game/Chat/` | mechanics |
| 13 | map data and pathing | `map_data.rs` 1,197, `pathfind.rs` 1,795, `gwdat.rs` 432, `fa_compress*`, `path_tables.rs`, `graph_dump.rs` | `game/Terrain/` and `game/Pack/` | mechanics |
| 14 | cartography | `cartography.rs` | `game/Maps/` | mechanics |
| 15 | content itself | `default_equipment.rs`, `pvp_items.rs`, `inventory/seed.rs`, `travel_targets.rs`, 214 `INSERT`s | `data/` and the two conf files | mechanics |
| 16 | supervision | `asura-supervisor`, 10 files, 2,070 lines, `config/services.toml` | no reference equivalent - **open**, see below | structure |
| 17 | harness | `asura-fakeworld` 307 lines, 12 test files, 5 examples | `tests/` beside each crate | structure |
| 18 | evidence | `recordings/`, `data/portals.json`, `client-patcher/`, `logs/` | `recordings/` | structure |

Layers 1-7 and 16-18 are the structure: a cut there names no game noun. Layers 8-15 are the
mechanics, where the nouns are the subject, and they land only once a client is inside a map on
the structural tree.

## The first cut: the wire, written in the reference's shape

Layer 1 is first because nothing depends on it and it is the one layer provable with neither a
client nor a server: the recordings replay byte for byte.

**It is also the layer where the two arrangements differ most, and the reference wins.** The
reference does not keep its protocol in one table. It keeps four things in four places, all
measured in `src/server/`:

| what | where | size |
|---|---|---|
| the byte buffer | `shared/Packets/ByteBuffer.{h,cpp}` | 2 files |
| the packet | `game/Server/WorldPacket.h` | 1 file |
| the opcode table | `game/Server/Protocol/Opcodes.{h,cpp}`, `ServerPktHeader.h` | 4 files |
| one file pair per message family | `game/Server/Packets/`: `AllPackets.h`, `PacketUtilities`, and **17 families** - Bank, Calendar, Character, Chat, Combat, CombatLog, Guild, Instance, Item, LFG, Misc, NPC, Pet, Query, Quest, Totem, WorldState | 18 pairs |

ASURA keeps the same facts in a single file, and its own audit records what that cost: 742 of the
784 rows carry no evidence line (`crates/asura-protocol/src/packets.rs:215,233`).
One table for every message, in every direction, owned by one file - that is the arrangement this
rework exists to prevent, and it is exactly what a copy would bring back.

So the cut writes: a byte buffer that does what `ByteBuffer` does, an opcode table, and one file
pair per message family, named for Tyria's nouns rather than Azeroth's. The facts in them -
headers, sizes, field order, direction - come from what a client accepted, each row citing the
capture that proves it. The evidence lives under `recordings/`, copied from ASURA as bytes because
it is evidence, not code.

Six findings from ASURA's audit are answered by this cut rather than reproduced by it:

| finding | what the cut does instead |
|---|---|
| two account facts compiled in a builder: the campaign mask and the feature block's character-slot count (`client/messages.rs:600-605`) | they are configuration, read as arguments |
| the server's private exponent is compiled, one key for every deployment (`client/handshake.rs:53,57`) | reported to the owner; the value has a file or it has a written reason |
| missing input written as zeros, so "no character" and "the zero uuid" are one byte string (`client/messages.rs:481,614`) | the two are told apart |
| a recorded reference answer sitting in the crate as an oracle (`client/messages.rs:385`) | not carried: an answer is evidence, not a fixture |
| two handshake fields observed and never compared (`client/version.rs:151`, `client/game_version.rs:42,55`) | reproduced as the reference does it, and written down |
| 742 of 784 rows with no evidence line | no row lands without one |

## Open, and needing a word

| question | why it blocks |
|---|---|
| where the web gate lives - `game/Portal/`, or its own program beside the two | it is a client-facing HTTP service in neither the reference's tree nor this tree's naming |
| what replaces the supervisor | the reference has no such program; ASURA's is 2,070 lines that start, stop, deploy and report services |
| the five unassigned game directories (`Autobroadcast`, `Battlefield`, `Miscellaneous`, `Skills`, `Weather`) | `Skills` collides with `Spells/ -> Skills/`, so one of the two needs another name |
| `game/Modules/` (the reference has one) against cargo modules in `modules/` | two things called modules in one tree |
