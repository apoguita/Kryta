# The reference's structure, measured, and the order this port takes it

Read from `C:\Users\Apo\Documents\GitHub\WowWarband`, which is
`github.com/mod-playerbots/azerothcore-wotlk` at `30a36ec4d`. Every number below came from that
checkout today, not from a document about it.

Kryta ports the client-facing behaviour of the working ASURA server into AzerothCore's structure
and segmentation. ASURA's own architecture is not the arrangement being ported. The roles,
local copies and limits of ASURA, WowWarband and the other research projects are recorded in
`docs/research-sources.md`; their project documents are not interchangeable Kryta design decisions.

## Two programs, and what each does in its own order

| program | entry point | startup order |
|---|---|---|
| `authserver` | `src/server/apps/authserver/Main.cpp` | `GetConsoleArguments`, `sConfigMgr->Configure` + `LoadAppConfigs`, `sLog->Initialize`, the PID file, `StartDB`, `sSecretMgr->Initialize`, `sIPLocation->Load`, the "mark every realm offline" update, `sRealmList->Initialize`, `sAuthSocketMgr.StartNetwork(bindIp, RealmServerPort 3724)`, then the io loop |
| `worldserver` | `src/server/apps/worldserver/Main.cpp` | the same config and logging, `ThreadPool` (2) at `:243`, `sScriptMgr->SetScriptLoader(AddScripts)` at `:272`, `SetModulesLoader` at `:273`, `sScriptMgr->Initialize` at `:282`, `sWorld->SetInitialWorldSettings` at `:318`, `StartWorldNetwork(BindIP, WorldServerPort 8085, Network.Threads 1)` at `:363`, the console and the SOAP thread, then `sScriptMgr->OnStartup` at `:398` |

On shutdown the world runs `sWorldSessionMgr->KickAll` (`:372`), `sMapMgr->UnloadAll`,
`sScriptMgr->OnAfterUnloadAllMaps` and `sScriptMgr->OnShutdown` (`:426`). This tree does the same
order: `sockets.stop()` is the kick (a flag every session reads on its next turn, then the network
closes) and the world's own shutdown is raised after it; `sMapMgr->UnloadAll` and its hook have no
work here, because a fresh world disposes the copies an earlier run left at its start.

The split is by conversation, not by database: auth answers login and hands out a world entry; the
world owns the gameplay socket.

## Libraries, and the dependency chain its CMakeLists state

| library | files | links |
|---|---|---|
| `common` (`src/common/`) | 173 | nothing |
| `database` | 49 | `common` |
| `shared` | 23 | `database` |
| `game` | 651 | `shared` |
| `scripts` | 693 | `game` |
| `apps` | 17 | `authserver` -> `shared`; `worldserver` -> `modules`, `scripts`, `game` |

`src/common/` holds 16 directories: Asio, Collision, Configuration, Cryptography, DataStores,
Debugging, Dynamic, Encoding, IPLocation, Logging, Metric, Navigation, Platform,
PrecompiledHeaders, Threading, Utilities.

`src/server/shared/` holds five: Network, Packets, Realms, Secrets, DataStores. Its `Network/` is
`AsyncAcceptor`, `NetworkThread`, `Socket`, `SocketMgr` - four files, and the reference's whole
socket layer at library level.

## Where the 16 common directories go

Ten are `common/` here: Configuration, Cryptography, Debugging, Encoding, IPLocation, Logging,
Metric, Platform, Threading, Utilities.

Six are not, and each has a named home or a named reason:

| reference | here |
|---|---|
| `Asio` | nothing: the runtime is tokio, used directly by `shared/src/Network/` |
| `Collision`, `Navigation` | `game/Terrain/`: they are geometry, which is the game's half |
| `DataStores` | `shared/src/DataStores/`, where this server's structure puts it |
| `Dynamic` | arrives with `scripts/`, when Lua scripts exist |
| `PrecompiledHeaders` | a C++ build concern, no Rust equivalent |

## The port order

Each stage ends with the reference's behaviour reproduced. Where the client can see the stage,
the client is the judge, not a test.

1. **`common/`** - Configuration, Logging, Platform, Threading, Utilities. This stage's proof is
   that both programs read their settings file and log what they read.
2. **`database/`** - the connection, statements, results, the worker pool, then `Updater/`. Proof:
   the schema applies and `schema_version` records it.
3. **`shared/src/Network/`** - `AsyncAcceptor` -> a listener, `NetworkThread` -> N workers, `Socket`,
   `SocketMgr`. Proof: a connection is accepted, assigned to the thread with the fewest
   connections, and closed cleanly.
4. **`shared/src/Secrets/`** - secret retrieval. Proof: the value is read through the same interface
   the reference uses.
5. **`authserver/`** - the stages in the order the table above gives, ending at the auth
   conversation. **This is the first stage whose bytes a client reads**, and Tyria's protocol -
   not the reference's - is what goes on the wire, and the four places below are where the
   reference keeps it.
6. **`game/Server/`** - `WorldSocket`, `WorldSession`, `WorldSocketMgr`. The world program's
   listener, inside the game library, exactly as the reference arranges it.
7. **`scripts/`** - the two loader calls the program hands the game library's script manager (`game/Scripting/`), then `Initialize`; the world raises `OnStartup` when it is ready and `OnShutdown` on the way down, and every world tick raises `OnWorldUpdate(diff)`.
8. **`game/` nouns** - one directory at a time, each its own commit.
9. **`modules/`** - the first `mod-*` crate, and the cargo member line that names it.

Outside the server there is a second deliverable: **the tools**, in `tools/`. A client has to be patched
to talk to us, a machine setting decides whether it reaches us, and a capture is what settles a wire
question no log can - each of those is ours, and `docs/port/05-tooling.md` says what is ported, from
where, and what is still borrowed.

## Where the port stands

Updated as stages land, because this is a statement of today and not of intent.

| stage | state, and what proves it |
|---|---|
| `common/` Configuration | done - both programs read their settings file, JSON, through one reader; `5ac97ba`, `30c01ec` |
| `common/` Logging | done - both programs log through the appender chain, and write the same lines to console and file; `8475380` |
| `common/` Utilities | partly - `GetPID` and `CreatePIDFile`; the rest arrives with the stage that calls it; `9087e75` |
| `common/` Cryptography | started - RC4, against three published vectors; `3463ed2` |
| `common/` Platform, Threading, the rest | not started; nothing calls them |
| `database/` settings | done - the three logical databases and the connection string, in the reference's shape; `c39113a` |
| `database/` Updater, file half | done - which `.sql` files exist, ordered by name, five states; `6a98037` |
| `database/` connection and pool | done - the pool opens, pings, and runs a deferred statement on a worker; `eab6be5` |
| `database/` Updater, apply half | done - a file runs once, is recorded with its sha1, and a changed file is refused; `22767f7` |
| `database/` the login schema | done - `realmlist` and `updates`, applied by the program itself from `data/sql/base/db_auth/`; `22767f7`, `9ebbb45` |
| `database/` the character schema | done for `characters` - applied by the world program from `data/sql/base/db_characters/characters.sql` and read by both programs; the table is this tree's own design, `docs/decisions/0007-character-schema.md`. A login before the world's first start finds no table, says so, and lists no character |
| `database/` the world schema | the schema's tables and the state a world writes (`instance` rows); the maps, their entry points, the doorway walls and the arrivals are the world pack's files under `config/World/Vanilla/maps/`, read at start-up by `game/src/Maps/Pack.rs` (`docs/decisions/0014-the-world-content-is-a-pack.md`); `data/sql/base/db_world/`, `game/src/Maps/`, `game/src/Districts/` |
| `shared/src/Network/` | done as a library - a client is accepted, answered and counted; `4596b27` |
| `shared/src/Packets/` | started - the byte buffer; the message families are written one file per family under `game/src/Server/Packets/`; `9f456f1` |
| the protocol tables | **ported whole and split the way the reference splits them** - 782 rows: `game/src/Server/Protocol/Tables.rs` (682, the world connection) and `authserver/src/Protocol/Tables.rs` (100, the login), written in the vocabulary of `shared/src/Packets/Types.rs` and framed by the walk in `shared/src/Packets/Frame.rs`. Proved by framing every client message of a recorded session to the size that session recorded |
| the title display | the world pack supplies the captured vanilla states and custom title definitions. The gameplay connection test checks stock packets, select, switch, and clear. The owner verified the fixed-text Hog Jumper probe. The official client now retrieves custom text from the portal, with the integrated Hero Panel check pending |
| `shared/src/Worlds/` | done - the world list is read from `realmlist`, a login refuses to start on an empty one, and `data/realms.json` merges over it so an operator can state a world in a file; `7adbe37` |
| `shared/src/Secrets/` | not started - the reference's only secret is WoW's TOTP master key, so what this game's secret is has not been decided |
| `common/` Configuration | done - the reference's lookup, severity policy, environment override and masking, over **JSON** instead of the reference's `Key = value`: one handler (`Json.rs`), one hexadecimal decoder, and record lists read as rows rather than keys; `docs/decisions/0005-json-configuration.md` |
| `authserver/` | stages 1 to 8 - config, log, PID file, login database and its schema, world list, listener; it also reports its own status on a timer, because a server that cannot say whether anything reached it cannot be tested; `tests/end_to_end.rs` plays the client's login against the binary itself - the file check, the version message, the key exchange and the portal login - reads the character list it composes, asks for a map, and then connects to a spawned `worldserver` that claims the recorded travel and loads the map: the seam between the two programs, end to end |
| the file check | answered from its own constants, before the version message on the same socket: a client that asks gets its 32 bytes, or the socket closes. Payload, not an option - `authserver/src/Server/file_check.rs` |
| the auth conversation | the plaintext half, the login message read, and the five-message answer checked byte for byte against the reference trace; `13b5287`, `5802932` |
| the message stream | a table of the client's nine auth messages, because no message carries its length: a read holding four messages is answered four times. A run of this client sent them as `0x8001`, `0x8002`, `0x8000` and `0x8023` in one 76-byte segment, each named with its size, its build number and its decrypted body in the log |
| the rest of the login | the computer info (recorded, answered with nothing), the computer hash (the session info, with the salt already on the wire), the heartbeat (`0x0000` and tick 16), the EULA (answered with nothing, as the reference does), the ask-response, the player status, a character name chosen or deleted (status 0, 23 for a name the account does not hold, 2 for a failed delete - the character list is not re-sent), and the request for a map, which is answered with the world's own address in `0x0009` and an `0x0003` behind it |
| the travel record | when a login answers a map request it writes one `travel` row - account, world, map, district, language, player token - and the world claims it once by the pair its version message presents; a login without a record falls back to the version message's map. `data/sql/base/db_auth/travel.sql`, `game/src/Maps/Travels.rs` |
| which address a client dials | **measured, 2026-11-10, build 39167**: the launch address, not the one it is handed. A listener on the named port accepted nothing and the gameplay connection arrived on the auth port 35 ms later. The auth program is therefore the front door and hands it to the world; `docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md` |
| the portal | the four endpoints served over HTTP on 6601 by the auth program, answering from the login database's `account` table; `game/src/Portal/`, `authserver/src/Server/accounts.rs`, `docs/decisions/0003-portal-host.md`, `0006-accounts-are-rows.md` |
| the front door | a gameplay connection that arrives on the auth port is read for the world it names (`game/src/Server/Protocol/`), copied to that world's address from the world list (`shared/src/Network/Relay.rs`), and the world's answer carried back. A world that does not answer closes the client within 3 s rather than leaving it waiting; `authserver/src/Server/front_door.rs` |
| the account table | `account` in `kryta_login`: the login name the client types, and the pair the portal issued for it. A name it does not hold is the reference's own `400 who are you?`. Rows are inserted by an operator - creating an account is not built yet. The EULA version the client accepts (`0x8026`) is recorded on the row, which is what stops it asking again, and so is the UI settings blob the client uploads (`0x8021`, `0x8020`; `data/sql/base/db_auth/account_settings.sql`), which the next login's `0x0016` carries back so the layout survives a restart |
| `worldserver/` | stages 1 to 9 - config, log, PID file, the script manager (0 scripts, 0 hooks registered), its three databases opened and pinged (world, characters, and the login database the travel record lives in), the map catalogue read from the world pack (`config/World/Vanilla/maps/**`, `game/src/Maps/Pack.rs`), the item templates seeded and read back, its listener on `BindIP`:`WorldServerPort` as world `RealmID`, and then the world loop - one thread, the reference's `WorldUpdateLoop` (`Main.cpp:584-620`): it measures how long it has been, sleeps the rest of `MinWorldUpdateTime`, and calls the world's own update, which tells the scripts every tick and sweeps the copies on `World.SweepIntervalMs`; the status line that names the maps it hosts and the instances it holds open rides the same loop on `StatusInterval` seconds; `tests/end_to_end.rs` plays the client's whole conversation against the binary itself - creation, the load of map 148, the items, a spoken line and the chat commands, the travel request and the ticks - and is the program's own proof, and `tests/fresh_world.rs` starts the program against a database it makes and drops itself, proving the start-up a fresh machine would see (a role without `CREATEDB` makes it say so and stop) |
| `scripts/` | `AddScripts` and `AddModulesScripts`, the two loaders a program hands the script manager - the reference's `AddScripts()` and `AddModulesScripts()` (`Main.cpp:272-273`), with the shape of `custom_script_loader.cpp` for the first script. The loaders register no scripts; Lua integration remains planned in `docs/port/11-content-and-behaviour.md`. |
| `game/World/` | the world a program represents: its id, the maps it can put a client in - `load_maps` reads the pack's map records and entry points at `SetInitialWorldSettings`, and the world keeps them with their spawn points - its scripts (`scripts()`, `set_scripts`), and its own tick, `update(diff)`, which tells the scripts (`sScriptMgr->OnWorldUpdate`, `World.cpp:1355`) and disposes the copies nobody is inside past their hold (the reference's `sWorld->Update(diff)`, `World.cpp:1131`) |
| `game/Terrain/` | the ground an agent walks on, ported whole from `ASURA/crates/asura-world/src/`: `MapData.rs` parses a map blob into `PathStaticData` (`map_data.rs`), `GraphDump.rs` reads and writes the `ASURAPG1` dump (`graph_dump.rs`), `PathTables.rs` the reference table-driven arithmetic - `PathReciproc` and `PathSqrt` as 256-entry tables of float bit patterns combined by integer arithmetic, copied from `GmPaths.c` through ASURA and checked entry for entry, because the path search costs come out of them and an FPU division would visit different trapezoids - and `Pathfind.rs` plus `Pathfind/Directional.rs` the finder itself: the BSP walk, the heap, the A* over trapezoids and portals, the corridor walk, and `PathGraphs` with `from_dump` and `bind` on the serving path. The world installs the shipped cache before its listener opens. It binds any subset of hosted maps the cache contains; a map without a graph answers `NoGraph` (`docs/decisions/0014-headless-navigation-assets.md`) |
| `game/Pack/` | the client's archive reader: `GwDat::open` reads `Gw.dat`'s header, file table and table of contents (a few megabytes of 3.6 GB), and an entry is fetched by the file id a map's own record names - `0x0001B97D` for map 148. Ported from `ASURA/crates/asura-world/src/gwdat.rs`, the index half of the reference's `FaArchive.c`; the manager's explicit offline map dump uses it. `worldserver` does not |
| `game/Maps/` | the map catalogue the world serves, read from the world pack (`Pack::load`, `game/src/Maps/Pack.rs`): 265 records (145 hostable; 120 destinations the client can resolve and this world cannot load, `is_hostable`), 467 entry points with the facing each states (`spawns.json`), and the doorway walls and arrivals each map owns (`portals.json`; 28 arches and 28 arrivals - an arrival names the map it comes from under the file's `from` key, ASURA's `portals.rs` `Arrival` shape, and the receiving map owns it, R21); `DatabaseMaps` answers the same three questions from rows when a database source is selected. `Portals.rs` is the whole of `ASURA/crates/asura-world/src/portals.rs` - a wall is crossed only when the step passes through it, a point doorway by the edges of `World.PointBoxHalfExtent`'s box, and a map is judged by its own walls and nothing else. `DatabaseTravels` is the hand-off a login writes and a connection claims once; `TravelTargets` holds the reference's twenty `/tp` aliases; `Cartography` is the exploration grid (`init_frames` builds the `0x008B` announcement and its `0x008A` slices of a Tyria grid with nothing explored); `InstanceTable` sends the destinations the client can resolve (`0x01A0` batches of fifteen pairs from each map's `instance_value`, 137 of them), right after the instance's record where a live server sends them (`ASURA/crates/asura-world/src/map.rs:22-58`) |
| `game/Parties/` | the party window and the party's travel: one table per running copy, every player founded in a party of one when it loads, the six outpost requests (invite `0x80A0`, accept with its merge `0x809C`, withdraw `0x809D`, decline `0x809E`, leave `0x80A2`, kick `0x80A9`), the batch each leaves, and the member stream a load ends with. Sizes come from `party_sizes.json`, R15's `PartySizeCap` for a map it does not list. A doorway crossing and a leader's world-map travel ask every present member's own session to run the same redirect (`World::queue_travel`) and carry the roster across (`World::hold_party`, `Table::adopt`); the leader's travel runs `World.CountdownSecs` with `0x01DA`/`0x01BC`, the two `0x01E7` confirmation questions and `0x80A3` to cancel. `/resign` marks a member in an explorable, sends its death after `World.ResignSecs`, sends `0x01D9` when every present member resigned, and the leader's `0x80A7` returns the party to its last outpost. **Not here**: the database party ledger, the timed expiry of a held place, truncation at a smaller destination cap, heroes/henchmen, and party search/difficulty (`docs/port/09-parity-audit.md` 7c) |
| `game/Districts/` | the running copies of maps, their population (`Peers.rs`: `PeerView`, the whole `introduction` and `farewell` an arrival and a departure are told with, ported from `ASURA/crates/asura-world/src/instance.rs:1031-1124` - the player, its party pair, professions, level, flags or title, effects, agent, worn items, visual equipment and cape, after the newcomer's own block - so two players in one map see each other correctly) and the district numbers the client shows: entering an outpost takes the first district with room (R22) or the one the travel named (R57), a full district is refused rather than moved (R58), a new one opens when the allotment is reached (R5, R16), an explorable opens a copy of its own (R3), and a connection gives its place back when it ends. Each copy is a row in `kryta_world.instance` (`DatabaseInstances`), holds for 15 minutes of nobody inside (R37), and is disposed by the world's tick (R40) when the hold passes |
| `game/Characters/` | the character row: `Sheet.rs` (`CharacterSheet`, what a load reads from the row, and `CharacterSheet::missing` - the fresh Prophecies Warrior a row that cannot be read is replaced by, the reference's `Player::LoadFromDB`), `Character` and its 62-byte settings blob, and `DatabaseCharacters` - the login's list, the tool's page, and the saved position a reconnect resumes at (`Position`, `position`/`record_position`/`forget_position` from `ASURA/crates/asura-store/src/state/character.rs:213-294`), what a load writes (`last_outpost`, R14), a character's items as rows (`item_instance`: the starting set is written at the confirm and drawn by the load), and its unlocks (`character_unlock`: a new character starts with the world's first outpost, and a load only enters an unlocked map - one with no rows is written the first outpost as it loads, R12) |
| `game/DataStores/` | the item templates: `DatabaseItems` seeds one row per distinct item the creation record lists name (`config/World/Vanilla/creation/inventory.json`, `config/World/Vanilla/creation/equipment.json`) at start-up, reads them back into memory, and the load resolves a character's held items against them - what an item is, once, however many of it exist |
| `game/Agents/` | the player's own agent, ported from `ASURA/crates/asura-world/src/agent.rs`: `AgentState` (its position, plane, facing, the walk it is in, its last report and speed), `begin_walk`/`advance`/`cancel_movement`/`stand_still`, `adopt_report` with the `50 + speed * elapsed` budget, `adopt_correction` with the `MaxAllowedCorrection` ceiling, `densify` at the 145-unit live cadence, `Entry.rs` the rule that decides where a character enters a map - a doorway's landing, else an outpost's scattered point (R13), else a saved position when the map is not an outpost (R14), else the map's first point (ASURA's `Standing::at_spawn`/`arrival`/`resume`, `agent.rs:460-545`), and `Player.rs` the load's three answers (`ASURA/crates/asura-world/src/player.rs`, 540 lines - `0x8091`'s inventory stream and map record, `0x8088`'s entry point and `0x8090`'s whole block, frame for frame, moved out of the session because the reference's player object owns them, `SendItems` and `SendInitialPackets*`); `game/Terrain/Pathfind.rs` holds the graphs the walks ask for, loaded from the shipped `data/path-graphs.bin` cache at start-up. The cache can cover only some maps; a map without an entry answers `NoGraph`, while a click on a bound map is answered with the finder's first waypoint and a manual move walks the offset through adjacent cells. The server never imports from the client archive; the manager's explicit offline dump owns that work. The map's own agents - the NPCs on spawn rows - arrive with step 6 |
| `game/Chat/` | the channel rule and the command surface: the row list is **data**, `config/Server/commands.json` (its rows bind `tp`/`teleport`, `maps`, `stuck`, `resign`, `guild`, `.exp` and `xp` to their operations), read by `commands::Table`, and Rust keeps the operations - `alias_key` and `travel_target` (the reference's `GmCommand_ToAscii` lookup in `game/Maps/TravelTargets.rs`'s twenty aliases) and `maps_answer`, the alias table `/maps` prints, one row per alias with the chat half wrapped at the reference's own hundred characters. A name the file grants with no operation behind it is refused when the file is read (`docs/decisions/0011-the-command-surface-is-content.md`) |
| `game/Handlers/` | the gameplay messages, one file per family as the reference keeps them (`src/server/game/Handlers/`): `ItemHandler.rs` (the inventory drag, the equip, the destroy and the unequip), `CharacterHandler.rs` (the creation screen's three asks and its confirm, with `Creation` and the block it is drawn from), `MovementHandler.rs` (the four movement messages), `TravelHandler.rs` (the travel request and its party countdown, a travel object's touch, the transfer's start, the arriving connection's landing, the defeated party's return and the `/tp` command), `ChatHandler.rs` (a spoken line and `/resign`) and `GuildHandler.rs` (the guild panel and registration) - each an `impl WorldSession` member, with the decisions left in the nouns they belong to (`characters::equip_place`, `AgentState`, `Maps::Portals`, `place_for_travel`, `DatabaseCharacters`, `crate::creation`) and the session's surface given to them as `pub(crate)`; `docs/port/10-separation-guide.md` |
| `game/Scripting/` | the script manager, where the reference keeps it (`src/server/game/Scripting/`): `ScriptObject` (`ScriptObject.h:38`), `WorldScript` (`WorldScript.h:45`), and `ScriptMgr` with the reference's registration and lifecycle - a loader set by the program, `initialize`, a script numbered by its order of registration, a name registered twice refused, and a hook answered only by the scripts that declared it (`AddScript(script, enabledHooks)`, `ScriptMgr.h:796`). Three of the thirteen world hooks are here - `OnUpdate`, `OnStartup`, `OnShutdown`, the three moments this tree raises - and the other ten are named as ported thin in `docs/port/10-separation-guide.md`, because each needs an engine moment and argument types this tree has no counterpart for |
| `game/Server/` | the gameplay connection: the 64-byte version message (`Protocol/`), the instance, item, chat, party and guild families (`Packets/`), `WorldSession` (the exchange, the creation block and the confirm - the families that follow dispatch into `game/Handlers/` - and the load in the measured order: the head block and the ping pair unasked, and a ping on the live connection answered with `0x000D` and `World.PingValue` - the load head, the player-data start, the character's name, the instance's record, the exploration grid and the instance table - then `0x8091` -> the character's items as rows and the map's record with its manifest, `0x8088` -> `0x0196`, the entry point and nothing else, the ticks coming from the ticker, `0x8090` -> the whole load block, built by `game/Agents/Player.rs`: the PvP unlocks and the item table, the account features, the unlocked lists, the skills and the progress record, the flags, the player and the agent it is drawn as with its properties and look, the party (`0x01D3`, `0x01CC`, `0x01D4`, `0x01B3`, replaced by the table's own stream when the player already belongs to one) and `0x018E`; from there the connection stays and is ticked every `World.SimulationTickMs` milliseconds, a second player's arrival is told with its player, party, agent, worn items and look (`0x0059`, `0x00B0`, `0x00B1`, `0x0020`, the unnamed items, `0x006E`, `0x0048`) and its departure with `0x005A` and `0x0021`, an inventory drag (`0x8072`) moves the item's row when the placement is one it may have and is answered with its placement, while a refused one is answered with where the item really is (`ASURA/crates/asura-world/src/item.rs:255-436`), an equip (`0x8030`) puts it in the slot its kind is worn in, takes whatever was worn there off to the first free carrying slot, and sends the agent's worn items again, a destroy (`0x8069`) removes the row and answers `0x014D`, the four movement messages (`0x803E`, `0x803D`, `0x8028`, `0x8047`) are answered by ASURA's own handlers and a tick advances the walk, a spoken line (`0x8064`) is answered with the text (`0x005D`) and who spoke it (`0x0061`) while `/maps`, `/stuck`, `/tp` and `Unknown command` are answered in the log as the reference answers them, the client's travel request (`0x80B1`) and a travel object's touch (`0x80C1`) are answered as `ASURA/crates/asura-world/src/travel.rs` answers them - the request with the eight frames of `480-518` (the effect and the stop, `0x01A6`, the redirect, the party records, and the removal of the player and the agent) off a hand-off row and a freshly drawn token, whose `is_explorable` is `World.TravelExplorable` and whose default is ASURA's `0`, and the touch with the log line alone (`132-169`) - and a step across one of the map's own walls starts that same transfer with the receiving map's arrival as the landing, which the connection the client opens next stands on and faces along (`Portals.rs`, `travel.rs:319-388`), a drop (`0x802C`) takes the stack off the ground without a word, `0x8008` ends the connection, and the load records the arrived map as unlocked (R12)) and `WorldSocketMgr` (the listener) |
| `tools/manage/` | the administration service implementation and its loopback panel, layered as `web/` (shell, stylesheet, components, HTTP helpers), `view/` (the game's names and row shapes), `pages/` (one module per destination) and one routing table; the hierarchy is the game's own - the server hosts Accounts, an account owns its Players (characters), a player page owns its data - and the account and player writes go through the nouns' repositories; account/player APIs and online session coordination remain outstanding (`docs/decisions/0008-administration-tool.md`) |
| `tools/management-service/` | the executable that starts the management service and its panel, independently or under the supervisor; account and character APIs and session coordination remain outstanding. |
| `game/World/`'s `WorldConfig` | the numbers a world runs by, one field per rule that names one (R5, R13, R15, R33, R34, R37, R53, R56), read from the program's `World` section and logged at start-up |
| `game/Creation/` | the creation screen's content: the reference's six bags, the items it creates before a choice, the 32 (campaign, profession) sets, and the player-data block built from them; the data is `config/World/Vanilla/creation/inventory.json` and `config/World/Vanilla/creation/equipment.json`, imported from its rows by `tools/import-creation-inventory.py` |
| `modules/`, the rest of `game/` | not started |

What "done" means here is narrower than a finished server: it means the stage behaves as the
reference behaves and this tree's own tests or a real run prove it. `docs/port/02-wire.md` says what
the wire knows and what it does not.

## Where the wire lives, measured

The reference keeps its protocol in four places, and this is the arrangement the port copies:

| what | where | measured |
|---|---|---|
| the byte buffer | `src/server/shared/Packets/ByteBuffer.{h,cpp}` | 2 files |
| the packet | `src/server/game/Server/WorldPacket.h` | 1 file |
| the opcode table | `src/server/game/Server/Protocol/Opcodes.{h,cpp}`, `ServerPktHeader.h`, `PacketLog.{h,cpp}` | 5 files |
| one file pair per message family | `src/server/game/Server/Packets/` | 18 pairs: `AllPackets.h`, `PacketUtilities`, and 17 families - Bank, Calendar, Character, Chat, Combat, CombatLog, Guild, Instance, Item, LFG, Misc, NPC, Pet, Query, Quest, Totem, WorldState |

The buffer is shared, the packet and the opcode table are the game library's, and the message
structs are grouped by family rather than by direction or by connection. A single table holding
every message in both directions is the arrangement this project exists to avoid.

## Two facts that stop a careless port

- **The wire's contents cannot come from the reference.** Its message table is Azeroth's protocol.
  Tyria's - headers, sizes, field order, direction - is measured from this client, and the
  captures under `C:\Users\Apo\Documents\GitHub\ASURA\recordings\` are where that was recorded.
  The reference supplies the arrangement above and nothing about its contents.
- **`shared/src/Worlds/` is the reference's `Realms/`.** A realm there is the server a player picks;
  in this game that thing is a world. The rename is the only one in `shared/`.

## Open, and needing a word

| question | why it blocks |
|---|---|
| the five unassigned game directories (`Autobroadcast`, `Battlefield`, `Miscellaneous`, `Skills`, `Weather`) | `Skills` collides with `Spells/ -> Skills/`, so one of the two needs another name |
| whether `game/Modules/` (the reference has one, 2 files) stays separate from cargo modules in `modules/` | two things called modules in one tree |
| where a pack's configuration lives now that content is not SQL rows | the reference's `data/sql/` model does not describe `Gw.dat` |

## What is next, and what stops it

The first operator slice has landed, with its contract: an embedded world console that reads its
commands from `config/Server/commands.json` and queues them to the world update; a game-owned operation registry
(`game/src/Chat/Operations.rs`) whose registrations declare their values, whose calls carry a caller and
whose refusals carry a stable code; rows that bind a typed name to a registration (`server info` ->
`world.status`) or to one of the closed chat operations, with `surfaces` deciding who may type them; a
loopback, credential-authenticated control endpoint in worldserver (`POST /v1/operations`) answering
200/400/401/404/503 with those codes; a standalone `kryta-cli`; and a `/status` page in `tools/manage`
that reads the running world through that endpoint, with `tools/manage` a supervised stack service.

The supervisor's half has landed too: a loopback control channel (`status`, `stop [service]`,
`restart <service>`, `quit`), the reference's exit-code contract ported into the world
(`0`, `1`, `2`, `Main.cpp:434-439`), the console rows that ask for it (`server exit`, `server restart`),
and the policy that turns exit code `2` into a fresh process - so a console can restart the world and
the stack keeps one world in it, not two. `console_service` in `stack.json` gives the terminal to one
service and `Stdio::null()` to the rest, so two programs cannot read one terminal. Before a service
starts, the supervisor claims the port `stack.json` names: a leftover instance of that service's own
program is stopped and replaced, so a stale service cannot take the stack down; any other process
holding the port is named and refused, never killed (`tools/supervisor/src/main.rs`, `take_port`). The
supervisor's own control port is claimed the same way (`take_control_port`), so a second stack replaces
the first instead of half-existing beside it.

The Configuration Manager was rebuilt in `tools/manage` (2026-11) around a layered arrangement: `web/`
holds the shell every page is drawn into, its stylesheet and the components a page is built from;
`view/` holds the game's names for the numbers a row stores and the row shapes a page lists; `pages/`
holds one module per destination; and one routing table in `lib.rs` names every address. The
destinations follow the game's ownership - the server hosts Accounts, an account owns its Players (the
characters), and a player page owns its progress, position, inventory, titles and unlocks - and only
destinations that work are in the rail. Services (supervisor status/stop/restart) and Console (the
world's registry) are clients of their owners' existing channels, and their pages answer before the
owner is asked; the world's status and the navigation cache are read the same way. World content is read
from the world pack, which is what the world serves; configuration editing, generic asset editing and
logs remain non-operational until their ordered steps land, and the authoritative roadmap, actual status
and next step are `docs/port/14-operator-surfaces.md`, "Agreed scope and roadmap".

`cargo test -p worldserver`, `-p game` and `-p kryta-supervisor` cover the queue, the console, every
control answer and refusal, the registry, the rows' binding and the channel's verbs;
`docs/port/05-tooling.md` records the live proofs. The chat line is on the same registry as every other
surface: `config/Server/commands.json` rows name registered operations (`chat.transfer`, `chat.maps`,
`chat.stuck`, `world.status`, `world.exit`, `world.restart`), and the chat frames were captured before
and after that move and are byte-identical. The recordings loop is in too: `worldserver`'s
`CaptureFile` records the wire in both directions (`game/Server/Capture.rs`), `tools/capture/read.py`
reads a capture in the layer it declares, and `tools/capture/diff.py` compares two captures message by
message. What remains in `docs/port/14-operator-surfaces.md` is richer argument types and target
context, and administrator authentication before the manager leaves loopback.

The current implementation and its evidence are in the status table above. The outstanding
client-facing work and its port order are in `docs/port/09-parity-audit.md`; the world subjects
and their dependencies are in `docs/port/06-world-plan.md`. The `Gw.dat` reader and pathfinder
are present in `game/Pack/` and `game/Terrain/`, as recorded in the status table.

`docs/port/15-feature-checklist.md` compares every proved ASURA_testfork checklist feature with the
current Kryta tree. It is an evidence-backed comparison, not a port backlog or a replacement for the
owning evidence.
