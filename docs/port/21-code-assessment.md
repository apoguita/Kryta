# Code assessment

What the code is like, read before any of it is changed. It answers four questions: does the code have
a style, does it follow one paradigm, are its structures well delimited, and is it patched
functionality. Nothing was changed to write this.

Read on 2026-11-23: all Rust under `common/`, `database/`, `shared/`, `game/`, `scripts/`,
`authserver/`, `worldserver/` and `tools/`, and the Python, PowerShell and C++ under `tools/` - about
55,000 lines of Rust in 190 files. Six readers read every file in full; the two generated protocol
tables and the PvP item table were read for shape only. A finding marked **checked** was then
confirmed against the code a second time; the rest are as the readers reported them.

## The short answer

Most of the code has a real, consistent design. The problems are concentrated in a few places, and
those places are the centre of the server: the gameplay connection, the two programs' start-up, the
title code, and what was left behind when one implementation replaced another.

| question | answer |
|---|---|
| Does it have a style? | Yes. One voice runs through almost every file: every item documented, every value citing where it came from, tests named as sentences, almost nothing that can crash on bad input. A few files are in a different hand: the title code, the control endpoint, parts of the panel. |
| Does it follow one paradigm? | Mostly. Objects with their own methods, dependencies passed in, no global state. The one real mix: the network code is asynchronous but waits on the database in a blocking way. |
| Are the structures well delimited? | The small ones are, and they are good. The large ones are not. |
| Is it patched functionality? | In specific places. The original port is disciplined; later additions were bolted on, and replaced code was left beside its replacement. |

## What is good and stays

- **The packet builders** (`game/src/Server/Packets/`): small pure functions, one family per file, each
  tested byte for byte against a recording and citing its source.
- **The protocol module** (`game/src/Server/Protocol/mod.rs`) and **the framing** (`shared/src/Packets/Frame.rs`):
  typed errors, table-driven, no per-message special cases.
- **The small game rules**: `Maps/Placement.rs`, the geometry in `Maps/Portals.rs`, `Maps/InstanceTable.rs`,
  `Maps/Cartography.rs`, `Districts` (`enter`, `leave`, `sweep`), `Characters/Equipping.rs`,
  `Agents/mod.rs` and `Agents/Entry.rs`. Pure logic, real tests.
- **The command registry** (`game/src/Chat/Operations.rs`, `Commands.rs`): operations return an outcome
  and never send; chat, console and the control endpoint share one implementation.
- **The portal** (`game/src/Portal/`), **the script manager** (`game/src/Scripting/`) and
  **`Accounts/mod.rs`**, which is the shape the other repositories should have.
- **Terrain and Pack**: function-for-function ports of ASURA's pathfinder and archive reader, with
  their oddities explained where they occur. Their internal style mirrors the source on purpose.
- **The world pack loader** (`game/src/Maps/Pack.rs`): strict, refuses a folder that disagrees with its
  record.
- **The network layer** (`shared/src/Network/`), **logging**, **the JSON configuration reader** and
  **the cryptography**: small, tested, clear ownership.
- **`authserver`'s leaves**: `login_reply.rs`, `front_door.rs`, `file_check.rs`, `stream::take`.
- **The panel's layering** (no SQL in pages; data through the game library's repositories), **the
  supervisor's control channel**, and **`tools/capture/`**, the best-delimited code under `tools/`.
- **The guard tests**: `separation.rs`, `schema_has_no_content.rs`, `content_is_held.rs`.
- **Habits across the code**: no `unwrap` in production code apart from one, no global state, SQL
  always parameterised, refusals logged with a reason.

## Where the problems are

### 1. The gameplay connection is one giant object (checked)

`game/src/Server/WorldSession.rs`, 3,966 lines, 47% of it tests.

- The struct has 28 fields (`:150-242`). Ten are locked separately (`account`, `moved`, `character`,
  `landing`, `portal`, `entered`, `agent`, `outbox`, `id`, `unanswered`), although one task owns the
  session. There are 28 `lock()` calls, and a failed lock is swallowed: the player quietly becomes id 1
  or the write is skipped (`:1158-1163`).
- It copies the world's data into every connection - maps, spawns, arrivals, commands, settings, item
  templates, creation tables (`:250-281`) - and also holds the world itself.
- Two functions hold the whole conversation: `converse` (`:483`, about 510 lines) and `load_map`
  (`:1212`, about 550 lines), with database writes, peer logic and the map's clock inline.
- It has seven constructors (`:250-367`); three exist only for tests and one has no caller.
- It carries about fourteen responsibilities: version check, key exchange, travel claim, district entry
  and exit, load assembly, framing, two dispatches, the simulation clock, unlock reads, peer registry,
  position save, sending and capture.

Size of fix: large. It is the central refactor.

### 2. Two message loops that have drifted apart (checked)

The creation screen dispatches in `answer` (`:999`, `match` at `:1009`); a loaded map dispatches inline
in `load_map` (`match` at `:1343`). They are separate and no longer agree.

- A clean disconnect (`0x8008`) is handled only in the creation loop (`:1037`). On a loaded map it
  reaches `other => report_unanswered` (`:1688-1692`). ASURA ends the connection (`dispatch.rs:86`).
- The ping is written in both. Stall detection and inbound tracing exist only in the first.
- Three arms are written in place in `load_map` instead of calling a handler: the players load, the
  spawn answer, and titles.

Size of fix: medium.

### 3. Each program starts up in one function (checked)

`authserver/src/main.rs` `run` is 756 lines (`:46-801`); `worldserver/src/main.rs` `run` is 764
(`:42-805`).

- The "read one setting, report its notes" block is repeated 17 and 21 times.
- `Options`, its parser, `report`, `print_usage`, the PID stage, the banner and the READY block are
  textually the same in both files.
- Work that should be a named stage is inline: the admin bootstrap with hand-written SQL
  (`authserver/src/main.rs:238-288`), the world-list refresh, the three-database open loop
  (`worldserver/src/main.rs:189-272`, with an unreachable branch and two comments that contradict each
  other), operation registration, the world loop.
- `tools/manage/src/lib.rs` `run` (283 lines) and `tools/supervisor/src/main.rs` `main` (265 lines) have
  the same shape.

Size of fix: medium.

### 4. Titles are bolted on (checked)

- Definitions and values are one record. `config/flavors/Vanilla/World/titles.json` gives each custom track its
  `points` and `selected`, and `vanilla-title-wire.json` gives 15 of 45 tracks a captured character's
  values (Hero at 1,448 points), so every character is sent the same progress
  (`game/src/Characters/Titles.rs:48-63`, `:219-226`).
- Nothing is stored per character. The displayed rank lives only in memory.
- `game/src/Characters/Metrics.rs` is not connected to the titles: its own header says the title layer
  reads `jumps`, and nothing does. A title's `metric` is free text nothing checks; the shipped file
  names one that is not in the catalog.
- The title message is handled inline in the session's dispatch (`WorldSession.rs:1577-1593`) with the
  only `unwrap` in the file. Every other message has a handler with a length check.
- `titles.json` is loaded by three programs.
- The types are named for something else: `VanillaTitles` also holds custom titles, and `Captured*`
  structs hold uncaptured ones.

Size of fix: large. `docs/port/20-open-issues.md` items 10a and 10b.

### 5. Replaced code was left beside its replacement

- **The database map source is dead** (checked). `game/src/Maps/mod.rs:113-381`, `DatabaseMaps` and the
  `MapSource` trait: the program loads only the pack, and the tables are empty by design. Two tests
  still use it (`game/src/World/mod.rs:949`, `worldserver/tests/end_to_end.rs:464`).
- **A whole byte buffer is unused** (checked). `shared/src/Packets/ByteBuffer.rs`, 494 lines: only its
  own module declaration names it.
- **A second configuration parser** that only tests call: `common/src/Configuration/mod.rs:224`, `:356`.
- **Half the database pool has no caller**: `execute`, `execute_sync`, `query_sync`, `query_scalar`
  (`database/src/Database/Pool.rs:203`).
- **Two item-template implementations**: start-up uses `definitions_as_templates`; `DatabaseItems::seed`
  is called only from a test, and the file headers describe the dead path (`game/src/DataStores/`).
- **An old size table in the login framing**: `authserver/src/Server/stream.rs:40-59`, `:119-130`,
  hidden from the compiler by `#[allow(dead_code)]` on the whole module (`authserver/src/main.rs:15`).
- **File-wide `allow(dead_code)`** in `game/src/Terrain/Pathfind.rs` and `MapData.rs` hides what is
  really unused there.
- **Three names for the same opcodes**: `CLIENT_*` constants at the top of `WorldSession.rs` (19 of
  them), per-family constants in the packet files (91), and the generated `GAME_CMSG_*` table, which the
  game library uses in exactly one place.

Size of fix: small to medium each.

### 6. Tests that no longer test the code

- `worldserver/tests/fresh_world.rs:149-163` asserts log lines the program no longer prints (checked:
  "Item templates read back:" exists nowhere in the code).
- `worldserver/tests/end_to_end.rs:464`, `:543`, `:1157`, `:1167`, `:1407` build their expected bytes
  from map and item tables the migrations empty.
- Ignored tests in `WorldSession.rs` have rotted: one indexes past its buffer and asserts a tick the code
  no longer sends (`:3371-3379`).
- Most of these are `#[ignore]`, so nobody sees them fail. The end-to-end tests are single functions of
  200 to 770 lines that replay bytes and read values out of log sentences.

Size of fix: small to medium.

### 7. Comments that are stale or tell history

About forty across the code. A header says something is "not here yet" above the code that does it
(`game/src/Terrain/Pathfind.rs:8-17`, `shared/src/Cryptography/Handshake.rs:24`,
`worldserver/src/main.rs:6-10`, `game/src/lib.rs:14`), a comment describes a past bug ("used to", "until
2026-11-12"), or a comment sits on the wrong item (`WorldSession.rs:111`,
`authserver/src/Server/session.rs:1220-1229`). `game/src/Maps/Portals.rs:3-6` claims a whole port of
which only `crossed` exists.

Size of fix: small each.

### 8. The asynchronous code waits on the database

Sessions are asynchronous, and every database call inside them blocks the runtime thread
(`reply.recv()`). With one network thread, one slow query stalls every connection on it. Equipping an
item reads the item table about seven times this way (`game/src/Handlers/ItemHandler.rs:150-290`).
`authserver/src/Server/accounts.rs:9-12` documents the hand-off, so it may be deliberate at this scale.

Size of fix: medium to large.

### 9. Smaller design faults

| what | where |
|---|---|
| `World` is a bag: 17 fields, about 35 accessors, built in two phases in an order only `main.rs` knows | `game/src/World/mod.rs:111-159` |
| Errors are plain strings in most modules and typed in others; tests match on substrings | throughout |
| Three different policies for a poisoned lock: skip silently, return an error, recover | `World/mod.rs`, the repositories, `Pathfind.rs` |
| The same lock-and-receive boilerplate 21 times, with three error policies | `game/src/Characters/Repository.rs` |
| A map id is `u32`, `u16` and `i32` in different modules, joined by silent casts | `Maps`, `Placement`, `PathGraphs` |
| A port above 65535 is silently truncated | `shared/src/Worlds/RealmList.rs:51-97` |
| Database errors read as "no table" | `database/src/Updater/Apply.rs:146`, `:169` |
| `campaign == 0` documented as both "Prophecies" and "PvP" (checked) | `Characters/Sheet.rs:19`, `Characters/mod.rs:88` |
| A failed travel leaves the connection unable to travel again: the flag is set before the steps that can fail (checked; may mirror ASURA) | `game/src/Handlers/TravelHandler.rs:168-185` |
| `confirm_creation` is 208 lines, three non-atomic writes, and tells the player the name is taken on any failure | `game/src/Handlers/CharacterHandler.rs:187-394` |
| The same item is three structs with hand-copied conversions | `HeldItem`, `creation::Item`, `ItemDefinition` |
| Character creation is written a second time inside a web page | `tools/manage/src/areas/players.rs`, `create_character_duplicating_the_server` (the panel was rebuilt on 2026-11-23; this duplication was kept and named) |
| The supervisor starts the old binary when a build fails (checked) | `tools/supervisor/src/main.rs:130-136` |
| The supervisor decides port ownership by parsing `netstat` and `tasklist` text, and force-kills by program name | `tools/supervisor/src/main.rs:185-310` |
| 28 of 38 files in the shared libraries are not formatted by the standard formatter; several files show paste damage | `WorldSession.rs:2303`, `Repository.rs:983` and others |
| Values written inside logic: the ping value, account features, attribute points, speeds, move types | `Agents/Player.rs:66`, `:130-140`, `Agents/mod.rs:195-256`, `WorldSession.rs:1225` |
| Settings nobody reads: `compass_range`, `mission_button`; `party_size_cap` is only logged | `game/src/World/WorldConfig.rs` |
| 28 of the 43 directories under `game/src` hold only a comment | `game/src/` |

### 10. The title client patch

Replaced on 2026-11-23 by the client platform (`docs/port/22-client-platform.md`), and removed once
the owner had tested the new client. How it worked when this assessment was made:

- `tools/client/patch-gw.py` applies the connection patch, then always calls `patch_titles`, which
  refuses any file that is not the one stock build 39167 by its SHA-256 (`title_patch.py:99-100`,
  checked). So the whole patcher now works on exactly one `Gw.exe`.
- It appends two sections to the executable, rebuilds the import table to add `KrytaTitles.dll` as a
  hard dependency, and removes the file's signature.
- It overwrites three places at fixed addresses with machine code written as hex strings in Python.
- The DLL fetches `/kryta/custom-titles.json` over plain HTTP on the client's own thread, every three
  seconds while titles are drawn.
- The DLL itself is small and careful. The weakness is how it is loaded and called.

## Progress

Steps 1 to 5 are done and confirmed by the owner's live test. This section says what was done; the
findings above are as they were read on 2026-11-23, before any change.

### Step 1 - done, 2026-11-23

Removed:

- `DatabaseMaps`, the `MapSource` trait and their three database tests (`game/src/Maps/mod.rs`).
  `World::load_maps` takes the pack.
- `DatabaseItems`, its SQL and its seeding (`game/src/DataStores/Items.rs`). The panel's character
  creation was reading item definitions from the empty `item_template` table through it; it now builds
  them from the creation content, as the world does (`tools/manage/src/areas/players.rs`).
- The old size table and its helpers in the login framing (`authserver/src/Server/stream.rs`), and the
  `#[allow(dead_code)]` on the whole `server` module. What the compiler then reported was removed
  (`DatabaseAccounts::pool`, `FileCheck::from_bytes`) or scoped to tests (`count`, `request`, `refusal`,
  `GM_ERROR_AUTH_ERROR`).

Tests repaired:

- `worldserver/tests/end_to_end.rs`: all four tests were failing before any change. They waited for
  an "is loading" log line the program prints only at `Debug`, read maps and items from tables that are
  empty by design, and asserted that `/maps` is not sent (decision 0012 sends it). They now start the
  program with a configuration of their own under `target/` that raises the log level, read the world
  pack, and read the chat answers by their measured size. All four pass.
- `worldserver/tests/fresh_world.rs` asserts the lines the program prints today. It cannot be run on
  this machine: the database role may not create a database, so the test skips.
- `game/src/Server/WorldSession.rs`, `a_travel_record_decides_the_map_and_the_district`: three stale
  expectations (the explorable byte, a tick after the entry position, twice).
- `game/src/Accounts/mod.rs`: the account test set the EULA by a name it had just renamed away.
- `game/src/World/mod.rs`, `every_hosted_map_walks_from_one_of_its_own_spawn_points`: reads the pack,
  and finds the archive by `KRYTA_GW_DAT` like the other archive tests.
- `common/src/Logging/Logger.rs`: three tests asserted nothing; they now assert what their names say.

Kept, each for a recorded reason:

- `shared/src/Packets/ByteBuffer.rs`. Nothing uses it, but it is the reference's byte buffer, and
  `AGENTS.md` names it as part of the structure. Removing it is a deviation that needs the owner's word.
- The `Key = value` configuration parser. `docs/decisions/0005-json-configuration.md` keeps it.
- The pool's synchronous calls. `docs/decisions/0002-worker-pool-threads.md` describes them as part of
  the arrangement.
- The unused functions inside Terrain and Pack. They are whole ports of ASURA's files.

State after the step: the workspace builds with no warnings; 506 tests pass; of the 22 ignored tests,
the 21 that need this machine's databases or archive were run and pass (10 in `game`, 3 in `authserver`,
4 in `worldserver`, 4 in `database`), and 1 skips.

### Step 2 - done, 2026-11-23

The stale and history-telling comments were corrected in 70 files, comment lines only. A comparison of
every source file with a copy taken before the step, ignoring comment lines, shows no code line
changed; the workspace builds with no warnings and the same 506 tests pass.

- Headers that said something was "not here yet" now say what the file holds and who calls it
  (`Terrain/Pathfind.rs`, `Terrain/MapData.rs`, `Pack/Gwdat.rs`, `shared/src/Cryptography/Handshake.rs`,
  both programs' `main.rs`, `game/src/lib.rs`).
- Comments that told the history of a bug now state what the code does.
- Comments on the wrong item were moved to the item they describe.
- "The reference" now means AzerothCore only. Where it meant the C Guild Wars server, the comment
  names GWPS or OpenTyria, the project GWPS was forked from; where the readers could not tell which
  project a comment meant, it was left (about 60 places).
- `game/src/Maps/Portals.rs` claimed a whole port and now declares itself ported thin, listing what is
  not here; `docs/port/09-parity-audit.md` carries the same verdict.
- `campaign` is the client's chapter byte: 0 is no campaign (a PvP character), 1 to 3 are Prophecies,
  Factions and Nightfall. Two comments said 0 was Prophecies.

Found while doing it and not changed, because each is code or text the program prints
(`docs/port/20-open-issues.md`, "Code defects found while reading"):

- `CharacterSheet::missing` gives the fallback character campaign 0, which is PvP; its test calls it
  Prophecies.
- `World.MaxAllowedCorrection` defaults to 250 and cites OpenTyria's constant, which is 100 in the
  copy under `research/OpenTyria`.
- Log and page text that is wrong: "the first administrator" (`authserver/src/main.rs`), "until the
  map_arch table is readable" (`worldserver/src/main.rs`), "needs 16" where the check is 14
  (`MovementHandler.rs`), the `/stuck` help text, and six strings in the panel.

### Step 3 - done, 2026-11-23

The gameplay connection has one message loop and one dispatch (`game/src/Server/WorldSession.rs`).

- `Phase` says where a connection is: the creation screen, a map loading, a map loaded. It replaces
  the two separate loops and the `loaded` flag.
- `serve` is the one loop: it reads and frames messages in every phase, and runs the map's clock and
  the outbox only on a loaded map, as before.
- `dispatch` is the one `match`, on the header and the phase. Every arm is one call into
  `game/src/Handlers/`; a message that arrives in a phase it has no meaning in is reported as
  unanswered, as before.
- The three arms that were written in place are handlers now: `Handlers/InstanceHandler.rs` (the three
  asks of a map load) and `Handlers/TitleHandler.rs` (the title display, with a length check where the
  file's only `unwrap` was).
- `converse` went from 510 lines to 328 and `load_map` from 549 to 179.

One behaviour changed, on purpose: a disconnect message (`0x8008`) ends the connection in every phase,
as ASURA's does (`dispatch.rs:86`). It was handled on the creation screen only.

Left as it was, so the log does not change: inbound trace lines and the "closed" line are still printed
on the creation screen only. One thing is new: a message that sits half-arrived for 900 ms is now
reported on a map too, where before only the creation screen reported it.

### Step 4 - done, 2026-11-23

Each program's `run` is a list of named stages (`authserver/src/main.rs` from 756 lines to 112,
`worldserver/src/main.rs` from 764 to 66). The stages are functions in the same file, in the order the
code ran before.

- The block that read one setting and reported its notes, written 17 and 21 times, is one helper per
  value type in `common/src/Configuration/Startup.rs`, with `report`, the `--config-policy` block and
  the usage text, which were the same in both programs.
- `Options` and its parser, the logging, PID and READY stages stay one per program: each carries its
  program's name or log category, and the reference keeps a `Main.cpp` per program too.
- Removed from the world's database stage, shown unreachable by reading: an `else` branch no database
  kind reached, the list it filled, and the warning for a world database that was never absent.

The login program prints the same start-up lines as before, line for line. The world program prints
the same lines, with two adjacent ones - the `CaptureFile` warning and the "Chat commands" line -
sometimes in the other order from one run of the same binary to the next: one is printed directly and
the other through the log's worker.

State after steps 3 and 4: the workspace builds with no warnings, the 506 tests pass, and the 21
database and end-to-end tests pass, run one program at a time.

### Step 5 - done, 2026-11-23

The gameplay connection is two things now (`game/src/Server/WorldSession.rs`):

- `WorldSession`, six fields that are fixed for the connection's life: the log, the world, the two
  database pools it was given, a master secret a test supplies, and the closing flag.
- `Connection`, ten fields of plain data with no lock: the account, the travel-in-flight flag, the
  character, the landing, the last portal step, the district entered, the agent, the outbox, the player
  id, and the count of unanswered messages. `run` creates it and hands it to the loop, the dispatch
  and the handlers.

What went:

- The ten separate locks, and with them the places where a failed lock quietly made a player "player
  1" or skipped a write. The thirteen locks that remain in the session and the handlers are on state
  that really is shared between connections: the districts and the peer registry.
- The copies of the world each connection carried - maps, spawn points, arrivals, commands, settings,
  item definitions, creation tables. A connection reads them from the world.
- Six of the seven constructors. There is `WorldSession::new` and three builders: `with_characters`,
  `with_login`, `with_master`.

Before a lock was removed, each field was checked for use outside the connection's own task; none was.

State after the step: the workspace builds with no warnings, the 506 tests pass, and the 21 database
and end-to-end tests pass, run one program at a time. No test's expected bytes, log text or rows
changed in steps 3 to 5; test set-up changed where a constructor did.

### Not done in these five steps

- The findings in sections 4 and 10 (titles and the title client patch) are the next phase
  (`docs/port/22-client-platform.md`).
- Section 8 (the asynchronous code waiting on the database) and the table in section 9 are untouched,
  except where a step happened to remove an item: the title `unwrap`, the seven constructors, the
  item-template duplicate.
- `World` is still the bag section 9 describes, and the three names for the same opcodes remain.

### Confirmed, 2026-11-23

The owner ran the five steps against a real client and the live tests succeeded. Steps 1 to 5 are
confirmed. Step 6, titles, is the next phase; how the client side of it is to be handled changed, and
is `docs/port/22-client-platform.md`.

## What this means for changing the code

The order the findings suggest, cheapest and safest first:

1. Remove what is dead, and fix the tests that test nothing, so the code and its tests say what is true.
2. Correct the stale comments.
3. Give the gameplay connection one message loop and move the inline arms into handlers.
4. Split the two start-up functions into named stages and share what is copied.
5. Turn the session's state into one plain structure and stop copying the world into it.
6. Rework titles: definitions in the files, values per character, and the client side on the DLL
   platform of `docs/port/22-client-platform.md` (the client side is done and confirmed; the server
   side is `docs/port/20-open-issues.md` items 10a and 10b).

Each step leaves what the client sees unchanged, and the packet tests and the manual client test are
what would show it.
