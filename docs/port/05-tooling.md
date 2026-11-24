# The tools: what this tree owns, and what it still borrows

A server is not the only thing this project runs. There is a client to patch so it talks to us instead
of ArenaNet, a machine setting that decides whether it reaches us at all, and captures that say what
the client sent when a log is not enough. Those are tools, they are part of the deliverable, and they
live in this tree.

## The rule

**No tool reads another project's tree at run time.** A tool here takes its input from this repository
or from the machine, and a fact it needs from outside is imported once, as data, with the import named.
A port that reaches into `C:\Users\Apo\Documents\GitHub\ASURA` when it runs is not ported yet - it is
borrowed, and the borrow is the thing that breaks when that tree moves.

Two consequences:

- An imported row set is data, not a link: `config/World/Vanilla/creation/inventory.json` and `config/World/Vanilla/creation/equipment.json` are this
  tree's files, and `tools/import-creation-inventory.py` is the record of where they came from. Running
  it again is provenance, not a dependency - nothing in `cargo test` or either program calls it.
- A tool that must depend on something external says so at the top of its own file, and says what
  happens without it.

No Rust code in this tree names ASURA's tree or `research/` at run time; the mentions are citations in
comments. What remains: `tools/client/sniff-capture.py` still honours the `ASURA_PY32` variable beside
`KRYTA_PY32`, `tools/import-creation-inventory.py` defaults to `C:\Users\Apo\Documents\GitHub\ASURA`
(the provenance run), and the `doc` of most rows in `config/World/Vanilla/vanilla-title-wire.json` (37 of
45 tracks, 82 of 98 ranks) cites a capture under `research/`, which is gitignored and so is not in a
clone.

## What is here now

| tool | is | needs | ported from |
|---|---|---|---|
| `tools/client/patch-gw.py` | builds `Gw.custom.exe`: the connection patches and one import-table entry that makes the client load `Kryta.dll` by itself (`tools/client/dll_patch.py`); builds the DLL with cargo and installs it beside the client. Works on any client build its two connection patterns fit | `pefile`; Rust with the `i686-pc-windows-msvc` target (`rust-toolchain.toml` installs it), or a built DLL with `--dll` | ASURA `client-patcher/patch-gw.py` for the connection; [the client platform](22-client-platform.md) for the DLL |
| `tools/client/dll_patch.py` | adds `Kryta.dll` to a client's import table | `pefile` | this tree's own |
| `tools/client-dll/` | `Kryta.dll`, a Rust crate (`kryta-client`): what Kryta adds to the client, found by byte pattern and hooked while the client runs | built for `i686-pc-windows-msvc` | Py4GW Reforged Native for the scanner, the offset resolver and the hook layer |
| `tools/client-link/` | the messages of the link between `Kryta.dll` and the server, used by both ends (`kryta-client-link`) | - | this tree's own |
| `tools/client/keys.txt` | the key material the patch writes: `generator`, `prime`, `server_private`, `server_public` | - | ASURA `client-patcher/keys.txt`; the prime and `server_private` are the same values as `shared/src/Cryptography/Handshake.rs` |
| `tools/client/hosts-redirect.py` | sends the client's seven baked-in names to this machine, or puts them back; recognises a block an earlier tool left | an elevated shell to write the hosts file | ASURA `client-patcher/hosts-redirect.py`, itself from GWPS `tools/hosts-gwps.ps1` |
| `tools/adapt-protocol-table.py` | moved the ported message table into `game/src/Server/Protocol/Tables.rs`, once | nothing; it is the record of the move | ASURA `crates/asura-protocol/src/packets.rs` |
| `tools/import-creation-inventory.py` | imported the bags, items and profession sets into `config/World/Vanilla/creation/inventory.json` and `equipment.json`, once | a checkout of ASURA to read | ASURA `state/inventory/seed.rs` and `default_equipment.rs` |
| `tools/split-protocol-table.py` | split the merged protocol table into `game/src/Server/Protocol/Tables.rs` (682 world rows) and `authserver/src/Protocol/Tables.rs` (100 login rows), once | nothing; it is the record of the move | this tree's own move |
| `tools/extract-protocol-types.py` | moved the table's types into `shared/src/Packets/Types.rs`, once, after the split | nothing; it is the record of the move | this tree's own move |
| `tools/repoint-table-names.py` | points each split table's name module at `shared::packets::types`; idempotent | nothing | this tree's own move |
| `tools/recover-game-rules.py` | recovered the owner's ledger into `docs/game-rules.md`, as ASCII, from ASURA's history | a checkout of ASURA with its history | ASURA `git show f7c2f31:docs/36-game-rules-ledger.md` |
| `tools/client/sniff-capture.py` | captures the live client's packets into a JSONL file (see "Capturing a client" below) | Py4GW_Stealth and a 32-bit Python; an elevated shell | ASURA `tools/sniff-capture.py` |
| `tools/client/capture-fresh-character.ps1` | elevates, finds the one retail `Gw.exe`, and runs `sniff-capture.py --raw` into `recordings/client/raw/fresh-character-<stamp>.jsonl` while a character is created | 32-bit Python 3.13 under `%LOCALAPPDATA%`, Py4GW_Stealth (`PY4GW_STEALTH` or `%USERPROFILE%\Py4GW_Stealth`) | this tree's own |
| `tools/capture/read.py` | reads a capture back in the layer it declares - `--describe`, `--header`, `--limit`, `--assume-layer`, `--self-test` - and refuses a file that states no layer | nothing; it is Python 3 and standard library only | ASURA `tools/capture-read.py` |
| `tools/capture/diff.py` | compares two captures message by message: agreement count, then the first divergences with both payloads; refuses a struct-layer file against a wire-layer one, because the two widen fields differently | nothing; standard library only | new; the comparison workflow `0012-chat-text-is-sent.md` used by hand |
| `tools/capture/capture_file.py` | the one implementation of the layer rules both of the above use | nothing | ASURA `tools/capture-read.py`'s layer half |
| `worldserver --bin kryta-cli` | the standalone operator client: `list` asks the world which operations it registers, `<operation> [--arg name=value]...` calls one, `help` prints the usage, and `init-credential <file>` creates and restricts the service credential. The port and credential come from `Control.Port`/`Control.CredentialFile` in `worldserver/worldserver.json` (another file with `-c`), or from `--port` and `--credential-file`, so no plumbing is typed per call | a running world with `Control.Enable` and a credential file | new; the client ASURA's supervisor verbs are the precedent for |

## Capturing the server

`worldserver`'s `CaptureFile` names where to record the wire: every game message, both directions, as
plaintext JSONL with two-byte headers, after ARC4 on the way in and before it on the way out
(`game/Server/Capture.rs`, ported from ASURA `crates/asura-world/src/capture.rs`). Empty is off. The
record is `{seq, t, direction, header, size, data}` after a header record that states the layer, so a
reader never has to guess; the writer flushes after every batch it sends and every 200 ms otherwise, and
flushes when a connection ends and when the program stops, because the last frames of a session are what
a capture is for.

```powershell
# in worldserver.json: "CaptureFile": { "value": "logs/session.jsonl" }
python tools/capture/read.py logs/session.jsonl --describe
python tools/capture/read.py logs/session.jsonl --header 0x005D
```

The path is the operator's: `logs/` is a scratch folder Git ignores. `recordings/asura/` holds ASURA's
own captures, and no folder under `recordings/` is assigned to this server's yet
(`recordings/README.md`).

Proved against a live session on this machine: the end-to-end test's world, run with
`AC_CAPTURE_FILE` set, wrote 564 records of a real creation-and-load conversation; the reader described
the file (`tool: worldserver`, `layer: wire`, `header_len: 2`) and printed the load head `0x017C`, the
inventory stream `0x0161`, the chat body `0x005D` and its tag `0x0061`. **That last pair is also how a
fault was read rather than guessed**: the body's declared unit count is `0800` (8),
`docs/decisions/0012-chat-text-is-sent.md` records the measured bytes as `5d00 0800 0108 0107 ...`, and
the assertion in `worldserver/tests/end_to_end.rs:960-963` expects the same 8.

`tools/capture/diff.py` was proved on the same pair: a capture against itself agrees on every one of its
564 messages and answers zero; two runs of the same test agree on 551 of 564 and the first three
divergences are named with both payloads - a fresh character's guid in `0x0188`, a scattered spawn point
in `0x0196`, and a counter in `0x001F` that read 6 and 7. A pair whose layers differ is refused (exit 2)
rather than compared, because a struct record and a wire record are not the same bytes.

## The supervisor's channel and both exit codes

The channel is tested over a real loopback socket, not only by hand: `Control.rs` binds port 0, connects
a client, sends `stop world` and answers it the way the supervisor's loop does, then reads `status` and
gets one line per service, then a line that is not a verb and gets an error. `cargo test -p
kryta-supervisor` is the command.

Both exit codes of the reference's contract (`Main.cpp:434-439`) were observed on this machine:

- **`2` restarts the service.** `kryta-cli ... world.restart` answered "Restarting: this process stops
  and the supervisor starts it again"; the supervisor logged `asked to restart (exit 2), request 1 of 5`,
  the world's process id changed, the stack stayed up with one world in it, and the new world answered
  `world.status` again.
- **`0` stops the stack.** `kryta-cli ... world.exit` answered "Stopping the world"; the supervisor
  logged `world | exited with exit code: 0; stopping the rest`, stopped `auth` and `manage`, and left no
  process behind. That is the rule this file states: a service that ends on its own stops the rest.
  The first service that makes that terminal decision supplies the stack's exit code. Remaining children
  are then stopped as consequences; on Windows their control-event status (for example
  `0xc000013a`) cannot replace the initiating service's normal `0`.

The clients' own codes were checked too: `kryta-supervisor status` answers 1 when nothing is listening,
`--help` answers 0, and `kryta-cli` answers 1 with the reason when the world is not there.

## One command surface, one registry

`config/Server/commands.json` is the whole command surface: every row names a registered operation and the values
it takes, and there is no enum of commands in Rust and no match on one. `game/src/Chat/Operations.rs`
holds the registry, `chat.transfer`, `chat.maps` and `chat.stuck` are registrations like `world.status`,
and a row whose operation the world does not register - or whose parameter count the operation does not
declare - is refused when the file is read. The chat line, the console and the control endpoint all
resolve the same rows and call the same function; a function answers with `Text`, `Frames` or a
`Transfer` and the surface performs it, so one implementation serves every door.

Proved on this machine: the same end-to-end session was captured before and after the chat line moved
onto the registry, and `python tools/capture/read.py <capture> --header 0x005D` plus `--header 0x0061`
answers the identical two records both times (`5d00 0800 0108 0107 hello 0001`, then `6100 0100 03`).
The console still answers `server info`, and the API refuses `chat.stuck` with "needs a player session",
which is the outcome mapping working over the wire.

**Accepted on a live client after that move**: the owner connected `Gw.custom.exe` to this tree's world,
typed a line in chat and saw it rendered, and typed `/maps` and saw a list of maps. The line's frames are
`5d 00 08 00 08 01 07 01 h e l l o 01 00` and `61 00 01 00 03` in the capture, which is what
`docs/decisions/0012-chat-text-is-sent.md` records as measured - the body's count is eight because it
counts the two wrapper units, the text and the terminator.

The recovered control-panel and research requirements are in `docs/port/12-research-and-control-panel.md`.
The research and plan for the operator's surfaces - the command line, a console over the same command
table in-game chat uses, and a control console over recordings, analysis and maps - are in
`docs/port/14-operator-surfaces.md`, which also surveys research projects. Eden is a potential
patchable client for debugging and automated research; its proposed role is recorded in
`docs/port/12-research-and-control-panel.md`.

## The administration tool

```powershell
copy tools\manage\manage.json.dist tools\manage\manage.json   # then state the three database strings
cargo run -p kryta-management-service -- -c tools\manage\manage.json  # http://127.0.0.1:6602/
```

`tools/manage/` is the Configuration Manager: the operator's web front door. It is not a game server -
no client talks to it, it serves no game protocol, and neither server program knows it exists
(`docs/decisions/0008-administration-tool.md`). It links the same libraries the servers link and reaches
existing data through the nouns' own repositories, so what a page writes is what a server reads. Its
scope and ordered delivery record are canonical in `14-operator-surfaces.md`.

The panel is layered, and each layer knows only the one below it. `web/` holds the shell every page is
drawn into, the panel's stylesheet and the components a page is built from; `view.rs` holds the game's own
names for the numbers a row stores and the row shapes a page lists; `pages/` holds one module per
destination; and `lib.rs` holds one routing table plus the two owners the panel asks - the supervisor for
process lifecycle, the running world for operations. Its destinations follow the game's ownership, not
the database layout: the server hosts Accounts; an account owns its Players (the characters); and a
player's page owns its own data - campaign and professions, level and stored experience, carried gold,
position, inventory and equipment (the item rows), titles, and the maps it has unlocked. Accounts and
Players are separate rail categories because account-owned state and player-owned state are different
owners; the account page lists the players it owns and its Players tab opens them.

Three rules keep the surface trustworthy. A page is served before any other process is asked: the
supervisor's status, the world's registered operations and the world's own status are probed by the page
after it has answered, so a missing or slow owner never delays a page. A destination that does not work
is not in the rail - there is no placeholder page pretending to be a roadmap step. And every write
answers a `303` whose `Location` header the browser follows, so a form's outcome is the page it names.
The stylesheet is one asset (`GET /static/style.css`) served from the program's own memory; there is no
JavaScript except the three probes, and the pages work without it.

`tools/supervisor/` is `kryta-supervisor`, and it is the one command that runs the stack: it reads
`tools/supervisor/stack.json` - a service per row, with the program, its arguments and the port it
binds, plus `control_port` and `console_service` - starts every service, waits for its READY handshake,
and stops them all together on Ctrl+C. A service that dies on its own stops the rest and the command
answers that service's exit code. The arrangement is ASURA's `asura-supervisor` (this tree's programs
are several, and one command that runs them is what makes a stack a stack); the *list* is data, so a
second world or a changed port is an edit in that file, not a program change.

**Its console is the stack's, not the services'.** A service's standard output is never shown: the
supervisor reads it for the handshake, keeps a thirty-line tail for a failure report, and prints only
its own lifecycle lines and a service's standard error. `--logs` mirrors the output when that is what is
wanted. The services themselves log where their appenders say - `Server.log`, `Auth.log` and
`Errors.log` - and `03-tracing.md`, "What reaches a screen", is the rule. The `READY: listening on ...`
handshake is printed to standard output only when `KRYTA_STACK` is set, which this program sets; it is
consumed here and never shown.

`console_service` is empty, and that is deliberate: with a service's output hidden, an embedded console
would answer into nothing. Run a program directly when you want its console, and use `kryta-cli` (or
`kryta-supervisor status`) for one that is running under the stack. Set the key to a service name only
if that service's output will not be hidden - whose only current reader is a terminal run of the
supervisor with `--logs`.

It owns the control channel (`tools/supervisor/src/Control.rs`), a loopback line protocol with
`status`, `stop [service]`, `restart <service>` and `quit`, so a running stack can be asked what it is
doing and told to restart one thing without a second owner of its processes. And it reads the
reference's exit-code contract (`Main.cpp:434-439`): a world that exits `2` asked to be restarted, and
this program - the restarter that comment names - starts it again, up to five times before stopping the
stack. Health checks beyond the handshake are still not done, and the header says so.

**A port belongs to the stack.** `stack.json` names a port for each service, and before a service
starts the supervisor claims it (`take_port`). If a listener is already there, it asks the operating
system which process owns it. A process running the same program as the service is an instance left
over from an earlier session: the supervisor stops it, says so, and starts the service on its own
port - a stale copy of one of the stack's own services can no longer take the stack down. A program
this stack does not run is never killed: the supervisor names it and refuses to start, so a stranger
holding one of the stack's ports is one clear line instead of a half-up stack. The same claim runs on
every start, including `restart <service>` from the control channel and the exit-code-2 restarter, and
it covers the supervisor's own control port (`take_control_port`), so starting a second stack replaces
the first instead of half-existing beside it. Proved on this machine by leaving a whole stack's
services behind a killed supervisor: the next `--check` reported each port's earlier owner, stopped it,
and the stack came up; with a `powershell.exe` holding 6602 the supervisor refused, exited non-zero,
and left the stranger alive.

```powershell
cargo run -p kryta-supervisor                # the stack: every service, stopped together on Ctrl+C
cargo run -p kryta-supervisor -- --check     # start, wait for every READY, stop, report
cargo run -p kryta-supervisor -- status          # ask the running stack what it is doing
cargo run -p kryta-supervisor -- restart world   # start one service again
cargo run -p kryta-supervisor -- quit            # stop every service and exit
```

The binaries it starts are `target/debug/<program>.exe`; every one is built by the supervisor itself
before it starts (`cargo build -p <program>`, with cargo's own output on the console), so the stack runs
the code in the tree and the single command is enough from a fresh checkout, and a program that fails
to build says so with cargo's reason rather than starting nothing.

| page | is | state |
|---|---|---|
| `/` | the overview: the counts, the live world's own answer (probed after the page), and the working areas | done - unreachable data is named, not shown as zero |
| `/accounts` | the account directory - the login identities the server hosts - and a new account's form | done - an account is the login identity; it owns its players and the state that belongs to the person |
| `/accounts/{id}` | one account: identity, EULA state, activation and removal, the players it owns, with its section tabs (Account overview, Players, and the two account-wide areas) | done - Shared storage and Account unlocks are named as not recorded by this server yet; the Players tab opens `/players?account={id}` |
| `/players` | every player (character) the server hosts, with the account that owns it, and the creation form whose account list says who will own the player | done - a player is created from the creation record lists and the pack's campaign spawn data, then its own page opens |
| `/players?account={id}` | the same list, filtered to one account's players | done - the account page's Players tab and the creation form's account choice both use it |
| `/players/{uuid}` | one player: campaign, profession pair, level, stored experience, gold, PvP, last outpost, position, appearance and the professions it has unlocked; its inventory and equipment (the item rows); the metric catalog, all values zero, and the 47 title rows the pack's `titles.json` names - its 45 vanilla tracks and the two custom ones, `Hog Jumper` (from `jumps`) and `Embiggener`; its unlocked maps (R12); the account that owns it (linked); rename, level (1 to 20, R48) and removal | done - no metric is recorded yet, but every title row is visible for the later source to fill; removal deletes the row with its own items and unlocks |
| `/services` | the running supervisor's state and its named stop/restart controls | done against the existing loopback channel: `SupervisorStackFile` supplies its `control_port`; the manager cannot start a missing supervisor because it has no start protocol and must not become one |
| `/console` | browser adapter for the live operation registry | done against the existing world endpoint: lists `world.operations` (asked after the page), submits named `name=value` arguments to `/v1/operations`, and displays the world's result or refusal; it does not dispatch commands itself |
| `/status` | the running world's own answer to `world.status`, asked of its local control endpoint | done - one operation carried by the world's own queue (`docs/port/14-operator-surfaces.md`); a world that is unavailable or refuses is drawn differently from an empty answer, and `ControlPort`/`ControlCredentialFile` in `manage.json` configure it |
| `/worlds` | the realm list and `data/realms.json` merged over it | done - read only, from the login's own reader |
| `/content` | the world pack's maps with their entry points and the arrival points they state (R13/R21), the item definitions and profession sets a creation screen draws from, and the copies the world has open | done - read only for now: content is the pack (`docs/decisions/0014-the-world-content-is-a-pack.md`), read through the same reader the world uses; editing it is roadmap step 6 and is not pretended |
| `/navigation` | the extracted path-graph cache: inspection, and one selected map dumped in from `MapArchiveFile` | done - the cache is an operator asset, not state the running world owns |
| `/schema` | each database's own record of what its updater has applied, against the files on disk | done - read only; `changed` is the one to act on before a server refuses to start |
| `/settings` | each program's own JSON, key by key, as the file states it | done - read only, passwords masked by the reader; `WorldConfigFile` and `AuthConfigFile` in `manage.json` say where the files are |
| `/accounts?account=`, `/characters?account=`, `/players/{id}`, `/players/{id}/characters` | the panel's old addresses | redirect (`303` with its `Location`) into the hierarchy - the account by row id, the player by client id - so bookmarks keep working |

Proved on this machine against its own databases. Every page answered `200` in 10-110 ms, and the
stylesheet (`GET /static/style.css`, 10.8 KB) is served once instead of being repeated inside every page.
The hierarchy was exercised end to end: `POST /accounts/create` wrote an account and answered `303` to
`/accounts/9`; `POST /players/create` created its player from the creation record lists and the pack's
campaign spawn and answered `303` to `/players/{uuid}`; the player page read it back (level 1,
Prophecies, Ranger, experience 0, Ascalon City (pre-Searing), its inventory rows and unlocked maps);
`POST /players/{uuid}/level` set 4 and `.../rename` renamed it; `.../delete` removed the player with its
items and unlocks; and `POST /accounts/9/delete` removed the account - each answer a `303` with the
`Location` a browser follows. The accounts directory listed each account with its player count; the
account page linked every player it owns and its Players tab the filtered list; the players list showed
each player with the account that owns it. The content page listed the pack's maps (Random Arenas,
Heroes Ascent, ...) with their entry points and stated arrivals, and the item definitions and profession
sets - the world database's content tables are empty by design (`0013-content-is-the-pack.sql`), so the
page reads the pack itself. The probes answered without delaying their pages: `/services/status`,
`/console/operations` and `/status/data` reported that their owners were not running.

`/status` was proved against a live world on this machine, three ways. With the world running and
`Control.Enable` on, `kryta-cli --port 7878 --credential-file worldserver/control.credential world.status` answered
`World 2: 265 map(s) loaded.` and the same operation through `kryta-cli` with a wrong 32-character
credential answered `authentication failed` with exit 1; `GET /status` on the manager rendered
`World 2: 265 map(s) loaded.` from the world's own queue; and with the world stopped the same page
answered "The running world did not answer. This is not an empty result" naming the refused connection.
`kryta-supervisor --check` then started `auth`, `world` and `manage` together and reported
`the stack is up: auth on 6112, world on 6113, manage on 6602`, so the manager is a stack service and
not a program started by hand.

`kryta-cli` was proved against a live world with the settings doing the plumbing: `kryta-cli list` printed
all eight registrations with their help and their values (`ListTargets`, `chat.maps`, `chat.stuck`,
`chat.transfer destination`, `world.exit`, `world.operations`, `world.restart`, `world.status`);
`kryta-cli world.status` answered `World 2: 265 map(s) loaded.`; `kryta-cli chat.stuck` refused with
`needs a player session ... (refused)`; and `kryta-cli world.status --arg x=y` refused with
`argument "x" is not one this operation takes (invalid_arguments)`. All four read the port and the
credential from `worldserver/worldserver.json`, and the client names no command of its own - `list` asks
the world for `world.operations`, whose answer is the registry itself.
The endpoint's contract was then exercised live, one raw request per case: `world.status` answered
`{"ok":true,"result":"World 2: 265 map(s) loaded."}`; an unregistered name answered 404 with
`"code":"unknown_operation"`; an argument the operation does not take answered 400 with
`"code":"invalid_arguments"` and the message naming the argument; and a request with no credential
answered 401 with `"code":"authentication_failed"`. `cargo test -p worldserver` pins the same cases
plus a full queue answering `unavailable` and a world that never answers answering `unknown_completion`,
and `cargo test -p game` pins the registry: a duplicate name refused, an unregistered name refused by
code, a value count checked against the declaration, and the caller reaching the operation.

The EULA itself is a column now (`data/sql/base/db_auth/account_eula.sql`): the version an account
accepted, or 0 for none. The auth program's answer reports it, so a client that has accepted stops asking.

## Building and running the client

```powershell
pip install pefile                                                                    # once per machine
python tools/client/patch-gw.py "F:\GW\GW1\Gw.exe" tools/client/keys.txt
python tools/client/hosts-redirect.py status
& 'F:\GW\GW1\Gw.custom.exe' -authsrv 127.0.0.1 -portal 127.0.0.1
```

The patch does two things: it redirects the client to this server, and it adds `Kryta.dll` to the
client's import table, so the client loads the DLL by itself when it starts. The patcher then builds
the DLL (`cargo build -p kryta-client --release --target i686-pc-windows-msvc`) and writes it beside
`Gw.custom.exe`. `--dll` takes a DLL that is already built. The patcher does not check which client
build it is given.

What the DLL does, and what it needs:

- When the client starts, the DLL connects to the login program's client link - the `-portal` host,
  port 6603 (`ClientLink.Bind` in `authserver.json`). A file `Kryta.json` beside the client,
  `{ "link": "host:port" }`, names another address.
- The server sends it the offsets under `config/Client/`; it finds the client's title functions by
  them, installs its hooks, and is sent the custom title text.
- **The stack must be running before the client is started.** A client that cannot reach the link,
  or whose build the offsets do not fit, shows a message that says why and ends.
- It writes `Kryta.<pid>.log` beside the client: the build it is in, every offset, every hook, the first
  few times a title hook answers the client, and any fault inside the client with where it happened.

Edit the world pack's `titles.json` and restart the stack to change the title text. After a client
update, see `config/Client/README.md`: the offsets are corrected on the server and nothing is
rebuilt or redistributed.

`--retire-title-probes` archives the old title test executables beside the client.

The hosts block is **off by default and that is the normal state**: with the patch above the client
dials the address on its command line, so nothing needs the block. `enable --file-server` is for a
client built with `--detach`, which repoints the client's own DNS base name and therefore needs both
the block and a content server.

## Capturing a client

```powershell
# start an elevated PowerShell first: py4gw.connect() is a write, and refuses without it
python tools/client/sniff-capture.py --seconds 90 --dir both --out recordings/client/feature/<subject>/<name>.jsonl
python tools/client/sniff-capture.py --self-test        # needs neither a client nor elevation
```

Proved: the self-test passes from this tree (masking, record writing, the capture header), and the
copy carries the same records ASURA's did.

The capture tooling stays, in Python: many more recordings will be made (owner, 2026-11-23).

**Py4GW_Stealth is a declared dependency of this tool and of nothing else here.** It is a separate
project that injects into the client (`--py4gw-root`, or `PY4GW_STEALTH`); no program in this
repository reads from it. A record is the client's message *struct*, not the packed frame -
`docs/port/02-wire.md`, "Three kinds of capture", says which one a file holds, and so does the `layer`
field in the header of a capture this tool writes now; the older committed captures have no such field.

## Evidence

`recordings/` and `docs/evidence/` hold the measurements this tree cites, copied here so a citation
points at something in the repository. `docs/evidence/README.md` says what each file is, where it came
from, and that a document there is evidence rather than a rule.

## What is still borrowed

| borrowed | used for | status |
|---|---|---|
| `ASURA/crates/asura-protocol`, `crates/asura-world` | read for the protocol: the message table, the builders, the creation handlers | not a dependency: read, ported into `game/src/Server/Protocol/`, `Packets/` and `Creation/`, cited where a fact came from, and never called at run time |
| `ASURA/tools/*` beyond those listed above | their own milestones: test batteries, seed scripts, table generators | not used here; a tool this tree needs is ported before it is used |

## Where a tool goes

`tools/` is one directory per subject: `tools/client/` is everything about the client the server talks
to, `tools/capture/` reads captures, and `tools/manage/`, `tools/management-service/` and
`tools/supervisor/` are the three programs. The scripts at its top are one-time records of an import or
a move (`import-*`, `adapt-*`, `split-*`, `extract-*`, `repoint-*`, `recover-*`), not tools to run again.
A new tool states, at the top of its own file, what it reads, what it writes, and what it needs that
this tree does not have.
