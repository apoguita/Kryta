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
| the function, working | `C:\Users\Apo\Documents\GitHub\ASURA` at `ca9f532`, frozen. Its `crates/` is the code a real client logged in, played and travelled against - a layer **moves** from there, unchanged in behaviour. Its `recordings/` are the evidence each byte rests on. Its `docs/` are the record of what the code does and of what failed here. |
| the layering | `docs/port/02-layers.md`: ASURA's 18 layers in dependency order, and the directory here that owns each. |

Three sources, three questions: the reference answers *what the structure is*, ASURA's code
answers *what the function is*, and a running Tyria client answers *whether it is right*.

A port means the reference's structure and the function's behaviour, field order, sizes and order
of operations, translated into Rust. It does not mean the reference's improvements, its comments
or its C++ idioms.

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

## How a cut lands

Kryta exists because fixing ASURA in place failed. ASURA works - a live client logged in, entered
a map, walked, chatted and travelled - but one process answered login and relayed gameplay, one
library held the whole world, and server and game were fused into the same files. The attempt to
re-cut it in place reached phase 4 of 6 and never proved a single phase with a client before it
was stopped. So the function is moved, not redesigned, and it moves into a tree that is right
before any of it arrives.

**A layer is covered only when all six hold** (ASURA, `docs/work/layer-rework-checklist.md`):

1. The file is the source of truth: the value lives in `data/` or a program's conf file, and
   nowhere in code.
2. Every reader reads the file.
3. The old source is deleted, in its own commit.
4. The equivalence diff is empty, and the command that says so is named in the commit.
5. Every file is ASCII, and the tests the change touches pass.
6. **A real client was tested**, wherever the layer is client-visible.

**The rules for every cut:**

1. **Additive.** The new home is built beside the old, callers move onto it, and the old path is
   deleted when nothing uses it. Here the old is ASURA at `ca9f532`, frozen; a cut copies what it
   needs and this tree stays green after every commit.
2. **The packet stream is frozen.** No cut changes a byte a client receives.
3. **One cut, one commit, green.** Build succeeds, tests pass, the client does the thing where the
   client can see it. No cut carries a feature.
4. **A cut that cannot be built beside what runs is split again** rather than forced.
5. **A structural cut names no in-game noun** - no map, no spawn, no agent, no walk, no tick. A
   suite that fails on a game value stays red, and the reason belongs to the mechanics
   workstream. This is the rule the last project broke most often, and it is why infrastructure
   work turned into arguments about map kinds.

Rule 5 is what separates the two halves the first rule of this file keeps apart: layers 1 to 7 and
16 to 18 are structure, and a cut there names no game noun. Layers 8 to 15 are the mechanics
themselves, where the nouns are the subject - and they land only after a client is already inside
a map on the structural tree.

## The function inventory

ASURA measured 18 layers in dependency order, and they are what this server has to do. Every
layer, its files and the directory here that owns it are in `docs/port/02-layers.md`. The order is
the bottom of the dependency graph first, not the order a client meets them.

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
