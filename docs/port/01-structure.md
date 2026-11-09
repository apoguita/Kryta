# The reference's structure, measured, and the order this port takes it

Read from `C:\Users\Apo\Documents\GitHub\WowWarband`, which is
`github.com/mod-playerbots/azerothcore-wotlk` at `30a36ec4d`. Every number below came from that
checkout today, not from a document about it.

## Two programs, and what each does in its own order

| program | entry point | startup order |
|---|---|---|
| `authserver` | `src/server/apps/authserver/Main.cpp` | `GetConsoleArguments`, `sConfigMgr->Configure` + `LoadAppConfigs`, `sLog->Initialize`, the PID file, `StartDB`, `sSecretMgr->Initialize`, `sIPLocation->Load`, the "mark every realm offline" update, `sRealmList->Initialize`, `sAuthSocketMgr.StartNetwork(bindIp, RealmServerPort 3724)`, then the io loop |
| `worldserver` | `src/server/apps/worldserver/Main.cpp` | the same config and logging, `ThreadPool` (2) at `:243`, `sScriptMgr->SetScriptLoader(AddScripts)` at `:272`, `SetModulesLoader` at `:273`, `sScriptMgr->Initialize` at `:282`, `sWorld->SetInitialWorldSettings` at `:318`, `StartWorldNetwork(BindIP, WorldServerPort 8085, Network.Threads 1)` at `:363`, the console and the SOAP thread, then `sScriptMgr->OnStartup` at `:398` |

On shutdown the world runs `sWorldSessionMgr->KickAll` (`:372`), `sMapMgr->UnloadAll`,
`sScriptMgr->OnAfterUnloadAllMaps` and `sScriptMgr->OnShutdown` (`:426`).

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
| `Asio` | nothing: the runtime is tokio, used directly by `shared/Network/` |
| `Collision`, `Navigation` | `game/Terrain/`: they are geometry, which is the game's half |
| `DataStores` | `shared/DataStores/`, where this server's structure puts it |
| `Dynamic` | arrives with `scripts/`, when Lua scripts exist |
| `PrecompiledHeaders` | a C++ build concern, no Rust equivalent |

## The port order

Each stage ends with the reference's behaviour reproduced. Where the client can see the stage,
the client is the judge, not a test.

1. **`common/`** - Configuration, Logging, Platform, Threading, Utilities. This stage's proof is
   that both programs read their conf file and log what they read.
2. **`database/`** - the connection, statements, results, the worker pool, then `Updater/`. Proof:
   the schema applies and `schema_version` records it.
3. **`shared/Network/`** - `AsyncAcceptor` -> a listener, `NetworkThread` -> N workers, `Socket`,
   `SocketMgr`. Proof: a connection is accepted, assigned to the thread with the fewest
   connections, and closed cleanly.
4. **`shared/Secrets/`** - secret retrieval. Proof: the value is read through the same interface
   the reference uses.
5. **`authserver/`** - the stages in the order the table above gives, ending at the auth
   conversation. **This is the first stage whose bytes a client reads**, and Tyria's protocol -
   not the reference's - is what goes on the wire, and the four places below are where the
   reference keeps it.
6. **`game/Server/`** - `WorldSocket`, `WorldSession`, `WorldSocketMgr`. The world program's
   listener, inside the game library, exactly as the reference arranges it.
7. **`scripts/`** - `ScriptMgr` and its hooks, then the world program's two loader calls.
8. **`game/` nouns** - one directory at a time, each its own commit.
9. **`modules/`** - the first `mod-*` crate, and the cargo member line that names it.

## Where the port stands

Updated as stages land, because this is a statement of today and not of intent.

| stage | state, and what proves it |
|---|---|
| `common/` Configuration | done - both programs read their conf file; `5ac97ba` |
| `common/` Logging | done - both programs log through the appender chain, and write the same lines to console and file; `8475380` |
| `common/` Utilities | partly - `GetPID` and `CreatePIDFile`; the rest arrives with the stage that calls it; `9087e75` |
| `database/` settings | done - the three logical databases and the connection string, in the reference's shape; `c39113a` |
| `database/` Updater, file half | done - which `.sql` files exist, ordered by name, five states; `6a98037` |
| `database/` connection and pool | done - the pool opens, pings, and runs a deferred statement on a worker; `eab6be5` |
| `database/` Updater, apply half | not started - needs the schema, which needs the tables this game has |
| `shared/Network/` | done as a library - a client is accepted, answered and counted; `4596b27` |
| `shared/Secrets/` | not started - the reference's only secret is WoW's TOTP master key, so what this game's secret is has not been decided |
| `authserver/` | through stage 5 (the login database); stops at stage 6 with a reason |
| everything after | not started |

`common/Platform`, `common/Threading` and the rest of `common/` are not started; nothing yet calls
them.

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
- **`shared/Worlds/` is the reference's `Realms/`.** A realm there is the server a player picks;
  in this game that thing is a world. The rename is the only one in `shared/`.

## Open, and needing a word

| question | why it blocks |
|---|---|
| the five unassigned game directories (`Autobroadcast`, `Battlefield`, `Miscellaneous`, `Skills`, `Weather`) | `Skills` collides with `Spells/ -> Skills/`, so one of the two needs another name |
| whether `game/Modules/` (the reference has one, 2 files) stays separate from cargo modules in `modules/` | two things called modules in one tree |
| where a pack's configuration lives now that content is not SQL rows | the reference's `data/sql/` model does not describe `Gw.dat` |
