# World content, scripting and profiles

The owner selected the following direction in ASURA on 2026-11-01 and 2026-11-02. This is a design
for future work, not a description of implemented Kryta features. Recovered source: ASURA
`docs/guidance/scripting-and-behavior.md`, `docs/guidance/game-packs.md`, and
`docs/reference/decisions/0014-behavior-tree-execution.md`, under `docs/evidence/asura-history/`.
Kryta's structure is governed by `AGENTS.md` and `docs/port/01-structure.md`; client-visible
behaviour is governed by Rule 0 and the rules and evidence in `docs/game-rules.md`.

## Rust mechanics and Lua behaviour

The owner chose Lua for external game behaviour and Rust for the server's mechanics. This distinction
covers the whole simulation. The script describes conditions, decisions, sequences, reactions and
what action to request. Rust owns authoritative state and carries out supported operations. A new
behaviour using existing mechanics should be authorable without rebuilding the server; a new mechanic
requires engine support. Script APIs, event names, packaging, reload semantics and the Lua integration
remain to be designed.

World content is JSON under `config/`, read on every start; nothing imports it into rows and nothing
seeds a database (`AGENTS.md`, rule 7). Database rows hold what a server writes while it runs: player
state, open instances, unlocks. Lua authors world behaviour and calls Rust operations. Rust owns
authoritative state and executes those operations. A GM command mapped in JSON to a Rust API function
calls that function directly; Lua is not a required intermediary. Lua can also call the same operation
when the runtime and bindings exist. The shared operator API, CLI and manager already use one operation
registry; their current scope is in `docs/port/14-operator-surfaces.md`.

| Subject | Behaviour authored in content | Mechanics supplied by the server |
|---|---|---|
| NPCs and AI | Priorities, targets, tactics and reactions | World queries, behaviour execution and actor actions |
| Movement | Patrol, chase, follow and retreat decisions | Pathfinding, collision, movement and position updates |
| Combat and skills | Use conditions, targets, timings, effects and combinations | Resource accounting, casting, projectile, effect, damage and death operations |
| Quests and missions | Prerequisites, objectives, branches, progression and rewards | State, event delivery and supported actions |
| Spawns and encounters | Population, conditions, waves and progression | Creation, placement, lifecycle and state |
| Dialogue and interactions | Choices, conditions and consequences | Interaction delivery, client messages and state changes |
| Maps and instances | Entry reactions, encounters and scripted transitions | Map loading, instance lifecycle, placement, travel and geometry |

These examples describe intended ownership, not current APIs. Measured client sequences and the
rules ledger remain the basis for reproducing base-game behaviour. Content can only request actions
that the engine supports. The proposal to use event-driven handlers is recorded as a proposal; event
ordering, scheduling, retries and failure semantics are open.

## AzerothCore Lua and commands as references

The owner wants Lua to be the primary authoring surface for world behaviour and rules, and wants
in-game GM and user commands and interactive queries like the WoW `/run` quest-completion example
(2026-11-17). WoW client-side Lua and AzerothCore server-side Lua are distinct execution surfaces.
The example establishes the desired interactive outcome, not that Guild Wars supports WoW's client
API or the same command syntax.

The local WowWarband checkout provides these concrete references:

- `modules/mod-ale/` is the AzerothCore Lua Engine module, at its separately checked-out revision
  `84b85cc55980dbf67d596f3d5cb34ded10c59102`. Its `README.md` describes server Lua scripts and
  `docs/USAGE.md` demonstrates player and creature event registrations. `conf/mod_ale.conf.dist`
  describes script paths and reload settings. These are reference capabilities, not Kryta features.
- `modules/mod-ale/src/LuaEngine/hooks/PlayerHooks.cpp:42` implements a command hook;
  `src/LuaEngine/Hooks.h:209` describes the Lua command event and its return value. This is a
  hook for handling commands, not evidence that arbitrary WoW `/run` input runs on the server.
- `src/server/game/Chat/Chat.cpp:58` checks permissions, `:236` dispatches commands and `:168`
  sends system messages. `src/server/scripts/Commands/cs_quest.cpp:30` registers quest commands,
  including `status`, and `:751` implements the status query. These are examples of server command
  organization and access control, separate from the WoW client Lua example.

## Chat is the server's command line, and its text now reaches the client

Chat carries player conversation and server commands. Command names and aliases are content:
`config/flavors/Vanilla/Server/commands.json` maps a name to an operation, and the current reader resolves that operation through
the shared registry (`game/src/Chat/Commands.rs`, `game/src/Chat/Operations.rs`). A mapped GM command
calls the registered Rust function; Lua is not required for that call
(`docs/decisions/0011-the-command-surface-is-content.md`).

- **The command mapping is content.** Names and arguments are rows; the travel target alias table
  and `/maps` text remain in Rust. Any relocation must preserve the client-visible behaviour already ported.
- **The text is sent** (`docs/decisions/0012-chat-text-is-sent.md`). A line is one or more `0x005D`
  bodies carrying a coded string, then exactly one commit tag - `0x0061 [u16 player, u8 channel]` for a
  player's line, `0x005E [u16 player, u8 channel]` for one the server composed. Answers go out on
  `Channel_Global` as GWPS's own invalid-command path does (`GmChat.c:116-143`), the player's
  line is broadcast to the map and district, and both were measured on a live client before they were
  written here (`research/Rurik/studies/chat/FINDINGS.md:255-268`).

The owner typed `/maps` against a real client and read the answer in chat, and later typed a line in chat
and read his own message rendered - both halves of the text path, on a live client. A quest-status
command still needs quest progress and an exposed query; a Lua runtime alone does not supply them.

## The scripting contract (plan step 1)

The plan below opens with the contract, because everything after it is cheaper to build than to change.
This is that contract, compared against the recovered ASURA plans and the AzerothCore Lua Engine
(`modules/mod-ale`, at its separately checked-out revision `84b85cc`). Eight clauses, each sourced.

1. **The boundary is the three layers.** A script holds facts (JSON), composes rules (Lua) and calls
   operations (Rust). It never holds authoritative state and never builds a packet: every effect is a
   validated operation call, so a script cannot express something the engine does not already do.
2. **The engine is a script, not a special path.** The reference's Lua engine subclasses its
   world-script interface and answers the same moments as any other script object
   (`LuaEngine.h:371` `OnWorldUpdate`, `:591` `OnStartup`). Ours does the same through
   `game/src/Scripting/`'s `WorldScript`: it is registered on the manager, numbered, and called only for
   the hooks it declared. The manager gains no Lua-shaped branch.
3. **Scripts are files in a directory, named by a setting.** The reference names it `ALE.ScriptPath`,
   defaults to `lua_scripts`, and accepts a relative or absolute path (`conf/mod_ale.conf.dist`). Every
   `*.lua` under it loads in name order, and a file that fails to compile is reported with its path and
   line while the rest still load.
4. **Registration is the reference's own shape.** `Register<Scope>Event(event_number, function)`, with
   the reference's numbers and scopes - `SERVER_EVENT_*` 1-7, `WORLD_EVENT_*` 8-16, `MAP_EVENT_*` 17 and
   up (`Hooks.h:110-124`), and its function names (`LuaFunctions.cpp:69-84`). This tree raises only the
   three world moments it has: `WORLD_EVENT_ON_UPDATE` 13, `ON_STARTUP` 14, `ON_SHUTDOWN` 15. A number
   this tree does not raise is refused at registration with a reason, not stored and silently never
   fired - an event that can be registered but never called is worse than one that cannot be registered.
5. **A script-defined command is a separate design subject.** The reference offers
   `PLAYER_EVENT_ON_COMMAND` (42), with the reference contract: `(event, player, command,
   chatHandler)` and a `false` return cancels the command (`Hooks.h:209`, `PlayerHooks.cpp:42`). The
   integration of that hook remains open. JSON mappings to Rust API functions do not require it.
6. **Failure is reported and contained.** A handler that errors is reported with its event, script and
   line, and the remaining handlers for that event still run; an operation that fails returns an error
   the script may inspect. Nothing is rolled back because nothing partial exists: an operation either
   validates and acts or refuses. The limits that make this bounded - an instruction budget, a memory
   ceiling, no `io`/`os`/`debug`/`package`, and no bytecode loading - are the policy ASURA recorded as
   still to be defined (`docs/evidence/asura-history/docs/guidance/scripting-and-behavior.md:19-22`),
   and they arrive with the runtime rather than being promised here.
7. **Ownership is by id, never by reference.** A script addresses a player, an agent or a map by the id
   the engine gave it, and every operation re-validates that id. A stale or foreign id is an error
   result, not a crash and not a silent no-op.
8. **The tree runs in Rust and is written in Lua.** Rust owns the behaviour-tree runtime: node
   execution, ticking, per-agent running state, halting and reset (ADR 0014's checkpoint). A profile's
   Lua content defines the trees and their game-specific leaf decisions. "HeroAI does not tick" means
   exactly that: the tree is handled in Rust and defined in Lua, and Rust drives the tick that calls the
   Lua content it holds. What a script may not do is own a clock - hold an awaiting coroutine, a timer
   or a half-finished sequence across ticks. So the two statements that were once in tension are both
   true under a call contract: Lua declares the tree and decides the leaves it is asked about, while the
   ticking and the per-agent state are Rust's.

**What step 2 inherits from this**, and must not re-open: the runtime is selected there (lua54, statically
linked, is what this machine builds - verified by a probe that was reverted, `mlua` with `lua54` and
`vendored` compiled against MSVC here), as are placement, load order, reload, logging and the sandbox
limits of clause 6.

## Lua backend and chat commands: implementation plan

This is a plan for a future implementation, not authorization to implement it. The owner wants Lua
as the primary authoring surface for world behaviour and rules, and server-side administration via
in-game chat: for example, state queries, teleportation, NPC and item spawning, and GM tasks. The
Guild Wars client does not execute Lua. It sends chat; the server recognizes a command, executes the
server-side handler and returns a result. **Both forms are now settled by measurement**: the input is
one sigil unit then the text (`!` all, `#` group, `$` trade, `/` a command, `GmChat.c:193-207`), and the
output is one or more `0x005D` coded bodies followed by one commit tag, which a live client rendered
(`docs/decisions/0012-chat-text-is-sent.md`). What is **not** settled, and has no source anywhere, is
the sender's permissions: neither ASURA nor `GmChat.c` checks an authority before answering a command,
and this tree's account rows carry no access column.

Rust owns authoritative state and provides validated services; Lua defines behaviour and calls
available services. A JSON-mapped GM command calls its registered Rust API function directly.
Script-defined commands and Lua handlers remain a separate open design. Neither a Lua runtime nor a command handler
supplies game mechanics that have not been implemented. Preserve the client-facing behaviour already ported,
and follow Rule 0, the parity audit and the recordings when developing client-visible paths.

1. **Scripting contract.** Done: the section above. It settles the boundary, organisation, registration,
   events, failure, ownership and the behaviour-tree question, and it names what it deliberately leaves
   to step 2.
2. **Runtime integration.** Next. Design engine placement against AzerothCore's structure and Kryta's
   existing script manager. Specify loading, configuration, logging, startup, shutdown, reload and the
   handling of failed or long-running scripts. Select a runtime as part of that design - lua54 linked
   statically is the working candidate, since it is what this machine builds.
3. **Server API.** Inventory queries, events and actions by player, NPC, map, instance, inventory,
   quest and administration. Define validation, state ownership and error results. Mark operations
   dependent on absent mechanics as future work rather than presenting them as available.
4. **Chat commands.** Done for the wire: the input and output forms are measured, ported and named
   above, and the command surface's home is decided (`0011`). What remains is the half this plan owns -
   registration, parsing, feedback and audit logging for a command a script defines - designed against
   AzerothCore's command system and ALE's `PLAYER_EVENT_ON_COMMAND` hook
   (`modules/mod-ale/src/LuaEngine/hooks/PlayerHooks.cpp:42`, `Hooks.h:209`). Permissions stay open and
   are the one part with no counterpart to port (see above).
5. **First end-to-end slice.** Select a Lua-defined command that reads an existing server fact and
   returns it. The chat half of it now exists: `answer_line` sends a composed line the client renders.
   The slice therefore exercises loading, chat input, execution and client-visible output, and it is
   blocked only on the runtime and on the fact being exposed - not on the wire.
6. **Incremental expansion.** Add events and APIs alongside their underlying mechanics. Teleport,
   NPC spawn and item creation require supported server services and evidenced client-facing
   behaviour; world scripts and administrative commands can use those services.
7. **Validation.** Check loading, errors, dispatch, permissions and command results; compare
   client-facing frames with their source and the recordings; verify complete workflows with a real client.
   Passing tests alone does not establish client acceptance.

Before implementation, decide whether the interface permits arbitrary interactive server-side Lua
or only named commands; roles and permission administration; reload and running-state semantics;
script trust, resource access and execution limits; and the first existing query to demonstrate.
Named Lua-backed commands first are a recommendation, not an owner decision. The exact API, command
syntax and interactive execution surface remain open.

## HeroAI and behaviour trees

The owner chose BehaviorTree.CPP as the semantic reference for the Rust execution engine and Py4GW
Reforged HeroAI as a source for game decisions. The selected profile supplies skill behaviour and
HeroAI's rules for using skills in Lua. Rust owns node execution, per-agent running state, ticking,
halt/reset and access to authoritative queries and actions; the profile's Lua content declares the trees
and decides the game-specific leaves, and Rust calls it (clause 8 above). What is still open is the
mechanical part rather than the boundary: the shape of a Lua tree declaration, and the callback timing
between a Rust tick and the Lua leaf it calls.

A future acceptance case copies a base profile, adds a custom skill using existing mechanics, and
teaches HeroAI to use it through that profile's Lua content without changing or rebuilding Rust.
The original profile remains unchanged. The skill's behaviour and AI usage are separate content.

The source comparison in `docs/evidence/asura-history/docs/reference/decisions/0014-behavior-tree-execution.md`
records material differences between Py4GW nodes and BehaviorTree.CPP. A migration must compare
running, interruption, reset and completion semantics for each used branch. Its closest mappings
include sequence to Sequence and selector to Fallback; reactive variants can change reevaluation and
cancellation. Parallel completion, delayed action completion, repeat, running conditions and halting
also need explicit examination. A matching node name alone does not establish parity.

`docs/evidence/asura-history/docs/research/heroai-server-port.md` records the source-only analysis at
Py4GW Reforged commit `a5fd2e54d504605200a752b1bc991d618ed33c41`, including combat selection,
target scans, skill preparation, observations and interruption. The owner's sizing example was 100
instances with 200 AI-controlled agents each. No benchmark in that review establishes capacity.

## Profiles and isolated experiences

One selected, self-contained profile supplies the game content for a server launch. It does not
inherit missing content from another profile. A profile containing three quests has three quests;
a copy of the base profile with three added has the copied quests plus those additions. The copy is
independent. Engine resources may be shared where explicitly supported.

Each profile is a separate experience with its own account settings, characters, levels, inventory,
quest history, unlocks and other progress. Returning to a profile resumes its own saved experience.
The selected profile owns its Lua behaviour and HeroAI skill decisions. Copying distributable content
must not copy credentials, active sessions or private player progress.

The owner selected logical isolation; database versus schema versus profile-scoped rows remains open.
Profile identification, installation and distribution, engine compatibility, selection input and
launcher integration also remain to be designed. The presence of JSON configuration or database rows
in Kryta does not establish a profile loader or isolated saved state. A future launcher can supply the
selection, but the support need not depend on building that launcher first.

## World-content storage

ASURA's historical D20 records the owner's 2026-11-03 direction to move world configuration and
content into files and keep operational state in the database. D24 describes selecting one world
pack. The relevant source is `docs/evidence/asura-history/docs/reference/decisions/layer-decisions.md`
and earlier versions of `docs/38-layer-decisions.md` in ASURA history. D39 describes ASURA's then
chosen map files and destination-owned arrivals.

Kryta adopted that direction in `docs/decisions/0014-the-world-content-is-a-pack.md`, which
supersedes `0009-world-schema.md`: a world's map content is a pack of JSON files under
`config/flavors/<flavor>/World/`, read at start-up by `game/src/Maps/Pack.rs`, and the pack the world serves is
named by its `WorldPack` setting. This tree has one pack, `config/flavors/Vanilla/World/`. The world database's
content tables are empty by design (`data/sql/base/db_world/0013-content-is-the-pack.sql`). One pack
per launch is not yet a profile: profile selection, isolation and distribution remain undesigned
(above).

## Content authoring

The recorded design distinguishes agent definitions, placement and facing, groups, movement
assignments, spawn conditions, lifetime and quest relationships. A quest may involve collecting,
fighting, reaching a location, escorting, following and prerequisite progress. The scripting
boundary supports combining available operations into such sequences. Which ordinary objectives are
data, Lua or both remains open. Phasing and layering need explicit population and state ownership;
they are not supplied automatically by a scripting language.

The older ASURA plans describe a map/spawn/portal manager and later content editors. Kryta's existing
administration tool is described in `docs/port/05-tooling.md`; its pages do not establish a complete
profile or quest editor. Keep operator settings, distributable content and player state distinct when
those interfaces are designed.

## Remaining design questions

- Which game decisions, including existing placement rules, are profile-authored and which remain in
  Rust under the current packet-parity and AzerothCore structure requirements?
- How are scripts registered, scheduled, interrupted and bounded, and what happens to state when a
  script fails or content changes?
- What events and world operations can scripts use, and what ordering and transactional guarantees
  do they have?
- What decides who may run a command? Neither ASURA nor GWPS checks an authority and this tree's accounts carry
  no access column, so the permission model and any schema remain open; AzerothCore's `account_access` is a reference,
  not an adopted design.
- How do profile selection, storage isolation, distribution and compatibility work in Kryta?
- How do quest state, party state, phasing, instance state and spawn selection interact? See the
  unresolved world-model questions in `docs/game-rules.md`.

## Status and evidence

`game/src/Scripting/` owns the script manager. Three world lifecycle hooks are wired, and their
current limits and proof are in `docs/port/10-separation-guide.md`. Chat works: the input rule, the three
text frames, the answer path and the broadcast are in, its frames come from measurement rather than
ASURA's table (`docs/decisions/0012-chat-text-is-sent.md`), and both halves were read on the owner's live
client - the server's answers and a player's own line. The command rows and their arguments are in
`config/flavors/Vanilla/Server/commands.json`, and every row names a registered operation
(`game/src/Chat/Operations.rs`), so the chat line, the console and the control endpoint are one
execution path; the alias table and the `/maps` text are still Rust, which is where a rule belongs until
this document's scripting layer takes it. A Lua runtime, content loading, world APIs, a command
registration a script can reach and a module crate remain future work. No conclusion about client
acceptance follows from this design: a live client is the only judge, and the check is named in 0012.
`docs/port/06-world-plan.md` maps world subjects to their nouns.
