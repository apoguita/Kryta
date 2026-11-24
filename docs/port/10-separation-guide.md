# Separation of server infrastructure and the Guild Wars world

This guide records the code inspected on 2026-11-14. It identifies the boundaries already in place,
where responsibilities overlap, and what must be examined to complete the intended separation.
It does not establish a new design or change client behaviour. The design authority is the local
AzerothCore checkout at `30a36ec4d`; client-facing behaviour is ported from a source under Rule 0, and
what is here today was ported from ASURA at `ca9f532`.
`docs/port/01-structure.md` records what runs, and `docs/port/09-parity-audit.md` records protocol
parity. Each proposed change needs its own comparison with those authorities.

## Current boundaries

| Boundary | Status | Evidence |
|---|---|---|
| Infrastructure and game | Established at crate level | `Cargo.toml` and each crate's `Cargo.toml`: `common`, `database`, and `shared` do not depend on `game`; `game` depends on them. |
| Login and gameplay processes | Established, with code coupling | `authserver/src/main.rs` starts the auth listeners. `game/src/Server/WorldSocketMgr.rs` starts the gameplay listener for `worldserver`. Auth depends on `game` as described below. |
| Game mechanics and gameplay connection | Partial | `game/src/Agents/mod.rs`, `game/src/Terrain/`, and `game/src/Maps/Portals.rs` contain mechanics. `game/src/Server/WorldSession.rs` also owns gameplay decisions and state. |
| Game engine and scripts | Partial | `game/src/Scripting/` registers and dispatches three world lifecycle hooks; the loaders register no content and scripts have no world-action interface. |
| Modules | Not yet implemented | `modules/README.md` states that there are no module crates; `scripts/src/AddModulesScripts.rs` is empty. |
| Operator tools and the servers | Established at crate level | Three more workspace members live under `tools/`: `kryta-manage` (`tools/manage`) links `common`, `database`, `game` and `shared`; `kryta-management-service` (`tools/management-service`) links `kryta-manage`; `kryta-supervisor` (`tools/supervisor`) links `common`. No server crate depends on them. |

The AzerothCore reference deliberately places its gameplay socket inside the game library. A socket
in `game/Server/` is therefore consistent with the reference's structure. Process boundaries and
crate dependencies also do not establish that all responsibilities inside a crate are separated.

## 1. Auth depends on the game library

**What exists.** `authserver/Cargo.toml` includes `game`. The auth program hosts the identity portal
implemented in `game/src/Portal/`, as recorded in `docs/decisions/0003-portal-host.md`. The gameplay
front door reads a game version message and relays the connection to the world
(`authserver/src/Server/front_door.rs`; `docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`).
The world owns the gameplay session.

**Done (2026-11-14, the inventory and the decision).** `docs/decisions/0010-auth-and-the-game-library.md`
holds both. The edge is 38 `game::` uses in six files, and every one of them is one of six things: the
portal (`0003`'s subject, including the title catalog it serves, `authserver/src/main.rs:489`), the
client's 64-byte version message (a protocol message), the character's rows, the travel hand-off row, the account table (rows), and the pure rule that decides where a
character enters (`game::maps::placement`, `session.rs:1075-1111`). Measured against ASURA, that is
exactly its own layering: `crates/asura-auth` links `asura-protocol`, `asura-db`, `asura-store`,
`asura-rules` and `asura-config`, and links `asura-world` not at all - and its accepted decision D25 is
its own rule that a game rule a protocol answer needs belongs in a rules layer rather than in the world
library. The decision taken: the edge stays, because the reference's chain has no layer for four of the
six pieces, and moving the pure rule into `shared` would put a game rule in the library both programs
link - the blur `AGENTS.md` forbids.

**What that leaves: nothing open.** The one piece ASURA puts elsewhere is the pure rule, and by this
tree's authority order it stays here - `AGENTS.md` gives the reference the layers and ASURA the
conversation, the reference has no rules layer and keeps its repositories as nouns, so no `rules/` crate
and no store layer is created. D25 is the evidence that the login's conversation needs the rule, and the
reason ASURA needed a layer for it (its `asura-world` may not exist when its login decides) is not a
reason here, where the rule's nouns are in `game` whether or not a world has started. A real client still
holds the last word on the login and entry sequence. Hosting an HTTP listener (`0003`) and the socket
layer that classifies a connection (`0004`) are separate decisions and are not touched by this.

**Evidence of completion.** The dependency graph and actual imports agree with the resulting design,
the hand-off and login tests pass, and a real client verifies the login and entry sequence. Any
remaining deviation is described accurately in its decision record.

## 2. Gameplay session owns several mechanics

**What exists (2026-11-14, after the families moved).** `game/src/Server/WorldSession.rs` holds what a
connection is: the seed exchange, the version message, the travel row's claim, the head block, the read
loop, the outbox and the district it gives back. Seven handler files are in `game/src/Handlers/` (item, movement, travel, chat, character, the
three asks of a map load, and the title display), the frames of the load's answers are
`game/Agents/Player.rs`'s, and where a character enters a map is
`game/Agents/Entry.rs`'s. What is still the session's and should not be is the map's clock
(`step_map`, which calls `Agent::advance` and `Portals::crossed` and decides when to advance and what
to do with the result) - the third item's subject.
**Done (2026-11-14, item family).** The item messages are the first family moved, all four of them:
`answer_item_move`, `answer_equip_item`, `answer_drop_item` and `answer_unequip_item` are
`impl WorldSession` members in `game/src/Handlers/ItemHandler.rs`, and the
decision they used to hold - where an item would be worn - is `characters::equip_place` in
`game/src/Characters/Equipping.rs`, a pure function over the character's rows, the item templates and
the equipped bag, which prints nothing and reads nothing. `move_item`, `equipped_item_ids` and
`first_free_carrying_slot` were already the noun's. No frame, order, size or log line changed; the
end-to-end item journey (drag, equip, drop, unequip) passes untouched. The session exposes what a
handler needs (`say`, `send`, `characters()`, `bags()`, `item_templates()`, `equipped_bag()`,
`player_id()`) as `pub(crate)`, which is the surface every later family will use.

**Done (2026-11-14, movement family).** The four movement messages moved the same way:
`answer_move_to_coord`, `answer_position_update`, `answer_cancel_movement` and `answer_last_position` are
`impl WorldSession` members in `game/src/Handlers/MovementHandler.rs`, and the dispatch in the session is
one call each. Their decisions were already the nouns': `Agent::adopt_report` (ASURA's `AgentState`),
`begin_walk`/`begin_manual_walk`/`advance`/`cancel_movement`/`adopt_correction` in `Agents/`, the route
in `pathfind`/`directional`, and the wall in `Maps/Portals.rs`. What stays in the session is what is not a
message: the map's clock (`step_map`) and the transfer it can start (`cross_portal`), which the guide's
third and fourth items own. No frame, order or size changed; the end-to-end journey's walk, its doorway
crossing and its tick measurement all pass untouched.

**Done (2026-11-14, travel and chat families).** The five travel messages and the one chat message
moved as they stand, doc comments and ASURA citations intact: `cross_portal`, `answer_party_travel`,
`begin_transfer`, `answer_travel_init` and `answer_teleport_command` are `impl WorldSession` members in
`game/src/Handlers/TravelHandler.rs` (from `travel.rs:132-169`, `184-258`, `279-302`, `319-388`,
`401-534`), and `answer_chat_message` in `game/src/Handlers/ChatHandler.rs` (from `chat.rs:193-353`).
The decisions these carry - the doorway test, the travel row's claim, the landing point - stay in the
nouns they belong to: `Maps/Portals.rs`, `Maps/Travels.rs`, and `place_for_travel` in the session, which is
where the map's own placement rule is read. What the session now keeps from the travel path is the
hand-off itself, and the map's clock, which the third item owns. No frame, order, size, broadcast or
log line changed; the end-to-end arch journey and the login journey pass untouched.

**Done (2026-11-14, character family).** The creation screen's three asks and its confirm moved to
`game/src/Handlers/CharacterHandler.rs`: `answer_creation_request_player`,
`answer_creation_request_armors`, `answer_creation_change_prof` and `confirm_creation`, with the
`Creation` state they read and `creation_player_block` - the block a screen is drawn from - beside
them (from `ASURA/crates/asura-world/src/creation.rs:190-255`, `259-271`, `275-341`, `346-519`,
dispatched in that order by `dispatch.rs:88-103`). The confirm's decisions were already the nouns':
the row is `DatabaseCharacters` (`write_starting_items`, `unlock_map`), the starting set is
`crate::creation`, and where a character starts is the world's own first outpost. No frame, order, size
or log line changed; the creation journey (the block, a changed profession, the confirm's row and the
refusal of a taken name) passes untouched.

**Done (2026-11-14, where a character enters).** The rule that decides the point, plane and facing a
character enters a map with moved out of the connection into `game/src/Agents/Entry.rs`
(`entry_point` over the map, its spawn points, a crossing's landing, a saved position and the world's
scatter), which is where ASURA keeps it - `Standing::at_spawn` (`agent.rs:460-476`), `arrival` and
`resume` (`:478-545`) - and the text is the same text, moved. What the connection keeps is what only it
knows: which landing its claimed travel stated, where the character was saved, and the world's scatter
setting. Five tests hold the rule itself now (a scattered point of an outpost's own list, a crossing's
landing winning, a landing for another map not applying, a saved position resumed, a map with no point
of its own having no entry), and the four end-to-end journeys - creation, the second outpost, the
resume-on-reconnect and the arch crossing to Lakeside County - pass untouched.
There is already useful extraction: agent state and movement operations are in `game/src/Agents/`,
pathfinding in `game/src/Terrain/`, portal geometry in `game/src/Maps/`, and character item persistence
in `game/src/Characters/`. The presence of these modules does not make every session operation a
separate mechanic.

**Reference.** AzerothCore separates message-family handlers under `src/server/game/Handlers/`.
For example, `ItemHandler.cpp:155-265` handles the request and calls player operations including
`CanEquipItem` and `EquipItem`. Its `WorldSession` still has responsibilities, so individual
operations need comparison before assigning them elsewhere.

**Work to resolve (remaining families).** Trace each implemented session path from packet through state
change and reply.
For each subject, compare the matching AzerothCore handler and game objects for ownership, then the
ASURA handler for the complete Guild Wars behaviour and message sequence. Place the resulting
responsibilities in the reference's corresponding layers. Preserve all branches, order, persistence,
error handling and frames. Moving a function to another file alone does not settle its ownership.
Start with a specifically selected subject and keep the work and parity audit scoped to it.

**Done (2026-11-14, the load's three answers).** The load's answers got the noun the reference gives
them. `0x8090` - the players request - is `Player::load_block`; `0x8091` - the inventory stream and the
map's record - is `Player::items_answer`; `0x8088` - the entry point - is `Player::spawn_answer`; all
three in `game/src/Agents/Player.rs`, frame for frame in the reference's order
(`ASURA/crates/asura-world/src/player.rs`, 540 lines, `player_block`). The frames moved verbatim; the
only lines that changed spelling are the ones that read the character's row (`sheet.` -> `self.`) and
the world's settings (`self.settings.` -> `settings.`). What stayed is what is not the player object's:
the database reads that decide the unlocked-instance bitmap (the character's rows, R12's arrival rule),
the entry point itself, the district's introductions, the broadcast that tells the copy this body is
here, and the agent the session creates at the entry point - `Player` is given the state and returns
the frames. `PLAYER_TEAM_TOKEN`, `UNLOCKED_MAP_ZEROS` and `MORALE_FULL` moved with it, to the
`player.rs` the working implementation keeps them in. No frame, order or size changed; all four
end-to-end journeys and the login journey pass untouched.

**Done (2026-11-14, the sheet a load reads).** The one statement the session still wrote itself -
`SELECT name, campaign, primary_profession, gold, appearance, level, unlocked_professions FROM
characters` - moved into `DatabaseCharacters::sheet` (`game/src/Characters/Repository.rs`), with the
sheet's shape and the sheet a failed read falls back to in `game/src/Characters/Sheet.rs`
(`CharacterSheet`, `CharacterSheet::missing`). That is the reference's `Player::LoadFromDB` seen from
this tree's side, and it is the same table the noun's other reads use; the session now keeps no SQL of
its own outside the confirm's statement, which is the character family's. The fallback's fields are
tested one by one (name "Player", Prophecies, Warrior, level 1, gold 0, the creation screen's unlocked
professions), and the four end-to-end journeys pass untouched.

The message families are therefore all out of the session: item, movement, travel, chat, character and
the load's three answers. What is left in `WorldSession` is the connection's own cycle - the seed
exchange and the version message, the travel row's claim, the head block, the read loop, the map's
clock, the outbox and the district it gives back - which is the reference's `WorldSession` and ASURA's
`connection.rs`, not a message family. The clock is the third item's.

**Evidence of completion.** The subject has a traceable handler-to-mechanic path, the mechanic's
state and decisions have a clear owner, existing callers and persistence still work, and recorded
frames and the client confirm the same behaviour.

## 3. Simulation and lifecycle ownership

**What exists.** The session advances its player's agent in `WorldSession::step_map`.
`worldserver` drives `World::update`, which dispatches world hooks and sweeps empty districts.
`game/src/World/mod.rs` holds shared world data and districts. The agent tick remains in the session.

**Reference.** AzerothCore's `src/server/apps/worldserver/Main.cpp:618` calls `sWorld->Update(diff)`;
`src/server/game/World/World.cpp:1131` implements the world update, calls `sMapMgr->Update(diff)` at
`:1254`, and calls the script world-update hook at `:1355`. The maps tick themselves from there
(`MapMgr::Update`, `src/server/game/Maps/MapMgr.cpp:251`, scheduling every map's own `Update`), so a
program's loop drives the world, the world drives the maps, and a session ticks nothing.

**Done (2026-11-14, the copies' sweep).** The first piece of the world's own update: `World::update(diff)`
(`game/src/World/mod.rs`) tells the scripts (`sScriptMgr->OnWorldUpdate(diff)`, `World.cpp:1355`) and
disposes the copies that have been empty past their hold, naming them (R37, R40), and `worldserver`'s
timer calls it instead of locking the district table itself - the reference's `sWorld->Update(diff)`,
with the sweep as the one thing this world has to drive today. Same interval, same log line, same rows.

**Done (2026-11-14, the kick and the shutdown order).** A connection now ends itself when the world is
going down, instead of being dropped mid-future with its place in the district still held and its
position unwritten. `WorldSocketMgr::stop` sets one flag every session reads at the top of its turn
(`WorldSession::closing`), waits (bounded) for the live count to fall, and only then stops the network -
the reference's `sWorldSessionMgr->KickAll()` ("save and kick all players", `WorldSessionMgr.cpp:220`),
the `UpdateSessions(1)` pass that unloads them, then `sWorldSocketMgr.StopNetwork()`
(`Main.cpp:370-374`). The program's own shutdown now follows the reference's order: the network goes
first and `sScriptMgr->OnShutdown` is raised after it (`Main.cpp:426`), where before it was raised while
the listener still served. Nothing a client sends or receives changed;
`a_session_told_the_world_is_closing_ends_without_answering` proves a kicked session closes without a
word, and the end-to-end journeys still start and stop the real program.

**What is not here, and why.** `sMapMgr->UnloadAll` and `OnAfterUnloadAllMaps` have no counterpart work
in this tree yet: a fresh world disposes the copies an earlier run left
(`DatabaseInstances::dispose_all` at start-up), which is what an unload at the end would achieve, and the
hook arrives with the moment it would be raised. The session registry the reference kicks through is not
here either: sessions are tasks, so the kick is a flag rather than a walk over a map of them - one
place, one message, no new state to keep in step.

**Done (2026-11-14, one loop drives the world).** The program's two timers became one thread, as the
reference's `WorldUpdateLoop` is one loop (`Main.cpp:584-620`): it measures how long it has been, sleeps
the remainder of `MinWorldUpdateTime` (the reference's own key, 1 ms) rather than updating twice inside
it, and calls `sWorld->Update(diff)`. The world decides inside which of its own slower things are due -
the copies' sweep, on `World.SweepIntervalMs`, by the time that has passed rather than by a thread's own
sleep (the `World`'s `sweep_elapsed`, which is the reference's `WorldUpdateTimer` arrangement) - and
tells the scripts every tick. The status line is this tree's own diagnostic and not the world's, so it
rides the same loop on `StatusInterval` seconds. Two threads became one, no client-visible behaviour
changed, and `World::update` takes only the tick it is told about, reading the clock itself when a slow
thing is due.

**Work to resolve (the rest).** The maps' own clocks: `WorldSession::step_map` still advances the
agent and tests the doorway inside the connection's read loop, where ASURA has one clock per map
(`Instance::run`, `ASURA/crates/asura-world/src/instance.rs:394-403`) that every connection in that map
races against (`connection.rs:494-548`). Moving it changes who advances an agent and in what order
relative to traffic, so the client's own timing is the measure, not an opinion; the reference's shape is
the world update driving each map, and a map's clock serving its connections.

**Evidence of completion.** A trace shows which component schedules and updates each implemented
piece, instance lifecycle and movement remain correct with multiple connections, and client-visible
results match ASURA. Automated tests support this check; the client is the judge of visible behaviour.

## 4. Script and module boundary

**What exists (2026-11-14).** The manager lives where the reference keeps it - `game/src/Scripting/`,
beside the interfaces - and holds real scripts, not counts. `scripts/` keeps what the reference keeps
there: the two loaders. No module crate exists.

**Done (2026-11-14, the manager and its route).** `ScriptObject` (`ScriptObject.h:38`), `WorldScript`
(`WorldScript.h:45`) and `ScriptMgr` (`ScriptMgr.h` 940 lines, `ScriptMgr.cpp` 292) are ported as the
reference has them: the program sets a loader per kind, `initialize` runs them once, a script is
numbered by its order of registration (`ScriptPointerList[_scriptIdCounter++]`, `:820`), a name
registered twice is refused, and a hook is answered only by the scripts that declared it
(`AddScript(script, enabledHooks)`, `:796`; `CALL_ENABLED_HOOKS`, `ScriptMgrMacros.h:73-77`). The
engine now raises its own moments: `World::on_startup` at the program's ready line
(`sScriptMgr->OnStartup()`, `Main.cpp:398`), `World::update(diff)` from the world's loop
(`OnWorldUpdate(diff)`, `World.cpp:1355`), and `World::on_shutdown` on the way down
(`Main.cpp:426`) - the same three moments, in the same order, in the reference's own places. The world
owns the manager the way it owns its copies, so a session can reach it without a global.

`World::update` is proven to reach a registered script, and to skip one that did not declare the
hook, by `game/src/World/mod.rs`'s `the_world_raises_its_moments_to_the_scripts_that_declared_them`.

**Ported thin, and said so.** Three of the reference's thirteen world hooks are here: `OnUpdate`,
`OnStartup`, `OnShutdown` - the three moments this tree raises. The other ten (`WorldScript.h:30-43`)
each need both an engine moment and argument types this tree has no counterpart for (a locale, a
`ShutdownExitCode`, a `ShutdownMask`, a client cache version, an MOTD text), and writing those types
first would be inventing them. The other script *families* the reference has - `PlayerScript.h`
(51,849 bytes), `AllMapScript`, `CreatureScript`, `ItemScript` and the rest - are not here: they arrive
with the game objects they are about.

**Work to resolve.** The route exists; the *action* does not yet. Nothing a script is handed today can
change the world: the hooks take no argument, exactly as the reference's do, because the reference's
scripts reach `sWorld` and `sObjectMgr` as globals and this tree has no globals - so a script's first
authorized action needs an interface to act through, and that interface is the content boundary the
owner has to set (`docs/port/11-content-and-behaviour.md`, the Lua/Rust split). The reference's first
such family for this world is `PlayerScript`'s chat hooks, which is what `scripts/World/chat_log.cpp`
is written against and what would move the chat line out of `Handlers/ChatHandler.rs` and into a
script. A module crate is the last piece: `modules/` has the loader hook and no crate.

**Evidence of completion.** A registered script can receive an implemented engine event and affect
an authorized game action through the intended interface. A module can register and receive its
hooks. Lifecycle, ordering and client-visible behaviour have corresponding verification.

## Applying the guide

These items are findings and work to investigate, rather than authorization to alter the protocol or
choose a new architecture. Follow `AGENTS.md` before each change, including its comparison with the source (Rule 0),
pre-flight sentence, single-subject and audit requirements. `docs/port/06-world-plan.md` maps game
systems to their owners and `docs/game-rules.md` records their expected behaviour. Reassess this guide
when a boundary changes, and correct outdated descriptions where they already occur.
