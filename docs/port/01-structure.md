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
   not the reference's - is what goes on the wire; `shared/Packets/` is that stage.
6. **`game/Server/`** - `WorldSocket`, `WorldSession`, `WorldSocketMgr`. The world program's
   listener, inside the game library, exactly as the reference arranges it.
7. **`scripts/`** - `ScriptMgr` and its hooks, then the world program's two loader calls.
8. **`game/` nouns** - one directory at a time, each its own commit.
9. **`modules/`** - the first `mod-*` crate, and the cargo member line that names it.

## Two facts that stop a careless port

- **`shared/Packets/` cannot be ported from the reference.** Its message table is Azeroth's
  protocol. Tyria's is measured material, and ASURA's `recordings/` and `docs/reference/` are
  where it is written down. The reference supplies the *shape* of the layer - a byte buffer, a
  header-keyed table, framing - and nothing about its contents.
- **`shared/Worlds/` is the reference's `Realms/`.** A realm there is the server a player picks;
  in this game that thing is a world. The rename is the only one in `shared/`.

## Open, and needing a word

| question | why it blocks |
|---|---|
| the five unassigned game directories (`Autobroadcast`, `Battlefield`, `Miscellaneous`, `Skills`, `Weather`) | `Skills` collides with `Spells/ -> Skills/`, so one of the two needs another name |
| whether `game/Modules/` (the reference has one, 2 files) stays separate from cargo modules in `modules/` | two things called modules in one tree |
| where a pack's configuration lives now that content is not SQL rows | the reference's `data/sql/` model does not describe `Gw.dat` |
