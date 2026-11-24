# Kryta code feature inventory

This is the code-derived inventory of Kryta as it exists now. It answers "what can Kryta do?"
without treating a directory, a packet-table row, or a load-time placeholder as a completed feature.
The research inventory is `docs/port/17-research-code-feature-inventory.md`; structural stage status
remains in `docs/port/01-structure.md`; client packet verdicts remain in
`docs/port/09-parity-audit.md`. The canonical sequence for turning a research feature into a
Kryta-owned, editable feature is `docs/port/06-world-plan.md`, "Feature delivery model".

For a client-visible row below, **implemented** means the current tree has the request dispatcher,
state owner, ordered reply/builders, and a test or an end-to-end proof. **Load-time only** means Kryta
sends or stores data during creation/load but has no general interactive contract for changing it.

## Implemented server and client features

| feature | current Kryta behavior | code evidence |
|---|---|---|
| Configuration, logging, JSON settings, database migrations and worker pools | Both programs read JSON settings, initialize structured console/file logging, open PostgreSQL pools, and apply/check schema updates. | `common/src/Configuration/`, `common/src/Logging/`, `database/src/Database/`, `database/src/Updater/` |
| Shared networking and packet framing | The server accepts TCP sessions, assigns them to network workers, records connection counts, reads/writes time-bounded streams, frames messages from the protocol table, and has RC4-style session encryption/key exchange. | `shared/src/Network/`, `shared/src/Packets/{Frame,ByteBuffer,Types}.rs`, `shared/src/Cryptography/` |
| Protocol declaration | The complete measured table is present: 100 auth and 682 game rows, with field shapes and direction. This is protocol knowledge, not 782 completed functions. | `authserver/src/Protocol/Tables.rs`, `game/src/Server/Protocol/Tables.rs` |
| File check, auth handshake and encrypted auth conversation | A client can make the file-check exchange, version/seed exchange, encrypted login, computer-info/hash, heartbeat, EULA acknowledgement, status update, character select/delete/settings update, and game-instance request. | `authserver/src/Server/{file_check,handshake,stream,session,login_reply}.rs`; `authserver/tests/end_to_end.rs` |
| Account and portal service | HTTP portal endpoints issue the session and the user/token pair for an account the table holds; account rows support listing, creation, editing, EULA state, activation/deactivation and deletion. Inactive accounts are denied future login while their rows and characters remain. | `game/src/Portal/{mod,serve}.rs`, `game/src/Accounts/mod.rs`, `authserver/src/Server/accounts.rs`, `tools/manage/src/areas/accounts.rs`, `tools/manage/src/areas/players.rs` |
| Auth-to-world front door and travel claim | The auth listener distinguishes the game connection, relays it to the chosen world, and the world claims a one-use travel record by account/world/player token. | `authserver/src/Server/front_door.rs`, `shared/src/Network/Relay.rs`, `game/src/Maps/Travels.rs` |
| World lifecycle and controlled shutdown | The world loads maps, data, scripts, graphs and instances; runs the update/sweep loop; reports status; and exits normally or requests a supervisor restart. | `worldserver/src/main.rs`, `game/src/World/mod.rs`, `game/src/World/WorldConfig.rs` |
| Archive, map-data and path graph loading | Kryta's offline manager can read `Gw.dat`, decompress entries, parse a selected map blob and update the extracted graph cache. `worldserver` reads only that cache and pathfinds over its entries; a map without a graph is refused rather than guessed. | `tools/manage/src/path_graphs.rs`, `game/src/Pack/`, `game/src/Terrain/{MapData,GraphDump,PathTables,Pathfind}.rs` |
| Map catalog, spawns, arrivals, portals and districts | Maps, entry spawns, doorway arches and arrivals are the world pack's per-map files (`config/World/Vanilla/maps/<campaign>/<area>/<kind>/<map id>/{basic,spawns,portals}.json`), read at start-up; the receiving map owns the arrival a crossing lands on. Entering picks or opens a district, leaving/sweeping disposes idle copies, and `kryta_world.instance` records the copies a world has open. | `game/src/Maps/{Pack,Portals,Placement}.rs`, `game/src/Districts/{mod,Instances}.rs` |
| Character persistence | Characters have account-scoped list/select/delete state, a settings blob, last outpost, saved position, held items, equipped-item lookup and unlocked-map persistence. | `game/src/Characters/{mod,Sheet,Repository,Equipping}.rs` |
| Character creation | The creation screen receives its player/armor/profession responses; confirmation writes the character and its starting inventory, with the packed appearance stored whole and a duplicate/taken name refused. Not present: the three flags of the confirm (Reforged, Dhuum, Melandru) are unread and unstored, the seven `0x8084` armour colours the client sends after each profession preview are unread, the client's name rule is not re-checked server-side, the refusal code is one the client has no text for, and every campaign starts in the same map instead of its own. The slot a worn piece occupies is content (`config/World/Vanilla/creation/worn_slots.json`, `EquippedSlotsFile`) rather than a compiled table | `game/src/Handlers/CharacterHandler.rs`, `game/src/Creation/mod.rs`, `game/src/Server/Packets/CharacterPackets.rs`, `config/World/Vanilla/creation/inventory.json`, `config/World/Vanilla/creation/equipment.json`; the gaps are `docs/port/08-rules-coverage.md`, "Character creation" |
| Map/instance load | After the game handshake, Kryta sends the ordered load block: head, player data/name, instance info, cartography grid, instance table, inventory stream, player/agent state, and finish. It joins the character to a district and introduces existing peers. | `game/src/Server/WorldSession.rs:1201-1618`, `game/src/Server/Packets/{Instance,Item,Character,Agent,Party,Misc}Packets.rs`, `game/src/Districts/Peers.rs` |
| Peer visibility | Multiple players in the same map/district receive each other's introduction, visual equipment, movement updates, chat and departure packets. | `game/src/Districts/Peers.rs`, `game/src/Server/WorldSession.rs` |
| Click movement, reported position, stop and correction | Kryta handles path requests, client position/direction reports, cancellation and last-position correction; validates against the loaded graph; updates persistent position; emits movement/tick frames; and informs peers. | `game/src/Handlers/MovementHandler.rs`, `game/src/Agents/{mod,Entry,Player}.rs`, `game/src/Terrain/Pathfind.rs` |
| Map travel and portal crossings | World-map travel, `/tp` aliases, and a movement step through a configured portal all use one transfer flow: travel record, effect/stop, redirect, map update, party/player/agent withdrawal, then reconnect/load. | `game/src/Handlers/TravelHandler.rs`, `game/src/Maps/{Portals,Travels,TravelTargets}.rs` |
| Inventory item move and equip | Inventory drags persist a valid position or restore the client to the authoritative one. Equip validates ownership/type/space, moves displaced gear, and republishes visual equipment. | `game/src/Handlers/ItemHandler.rs`, `game/src/Characters/{Repository,Equipping}.rs` |
| Item drop state | Drop (`0x802C`) decrements/removes an owned stack but intentionally emits no ground-item frame because the known alternatives crash the live client. There is no destroy (`0x8069`) handler. | `game/src/Handlers/ItemHandler.rs`, `game/src/Characters/Repository.rs` |
| Title display (partial) | The load sends the world pack's title tracks and ranks (`0x00F6`, `0x00F3`); display and hide (`0x8058`, `0x8059`) are answered with `0x00F4` to the player and its peers; the Portal serves custom title text. Earning and persistence are absent. ASURA has no counterpart; the audit records the packets as ported thin from the retail and Preforged recordings. | `game/src/Characters/{Titles,Metrics}.rs`, `game/src/Server/Packets/TitlePackets.rs`, `game/src/Server/WorldSession.rs:1577`, `config/World/Vanilla/titles.json`, `docs/port/19-title-research.md` |
| Chat and chat commands | Local chat is parsed and broadcast in the district. The built-in command table routes `/tp`, `/maps`, and `/stuck` through the shared operation registry. | `game/src/Handlers/ChatHandler.rs`, `game/src/Chat/{mod,Commands,Operations}.rs`, `config/Server/commands.json` |
| Game operations and operator API | The registry verifies caller/surface/arguments and runs calls on the world thread. Eight operations are registered: `chat.transfer`, `chat.maps`, `chat.stuck`, `world.status`, `world.exit`, `world.restart`, `world.operations` and `ListTargets`. A loopback credential-authenticated HTTP endpoint exposes them. | `game/src/Chat/Operations.rs`, `worldserver/src/{CommandLine,RemoteAccess}.rs` |
| Standalone and in-game console | `kryta-cli` lists/calls operator operations over the API; the world console uses the same registry; configured chat commands use the same operations rather than a second command system. | `worldserver/src/CommandLine/{client,mod}.rs`, `config/Server/commands.json` |
| Supervisor | The supervisor starts configured services, watches their output/exit code, stops/restarts them, retains log tails, and exposes loopback `status`, `stop`, `restart`, and `quit` controls. | `tools/supervisor/src/{main,Control}.rs` |
| Configuration Manager, current functional surfaces | The local manager was rebuilt on 2026-11-23 as a JSON service with static pages (`src/api.rs`, one module per area under `src/areas/`, the pages under `ui/`; `docs/port/23-control-panel.md`), and follows the game's hierarchy: the server hosts Accounts; an account owns its Players (characters); a player page owns its progress (level, experience, gold), position, inventory and equipment, titles and unlocks. It creates, edits, deactivates and removes accounts; creates, renames, levels and removes players; reads each player's account and data (the titles panel shows the world pack's title states, since titles are not recorded per character); shows supervisor status plus stop/restart through an already-running supervisor; exposes the world's registry as a console; shows the world's own status; reads the world pack's content and the running instances; and reads worlds, settings, schema and the navigation cache. A page never waits on another process. It does not launch a supervisor or an offline stack. | `tools/manage/src/{lib,view}.rs`, `tools/manage/src/web/`, `tools/manage/src/pages/`, `tools/manage/manage.json.dist` |
| Capture and comparison tooling | The world can write both directions of its client wire to a capture; reader/diff tools inspect and compare captures message-by-message. | `game/src/Server/Capture.rs`, `tools/capture/` |

## Data sent during load, but not an interactive feature yet

These values are real packets and are important for a valid load. They are not evidence that Kryta
implements the surrounding player-facing system.

| data currently sent or stored | present boundary |
|---|---|
| One-player party packets | Load and transfer send a one-player party representation. There is no complete invite, accept, leave, leader or multi-player party contract. |
| Profession, skillbar, attributes, unlocked skills/professions, factions and PvP unlock data | Creation/load packets can populate the client view. There is no general skill use, skillbar edit, attribute allocation, secondary-profession, faction or PvP feature contract. |
| Cartography and unlocked maps | Kryta sends an all-unexplored grid and persists basic map unlock rows. Exploration progression and a full world-map feature are not implemented. |
| Items, bags, weapon-set packets and gold storage field | Creation/load emits starter inventory representations. Storage, weapon-set selection, gold economy, merchants, crafting, salvage, trade and dye are not implemented. |
| Agent visual equipment/cape and player appearance | Load and peer visibility describe the player. There is no NPC/monster administration, general creature simulation, costume/dye contract, or appearance editor. |
| Script manager | The hook registry/lifecycle exists and is called, but the shipped loaders register no scripts and Lua is not implemented. |
| Manager settings and content pages | Settings, worlds, schema and world content are shown read-only. There is no configuration editing, no JSON asset editing and no log view, and no page for them: a destination that does not work is not in the rail (`docs/port/14-operator-surfaces.md`). |

## Explicitly not implemented

These directory names are deliberate structural homes, but their current `mod.rs` files contain no
feature implementation: Achievements, Addons, Behaviour, Cache, Calendar, Cinematics, Combat,
Conditions, Drops, Events, FileCheck, Globals, Guilds, Heroes, Misc, Modules, Motd, Parties, Pools,
Quests, Reputation, Skills, Support, Texts, Time, Tools, and Trader.

Consequently Kryta does not currently implement a full contract for combat, skill casts/effects,
NPCs/AI, quests, heroes/henchmen, social/friends/guilds, trade/merchant/crafter/collector, storage,
dye, title earning and persistence, achievements, cinematics, GM catalogues, Lua, or capture replay. Some packets for those
subjects are declared in the protocol tables; that is not implementation.

## Evidence and update rule

The implementation rows are grounded in current code and tests, including `authserver/tests/end_to_end.rs`,
the unit/integration tests beside `game/src/`, `worldserver/src/RemoteAccess/mod.rs`,
`worldserver/src/CommandLine/mod.rs`, and `tools/supervisor/src/{main,Control}.rs`. Run the relevant
package tests before changing a listed feature; a real-client run remains the final proof for a
client-visible change.

When a feature changes, update this inventory only to correct its broad feature boundary. Update
`docs/port/09-parity-audit.md` in the same commit for every client-facing packet/sequence change;
do not use this page as a second packet ledger.
