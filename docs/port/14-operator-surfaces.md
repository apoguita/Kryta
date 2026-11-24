# Server API, commands, CLI and configuration manager

This is the canonical scope, roadmap and delivery record for Kryta's operator surfaces. The owner clarified that
research projects are studied for their ideas, workflows and limitations: no code from those tools is
selected for reuse. AzerothCore remains the design authority, and client-visible behaviour is
ported from a source under Rule 0. Those repository rules are not displaced by this research.

## 1. Settled direction

One registered Rust API function performs each operation. `config/Server/commands.json` maps a GM command and
its aliases to that function: `tp` -> teleport, for example. The embedded server console, standalone
CLI, authorized in-game chat, management application and Lua
when it is available can call the same function, with the same execution. A command mapping to a Rust
function does not require Lua. Adding an alias for an existing function changes data; adding a function
requires its registration and any engine work it needs, without modifying the dispatcher or other
functions. The function performs validation and returns its result through the shared contract.

The API is intended for consumption by Kryta tools and third-party tools. Remote browser access to the
web manager is required; direct remote API or CLI access is desirable but not required for the first
delivery. Its public contract, authentication and transport must be designed. The manager and its
access interface are part of the server stack. The existing client portal is a possible host or route
to investigate, not a settled choice: it currently serves client identity through authserver, while
world operations belong to worldserver and process control to the supervisor (`0003-portal-host.md`).

GM commands are a way to call the API, not a separate implementation. Callers supply the information
the function needs; a caller without a player session can specify a target where the operation supports
it. No particular command is granted a new caller, argument or client-visible behaviour by this plan.
Preserve the ported behaviour and update `09-parity-audit.md` when a client-facing change requires it.

The supervisor owns service processes, their lifecycle and readiness. World operations belong to the
running world; an application asks the appropriate owner to act, and does not become another process
watcher. Configuration means program settings in JSON; world content and player state use their own
existing storage and repositories (`04-configuration.md`). The requested control panel also includes
research, recording and content workflows in `12-research-and-control-panel.md`.

The web manager is the primary operator interface; authorized in-game chat is second. The embedded
console, standalone CLI and API are additional access paths. The manager sends world operations to
the world server and service lifecycle operations to the supervisor. The connections and their
authentication remain to be designed; the same owners serve other supported callers.

## 2. Current state

The management service `kryta-management-service` serves the Control Panel itself. Start it with
`cargo run -p kryta-management-service`, or through the supervisor's stack. A numeric argument
overrides the configured port for that run. Account and character routes still use the panel's
repositories; their APIs, session invalidation and targeted disconnect remain to be implemented.

- `game/src/Chat/Commands.rs` reads command rows and resolves each row's `operation` through the
  registry in `game/src/Chat/Operations.rs`; there is no enum of commands and no match on one. Every
  surface - the chat line, the console, the control endpoint - resolves the same rows and invokes the
  same registered function; the chat line performs the answer a function returns (`Text`, `Frames`, or
  a `Transfer`), which is why `/maps` and the console's map list are one implementation and `/stuck`'s
  frame and `/tp`'s transfer are the engine's own. The frames were proved byte-identical across that
  move by capturing the same session before and after it.
- `tools/supervisor/src/main.rs` starts and watches services, answers a loopback control channel
  (`status`, `stop [service]`, `restart <service>`, `quit`), reads the world's exit code `2` and starts
  the service again. Before a service starts it claims that service's port: an earlier instance of the
  service's own program is stopped and replaced, and any other process holding the port is named and
  the stack refuses to start (`take_port`), so a stale service can never take the stack down and a
  stranger is never silently killed. `worldserver` has an embedded interactive console, a standalone
  `kryta-cli`, and the local control endpoint; its two lifecycle rows, `server exit` and `server
  restart`, ask the world to stop and set the exit code the supervisor reads.
- `tools/manage` is the panel's implementation, layered as `web/` (the shell, its stylesheet, the
  components and HTTP helpers), `view.rs` (the game's names for the numbers a row stores), `pages/`
  (one module per destination), `navigation.rs` (the path-graph cache reader) and one routing table in
  `lib.rs`. It is arranged around the game's own
  hierarchy: the server hosts Accounts; an account owns its Players (the characters); a player page
  owns its own data - progress (level, stored experience, gold), position, inventory and equipment
  (the item rows), titles, and unlocked maps. Accounts are created, edited, permanently removed and
  activated/deactivated through `game::accounts`, and an account's recorded EULA acceptance is set or
  cleared (`POST /accounts/{id}/eula`); players are created from the creation record lists and
  renamed, levelled (1 to 20, R48) and removed through `game::characters`. The settings, worlds,
  schema and content pages read; the content page reads the world pack the way the world does. The
  Navigation page inspects the extracted path-graph cache and replaces one selected map in it from the
  configured `MapArchiveFile` (`POST /navigation/dump`); the same two actions answer JSON at
  `GET /api/navigation/maps` and `POST /api/navigation/dump`. `GET /health` answers `ok`. A page
  never waits for another process: the supervisor's status, the world's registered operations and the
  world's own status are probed after the page has answered.
- Lua integration, API bindings and script-defined commands are planned, not implemented. Existing
  command wire behaviour and the live-client status are recorded in `0012` and `09-parity-audit.md`.
- See `01-structure.md` for current stages and proof. The planned operator features below are not
  claims that those capabilities run today.

### Agreed scope and roadmap

Kryta has four operator surfaces: the operator API, the standalone CLI, the in-game CLI, and the
Configuration Manager. The first three are functional infrastructure, not a promise of a comprehensive
administration command catalog. The Configuration Manager is the operator's front door, not a second
configuration-file editor: it owns navigation and will connect the existing supervisor, operator API,
configuration reader and asset readers without duplicating any of them.

| surface | actual state at `7679ba1` | boundary |
|---|---|---|
| Operator API | landed: the loopback `/v1/operations` endpoint invokes the shared registry | it is not yet a remote public API or an authorization system |
| Standalone CLI | landed: `kryta-cli` uses the world endpoint and registry | it does not own world operations |
| In-game CLI | landed for the existing chat command rows and live-client proof | GM authorization and new client-visible commands are not implemented |
| Configuration Manager | the layered panel in `tools/manage` (2026-11 rebuild): the Accounts -> Players -> player-data hierarchy, finished account and player writes, Services/Console/World status as async adapters, world content read from the pack, worlds/settings/schema read-only | monitoring (logs), configuration edits, generic JSON asset edits, remote access, and a supervisor start verb remain separate work; a destination without a function is not in the rail |

The Configuration Manager delivery order is binding:

1. **Management application foundation:** application structure, launch mechanism, and navigation for the working destinations; a destination appears when it works, and the configuration, assets and logs destinations join when their own steps land.
2. **Supervisor integration:** start, stop, restart, connect to an already-running stack, and report service state and failures through the existing supervisor.
3. **Integrated console:** use the existing operator API and shared command registry; no parallel command system.
4. **Monitoring:** show service state, server logs, and errors from their existing owners.
5. **Server configuration management:** read, edit, validate, and safely persist server settings through the programs' existing configuration rules; distinguish saved from effective values and restart requirements.
6. **JSON asset management:** generically browse, edit, validate, and persist existing JSON game assets; specialized editors are out of scope.
7. **Integration and acceptance testing:** prove the complete operator workflow with the server both running and offline where a feature does not require a running service.

`tools/manage` is the server-rendered web application; no UI framework is selected or introduced. The
2026-11 rebuild replaced the patched, single-file page set with a layered arrangement and removed the
destinations that had no function: the rail shows Overview, Accounts, Players, World content, Worlds,
Navigation, Services, Console, World status, Settings and Schema, and no page pretends to be a pending
roadmap step.

The shell is responsive and keeps the operator workstation look (`Kryta Control`); its stylesheet is
one asset (`/static/style.css`) served from the program's own memory instead of being repeated inside
every page. The hierarchy is the game's own, and the rail's categories name it: Accounts is the account
directory (account-owned state; an account page owns its identity, login state, removal, its players,
and the two account-wide areas this server does not record yet), and Players lists the characters across
accounts or for one account, each linking to the player's own page (progress, position, inventory and
equipment, titles, unlocks, the account that owns it). Players are created from the Players page, which
names the account that will own the new player. There is no JavaScript except three small probes (the
supervisor's status, the world's registered operations and the world's own status), and every page
works without them. Every write answers a `303` with the `Location` a browser follows. The old
`/accounts?account=`, `/characters?account=`, `/players/{id}` and `/players/{id}/characters` addresses
redirect into the hierarchy, so bookmarks keep working. Future work extends this arrangement rather than
patching a second page.

The Services page reads the existing `control_port` from `SupervisorStackFile` and calls only the
running supervisor's documented `status`, `stop <service>`, and `restart <service>` verbs. It cannot
start a stack because the current supervisor exposes no start verb once it is absent; asking the manager
to spawn one would make it a second process owner. The Console page asks the live world for
`world.operations` (after the page has answered), then sends the selected operation and named
`name=value` arguments to the existing `/v1/operations` endpoint. The world remains responsible for
validation, queueing, execution, and its answer. Both probes report an unavailable owner in the page
rather than simulating success or delaying the page.

**Validation.** `cargo test -p kryta-manage` passes the layout, component, view-label, name and redirect
tests; `cargo check --workspace` and `cargo test -p game --lib` pass. A live run with this machine's
`manage.json` reached the login, character and world databases and served every page in 10-110 ms. The
write paths were exercised end to end on a throwaway account: create account (303 to `/accounts/9`),
create its player from the creation lists and the pack's campaign spawn (303 to `/players/{uuid}`), read
the player page back (level, experience, campaign, inventory, unlocks), rename, set level, remove the
player with its items and unlocks, remove the account - each answer a `303` with its `Location`, and the
account and player were then removed. The old addresses were exercised too: `/players/2` and
`/accounts?account=2` redirect to `/accounts/2`, and `/characters?account=<portal id>` redirects to
`/players?account=2` or `/players/{uuid}`. The probes were exercised without a running world and
reported their owners unavailable instead of delaying their pages. The panel read
`worldserver/control.credential` and served the world-status page, which reported the refused
connection as unavailable, not as an empty result.

## 3. The API contract: what has landed and what is still to design

**The contract exists.** `game/src/Chat/Operations.rs` registers an operation as a name, a help
line and its declared positional values; a call is an `Invocation` (`operation`, `Arguments`, `Caller`
with its kind and vouched-for name) and an answer is the operation's text or a `Refusal` carrying a
stable code (`unknown_operation`, `invalid_arguments`, `refused`) and the message a person reads. The
registry refuses a duplicate name, checks the value count against the declaration before the world is
touched, and every `config/Server/commands.json` row binds a typed name to a registration by its
`operation` key - the chat operations (`chat.transfer`, `chat.maps`, `chat.stuck`) are registrations
like any other - with `surfaces` deciding who may type it. A door never holds
a command: the console resolves a line through the rows and the control endpoint resolves JSON through
the registration (`docs/port/05-tooling.md` records the live proof of each refusal code).

Still to design, in the items below: input types richer than one string, target context (a player, a
map, a district), progress for long operations, and the caller identity of a player-facing surface.

Define one registry binding stable function names to implementations. A JSON command name or alias
resolves to a registered name; missing or duplicate registrations and invalid mappings are reported.
A function can be called directly by a supported caller without inventing a second implementation.

The contract must specify, before implementation:

- Inputs, types, parsing, argument validation and help metadata, including multi-argument operations.
  Preserve the parsing and refusals of existing client commands as they were ported. **Part
  landed**: values are positional strings, validated against the registration, with the help line
  stored on the registration; richer types are still to design.
- Caller identity, permissions and target context: the running world, a map, a specific instance or
  district, a player or agent, as appropriate to the function. State what each function requires and
  return a clear failure when it is missing. The intended deployment is one game world with many map
  instances; do not treat instances as separate worlds. **Part landed**: every call carries a
  `Caller` (kind and name) supplied by the adapter, never by the caller's own words; permissions and
  target context are still to design.
- Results and errors in a form callers can use and display; client frames and existing text remain
  subject to Rule 0. Define how progress and completion are reported for longer operations. **Part
  landed**: a success text and a coded refusal, which is what the console prints and the endpoint
  returns as JSON; progress for long operations is still to design.
- World-thread ownership, queue bounds, timeouts and shutdown behaviour; avoid blocking the world tick
  on long I/O. A request accepted into a queue is not necessarily completed. **Landed**: the queue is
  bounded by `Console.QueueCapacity`, a full queue answers `unavailable`, and a timeout answers
  `unknown_completion` because it is not proof of refusal.
- Registration lifecycle and collision rules; adding an operation must not require modifying the
  dispatcher, another function or every caller. The underlying mechanic may still require engine work.
  **Landed**: a duplicate or empty name is refused; there is no dispatcher arm to edit.
- Invocation from another process: `tools/manage` and the running world do not share memory. Specify
  a control path to the world and a response path, distinct from the supervisor's own process control.
  External tools must be supported. Transport and authentication remain to be designed; direct remote
  API access is desirable but optional for the first delivery. Section 8 evaluates the portal and the
  reference's remote access interface and proposes the host and process boundaries. **Landed**: a
  loopback JSON endpoint with a service credential, `POST /v1/operations`; the supervisor's process
  control is a separate loopback channel (`tools/supervisor/src/Control.rs`).

The `commands.json` reader resolves each row to a registration; values are positional strings checked
against that registration, and richer input types remain to design. Whether every API function has a command
name, and how non-command callers discover functions, are design questions rather than enforced rules.

## 4. CLI, GM commands and access

The owner requires an embedded server console, a standalone CLI, and access to CLI command capabilities
from authorized in-game chat. These are entry points to the same operations, not separate implementations;
the design must account for target context and report when an operation cannot be completed. Preserve
ASURA's existing client-facing command behavior under Rule 0. The local world console is informed by
AzerothCore's `CliRunnable`, `CliHandler` and world command queue. The supervisor's process control is
a separate responsibility. The embedded console starts only on an interactive terminal. The standalone
`kryta-cli` calls the world's `POST /v1/operations` endpoint.

A command's availability to a caller and the caller's authorization are distinct facts. The plan must
not reject a row merely because it is console-enabled and also has a chat permission. Decide the
initial access model before exposing privileged operations. The owner wants an initial `admin` account
with password `admin`, kept in a server-restricted file, with administrators able to change passwords.
**Where it lives is decided**: `docs/decisions/0013-the-admin-identity-is-a-file.md` - the administrator's
identity is a file, never a database row, because an administrator must never be locked out of the
server they own; the file is protected the way the world control credential is, checked on every load,
and refuses to serve when the check fails.
Operators are server administrators and can perform every operation the server allows; a finer privilege
system can be designed later. Administrators sign in to the manager and can create other administrator
accounts, change passwords and remove access. An administrator links a game account to an administrator
identity to use in-game operator features; only authorized operators may use them. The default password
must be changed before remote access is enabled. The hash algorithm and its crate, sessions, recovery and
account linking still need design (`0013` names what it leaves open). ASURA does not establish a new GM
permission policy; `account_access` and RBAC are reference research, not an adopted schema. Do not invent
new client-visible refusals under Rule 0.

Access by third-party tools is required. Remote browser access to the manager is required; direct remote
access by third-party tools and the CLI is desirable. Remote access requires a design for identity,
authorization, credential protection, audit records and transport. The default password must be changed
before remote access is enabled. Local access also requires an
explicit trust boundary.

## 5. Configuration manager and control panel

The manager is a web application and the operator's comprehensive assistant for accessing and managing
the server, with scope growing as the server gains features. It includes server operation, settings,
game content, research and the workflows in `12-research-and-control-panel.md`. Its settings editor
must use the same configuration validation and storage rules as the programs. The owner approved a
workflow that saves edits, identifies whether each change can be applied live or needs a restart, and
lets the operator choose when to apply or restart. Player-affecting events are announced in chat;
saving alone does not restart a service. The design must cover:

- Choosing the program and actual configuration path; displaying descriptions, types, current saved
  values and validation failures. Preserve unknown data or refuse to save it rather than silently
  discard it; mask secrets and avoid exposing them through logs or command lines.
- Validating before saving, preserving existing metadata and using safe file replacement. Detect a
  concurrent external edit instead of silently overwriting it. State what happens on a failed save.
- Showing saved settings separately from values loaded by a running service, and indicating whether
  each change requires restart or supports a verified reload. Saving must not imply a live change.
- Separating JSON settings, database-backed game data and profile/content editing. Content apply and
  restart are explicit actions; verify the destination and outcome, and do not silently reseed over edits.
- Applying changes by the necessary supported mechanism (reload or restart). Where an operation
  affects players, broadcast the event to all players in chat. Define affected operations, notice
  timing and failure behaviour; client-visible messages remain subject to Rule 0 and the parity audit.
- Using the supervisor for process control and the world API for running-world operations. Define
  browser request protections.

`tools/manage` is the panel, and `kryta-management-service` serves it as the stack's `manage`
service on port 6602 (`tools/supervisor/stack.json.dist`). The manager must be accessible
from the server computer and remotely through a browser. Hosting, network exposure and profile
packaging remain to be designed; the manager is part of the server stack.

## 6. Research: applications and limitations

- `research/ASURA_testfork/run_server.py` is a Tk desktop/CLI server manager: setup checks, database
  settings, build, service operations through the supervisor and logs. It writes ASURA `.env` and its
  database setup can change an existing role password. Study its workflows, not its setup actions as a
  specification for Kryta.
- `research/ASURA_testfork/asura-cs/README.md` describes a PySide/Panda3D content editor with 2D/3D
  placement, undo, validation, profiles, scripts, quests and dialogue. `asura_cs/content/store.py`
  saves individual files via temporary replacement but ignores unknown fields on loading; whole-profile
  saves span files. The tool restricts its profile scope to pre-Searing. Its `deploy.py` uses ASURA SQL
  and the supervisor; the README says a full ASURA start can overwrite applied spawn edits. Full content
  bundles target an ASURA loader described as absent. These are tool-specific limits, not Kryta features.
- `research/Rurik/tools/orchestrator/README.md` describes a scenario editor with open/save, compile,
  launch and stop. It preserves certain fields the editor does not expose and previews compiled output.
  Its harness owns its own run; Kryta must retain supervisor ownership of its services.
- `tools/manage`, the AzerothCore console and the ASURA supervisor are further sources for current
  capabilities and architectural comparison. The remaining survey records research observations,
  not an instruction to copy third-party code.

## 7. Other research projects

### ASURA (`research/ASURA` and `research/ASURA_testfork`)

The ASURA predecessor this tree descends from has 12 crates, a PostgreSQL store, and the stack shape
(supervisor, auth, world, gateway, web gate). It is the only project here whose code is in this
tree's language and lineage.

- **A supervisor control channel** (`crates/asura-supervisor/src/control.rs`): a loopback line
  protocol whose verbs are `status`, `stop <service>`, `stop`, `restart <service>`, `quit`, plus
  `deploy-world <old> <new>` (`:3-4`), with a `request()` client at `:113` so the CLI is a thin
  client of a running supervisor instead of a second owner of the processes. This tree's supervisor
  has the same verbs except `deploy-world` (`tools/supervisor/src/Control.rs`).
- **A `tools/` directory of 37 files** - 19 Python, 16 PowerShell, two SQL seeds: capture
  (`sniff-capture.py`), reading
  (`capture-read.py` - layer-aware, `--describe`, `--self-test`, refuses a file that does not state
  its layer), analysis (`analyze-live-move.py`, `window-live-move.py`, `context-dump.py`,
  `inspect-reply.py`, `odd-manual-answers.py`), generation (`gen-map-records.py`,
  `gen-portal-table.py`, `merge-packet-tables.py`), verification (`test-battery.ps1`,
  `test-touched.ps1`, `m2-verify.ps1`), operations (`db-setup.ps1`, `seed-*.ps1`,
  `live-session.ps1`, `portal-probe.ps1`, `pg-recover.ps1`) and hygiene (`ascii-check.ps1`).
- **A server-side wire capture**: `crates/asura-world/src/capture.rs` writes packed frames as JSONL,
  the other half of `capture-read.py`'s two layers.
- **`recordings/`**: the same three kinds this tree carries, with `index.jsonl` as the search layer.
- **`docs/work/control-panel-plan.md`**: 341 lines of owner-confirmed panel requirements (seven
  workspaces, a test-run record, capture sessions with explicit PID pinning, live client data
  readers, the increments). This tree already carries the digested version in
  `docs/port/12-research-and-control-panel.md`.
- Research finding: the control channel, capture reader and writer show possible workflows and constraints.
  They are examples to study, not code selected for reuse.

### Rurik (`research/Rurik`)

A Guild Wars server emulator plus a content toolkit in Python 3: a stdlib-only `toolkit/` and
consumer tools in `tools/` and `apps/`. The richest operator tooling in the survey.

- **`tools/orchestrator/`** (PySide6, `orchestrator.py` 365 KB, README 383 lines): a window whose
  header holds a spec name, a one-line summary and three verbs - Compile, Launch, Stop - over four
  tabs (Skills, Party, Enemies, Attributes), with compile turning a spec into a content overlay,
  launch starting the client and servers and pointing the server at that overlay
  (`RURIK_CONTENT_EXTRA`, `RURIK_DAT`), and stop ending the harness, its servers and the client
  together in a kill-on-close job. Sessions land in `vault/captures/harness/<stamp>/`, and the run's
  verdict is read off what the harness printed.
- **`tools/viewer/`** (PySide6, README 122 lines): every model in `Gw.dat` browsable and textured,
  with a Maps tab that lists only the props a map references. The discipline to copy is in the
  README: **every archive fact comes from the stdlib `toolkit/mapdata/modelcatalog.py`**, the window
  only draws arrays it is handed, and the split is enforced by shape rather than by promise.
- **`toolkit/mapdata/`**: the `Gw.dat` reader, planner, writer, textures, and a catalogue cache
  keyed by the manifest's sha.
- **`toolkit/harness/`**: the run harness - `livesession.py`, `drive_client.py`, `marks.py`,
  `control.py` (a one-slot file mailbox between the driver and the running server, with a long
  argument about what it does not prove).
- **`toolkit/clientscan/`**: read-only client analysis (hooks, probes, replay of recorded client
  observations).
- **`schema/messages.json`** and `content/*.toml`: a wire schema and per-row content with provenance.
- Verdict: **idea and algorithm source, not code.** It is Python and Qt; `docs/port/05-tooling.md`'s
  rule is that nothing reads another tree at run time; this research does not select code for reuse.
  What transfers: the viewer's stdlib-data-layer/thin-UI split, the orchestrator's three verbs, the
  PNG-snapshot and `--smoke` habits, and the honesty about what a harness cannot prove.

### preservation-main (`research/preservation-main`)

A working single-process Go server (103 files, ~25k lines) multiplexing four services on one
listener.

- **A complete `Gw.dat` pathing pipeline** (`pathing/`, seven files, ~2,180 lines): archive reader,
  the custom Huffman+LZ77 decompressor, the RIFF `ffna` importer with planes and cross-plane portals,
  `TrapezoidAt`/`LineOfSight`/`Reachable` over a spatial grid, and A* with funnel smoothing.
- **`cmd/pathing_viz/`** (491 lines): renders the navmesh - trapezoid fills, edge outlines, XNode
  markers with direction vectors, per-plane colouring, layer toggles - to a bitmap. A ready template
  for a map tab.
- **`data/fields_2008.txt` / `fields_2025.txt`**: complete CtoS/StoC field-descriptor tables with a
  decoded type per field, plus a 12-value field-type taxonomy. A cross-check for the parity audit,
  never a source for it.
- Five `cmd/` binaries with stdlib `flag` only; no REPL, no console, no permissions, no capture.
- Verdict: **the pathing half is already here through ASURA** (`game/src/Pack/`, `game/src/Terrain/`
  port ASURA's `gwdat.rs`, `path_tables.rs`, `map_data.rs`, `pathfind.rs`, which are themselves this
  pipeline). What is new: the renderer as an idea, and the field tables as a cross-check.

### entice (`research/entice`)

A six-language historical snapshot (Scala old server, Elixir Phoenix web, C# client and interface
libraries). Half-working, documentation-rich.

- **The best command design in the survey** (`old-server/.../scripting/Command.scala`, 34 lines): a
  command is `info: CommandInfo` - one help string per positional argument, a general description and
  a usage line - plus `run(args, ctx): Option[String]`. Help is **generated from the metadata**, so
  `/help` and `/info <cmd>` cannot drift from the command. The dispatcher
  (`controllers/Command.scala`) scans a directory of command scripts, keys them by
  `info.command`, and has a working `reload` that re-reads the directory without a restart.
- **A private operator channel** (`gw-interface/.../Interaction/Chat.cs`): the client's `Channel`
  enum carries `Command '/'` and `InternalCommand 0xA8` - a second, non-game prefix so operator
  traffic is distinguishable from the game's own commands and can be kept out of the transcript.
- **A web surface** (Phoenix): 15 JSON routes, 6 websocket channels, 6 HTML pages, with `routes.md`
  and `topics.md` as specs, a token scoped to a map, and a 65-line `/client/:map` debug page that
  prints every server event into a scrolling log.
- Verdict: **idea only.** The help-from-metadata rule, the reloadable command directory, the separate
  operator prefix and the four-level reply taxonomy (`warning`/`info`/`debug`/`colour-test`) are worth
  citing. None of the code is portable, and the Phoenix web is built to *replace* the game protocol,
  which is the opposite of this tree's position.

### GameRevision-GWLP-R (`research/GameRevision-GWLP-R-4208ffa`)

A deprecated Java 7 / Maven skeleton (951 files, ~55k lines); the host starts, the behaviour is
stubbed.

- **A database-backed command registry**: `commands` and `usergroups` joined through
  `grouppermissions` (`database/src/main/sql/default_data.sql:200`, `:733`), a per-entity set of
  allowed command names checked before dispatch (`entitysystem/systems/CommandSystem.java:86`), and a
  command reply colour documented as "Command reply" (`DarkOrange_DarkOrange`).
- **A map table**: 728 rows of `(map id, type flag, map file id, name)`
  (`default_data.sql:464`) - a map id to archive file id table, which this tree's map records
  (`basic.json` in the world pack) and the archive index already cover.
- **Annotated packet classes** (216 outbound, 17 inbound) with per-field javadoc, and a reflection
  codec.
- Verdict: **idea only, as corroboration.** Its registry is the same shape as the reference's RBAC
  tables, and it is worth citing as a second witness that "commands are rows, permissions are a
  join". The map table is a cross-check; the packet classes are not a source under Rule 0.

### OpenTyria (`research/OpenTyria`)

A working partial C server (93 owned sources, ~33k lines, sqlite), auth and game in one process.

- **The upstream of the pathing this tree already has**: `FaArchive.c` (the archive and its MFT),
  `GmMapDataImport.c` (the `ffna` RIFF chunks and the path-data signature), and `GmPaths.c` (1,057
  lines of trapezoid A*, BSP nodes, portal pairs, waypoint reduction).
- **`code/OpenTyria/GmChat.c`**: the channel rule, and a command handler that is one hardcoded
  `/stuck` with no table and no permissions. It is the baseline this tree has already gone past.
- **`tools/packet.py`**: regenerates a whole message table by scraping the client's own in-memory
  descriptors - the best tooling idea in the survey, and the reason three separate projects share a
  layout table.
- **`code/OpenTyria/msgdefs.c`** (5,873 lines) and `opcodes.h`: a complete ordered field-layout table.
  A cross-check for `docs/port/09-parity-audit.md`, never a source, and never a second table in this
  tree.
- No CLI (everything compiled in), no console, no capture format, no web surface.
- Verdict: **already ported where it matters** (pathing and archive, through ASURA). The tooling idea
  - a descriptor scraper - is worth keeping in mind for measurements, not for the server.

### Headquarter (`research/Headquarter`)

Not a server: a headless client and bot harness in C11 with a plugin API (`include/client/
Headquarter.h`, ~150 exported actions) that logs into an auth service and plays.

- `tools/inflate.py` and `docs/compress.c`: the client archive's custom decompressor.
- `data/gw_<build>.pub.txt`: 118 builds of the handshake's `root`/`server_public`/`prime` - a
  versioned key table.
- `code/client/command.c`: a clean flag grammar (positional plugin, named flags, environment
  fallbacks) with generated help.
- Verdict: **not an operator surface for this tree.** The decompressor and the per-build key table are
  upstream of what `shared/src/Cryptography/` and `game/src/Pack/FaCompress.rs` already carry; the
  flag grammar is a decent precedent for `Options::parse`'s help text and nothing more.

### preforged / preforged-source (`research/preforged`, `research/preforged-source`)

A C++20 Guild Wars server with a private 2022 client (the runtime tree is 33 GB, 29 GB of it
captures; the source tree is 800 files under `src/` and cannot build as shipped because `tools/` and
`data/` are absent).

- **An in-game command console that is ahead of everything else surveyed**:
  `src/runtime/gw_chat_commands.h` (an ordinary table with usage strings and a per-player flood
  limiter), `src/runtime/gw_development_commands.h` (57 declarations, each with a tier - `gm`,
  `developer`, `staging`, `undocumented`, `private_extension` - and a `requires_authorization()`),
  an account allowlist in `config/private-admins.txt` with the note that typing `/.gm` cannot grant
  permission, and a per-account development toggle that rechecks that allowlist on every call
  (`gw_development_mode.h`). Feedback is delivered as in-game chat packets and every authorized
  command writes one audit line.
- **Playback as a server feature**: `src/runtime/gw_observer_replay.h` (242 lines) loads an imported
  observer match and serves it to a stock client through the normal handshake
  (`gw_auth.cpp:1328-1362`), entered with `/observe <id>`, with a per-reason unsupported report and
  its limits written down (`docs/observer-playback.txt`).
- **A capture pipeline**: a PID-verified controller executable plus an in-process collector library,
  writing JSONL with a per-opcode `schema` record, `complete` flags, and process and tick
  correlation (`bin/PacketLogger/README.md:13-25`, `bin/GuildWarsLogger/README.txt`), with a
  sha-keyed `capture-index.json`.
- No interactive stdin console, no web UI, no map viewer, and **no analysis programs at all** in
  either tree (`tools/` is absent, though `docs/` cites them).
- Verdict: **research precedent.** The console's three parts - a
  tiered declaration table, an allowlist plus a per-account toggle, and chat-packet feedback with an
  audit line - are the closest match in the survey to the reference's `{name, handler, permission,
  Console::Yes}` row plus RBAC; and "playback" is only a real feature here, in the observer form.

### prototype-offline (`research/prototype-offline-20261007-v1-apoguita-en`)

A packaged WebGL2 viewer over eleven `Gw.dat`-derived scenes, with offline Rust decoders. Rust (66
first-party files), Python 3, and ES modules; it runs.

- **The serving and rendering model**: `viewer/serve.py` is loopback-only, GET-only, an explicit
  finite path allowlist, a Host-header check, a CSP of `default-src 'none'`, no directory listing,
  and a sha256+size check of every scene at startup; the page renders static GLB scenes with
  orthographic orbit/pan/zoom and GPU colour-id picking, pins each scene's identity in
  `viewer/corpus.json`, and renders an unresolved material grey rather than guessing.
- **The CLI discipline**: mandatory input identity (`MFT` + source sha), an explicit input-domain
  token, stdout-only JSON, `create_new` outputs, refusal as a non-zero exit, and a stored expected
  output plus a negative case per tool.
- Verified limits in its own `STATUS.txt`/`ROADMAP.txt`: static scenes, no animation or collision, no
  controllable avatar, `nativeRenderParity: false`.
- Verdict: **a candidate serving and rendering precedent**, and as the honesty rule
  ("unknown is grey, and the page says so"). It has no chat, no permissions and no capture.

### WowWarband / the reference

Already `AGENTS.md`'s design authority; its console, its remote command endpoint and its world command
queue are in section 8, "Reference and existing boundaries". Its command scripts
(`src/server/scripts/Commands/`, 55 `cs_*.cpp` files) are the vocabulary a console should grow into,
and `cs_server.cpp` is the shape a server-level console group takes (`server
exit|restart|shutdown|idlerestart|idleshutdown`, `server set loglevel|motd|closed|security`,
`server info|debug|corpses`, `cs_server.cpp:55-94`), each row carrying an RBAC permission and
`Console::Yes`.

## Control panel hierarchy

The operator panel follows the game's ownership, not the database layout, and its categories name the
levels: the server hosts **Accounts**; an account owns **Players** (the characters); a player owns its
own data. Accounts are account-bound state - the login identity, its login state, shared Xunlai storage
(R49), and account-wide unlocks. A player page owns player-bound state - progress (level, stored
experience and carried gold, R45), position, inventory and equipment (the item rows), titles, and the
maps it has unlocked. Parties and instances are live world concepts, linked to a player where
applicable, not children of one account. Server services, settings, console and logs are server
administration. The panel keeps these relationships visible and retains the selected context across
actions.

Accounts and Players are separate rail categories. The Accounts directory is the account CRUD; an
account page exposes Account overview (identity, login state, removal), its Players, and the two
account-wide areas as sections, and the latter two are named as not recorded by this server yet rather
than shown empty. The Players list is every player across accounts (or one account's, via its filter
from the account page), each row naming the account that owns it and linking to the player's page. The
creation form names the account that will own the new player, uses the creation equipment and inventory
and the world pack's campaign spawn data through the shared character-row writer, and PvP creation is
refused because the spawn file names no PvP map. The player page shows its account (linked) and owns its
rename, level (1 to 20, R48) and removal actions; removal deletes the row with its own items and
unlocks. Slot capacity is not represented by the current account model and is not enforced. Creation
initialization uses several database writes and cleans up on failure; cleanup failure is reported.
Session coordination and account/player APIs remain outstanding: panel writes do not coordinate with
online players. Account deletion retains player rows; their policy needs an explicit decision before
changing it. Metrics are not recorded by this server yet; the player page lists the metric catalog
with every value at zero. The player page and world read names, rank thresholds
and display test points from the configured `WorldPack`'s `titles.json`. The
catalog defines two custom tracks: 48 `Hog Jumper` for the `jumps` metric at 1/10,000,
and 49 `Embiggener` for `Minipets Embiggened`. The login program sends their text to a patched client's
`Kryta.dll` over the client link (`docs/port/22-client-platform.md`). The standard
client receives the short rank name `Jump`. The fixed-text probe displayed
the full name and description in-game. The title messages are evidenced at E23 and
E156-E158; no metric source or database row exists yet.

World content is the world pack (`docs/decisions/0014-the-world-content-is-a-pack.md`), so the content
page reads the pack through the same reader the world uses and shows what the world will serve; editing
the pack is the roadmap's generic asset workflow (step 6) and is not pretended. The world database's
content tables are empty by design (`data/sql/base/db_world/0013-content-is-the-pack.sql`).

ASURA `creation.rs:346-475` validates the name and reads defaults and campaign spawn data. The game
handler still uses the world's first outpost and does not apply that validation; moving the row write
into the repository does not claim to resolve those parity differences. The panel uses the ledger's
R65 name rules and the campaign spawn file. Appearance flags and armour colour choices are not yet
stored by the game creation handler (the `0x808B` audit row).

## 8. Technical design and first implementation slice

This section is the design of the first slice. **The first slice and its contract have landed**: the
operation registry with declared values and a coded refusal, the world queue, the embedded console
adapter, the loopback control endpoint, the standalone CLI, and the manager's `/status` page, with
`tools/manage` a supervised stack service; `docs/port/01-structure.md` and `docs/port/05-tooling.md`
carry the status and the live proofs. What this section still describes ahead of the code is
administrator authentication.

### Reference and existing boundaries

- At reference revision `30a36ec4d`, `src/server/apps/worldserver/CommandLine/CliRunnable.cpp:231`
  queues console commands; `src/server/apps/worldserver/ACSoap/ACSoap.cpp:81-140` authenticates a
  remote command, queues it and waits for its result. `src/server/game/World/World.cpp:1350,1721-1736`
  processes the queue in the world update and invokes completion. `Main.cpp:338-342,400-408` starts
  the reference's SOAP and console threads. These establish ownership and execution order, not a
  requirement to copy SOAP's protocol or credentials into the manager.
- `tools/manage/src/lib.rs` (`run`) starts Kryta's existing HTTP manager with its own settings, and
  its routing table names every address. `tools/supervisor/stack.json.dist` lists auth, world and the
  manager, and `tools/supervisor/src/main.rs` (`start`, `restart`, `stop_all`) owns child processes
  and stack shutdown. The
  manager's current local token mechanism is not the planned administrator login or a remote security
  model.
- `docs/decisions/0008-administration-tool.md` records the current separate, loopback-only tool.
  The planned remote manager and live-operation connections extend that decision. When implemented,
  correct the decision and configuration documentation in the same change; do not claim the current
  tool already supports them. Keep the existing client portal for client identity, rather than adding
  administration routes to it; this preserves its boundary and avoids another dependency in login.
  `tools/manage` is the web-facing component of the stack and the supervisor starts it as a configured
  service. World and supervisor control endpoints remain owned by their respective processes.

### Operation contract

The game library owns a registry of named operations and their metadata; worldserver constructs it
and the world owns execution. Each registration supplies one implementation, stable name, help,
input specification and required context. Command names and aliases in `config/Server/commands.json` resolve
to those registrations; reject duplicate names and unknown mappings during validation. Add an operation
by registering its implementation, without adding a dispatcher match arm or changing other callers.
The command adapter owns command-line parsing; API calls carry structured input. Neither changes the
existing client command parser or frames until that subject is ported from its source.

An invocation contains an operation name, structured arguments, authenticated caller identity and
caller type, plus any context required by the operation. Identity is supplied by a trusted adapter,
never accepted as an unverified claim from a browser. An operation validates its own inputs and
context and returns structured success data or an error code and safe message. The adapter renders
the result for its surface. Missing context, unknown operation and invalid input are distinct errors.
Operation-specific targets and game behavior belong to the relevant game subject, not this contract.

For a world operation, a bounded queue carries the invocation and a response channel to the world
update. The world executes it and completes the response. Queue full, world unavailable and shutdown
are explicit errors; a timeout is an unknown completion state, not proof that a mutation failed.
The initial status operation is read-only, so a retry has no game side effect. Queue sizes, timeouts,
listener addresses and ports are settings in the owning program's JSON, with defaults documented
in `04-configuration.md` as part of implementation. A request ID correlates logs and responses;
credentials and sensitive arguments are excluded from logs.

### Connections and authentication

Keep `tools/manage` as the browser-facing web application and make it a supervised stack service.
Expose a separate, local-only HTTP/JSON control interface in worldserver, analogous in ownership to
the reference's worldserver SOAP endpoint. The manager calls it for world operations; the standalone
CLI uses the same control contract. A separate local supervisor control interface handles only service
status and lifecycle. Its control path is not implemented by the world or by spawning a second watcher.
The endpoint, its schema and its settings are the first slice below; the settings and their defaults
(`Control.Port` 7878, loopback only) are in `04-configuration.md`.

The manager authenticates administrators and presents authenticated requests to the control interfaces.
Local process-to-process authentication must be independently enforced; loopback alone is not identity.
Use a randomly generated service credential in a server-restricted file and compare it on each request;
do not pass it in URLs or logs. Protect the credential file with operating-system access controls and
fail closed if protection or credential loading fails. Operator identities forwarded to the world and
supervisor must be accepted only over that trusted channel. API clients authenticate independently;
the manager must not let a browser submit an arbitrary operator identity.

For browser access, use HTTPS for non-loopback access, a protected session cookie, CSRF protection,
login throttling and audit records. Store salted password hashes rather than plaintext passwords in the
restricted administrator file. Bootstrap the requested `admin` / `admin` only on first initialization;
never reset an existing account. Force a password change before non-loopback access and retain a local
recovery procedure. Server-side sessions are invalidated on password change or account removal. Full
operator access is the initial policy; game-account linking is required before admin chat commands.
Exact cryptographic libraries, deployment certificates and recovery steps must be validated in their
implementation stage. Direct remote API and CLI access remains a later extension.

### First slice: local manager reads world status

1. Add the operation registry and invocation/result types under `game/src/`, following the reference's
   `game/Chat/ChatCommands/` and world ownership; expose an operation `world.status` with no inputs.
   Its result contains the world identity and the existing status information already available to
   `World`; do not fabricate unavailable fields. Register it during world initialization.
2. Add a world-owned request queue and process it from the existing world update. The embedded console
   adapter can submit the same operation when it has an interactive input stream; a supervisor-started
   process must not compete for terminal input. No gameplay handler or chat packet changes in this slice.
3. Add the worldserver local control listener. `POST /v1/operations` takes JSON with `request_id`,
   `operation` and `arguments`; for `world.status`, arguments are `{}`. It returns the same request ID
   and either `{ "ok": true, "result": ... }` or `{ "ok": false, "error": { "code": ..., "message": ... } }`.
   Require the local service credential, validate input and enqueue the invocation; never execute a
   world operation on the HTTP worker. Define the complete schema and HTTP status mapping alongside
   the implementation, including rejected, timed-out and shutdown requests.
4. Add a manager status page backed by that endpoint; show unavailable/error separately from a valid
   empty result. Add the manager to the supervisor's configured services and readiness reporting.
   Keep the page on loopback until administrator authentication and remote protections are complete.
5. Prove the same `world.status` implementation through the queue and manager, including an unknown
   operation, rejected credential, unavailable world and shutdown. Confirm stack start/stop leaves no
   extra processes. Tests do not establish player-visible behavior, and this slice changes none.

The standalone CLI, supervisor operations, administrator login and remote browser access follow as
separate slices. Chat and Lua adapters use the same operation contract when their own prerequisites
are satisfied. Before modifying a client-facing handler or exposing new in-game behavior, read the
sources that implement it, `docs/game-rules.md` for the subject and `09-parity-audit.md`; port the
whole applicable behavior or stop where Rule 0 requires an owner decision. The current command and
chat implementation must be compared with its source before changing client-facing behavior; this plan does
not establish its correctness or live-client status.

## 9. Delivery plan and proof

Each subject below needs its own implementation scope and appropriate commits. Ordering is dependency
based, not permission to implement. Research features remain part of the requested panel, without
blocking the API and settings manager on playback or a map renderer.

1. **Local status slice.** **Landed.** The registry, world queue, local control interface and manager
   status page exist; the manager reaches the running world and receives a real result, and the failure
   paths - unknown operation, invalid arguments, refused credential, full queue, no answer - are proven
   (`docs/port/05-tooling.md`). No client-facing change was made.
2. **Shared execution.** **Landed.** Every surface - the chat line, the console, the control endpoint -
   resolves the same rows in `config/Server/commands.json`, and every row names a registration; there is no enum
   of commands and no match on one. The chat operations answer with `Text`, `Frames` or a `Transfer` and
   the surface performs it, so `/maps` and the console's map list are one function, and `/stuck`'s frame
   and `/tp`'s transfer stay the engine's own. Proof: the console and API tests invoke the same
   registrations; the chat frames are byte-identical across the move (captured before and after, then
   read with `tools/capture/read.py`), and `docs/port/05-tooling.md` records it.
3. **Service control and CLI.** **Landed.** The supervisor's loopback control channel answers `status`,
   `stop [service]`, `restart <service>` and `quit`; the world ports the reference's exit-code contract
   (`0`, `1`, `2`, `Main.cpp:434-439`) and carries it out through the rows `server exit` and
   `server restart`; the supervisor restarts a service that exits `2`, bounded to five requests, and
   `console_service` gives the terminal to one service only. Proof observed on this machine: `status`
   listed every service; `restart world` replaced its process; `world.restart` through the world's own
   control endpoint produced "asked to restart (exit 2), request 1 of 5", a new world process, the
   stack still up with one world in it, and the restarted world answering again; `quit` stopped
   everything and left no process behind.
4. **Configuration Manager foundation.** **Landed, then rebuilt (2026-11).** `tools/manage` now
   supplies a layered panel whose rail shows only working destinations (Overview, Accounts, Players,
   World content, Worlds, Navigation, Services, Console, World status, Settings, Schema), follows the
   game's Accounts -> Players -> player-data hierarchy, and answers every page before asking another
   process. The old addresses redirect into it. Its proof is in `docs/port/05-tooling.md`, "The
   administration tool".
5. **Supervisor integration and integrated console.** **Implemented against current owners.** Services
   reads the supervisor's own `control_port` from `stack.json`, shows its status, and sends its existing
   named stop/restart verbs. Console asks the running world for its registrations and calls those same
   registrations through `/v1/operations`, with the world retaining all validation and execution. A
   missing supervisor cannot be started from the manager because no existing supervisor start protocol
   exists; that remains a protocol/design addition, not a manager substitute.
6. **Monitoring, configuration, assets, and acceptance.** Remain ordered exactly as `Agreed scope and
   roadmap` states above. Each step needs its own proof and must reuse its owner: existing appenders for
   logs, configuration reader for settings, and asset readers/validators for JSON.
7. **GM access and Lua; research workflows.** GM authorization, Lua runtime and bindings, script-defined
   commands, and capture replay remain incomplete. **The capture half has landed**: `worldserver`'s
   `CaptureFile` records the wire in both directions (`game/Server/Capture.rs`),
   `tools/capture/read.py` reads a capture in the layer it declares, and `tools/capture/diff.py`
   compares two captures message by message, refusing a pair whose layers differ. Reading and
   comparison are therefore done; replay that sends inputs remains open.

## 10. Remaining design work

- Complete the endpoint schemas, JSON settings and implementation-level types as operations are added;
  apply the contract above and document any necessary deviation from the reference.
- Administrator credentials: the identity is a file (`0013`), and its hash algorithm and crate,
  bootstrap detail, password change, recovery and audit detail remain to be designed; initial
  `admin` / `admin` and full operator access are the owner's direction. Finer privileges are future work.
- **Console and standalone CLI: landed** (`eb66d88`): the embedded console over the one registry, the
  standalone `kryta-cli` with `help`/`list`/`<operation> [--arg name=value]`, the supervisor's control
  channel, and the exit-code contract. Still open: target selection for operations on players, maps,
  districts and instances.
- Configuration Manager monitoring is next: service failures and the existing service logs need a
  read-only view. Server-settings editing, generic JSON asset management, and their acceptance workflow
  follow. Remote browser hosting and access remain a separate design question; the existing portal is
  still only a possible host.
- Lua runtime and bindings, script-defined commands and the form of interactive execution; Lua support
  is required and `11` owns its plan.
- Profile storage and selection, map visualization, recording comparison and replay scope; `11` and
  `12` keep the related open questions.

No research code has been selected for reuse. What has landed is marked **landed** above. Nothing else
in this document - remote transport, administrator login, a permission schema, or any new
client-visible behaviour - is implemented or approved by it.
