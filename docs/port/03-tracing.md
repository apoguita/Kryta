# What the server tells you, and how to switch on the rest

A statement of today. Every line here is something the running programs print or can be made to
print; nothing here is a plan.

The rule behind all of it: a server that cannot say whether anything reached it, or what it did with
what reached it, cannot be tested by anyone - not by the person running it, and not by an operator
reading its log afterwards.

## Always on

The startup reports each stage as it happens, and then says what state it is in:

```text
authserver: config authserver\authserver.json (18 keys), would bind 0.0.0.0:6112
> Using configuration file       authserver\authserver.json
Kryta authserver 0.1.0
Opening DatabasePool 'kryta_login'. Asynchronous connections: 2, synchronous connections: 1.
Login database: 0 file(s) applied, 2 already current.
Started auth database connection pool.
World list: 1 world(s), refreshed every 20 s.
File check: 21 bytes of question, 32 of answer.
listening on 0.0.0.0:6112 with 1 network thread(s)
READY: listening on 0.0.0.0:6112; status reported every 5 s.
```

Then, while it runs:

| line | when |
|---|---|
| `accepted connection #N from ADDRESS` | every connection, the moment it is accepted, before any protocol sees it |
| `Connection from ADDRESS: auth version N` | the client said what build it is |
| `Connection from ADDRESS: file check answered with 32 bytes` | the client's file check was answered |
| `Connection from ADDRESS: the plaintext half of the exchange is answered` | the seed exchange completed |
| `Connection from ADDRESS: greeted with the session info and a heartbeat, N bytes` | the two messages the reference sends before the client asks for anything |
| `Connection from ADDRESS: login request N, user UUID session UUID character '...' type '...'` | the client's first scrambled message, read back as the portal login |
| `Connection from ADDRESS: the client chose the character '...', and this server answers yes.` | `0x800A`: the click on the list, and the character a following travel is placed by |
| `Connection from ADDRESS: request N for map M (type T, region R, district D, language L) - the client wants a world.` | `0x8029`, in the client's own numbers |
| `Connection from ADDRESS: the client named the creation instance as a place; map M is where it is sent instead, from ...` | the placement rule moved the destination to where the character stands - not a fault, and it names the row it used |
| `Connection from ADDRESS: neither the character's row nor its last outpost names a map, so it is sent to the fallback outpost M (FallbackOutpost).` | a character that has never been anywhere: the setting decided, and the line says so |
| `Connection from ADDRESS: the travel to map M (district D, language L) of account ... is recorded for world W; the world claims it when the connection presents token 0x...` | the hand-off row the world reads; M is the placed map, not the client's |
| `status: up N s, listening on ADDRESS:PORT, N connection(s) accepted, L live` | every `StatusInterval` seconds; `live` is what is being served now |
| `Portal: listening on ADDRESS:PORT.` | when the portal is enabled, on the address it bound |
| `ADDRESS: GET /Spawned/WebGate/session/create.xml -> 200 (N bytes)` | one line per portal request, with its status |
| `Stopped. N connection(s) were accepted while it ran.` | on shutdown |

A connection that says nothing produces `accepted connection` and then nothing else. That pair -
accepted, silent - is the difference between "the client reached us and gave up" and "the client
never reached us", which is the question that could not be answered before.

## Optional traces

There are three, each one key, and each needs its own logger - the bytes, the statements and the keys
are written at debug, and a switch that silently does nothing is worse than no switch. If one is on and
its logger is not, the program says so and names the line to add. The code's default for each key is
off, and `worldserver.json.dist` ships all three off; `authserver.json.dist` ships all three **on**,
with their loggers, so a login run from the shipped file traces by default.

| what | key | logger to add, under `Logger` | what it prints |
|---|---|---|---|
| every byte of every connection | `"Network": { "Trace": true }` | `"network.trace": [6, "Console Auth"]` | `trace ADDRESS in  4 bytes: 00 04 0C 00` and the outgoing side, plus a close line with the totals |
| every database statement | `"Database": { "Trace": true }` | `"sql.trace": [6, "Console Auth"]` | `query [database] SELECT ...` and `rows [database] 1`, `batch`, `execute`, `affected` |
| every key the configuration held | `"Log": { "EffectiveConfig": true }` | `"server.loading": [6, "Console Auth"]` | `> Config: LoginDatabaseInfo = 127.0.0.1;5432;kryta;********;kryta_login` - a connection string's password is masked, because a log line gets pasted into issue reports |

**A routine reply is a `Trace` line, not a `Debug` one.** A connection's events - a load, a chat line, a
transfer, a refusal - are `Debug` lines (the next section), and the world's own heartbeat is `Info`. Two
kinds of frame are not events and are written at `Trace` (level 6): below the console appender's `Info`
(level 4) and below the default log file's `Debug` (level 5), so neither prints them:

| routine line | why it is not `Info` |
|---|---|
| `... a tick answered with N message(s), M bytes.` | the simulation tick goes out every `World.SimulationTickMs` to every loaded connection; at `Info` that was the whole log. The tick's presence is read from its effect, as the table above says, not from a line per frame |
| `... the walk ...`, `... the walk the report asked for ...`, `... the stop ...`, `... the stopping point ...` | the acknowledgement of a movement report; a walking player sends one every step |
| `... message X ... again - sighting N; ...` | a client retrying a message this world does not answer. Its **first** sighting is an `Info` line, once per header per connection (`WorldSession::report_unanswered`), because the line reports a gap to fill; the repeats are the count |

To see them, give both the world logger and the appender that should carry them level `6` (`Trace`) in
`worldserver.json`, for example `"server.worldserver": [6, "Console"]` under `Logger` and level `6` in
`Appender.Console`, and they appear beside everything else. Nothing
else changes: the capture (`CaptureFile`, `05-tooling.md`) records every frame regardless of the log
level, so a session that needs the frames asks the capture, not the log.

The byte trace is the one that turns an unexplained client into evidence: whatever a client sends,
including traffic this server does not understand, appears byte for byte with its direction. The
statement trace is the same idea for the database: a startup that applied the wrong file looks like
a startup that worked until the statements are visible.

## Which of these are `Info`, and why that is the rule

**A line about one connection is a `Debug` line.** At a thousand players, `Info` has to mean what an
operator can read: the server's own state and its problems. So the lines below that describe what one
connection did - a load, an item move, a chat line, a travel, a refusal, the entry position, the ping -
are written at `Debug`, on the `server.worldserver` logger. A line passes the logger's level before any
appender sees it, and `worldserver.json.dist` sets `Logger.server` - which `server.worldserver`
inherits - to `4` (`Info`), so with the shipped file neither the console nor `Server.log` (level 5,
`Debug`) receives them; and the stack's console shows nothing either way, because it is not a log
viewer.

What stays at `Info` is the world's own state: the startup stages, `READY: listening on`, the status
heartbeat, the world-list refresh, an instance disposed, the creation tables, and every `Warn` or
`Error`. Those are the lines a person watches a server with, and there are a handful per minute, not per
connection.

To keep one connection's detail in `Server.log`, raise the logger to `5` (`Debug`) - or `6` for the
traces in the table above - in `worldserver.json`'s `Logger` (`"server.worldserver": [5, "Console
Server"]`); to watch it live as well, raise `Appender.Console` to the same level.

## What reaches a screen

Fatal startup errors go to the console. Routine logging follows the appender settings.

- A program writes where its appenders say. `Appender.Console` is at `Info` (level 4), so a program run
  by hand prints its own state and its problems and not its traffic; `Appender.Server` at level 5
  (`Debug`) keeps everything the levels above allow; `Appender.Errors` keeps the problems again. Set the
  console appender to `0` (`Disabled`) and a program writes only to its files.
- Under the supervisor, service stderr goes directly to the stack console. That includes a
  rejected title catalog, even if the service exits before reporting readiness. The supervisor
  reads stdout for readiness and keeps a short failure tail. `--logs` also mirrors stdout live.
- The one line a service prints only for a supervisor is its `READY: listening on ...` handshake: it is
  written to standard output only when `KRYTA_STACK` is set, which the supervisor sets, and it is
  consumed there and never shown. A program run by hand does not print it at all - the same words go to
  its log file.

So a stack on a thousand-player machine fills its log files and leaves the terminal alone; the terminal
carries the stack's own start, stop, restarts and failures, and the control channel answers the rest on
request.

## The front door

A gameplay connection is not a login, so it says different things. The auth program writes these at
`Info`, `Warn` or `Error`, so they print without any switch:

| line | means |
|---|---|
| `Gameplay connection from ADDRESS: build N, world W, map M, player token 0xTTTTTTTT` | a client arrived for world W. This is the line that answers "did the client reach us and what did it ask for" without watching a socket |
| `Gameplay connection from ADDRESS: world W at HOST:PORT, X byte(s) in, Y out, until the client left` | the copy ran to the end: the world answered and both sides closed |
| `... world W at HOST:PORT did not answer (...) ; the client is closed rather than left waiting` | the world's port refused or timed out. With no world running yet this is the expected line, not a fault |
| `... names world W, which is not in the world list; closing it` | the client named a world this program never offered - a stale client, or a world removed from `data/realms.json` and the `realmlist` table |
| `... ended inside its version message` / `... 0x... is not a gameplay connection` | the connection broke before its 64 bytes, or the bytes are not the message |

## The world's side of a gameplay connection

The login program's own lines are above; these are the world's, and they are what says how far a
client got inside the game:

| line | means |
|---|---|
| `Gameplay connection from ADDRESS: build N, world W, map M, player token 0xTTTTTTTT.` | the world read the version message the front door forwarded |
| `... names world W and this world is R; closing it.` | the client asked for another world; nothing else happens |
| `... the exchange is answered. Everything after it is scrambled.` | the seed exchange completed on this socket |
| `... the character-creation instance is opened, 8 bytes (load head and creation start).` | the unasked block went out and the creation screen may appear |
| `... 'name' (M) is loading - N message(s), B bytes: ... (district D, language L).` | an outpost or explorable load went out, and D/L are the district and language `0x019A` reports - the district the copy the connection entered holds (R22, R57); the map is named the way the working implementation names it, `MapEntry::label` |
| `... the ping pair` / `... the inventory and the map's record` / `... the entry position` / `... the player and the load's last message` | the four answers of a measured load, each logged with what it carried - the last one ends with the load's own message |
| `... the ping reply answered with 1 message(s), 6 bytes.` | the client pinged (`0x800D`) on a loaded map and was answered with `0x000D` carrying `World.PingValue`, as ASURA's `handle_ping_request` answers it |
| `... item N is not the character's, so its move to bag B slot S is refused.` | a drag that names an item the character does not hold: nothing is written, and the client is told nothing |
| `... the travel to map M is recorded for world W; ...` then `... the character '...' travels to map M ('...'); the client is told to dial HOST:PORT with token 0x...` | the world map or a doorway asked for a travel (`0x80B1`) and was answered with ASURA's redirect frames off a hand-off row; a request that cannot be served or handed off says why and the connection stays where it is |
| the world sends `0x001E` every `World.SimulationTickMs` (100 ms by default) once a map is loaded | the simulation's tick; its absence is a client whose world time is frozen |
| `status: up N s, listening on ADDRESS:PORT as world W, K connection(s) accepted, L live, M map(s) hosted, I instance(s) open: ...` | the world's own heartbeat, once per `StatusInterval`: what it hosts and which copies are open, answerable from the log alone; `live` is what is being served now |
| `World list refreshed: world W at ADDRESS:PORT online` / `... offline` | the `RealmsStateUpdateDelay` pass found the realmlist changed since the last refresh: a world came back online or was marked offline, and the front door follows. An unchanged refresh says nothing |
| `... was empty for H s and is disposed (R37, R40).` | the instance tick found a copy that had nobody inside past the hold, and its row is deleted - the count in the world's status line drops |
| `Disposed N instance row(s) left by an earlier run; a fresh world has nobody inside it.` | start-up found copies a previous run opened; they cannot have members, so they are disposed rather than held |
| `... district D of map M is full (...) and cannot be entered; the connection is closed ...` | a travel named a full district and R58 says it is refused; the refusal's own wire shape is not captured, so the connection is closed rather than lied to |
| `... the travel record is claimed - map M, district D, language L.` | the token pair matched a record the login wrote, and that map - not the version message's - is the one loading |
| `... no travel record was claimed; the map M is the one its own version message names.` | a connection with no hand-off: the creation screen, or a client that asked for nothing. Not a fault |
| `... the travel table could not be read (...); the version message's map is used.` | the login database did not answer; the connection still loads, and this line is the fault to chase |
| `... the creation screen's player data answered with N message(s), M bytes.` | the client asked for `0x8089` and got the block it is drawn from |
| `... the creation screen's PvP half answered with 3 message(s), 6 bytes.` | `0x808A`, answered empty |
| `... the creation screen chose campaign C and profession P.` | `0x8060`, and the choice was re-announced |
| `... message 0xXXXX with N bytes, which this world does not answer yet.` | the ask that is next to build: its header and size are in the line |
| `... a message with header 0xXXXX has no size this world knows` | the stream cannot be split further; every byte is printed beside it |
| `... a message with header 0xXXXX has been waiting N byte(s) for MS ms, and the row needs at least M ...` | a frame that will never finish: the client sent part of a message and stopped. Said once, after 900 ms of no growth, because a client that is stuck can only report its own connection error |

## Reading a failure

| what you see | what it means | where to look |
|---|---|---|
| no `accepted connection` line at all | nothing reached the port: the client is dialling elsewhere, or nothing is listening | check the `READY` line's address and port; then the client's own redirect |
| `accepted connection` then nothing | the client connected and sent nothing, or closed | is a trace on? if so the close line says how many bytes arrived |
| `... is neither the file check nor a connection this server recognises` | the first bytes are not something this server knows | `FileCheck::shipped()` and the three connection headers |
| `started like the file check but is not it` | the first four bytes matched and the rest did not | the trace's bytes against the constant's 21 |
| `sent a seed with header 0xXXXX` | the client's seed is not the shape expected | the exchange's sizes |
| `first encrypted message reads as 0xXXXX ... the login message is 0x8038` | the derivation, or the framing after it, is wrong | the trace's bytes for the first scrambled message |
| `Gameplay connection ... did not answer` | the login is fine and the front door is fine; the world is not running (or not on the named port) | the world list's port, and its own log for `READY: listening` |
| A screen that stops responding after `the character-creation instance is opened` | the world sent the block and the client is waiting on an ask the world has no answer for | the `message 0xXXXX ... does not answer yet` line names it |

## Where it goes

The `LogsDir` key decides where file appenders write; empty means the working directory. The shipped
configuration writes `Auth.log` for the auth program and `Server.log` and `Errors.log` for the world
program, and the console appender writes the same lines to the terminal.

## When a trace is not enough: the client's side

A trace here is this server's view. It says what arrived and what left, with sizes - and nothing about
what the client did with it. The other half is a capture of the client, made by
`tools/client/sniff-capture.py`, and the two are read side by side: the trace says what was sent, the
capture says what the client received and when.

Read a capture knowing which layer it holds. `recordings/` has three kinds, only two of which are the
wire, and a struct dump is two or more bytes wider than the frame - `docs/port/02-wire.md`, "Three
kinds of capture", is the rule. A capture this tree's `worldserver` or the current sniffer writes names
its `layer` and `header_len` in its header; most committed ones predate that and need `--assume-layer`.

