# AGENTS.md

Binding rules for any agent working in this repository. Read this file before the first edit of
a session.

## Rule 0 - the prime directive: every packet is a 1:1 port of a source

This rule outranks every other rule and every other sentence in this repository. It is not
negotiable and it is not buried: it is the first thing a session reads.

**The goal is packet parity with the client. All work comes from porting another project or from a
live recording - nothing else. Every message, every field, every length, every ordering, every
sequence, every step of every handler is copied from its source, 1:1. Not interpreted. Not reasoned
about. Not improved. Not "fixed" until it works.**

**ASURA (`C:\Users\Apo\Documents\GitHub\ASURA`) was the project this tree was ported from, and it was
the source of truth until parity with it was reached; `docs/port/09-parity-audit.md` is the record of
that port. Since parity it is no longer the source of truth (owner, 2026-11-23). Every research
project has its own features, and each is a source for the features it implements
(`docs/research-sources.md`).**

Concretely, and binding:

1. **Before writing any code that faces the client, find the sources that implement the same thing,
   open them, and port from the one whose protocol for it is the most complete:** the same messages,
   in the same order, with the same values, the same conditions and the same refusals. Cite the source
   file and line, or the recording and sequence number, in the code, in the comment above the port.
2. **No invention.** A number, a frame, an order, a tolerance, a default, a state machine: if it is not
   in a source project or in a recording, it is not written. Stop and ask.
3. **No inference.** "It must be roughly this" and "this should work" are not sources. A recording is
   a source, a research project's code is a source, `docs/game-rules.md` is a source. A belief is not.
4. **Where sources disagree, the most complete protocol wins.** The source that carries the fullest
   packet contract for the feature - every message, both directions, in order, to the point the client
   accepts the result - is the one ported, and the disagreement is written down in the audit.
5. **No fixing in place.** A port that misbehaves is compared, line by line and frame by frame, with
   the source it came from. The difference is found and removed; nothing is invented to paper over it.
6. **A missing source is a stop, not a guess.** Where no project and no recording has something, this
   tree says so and asks the owner. It does not write its own version first and reconcile later.
7. **A port is whole.** A subject arrives with every part its source gives it - every frame, every
   handler, every branch, the settings it reads, the data it carries - not with the slice a first run
   happened to need. A subject that is only partly ported is declared *ported thin* in
   `docs/port/09-parity-audit.md`, in the same commit that makes it so.
8. **The audit is the ledger of this rule.** `docs/port/09-parity-audit.md` holds every packet and
   every sequence with a verdict - ported, ported thin, missing - and the source each came from.
   Anything this tree sends or handles that has no source is a defect of this rule. The audit is
   updated in the same commit as any change to what it lists.

A session that has not read `docs/port/09-parity-audit.md` is not ready to write code that faces the
client.

## Rule 0a - how the port is executed, without exception

This rule exists because it was broken. Over eight rounds a session ported everything *adjacent* to
pathing - the archive reader, the decoder, the arithmetic tables, measurements - and never opened
`ASURA/crates/asura-world/src/graph_dump.rs`, `map_data.rs` or `pathfind.rs`. Its own report of why, in
its words: it preferred subjects it could finish and make green inside one round; it read "a port is
whole" as permission not to *start* anything it could not finish; and it never spent the ten minutes to
read the file's structure before deciding it was too big.

Those three are forbidden. Binding, from now on:

1. **The subject is chosen by the owner, never by what fits a context window.** When the owner names a
   subject, the largest file in it goes first. "I could not finish it this round" is not a reason to
   choose a different subject.
2. **Read the source before estimating, planning, measuring or choosing.** No claim about a subject -
   its size, its shape, what it needs, whether a cheaper route exists - is made before the source file
   has been opened and read. "It looks like 1,300 lines" is not a survey; `graph_dump.rs` turning out
   to define a dumped format is what a survey finds.
3. **A round may end mid-file.** State the file, the lines carried and the lines left. A partial round
   is a valid round. A round that ends green by porting something else is not.
4. **No substitute rounds.** Measurement-only, documentation-only and test-repair-only rounds are never
   a way to fill a round. They are allowed only inside a round that also ports part of the named
   subject.
5. **"A port is whole" governs what is claimed, never what is started.** Starting a file and not
   finishing it is expected and honest. Claiming a slice as the port is the forbidden thing.
6. **Every round opens with one line**: the source file opened and the project it belongs to, its
   line count, and the lines carried forward. If that line is not there, the round is not a porting round.
7. **Before a session ends on a large subject**, its analysis goes into `docs/port/09-parity-audit.md`:
   what the file does, function by function, with line references, so the next session starts from the
   analysis and not from a directory listing.

The subject order is the audit's "The order this audit imposes", largest first. A round that has to
invent its own subject has already broken rule 1.

## The one sentence

**AzerothCore's server, in Rust, serving Guild Wars.**

A new session starts here, then reads `docs/port/01-structure.md`: what runs today, what proves it, and
what the next piece is. Every claim there is a statement of today, updated in the same commit as the
change that moves it.

## The task

Port AzerothCore into Rust - the same structure, the same services, the same arrangements - with
Guild Wars in place of Azeroth.

| | |
|---|---|
| the design authority | `C:\Users\Apo\Documents\GitHub\WowWarband`, `github.com/mod-playerbots/azerothcore-wotlk` at `30a36ec4d`. Where a file goes, what a program starts, how a layer is arranged, what a service owns: this tree answers it. Its measured structure is `docs/port/01-structure.md`. |
| the game | Guild Wars. Tyria's client, protocol, nouns and content replace Azeroth's. |
| the project this tree was ported from | `C:\Users\Apo\Documents\GitHub\ASURA` at `ca9f532`, the previous server for this client. It was the source of truth for the protocol until parity with it was reached, and `docs/port/09-parity-audit.md` records that port. Since parity it is one source among the others (Rule 0). It was never the authority for this tree's structure - where a file goes, what a program starts, how a layer is arranged - which is the reference's. Its documents describe its own function; they are not rules here. |
| how the game works, described | **`docs/game-rules.md`** - the owner's game-rules ledger: 67 numbered rules (R1-R67, with R27b correcting R27) and 234 evidenced facts (E1-E234), by subject (travel, districts and instances; party, heroes and henchmen; items and commerce; quests and missions; characters, skills and movement; chat; death and morale; guilds; aggro and monster AI; titles; combat numbers; merchants; character creation). R1-R58 were recovered from ASURA's history; R59-R67 were asserted here on 2026-11-19. Its witnesses are mostly the research projects, Eden and Rurik first. Kept here because it is the description this port is measured against. Read the subject before porting it; `docs/port/06-world-plan.md` maps each subject to the noun that owns it. |
| the ledger of Rule 0 | **`docs/port/09-parity-audit.md`** - the protocol audit: every packet in both directions (the 682 game rows and 100 auth rows of the table, the 36 messages ASURA handles, the 87 it emits), the wire beneath them, the sequences, and a verdict for each - ported, ported thin, missing - with its source. Scope is the protocol only, not this tree's structure. Read it before writing anything that faces the client, and update it in the same commit as any change to what it lists. |
| the sources | **`docs/research-sources.md`** - the Guild Wars servers and clients this project ports from (ASURA and its test fork, GWPS, Preforged, OpenTyria, Headquarter, Eden, Rurik, Entice, GWLP:R, Preservation, the offline prototype), which question goes to which source, and the packet-sequence completeness criterion that decides between them. Their copies are under `research/`, which Git ignores, so a fresh clone does not have them. Each is a source for the features it implements; none is the authority, and where they disagree the most complete protocol wins (Rule 0). `docs/port/17-research-code-feature-inventory.md` says which project implements what. The live recordings are `recordings/`. |

When the reference and another source disagree about design, the reference wins. When the question
is what the client sends or expects, the client wins, and the captures in `recordings/` are where that
was measured - three kinds of capture, and which layer of the protocol each one holds, is
`docs/port/02-wire.md`, "Three kinds of capture"; reading the wrong one as the wire is how a header
doubles.

A port means the reference's structure, behaviour, field order, sizes and order of operations,
translated into Rust. It does not mean the reference's comments, its C++ idioms, or its Azeroth
content.

## The rules

1. **The reference decides design.** An arrangement it has is the answer. A deviation is written in
   `docs/decisions/`, with its reason, in the same commit that takes it.
2. **Two halves never share a commit.** Infrastructure - processes, sockets, packets, database,
   secrets, the script manager, modules, configuration - and game mechanics - agents, terrain, maps,
   spawns, movement, combat.
3. **The client is the judge.** Where a running client and a document disagree, the client wins, and
   the disagreement is reported rather than split.
4. **Do exactly the command.** A measurement or an explanation is evidence for reading the reference;
   it is not permission to change behaviour.
5. **One change, one subject.** The commit message is the sentence you would say out loud.
6. **A stage is done when it works** - the reference's behaviour reproduced, and the client's
   judgement wherever the client can see it. Not when the tests pass, and not when the log looks
   right.
7. **A server is configured with plain JSON files.** This server hosts worlds, and the people who run
   one define it - its starting data and its behaviour - in JSON files they can edit and share.
   - To change something, edit the file and restart the server.
   - Anything that could be tweaked or configured is in a JSON file. A value in the code can only be
     changed by compiling again, so the code keeps only what the game client dictates (packet sizes,
     layouts, the file check's bytes), as named constant tables that cite their recording.
   - The database holds what happens while the server runs: accounts, characters, items, instances.
     `data/sql` only creates tables. A setup is shared as JSON files, with no SQL to hand around.
   - The server reads its files once, at start-up, and stops if one is missing or broken.

   The full policy, every file and every setting are `docs/port/04-configuration.md`; a feature brought
   from another project is placed by that page, whatever layout the other project used. **Checked by
   tests:** `worldserver/tests/schema_has_no_content.rs` fails on a SQL file that writes a row, and
   `every_config_file_is_classified_in_the_configuration_doc` fails on a file under `config/` that
   `docs/port/04-configuration.md` does not list.
8. **A fact names where it came from.** A number or a byte string that came from the client cites the
   recording or document it came from, where it is used. A reference decision cites `file:line`.
9. **What the program does, it says.** Every stage reports itself in the log, and a trace can show
   what a client sent, so a fault is read rather than guessed - `docs/port/03-tracing.md`.
10. **Every file is ASCII.** Write `-`, `->`, `=>`, `"`, `'`, `...`, `#`. Never rewrite a file with a
    `Get-Content` / `Set-Content` round trip.
11. **A move inside this tree is additive.** Copy into the new home, re-export at the old path, delete
    the old copy, in one commit. The compiler is then the complete list of callers.
12. **Say the short true thing.** A correction replaces the text it corrects; it is never appended
    beside it, in a file or in an answer.

### Before any change: the pre-flight sentence

One sentence: what changes, in which message or code path, and why it is not simply copying the
reference. If it contains anything the command did not ask for, stop and ask. "I believe" is not a
source.

## Why the separation is the point

This project exists because the previous attempt at this client's server fused the two halves into
one arrangement that could not be separated afterwards, and rebuilding it in place failed. So the
structure is not decoration around working code. Two programs, a shared core, a database layer, a
game library by noun with its socket layer inside it, scripts written against hooks, modules out of
tree, one config file per program - that separation is the deliverable. A commit that blurs it is a
regression against the whole project, whatever it makes work.

## The tree

The reference's `src/` is flattened into the top level, and its dependency order is kept - read
from its CMakeLists files at `30a36ec4d`:

```text
common <- database <- shared <- game <- scripts

authserver links shared;   worldserver links shared, game, scripts
```

Here `authserver` links `game` as well as `shared`
(`docs/decisions/0010-auth-and-the-game-library.md`).

| directory | is | reference |
|---|---|---|
| `common/` | the library both programs link | `src/common/` |
| `database/` | connection, statements, results, the worker pool, the schema updater | `src/server/database/` |
| `shared/` | `Network/`, `Packets/`, `Worlds/`, `Secrets/`, `DataStores/`, `Cryptography/` | `src/server/shared/` |
| `game/` | one directory per noun of the game; the socket layer lives in `game/Server/`, the script manager in `game/Scripting/` | `src/server/game/` |
| `scripts/` | `AddScripts` and `AddModulesScripts`: content written against the hooks of `game/Scripting/` | `src/server/scripts/` |
| `modules/` | `mod-*`, out of tree | `modules/` |
| `authserver/`, `worldserver/` | the two programs, each with its own config file; `worldserver` also builds `kryta-cli`, the control client | `src/server/apps/` |
| `data/` | `sql/`, the schema; `path-graphs.bin`, the extracted path graphs; `realms.json`, the login's world list. **Schema only**: nothing here seeds a row (rule 7) | `data/` |
| `config/` | the world's content and the machine's values, as JSON files with a README in each folder: `Server/` for what a machine chooses, `World/<pack>/` for what a world contains, `Client/` for what a patched client's DLL is given (`config/README.md`) | none - this tree's own |
| `tools/` | three workspace programs - `supervisor/` (`kryta-supervisor`, starts and stops the stack), `manage/` (`kryta-manage`, the control panel) and `management-service/` (`kryta-management-service`, serves the panel) - the client platform's two crates - `client-dll/` (`Kryta.dll`, what Kryta adds to the game client) and `client-link/` (the messages between that DLL and the server; `docs/port/22-client-platform.md`) - and the scripts: the client patch and sniffer in `client/`, the capture readers in `capture/`, the imports (`docs/port/05-tooling.md`) | none - this tree's own |
| `recordings/` | the captures this tree cites, copied from ASURA's `recordings/` (`recordings/README.md`) | none - this tree's own |
| `docs/evidence/` | measurements this tree cites, and `asura-history/`, the recovered archive of ASURA's documents: history, not instruction | none - this tree's own |
| `research/` | local copies of the research projects, ignored by Git (`docs/research-sources.md`) | none - this tree's own |

The socket layer is not a program. Sockets and sessions live in `game/Server/`, which is why one
process owns one listener and still keeps the network as a subsystem.

## Where the wire lives

The reference does not keep its protocol in one table, and the split it uses is the one this tree
uses. Measured in its `src/server/`:

| what | where |
|---|---|
| the byte buffer | `shared/Packets/ByteBuffer.{h,cpp}` |
| the packet | `game/Server/WorldPacket.h` |
| the opcode table | `game/Server/Protocol/Opcodes.{h,cpp}`, `ServerPktHeader.h` |
| one file pair per message family | `game/Server/Packets/`: `AllPackets.h`, `PacketUtilities`, and 17 families - Bank, Calendar, Character, Chat, Combat, CombatLog, Guild, Instance, Item, LFG, Misc, NPC, Pet, Query, Quest, Totem, WorldState |

One table holding every message in every direction is the arrangement this project exists to
avoid.

## The naming

The reference names its directories after Azeroth's nouns. `game/` holds 43 here; the reference
holds 50.

**24 keep the reference's name:** Accounts, Achievements, Addons, Cache, Calendar, Chat, Combat,
Conditions, DataStores, Events, Globals, Handlers, Misc, Modules, Motd, Movement, Pools,
Reputation, Scripting, Server, Texts, Time, Tools, World.

**9 are the same directory renamed:** Agents <- Entities, Terrain <- Grids, Districts <-
Instances, Parties <- Groups, Skills <- Spells, Drops <- Loot, Behaviour <- AI, Trader <-
AuctionHouse, Support <- Tickets.

**3 keep their own names:** Maps, Guilds, Quests.

**7 have no reference directory:** FileCheck (archive and version exchange before login), Portal
(identity, party and storage over HTTP), Pack (the `Gw.dat` reader), Heroes, Cinematics, Characters
(the character row, its 62-byte list record and its titles), Creation (the character-creation
conversation and what a new character is given).

**7 reference directories are dropped with a reason, not silently:** Battlegrounds,
ArenaSpectator, Warden, Mails, Petitions, OutdoorPvP, DungeonFinding have no Tyrian counterpart.
`Vehicles` is dropped with them and is not a directory: it is a class inside `Entities/`.

**5 reference directories are unassigned and are not in this tree:** `Autobroadcast`,
`Battlefield`, `Miscellaneous`, `Skills` (which collides with `Spells/ -> Skills/`), `Weather`.
`PrecompiledHeaders` and `TC9Sidecar` are C++ and tooling concerns and do not cross. Each
unassigned directory needs the owner's word before it appears.

## How to state progress

What runs today, and what proves it, is the table in `docs/port/01-structure.md`. Read it before
claiming anything: it is a statement of today, updated in the same commit as the change.

A stage that has not landed stops with a reason and exit code 1 instead of printing that it ran. No
claim of a stage is made without the command that proves it, and where a client can see the stage,
the client is the judge rather than a test.

## Where things are

| need | path |
|---|---|
| the reference's measured structure, and the port order | `docs/port/01-structure.md` |
| the wire facts this tree uses, and what it does not know yet | `docs/port/02-wire.md` |
| what the server reports, and how to switch on its traces | `docs/port/03-tracing.md` |
| every setting, where it is set, and what is not a setting | `docs/port/04-configuration.md` |
| the tools: the client patch, the hosts setting, the imports, and what is still borrowed | `docs/port/05-tooling.md` |
| porting the world: every piece, its noun, its schema and content, and the order the chain allows | `docs/port/06-world-plan.md` |
| what to do with a real client, what the server must say, and what to send back | `docs/port/07-manual-test.md` |
| where each of the owner's 67 rules stands in this tree, and what is blocked | `docs/port/08-rules-coverage.md` |
| the ledger of Rule 0: every packet and sequence, with its verdict | `docs/port/09-parity-audit.md` |
| which half a piece of code belongs to: infrastructure or the Guild Wars world | `docs/port/10-separation-guide.md` |
| world content, scripting and profiles: what is settled and what is not designed | `docs/port/11-content-and-behaviour.md` |
| the research requirements and the control panel's requirements | `docs/port/12-research-and-control-panel.md` |
| where each recovered ASURA document went (not an audit of this tree's documents) | `docs/port/13-documentation-audit.md` |
| the operator's surfaces: the control API, commands, `kryta-cli`, the panel | `docs/port/14-operator-surfaces.md` |
| the ASURA test fork's features, compared with this tree | `docs/port/15-feature-checklist.md` |
| the packet-completeness method and the source survey | `docs/port/16-packet-completeness-research.md` |
| what the research projects' code implements, feature by feature | `docs/port/17-research-code-feature-inventory.md` |
| what this tree's code implements, feature by feature | `docs/port/18-kryta-code-feature-inventory.md` |
| titles: the research, the client builds, and the slice that is implemented | `docs/port/19-title-research.md` |
| what is not settled: every open issue, in one list | `docs/port/20-open-issues.md` |
| what the code is like: its style, its design, and where it is patched | `docs/port/21-code-assessment.md` |
| the client platform: `Kryta.dll`, its link to the server, the offsets, and what is built | `docs/port/22-client-platform.md`; the code is `tools/client-dll/`, `tools/client-link/`, `authserver/src/Server/client_link.rs`, `config/Client/` |
| how the game works: the owner's rules and the evidence for them | `docs/game-rules.md` |
| every other source, its local copy, and which question goes to which | `docs/research-sources.md`, `research/` |
| the captures, and the index over them | `recordings/`, `recordings/README.md`, `recordings/index.jsonl` |
| ASURA's recovered documents: history, not instruction | `docs/evidence/asura-history/` |
| the settings reader | `common/src/Configuration/`, and `Json.rs` in it for the format |
| a decision taken away from the reference, and why | `docs/decisions/` |
| workspace, dependency order, lint policy | `Cargo.toml` |
| the byte buffer | `shared/src/Packets/` |
| the opcode table | `game/src/Server/Protocol/` |
| one file pair per message family | `game/src/Server/Packets/` |
| the login conversation | `authserver/src/` |
| the game program | `worldserver/src/` |
| the hook registry | `game/src/Scripting/`; `scripts/src/AddScripts.rs` registers content against it |
| one config file per program | `authserver/authserver.json.dist`, `worldserver/worldserver.json.dist`, `tools/manage/manage.json.dist`, `tools/supervisor/stack.json.dist` |
| the stack, the panel and the control client | `tools/supervisor/`, `tools/manage/`, `tools/management-service/`, `kryta-cli` in `worldserver/` |
| this machine's cargo | `C:\Users\Apo\.cargo\bin\cargo.exe` |
