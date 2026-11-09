# The function, as ASURA measured it, and where each layer lands here

ASURA works. A real client logged in, entered an outpost, walked, chatted, travelled and left; the
recordings under `recordings/` are that run. What failed was the arrangement: one process answered
login and relayed gameplay, one library held the whole world, and the reference's boundary between
server and game was not there to be found afterwards. Two attempts were made to fix it in place -
the store split and the world program - and the last plan reached phase 4 of 6 without a single
phase proven by a client before the owner stopped it.

So the function is not redesigned. It is inventoried, and each piece moves into the directory
that owns it in a tree that is already right.

## The 18 layers, in dependency order

Dependency order, not the order a client meets them: nothing in a layer may depend on a later
one. The counts and the file names are ASURA's own measurement of its code
(`docs/work/layer-rework-checklist.md`, refreshed 2026-11-08).

| # | layer | ASURA's files | here | half |
|---|---|---|---|---|
| 1 | wire and codec | `asura-protocol`, 14 files, 8,415 lines: `packets.rs` 782 rows, `client/frame.rs`, `client/handshake.rs`, `client/arc4.rs`, `client/messages.rs`, `client/version.rs` | `shared/Packets/`, and `common/Cryptography/` for the hashers | structure |
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

## The first cut: layer 1, into `shared/Packets/`

Layer 1 is first because nothing depends on it and it is the one layer provable with neither a
client nor a server: the recordings replay byte for byte. That was ASURA's own build order, and
it is the only cut in this whole port that can be finished alone.

What moves: the packet table, the framing, the two handshakes and the cryptosystem, from
`crates/asura-protocol` at `ca9f532`. What does not move with it: the four things ASURA's own
audit found misfiled there, which are already out - the instance table (world), the file check's
byte strings and the 591-byte login blob (configuration), the default appearance (configuration),
and the 387-row PvP item table (items).

What the layer carries that the port must not copy silently - six open findings from that audit:

| finding | where it is in ASURA |
|---|---|
| two account facts compiled in a builder: the campaign mask and the feature block's character-slot count | `crates/asura-protocol/src/client/messages.rs:600-605` |
| the server's private exponent is compiled, so one key serves every deployment | `crates/asura-protocol/src/client/handshake.rs:53,57` |
| missing input written as zeros, so "no character" and "the zero uuid" are the same bytes | `crates/asura-protocol/src/client/messages.rs:481,614` |
| a recorded reference answer sits in the crate as an oracle | `crates/asura-protocol/src/client/messages.rs:385` |
| two handshake fields are observed and never compared | `crates/asura-protocol/src/client/version.rs:151`, `client/game_version.rs:42,55` |
| 742 of 784 table rows carry no evidence line | `crates/asura-protocol/src/packets.rs:215,233` |

The first four are decisions about *where a value lives* and the last two are the naming queue's.
None of them is a reason to change a byte.

## Open, and needing a word

| question | why it blocks |
|---|---|
| where the web gate lives - `game/Portal/`, or its own program beside the two | it is a client-facing HTTP service in neither the reference's tree nor this tree's naming |
| what replaces the supervisor | the reference has no such program; ASURA's is 2,070 lines that start, stop, deploy and report services |
| the five unassigned game directories (`Autobroadcast`, `Battlefield`, `Miscellaneous`, `Skills`, `Weather`) | `Skills` collides with `Spells/ -> Skills/`, so one of the two needs another name |
| `game/Modules/` (the reference has one) against cargo modules in `modules/` | two things called modules in one tree |
