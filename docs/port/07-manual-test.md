# The manual test: the client against this server, step by step

What to do with a real client, what the server must say while you do it, and what to send back when it
does not. Everything here is a claim about today's tree: if a step does not behave as written, the log
line it should have printed is the thing to report.

## Before the first run

```powershell
# the client, patched and pointed at this machine (docs/port/05-tooling.md); the patcher also builds
# Kryta.dll with cargo and writes it beside the client. Start the stack before the client: the DLL
# connects to the login program when the client starts, and says what it did in Kryta.<pid>.log
python tools/client/patch-gw.py "F:\GW\GW1\Gw.exe" tools/client/keys.txt --output "F:\GW\GW1\Gw.custom.exe"
python tools/client/hosts-redirect.py status
```

The launch line below passes `-authsrv 127.0.0.1 -portal 127.0.0.1`, and the client dials the address it
was launched with (measured; the front door exists because of it), so the hosts redirect can stay
`disabled`. `hosts-redirect.py status` says which state it is in; this machine's is `disabled`, which is
what the flags need.

The databases are the ones the programs are configured for: `kryta_login`, `kryta_world`,
`kryta_characters` on `127.0.0.1:5432` (the connection strings are in each program's own JSON, which is
not committed when it holds a password).

Starting from nothing: the programs create their own tables. The migrations in `data/sql/base/` are
schema only; the maps, their spawn points and the item definitions are the world pack under
`config/flavors/Vanilla/World/`, read at start-up and never seeded
(`docs/decisions/0014-the-world-content-is-a-pack.md`) - so a fresh machine only needs the database
server, a role that may create its databases and the three databases themselves (empty is enough). The
world's fresh start-up is proven by `cargo test -p worldserver --test fresh_world -- --ignored`; it
makes and drops a database of its own and needs the role to have `CREATEDB`.

## Starting the programs

One command, from the repository root:

```powershell
cargo run -p kryta-supervisor            # auth, world and manage; Ctrl+C stops them together
```

The supervisor starts the three services `tools/supervisor/stack.json` lists, waits for each READY line,
says `the stack is up: auth on 6112, world on 6113, manage on 6602`, and stops everything on Ctrl+C. It
prints its own lifecycle lines and a service's standard error; a service's standard output is shown only
with `--logs`, and every program logs to its own files (`docs/port/05-tooling.md`). Each program is
built before it starts (`cargo build -p <program>`), so the stack always runs the code that is in the tree and never a binary left over from an earlier build - a tree that has not changed
builds in a fraction of a second. `cargo run -p kryta-supervisor -- --check` starts, waits
for every READY line, stops and reports - the way to know the stack can come up without leaving it
running. The list it starts is `tools/supervisor/stack.json`.

The same programs by hand, each in its own window, if that is preferred:

```powershell
cargo run -p authserver
cargo run -p worldserver
cargo run -p kryta-management-service -- -c tools/manage/manage.json   # optional: http://127.0.0.1:6602/
```

The login program must end with `READY: listening on 0.0.0.0:6112` and the world program with
`READY: listening on 127.0.0.1:6113 as world 2` - before that it reads the world pack's 265 map records
(`Map pack ...: N map record(s), ...`), binds the path graphs it has (`Path graphs from ...: B of N hosted
map(s) bound`) and states its doorways (`Doorways: A arch(es) on M map(s)`); both also print a status line every `StatusInterval` seconds (5 for the login and 30 for the world in the shipped files) - the
world's says `status: up N s, ... A connection(s) accepted, L live, M map(s) hosted, I instance(s) open`,
the login's `status: up N s, ... A connection(s) accepted, L live` - and that line is the first thing to
read when something seems stuck: `accepted` says something reached the port, `live` says something is
being served.

## The journey, and what the server must say

| step | you do | the server must log |
|---|---|---|
| 1 | launch the client (`Gw.custom.exe -authsrv 127.0.0.1 -portal 127.0.0.1`) | `Connection from ...: Auth connection`, the exchange, then `the login` with the five-message answer |
| 2 | type an account that exists (`user1@example.com`) | `1 character(s) listed: ...` (or `0` for a fresh account) |
| 3 | open the world map on the character screen | `the district list asked (transaction N, territory T); answered with one district in language 0 (R16)` |
| 4 | click a character and enter | the login's `the travel to map M (district D, language L) of account ... is recorded for world 2`, then its `told to dial 127.0.0.1:6113 as world 2`, and the world's `the travel record is claimed - map 148, district 1, language 0` |
| 5 | create a character (the creation screen) | `the character '...' was created as ...`, `the character's N starting item row(s) are written`, `map 148 ('Ascalon City (pre-Searing)') is unlocked for the new character (R12)` |
| 6 | the outpost loads | `Ascalon City (pre-Searing) (148) is loading - 7 message(s), 337 bytes`, then on the client's asks: `the ping pair`, `the inventory and the map's record`, `the character stands at X, Y (plane 0) on map 148` (one frame, then the ticks), `the player and the load's last message`, `map 148 is loaded - the character '...' is in it`; and the map the character arrives in is added to its unlocks: `map 148 'Ascalon City (pre-Searing)' is unlocked - the character arrived in it (R12)` - the server never refuses a load for an unlock, because the walk sets the bit (R12, `ASURA/crates/asura-world/src/player.rs:442-471`) |
| 7 | stand still in the outpost | a `0x001E` tick every `World.SimulationTickMs` milliseconds (100 in the distributed configuration; visible with `Network.Trace` off as nothing, on as trace lines; the status line's `up` time climbing) |
| 8 | click a point across the outpost, then hold a movement key | the click is answered from map 148's own graph - the finder's first waypoint in `0x0029`, so the character walks a path rather than through a wall - and the key's offset is followed through adjacent cells (`0x803D` -> `0x0029`); the world keeps ticking and the status line's `up` climbs |
| 9 | walk into the walled arch at the edge of Ascalon City (the doorway that leads to Lakeside County) | `the character crossed the doorway of map 148 at (x, y) -> (x, y): crossing to 146 (arrival at (6602.0, 4485.0) facing -2.28)`, the redirect, and then the client reconnects into Lakeside County: `Lakeside County (146) is loading - 16 message(s), 1182 bytes`, `the character stands at 6602.00, 4485.00 (plane 0) on map 146`, `map 146 'Lakeside County' is unlocked - the character arrived in it (R12)` |
| 10 | drag an item in the inventory | `the item's new place answered with 1 message(s), 11 bytes`; the item stays where you put it |
| 11 | equip an item | the same placement answer, into the equipped bag's slot for that kind of item |
| 12 | drop one of a stack on the ground (`0x802C`) | `N of item I dropped; R left in the bag, and nothing is sent.` - the count is written to the row and the client is told nothing, as ASURA does (`item.rs:187-246`). Destroying an item (`0x8069`) has no handler here or in ASURA: it logs `... which this world does not answer yet` |
| 13 | say something on the all channel (`!hello`) | `... spoke "hello" on channel 3` and `a spoken line answered with 2 message(s), 25 bytes`; the client's chat box is the judge, and it has rendered: the owner typed a line and read it back on a live client (`0012`). The pair is the twenty-byte body `5d 00 | 08 00 | 0108 0107 | hello | 01 00` and the five-byte tag `61 00 | player u16 | channel` |
| 14 | type `/maps`, then `/stuck`, then `/tp ascalon` | `/maps` sends the alias table's chat half and logs the table - the owner's client rendered the list; `/stuck` is answered with `the agent's position` (16 bytes, `0x002C`); `/tp ascalon` says `You are already here.` when the character is in map 148 |
| 15 | type `/tp kamadan` | the result depends on the world's loaded map catalogue and the reference alias rule; record the server's response and whether the client completes the travel |
| 16 | open a second client, or log a second character in, and stand in the outpost | each client is told about the other: the first logs `N other player(s) introduced` and the second draws the first's body; when the second leaves, the first is told the player and its agent are gone |
| 17 | travel from the world map | `the redirect answered with 8 message(s), 114 bytes` and `the client is told to dial 127.0.0.1:6112 with token 0x...`; the client reconnects and loads the map it asked for |
| 18 | reopen the world map | the district count answer again (step 3) |
| 19 | leave the client | `Gameplay connection ... closed.` and, after the hold, `Instance on map 148, district 1, was empty for 900 s and is disposed (R37, R40)` |

## What is not expected to work yet, and why

These are the plan's known gaps:

- **NPCs and monsters in the outpost**: the map population is not supplied yet - ASURA sends none, so
  there is no counterpart to port, though the retail captures record the stream. Connected players
  are introduced to each other as described in step 16; client rendering still needs verification.
- **The map view's contents**: the grid that sizes its world map is sent (`0x008B`, `0x008A`) with
  nothing explored, and the map's own art and areas come from the client's archive.

## What to send back

For any step that behaves differently: the client's screen (a screenshot is enough), the last 50 lines
of each program's log, and the step number above. The log line the step's row says must appear is the
one that says which half broke - the login program's, the world program's, or the seam between them.

## The same journey without the client

The world half of this journey is also played automatically against the *program* by
`cargo test -p worldserver --test end_to_end -- --ignored --nocapture`: it starts the real
`worldserver`, performs the creation screen, loads map 148, asks for the items, the spawn point and the
players, clicks a point twenty units away and reads the walk it is answered with (`0x0029`, the
finder's own first waypoint - skipped when map 148 has no entry in the configured path-graph cache), drags and equips an item and drops one of a stack, speaks a line and types the chat commands, sends the travel
request and reads the redirect's eight frames - the same wire steps 5 to 16 do with a client.
The login half is `cargo test -p authserver --test end_to_end -- --ignored --nocapture`, which starts
the real `authserver` and performs the client's file check, version message, key exchange and portal
login, reading the character list it answers with - then the district list (`0x800B`, one district in
language 0, R16), the character click (`0x800A`, answered with status 0 for a name on the account) and
the map request (`0x8029`), checking the world it names in the answer and the travel row it records
(`0x0009` + `0x0003`, the hand-off the world claims). It then starts the real `worldserver` as well,
presents the tokens the login answered with - the world's port and the travel's player token - and
checks that the world claims that travel and answers a load block for the travel's map: the seam
between the two programs. Run it with both servers stopped, because it binds the world's own port.
Neither replaces the client: the client is still the judge of what it draws.
