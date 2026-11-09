# AGENTS.md

Binding rules for any agent working in this repository. Read this file before the first edit of
a session.

The one sentence: **AzerothCore's server, in Rust, serving Guild Wars.** Every rule below serves
it.

## The task

Port the reference server into Rust: the same structure, the same services, the same behaviour,
with the game it serves being Tyria's rather than Azeroth's.

| | |
|---|---|
| the reference, and the specification | `C:\Users\Apo\Documents\GitHub\WowWarband` = `github.com/mod-playerbots/azerothcore-wotlk` at `30a36ec4d`. Its measured structure, and the port order, are in `docs/port/01-structure.md`. |
| the game's protocol, measured | `C:\Users\Apo\Documents\GitHub\ASURA` at `ca9f532`, frozen. A **reading** source: its `recordings/` and its `docs/` hold Tyria's wire facts and what this client does with them. |
| what is not a source | ASURA's `crates/`. Nothing is copied out of them. A fact taken from ASURA is restated in this tree's words with the capture or recording named beside it. |

A port means the reference's behaviour, field order, sizes and order of operations, translated
into Rust. It does not mean the reference's improvements, its comments or its C++ idioms.

## The rules

1. **Two halves never share a commit.** Infrastructure - processes, sockets, packets, database,
   secrets, the script manager, modules, configuration - and game mechanics - agents, terrain,
   maps, spawns, movement, combat. A commit that needs both sentences is two commits.
2. **The client is the judge.** Where a running Tyria client and a document disagree, the client
   wins and the disagreement is reported, not split.
3. **Do exactly the command.** A measurement or an explanation is evidence for reading the
   reference. It is not permission to change behaviour.
4. **One change, one subject.** The commit message is the sentence you would say out loud.
5. **A stage is done when the reference's behaviour is reproduced and nothing else moved** - not
   when the tests pass and not when the log looks right.
6. **Values are files, decisions are code.** Every number lives in the program's conf file or in
   `data/`. A rule that needs a number takes it as an argument.
7. **Every file is ASCII.** Write `-`, `->`, `=>`, `"`, `'`, `...`, `#`. Never rewrite a file
   with a `Get-Content` / `Set-Content` round trip.
8. **Additive moves.** Copy into the new home, re-export at the old path, delete the old copy in
   one commit. The compiler is then the complete list of callers.

### Before any change: the pre-flight sentence

One sentence: what changes, in which message or code path, and why it is not simply copying the
reference. If it contains anything the command did not ask for, stop and ask. Name the source as
a `file:line` in the reference and quote it. "I believe" is not a source.

## The tree

The reference's `src/` is flattened into the top level, and its dependency order is kept - read
from its CMakeLists files at `30a36ec4d`:

```text
common <- database <- shared <- game <- scripts

authserver links shared;   worldserver links shared, game, scripts
```

| directory | is | reference |
|---|---|---|
| `common/` | the library both programs link | `src/common/` |
| `database/` | connection, statements, results, the worker pool, the schema updater | `src/server/database/` |
| `shared/` | `Network/`, `Packets/`, `Worlds/`, `Secrets/`, `DataStores/` | `src/server/shared/` |
| `game/` | one directory per noun of the game; the socket layer lives in `game/Server/` | `src/server/game/` |
| `scripts/` | `ScriptMgr` and `AddScripts`: content written against hooks | `src/server/scripts/` |
| `modules/` | `mod-*`, out of tree | `modules/` |
| `authserver/`, `worldserver/` | the two programs, each with its own conf file | `src/server/apps/` |
| `data/` | the content tree and its updater input | `data/` |

The socket layer is not a program. Sockets and sessions live in `game/Server/`, which is why one
process owns one listener and still keeps the network as a subsystem.

## The naming

The reference names its directories after Azeroth's nouns. `game/` holds 41 here; the reference
holds 50.

**24 keep the reference's name:** Accounts, Achievements, Addons, Cache, Calendar, Chat, Combat,
Conditions, DataStores, Events, Globals, Handlers, Misc, Modules, Motd, Movement, Pools,
Reputation, Scripting, Server, Texts, Time, Tools, World.

**9 are the same directory renamed:** Agents <- Entities, Terrain <- Grids, Districts <-
Instances, Parties <- Groups, Skills <- Spells, Drops <- Loot, Behaviour <- AI, Trader <-
AuctionHouse, Support <- Tickets.

**3 keep their own names:** Maps, Guilds, Quests.

**5 have no reference directory:** FileCheck (archive and version exchange before login), Portal
(identity, party and storage over HTTP), Pack (the `Gw.dat` reader), Heroes, Cinematics.

**7 reference directories are dropped with a reason, not silently:** Battlegrounds,
ArenaSpectator, Warden, Mails, Petitions, OutdoorPvP, DungeonFinding have no Tyrian counterpart.
`Vehicles` is dropped with them and is not a directory: it is a class inside `Entities/`.

**5 reference directories are unassigned and are not in this tree:** `Autobroadcast`,
`Battlefield`, `Miscellaneous`, `Skills` (which collides with `Spells/ -> Skills/`), `Weather`.
`PrecompiledHeaders` and `TC9Sidecar` are C++ and tooling concerns and do not cross. Each
unassigned directory needs the owner's word before it appears.

## How to state progress

The tree, the two programs' command line, and nothing else is ported. A stage that has not landed
stops with a reason and exit code 1 instead of printing that it ran. No claim of a stage is made
without the command that proves it, and prefer the client over a green test.

## Where things are

| need | path |
|---|---|
| the reference's measured structure, and the port order | `docs/port/01-structure.md` |
| workspace, dependency order, lint policy | `Cargo.toml` |
| the wire layer | `shared/Packets/`, `shared/Network/` |
| the login conversation | `authserver/src/` |
| the game program | `worldserver/src/` |
| the hook registry | `scripts/src/ScriptMgr/` |
| one config file per program | `authserver/authserver.conf.dist`, `worldserver/worldserver.conf.dist` |
| this machine's cargo | `C:\Users\Apo\.cargo\bin\cargo.exe` |

A correction replaces the text it corrects. It is never appended beside the old text, in a file
or in an answer.
