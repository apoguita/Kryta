# The client platform

How Kryta adds functionality to the game client: how it was first done, why that changed, the
direction the owner gave on 2026-11-23, and what is built. What the owner ruled is under "Settled";
what is left is under "Not settled" and "Not built". Statements marked *recommended* are the
assistant's advice, not the owner's ruling.

Custom titles are the first and, so far, the only feature that needs any of this.

## How it was first done

Removed on 2026-11-23, once the owner had tested the new client. It is described here because the
new DLL does the same thing at the same three places, and cites this code. The files are in the
history at commit `7679ba1`.

Three files under `tools/client/`:

| file | what it does |
|---|---|
| `patch-gw.py` | applied the connection patch (the redirect to this server), then always called `patch_titles` |
| `title_patch.py` | changes `Gw.exe` for titles, and builds the DLL |
| `title-runtime.cpp` | the source of `KrytaTitles.dll`, 187 lines of C++ |

What `title_patch.py` does to the executable:

1. It refuses any file that is not stock build 39167, by SHA-256 (`title_patch.py:99-100`).
2. It adds `KrytaTitles.dll` to the import table, so Windows loads the DLL when the client starts
   (`imports`, `title_patch.py:77-95`). It removes the file's signature to do so.
3. It appends two sections, `.titdata` and `.title`, and overwrites three places in the game's code at
   fixed addresses - the rank-name call at `0x51E7E0`, the hover formatter call at `0x4A88DB`, the
   metadata lookup at `0x1A7540` - with machine code written as hex strings in Python
   (`title_patch.py:17-19`, `rank_hook`, `hover_hook`, `metadata_hook`).

What the DLL does: it exports two functions the patched code calls, and it asks the server for the
title text. It requests `/kryta/custom-titles.json` from the portal on port 6601
(`title-runtime.cpp:14`, `:30`; served by `game/src/Portal/mod.rs:236`), again every 3 seconds while
titles are drawn (`title-runtime.cpp:117`), on the client's own thread.

How the DLL is built: `title_patch.py:165-192` finds Visual Studio and calls its x86 compiler. It
reads `nlohmann/json.hpp` from `Py4GW_Reforged_Native/third_party` under the user profile
(`title_patch.py:180-183`). So building that DLL needs Visual Studio and a checkout of another project.

## What is wrong with it

- **The patcher works on one build of `Gw.exe` only.** Every fixed address and every hex string is
  true for build 39167 and for nothing else. A client update means measuring the addresses again and
  rewriting machine code by hand.
- **The added behaviour is in the patch, not in the DLL.** The part that is hardest to keep up - code
  written into the executable - is where the work is.
- **The DLL polls.** The server cannot push; a change shows up to 3 seconds late, and every client
  asks every 3 seconds whether anything changed or not.
- **The server does not know who is asking.** It hands every client the same document, so it cannot
  send one character's values.
- **The build depends on Visual Studio and on a Reforged checkout.**

What is right and stays: a DLL is the right way to add functionality (owner), and the DLL being loaded
by the client itself, through the import table, already matches the direction below.

## The direction (owner, 2026-11-23)

The less the client file is patched, the less there is to keep up. So:

1. **The patch on disk does two things only.** It redirects the client to this server, as it does
   now, and it makes the client load the DLL by itself when it starts.
2. **There is no launcher and no injection from outside.** Kryta is a private server; it cannot wait
   until the client is open and then inject. Players run the patched `Gw.exe` and nothing else.
3. **Everything added to the client lives in the DLL**, done at runtime with hooks and callbacks.
4. **The DLL is a proper platform**, not one feature: offsets, hooks, callbacks and data transfer,
   with features built on top.
5. **The DLL receives its data from the server, and the server pushes it.** That connection is the
   link between the two. Titles are its first user.
6. **The server provides the offsets.**
7. **Py4GW Reforged is the reference for how hooks, callbacks and offsets are done.** Its code is
   ported into this tree. Kryta does not depend on Reforged.

The caution that goes with point 5: the DLL talks to the server and never to the database. It holds
no database credentials; anything stored is stored by the server.

What this means for today's code: the code overwrites at fixed addresses, the two appended sections
and the single-build check leave the patcher. The import-table entry stays. The three title hooks are
installed by the DLL at runtime.

## The platform

| part | what it is |
|---|---|
| start-up | The client loads the DLL. The DLL does almost nothing while it is being loaded - Windows restricts that moment - and starts its own thread, where the work is done. |
| offsets | Each function the DLL needs is described in JSON by a byte pattern, a mask and the steps that lead to the address. A scanner finds them at start-up. A new client build is handled by editing a file, with no recompile. |
| hooks | One layer installs and removes hooks, by name, at the addresses the scanner found. |
| callbacks | A feature registers for an event ("the client asks for a title's name") and never touches a hook directly. |
| server link | A standing connection to the server. A message from the server arrives as a callback, like any other event. |
| features | Titles is the first: it registers its callbacks and holds what the server sent. |

A pattern that is not found is named in the client's `Kryta.<pid>.log` and in the login program's log. A
resolver may state in its document that the DLL carries on without it (`on_fail: continue`); the six
the title feature needs do not, so a client build they do not fit does not start.

## Py4GW Reforged, the reference

The owner's own project, `C:\Users\Apo\Py4GW_Reforged_Native`, read at `e2f5664` (2026-11-03). It
builds a 32-bit `Py4GW.dll`. It is **not** Py4GW Stealth, which is a different project with no DLL;
Stealth is what `tools/client/sniff-capture.py` uses, and is unrelated to this page.

Reforged is the reference for hooks, callbacks and offsets only. How Reforged gets into the client - a
launcher that starts the client suspended and injects - is not what Kryta does.

| what | where in Reforged | size |
|---|---|---|
| the DLL entry: starts `RuntimeThread` and returns | `src/dllmain.cpp` | |
| the hook wrapper over MinHook (`HookBase::CreateHook`, `EnableHooks`, `RemoveHook`) | `src/base/hooker.cpp` | 102 lines |
| the scanner | `src/base/scanner.cpp` | 212 lines |
| the pattern and resolver reader | `src/base/patterns.cpp` | 1,033 lines |
| the callback registry (`PyCallback::Register`, `ExecutePhase`, pause, resume, remove) | `src/callback/callback.cpp` | 246 lines |
| the server-to-client packet hooks | `src/GW/stoc/` | 531 lines |
| the offsets | `offsets/*.json`, 30 files, one per subject | |
| MinHook | `third_party/minhook/`, with its own `LICENSE.txt` | |

The offset format, from `offsets/stoc.json`:

```json
{
  "namespace": "stoc",
  "patterns": {
    "handler_table_pointer": {
      "pattern": "\\x75\\x04\\x33\\xC0\\x5D\\xC3\\x8B\\x41\\x08\\xA8\\x01\\x75",
      "mask": "xxxxxxxxxxxx",
      "offset": "-0x6",
      "section": "text"
    }
  },
  "resolvers": {
    "handler_table_addr": {
      "steps": [
        { "name": "scan_pointer_ref", "op": "scan", "pattern": "handler_table_pointer", "out": "pointer_ref" },
        { "name": "deref_ptr", "op": "dereference", "in": "pointer_ref", "out": "final" }
      ]
    }
  }
}
```

Keeping this format identical in Kryta means an offset found for one project works in the other
(*recommended*).

What is not taken: the Python host, the overlay and ImGui, the listeners, the launcher. Reforged's
licence is Apache 2.0; MinHook's is in its own directory. Anything copied comes with its licence file.

Not yet checked: how cleanly the scanner, the resolver, the hook wrapper and the callback registry
separate from the rest of Reforged (the callback registry is written around Python callables). That is
read before the port is planned.

## The server link

Today's link is the HTTP poll described above. Its two real faults are that the server cannot push,
and that the server does not know which character is asking. That it serves only titles is not a
fault: titles are the only feature that needs it, and the link is widened when a second one does.

The direction is one standing connection on which the server sends data when it has some. Two ways to
build it were compared, and the owner chose the DLL's own connection (2026-11-23):

| | the DLL's own connection | inside the game connection |
|---|---|---|
| how | The DLL opens a second connection to the server and keeps it open. | The server sends extra Kryta messages on the connection the client already has; the DLL catches them before the client sees them. |
| who is asking | The DLL must prove which session it belongs to. | Already known: it is that character's connection. |
| the game protocol | Untouched: it stays a 1:1 port of its sources (Rule 0). | Gains messages no source has, which needs the owner's word. |
| a client without the DLL | Unaffected. | Breaks on a message it does not know, unless the server first checks the DLL is there. |
| hooking needed for the link | None. | A hook in the client's receive path. Whether Reforged's packet hooks can catch a message the client has no handler for has not been checked. |

With its own connection the game protocol stays a clean port, a fault in the link cannot disturb the
game connection, and the link works before any offset is known - which is what lets the server provide
the offsets.

### Which client is which

Several clients can run on one system, so the address a connection comes from does not say which
client it is. No in-game data is the key, because none exists until an account logs in and the link is
up before that (owner, 2026-11-23).

- **The key is the client's process id** (owner: the process id or the window handle; the window may
  not exist yet when the DLL starts, so the process id is the one used). The DLL sends it when it
  opens the link. With the address the link comes from, it names one running client from start-up.
- **The account name attaches a login to that key** (owner, 2026-11-23). The server handles the
  client's authentication, so it knows the account name; when an account logs in, the DLL reports the
  account name on its link and the server joins the link to that account's game connection.

Each client process loads its own copy of the DLL and each copy opens its own connection, so two
clients on one system are two links with two process ids.

The offsets are the same for every client of a build, so the link hands them out before any login.
The account is needed only for the data that belongs to one character.

*Recommended:* the server accepts an account name on a link only when that account is logged in from
the same address as the link, so a stranger cannot claim someone else's account.

The match relies on one login per account: only one login per account is ever permitted, and a new
login closes any old connection, so only one client has the account in use (owner, 2026-11-23). A
search of the code found nothing that closes an old connection when an account logs in again; that is
`docs/port/20-open-issues.md` item 10c.

## The server provides the offsets

The order at start-up that follows from it:

1. The DLL starts and connects to the server. This needs no offsets.
2. It tells the server which client build it is running in.
3. The server answers with the offsets for that build, then the data (titles first).
4. The DLL scans, installs its hooks and registers its callbacks.

On the server the offsets are JSON files under `config/Client/` (owner, 2026-11-23), one set per
client build, as rule 7 asks. A new client build is handled by adding a file on the server and
restarting it; players download nothing. `docs/port/04-configuration.md` lists the folder when it is
created.

The client needs a server to run; it does not run detached (owner, 2026-11-23). A client whose DLL
cannot reach the server, or whose build the server has no offsets for, does not carry on as a plain
client.

## The language

The DLL is written in Rust (owner, 2026-11-23). The C++ is one file, added with the title work; no
document gives a reason for the language, and the owner did not know the tree held C++.

The DLL becomes a member of the workspace and `cargo` builds it, with no call to
Visual Studio's compiler from a Python script; the link's messages are written once, in a crate both
the server and the DLL use; and there is one language to read. The cost is that the scanner, the
resolver and the callback registry are rewritten from Reforged's C++ rather than copied.

The patcher (`patch-gw.py`) is the same question on a smaller scale. Once it only redirects and adds
the DLL to the import table it is small enough to be a Rust tool; it does not have to move in the same
step.

## Titles on the server

The client side is half of the titles phase. The server side is in `docs/port/20-open-issues.md`:

- item 10b: the title files mix definitions and values. Definitions belong in the JSON files; each
  character's values belong in the database.
- item 10a: title earning and persistence do not exist.

With the link the server sends the title text; the `/kryta/custom-titles.json` endpoint and its poll
are gone. Items 10a and 10b are not done: the text the link sends today is
the same catalogue the endpoint served, read from the same file.

## The Python tools

The capture front end stays, in Python (owner, 2026-11-23): many more recordings will be made, and
tooling is needed for that. `tools/client/sniff-capture.py` and `capture-fresh-character.ps1` drive
Py4GW Stealth, where the sniffer itself lives; the readers are `tools/capture/`. The Kryta DLL is not
a recorder for the live game: it loads itself into a patched client.

## Settled (owner, 2026-11-23)

1. **The link** is the DLL's own connection to the server.
2. **The language** is Rust.
3. **The offset files** are under `config/Client/`.
4. **The client does not run detached.** It needs the server.
5. **The link's key is the client's process id**, and the account name attaches a login to it. No
   in-game data is the key.
6. **Only one login per account is ever permitted.** A new login closes any old connection.

## What is built (2026-11-23)

| part | where | state |
|---|---|---|
| the link's messages, for both ends | `tools/client-link/` (crate `kryta-client-link`) | built; 5 tests |
| the server end of the link | `authserver/src/Server/client_link.rs`, started by `start_client_link` in `authserver/src/main.rs`; settings `ClientLink.Enable`, `ClientLink.Bind` (`127.0.0.1:6650`), `ClientLink.Directory` | built; tests play the conversation against the real listener |
| the offsets the server provides | `config/Client/offsets/titles.json`, checked at start-up with the DLL's own reader | built |
| the DLL | `tools/client-dll/` (crate `kryta-client`, installed as `Kryta.dll`) | built |
| - the scanner | `src/scanner.rs`, port of Reforged's `scanner.cpp` and `file_scanner.cpp` | built; tested on the build machine |
| - the offset resolver | `src/patterns.rs`, port of Reforged's `patterns.cpp`, all ten step kinds | built; tested on the build machine |
| - the hook layer | `src/runtime/hooks.rs`, port of Reforged's `hooker.cpp` and `MemoryPatcher::SetRedirect`, over the `minhook` crate | built |
| - start-up and the link client | `src/runtime/mod.rs` | built |
| - custom titles | `src/titles.rs` (the logic of `title-runtime.cpp`) and `src/runtime/title_hooks.rs` (the three hooks of `title_patch.py`, installed at run time) | built |
| the patcher | `tools/client/patch-gw.py` with `tools/client/dll_patch.py`: the connection patch and one import-table entry, no build check; it builds the DLL with cargo and installs it beside the client | built |
| one login per account, on the login connection | `authserver/src/Server/logins.rs` | built; a test logs in twice and sees the first connection closed |
| one login per account, in the world | `game/src/Accounts/Logins.rs`; the login's number travels on the hand-off row (`travel.login`, `data/sql/base/db_auth/travel_login.sql`) | built; tested as a unit and through the database tests, not with two real clients |
| the DLL's log | `tools/client-dll/src/runtime/log.rs`: one file per client process, `Kryta.<pid>.log` beside the client, since several clients can run from one folder; logs a day old are removed | built |
| a crash recorder in the DLL | `tools/client-dll/src/runtime/crash.rs`: a fault inside the client is written to `Kryta.<pid>.log` with its address, module, registers and callers | built |

The conversation of the link, as built: the DLL says hello (its process id and the build stamp of
the client's file), the server sends the offset documents, the DLL reports which resolved, the server
sends the custom title text, and the connection stays open (`tools/client-link/src/lib.rs`).

### What was proved, and how

- `cargo test --workspace`: 540 tests pass. The database and end-to-end tests pass as well; the four
  world end-to-end tests now take turns, because each starts a world program of its own on the same
  databases and two at once dispose each other's rows.
- `cargo test -p kryta-client -- --ignored` with `KRYTA_GW_EXE` naming a client file: all six offsets
  resolve, in build 39167 at exactly the addresses `title_patch.py` patched, and in the older build
  of `Gw.custom.known-good.exe` at its own.
- A real client, patched by the new patcher and started against the running stack: it loaded
  `Kryta.dll` by itself at a relocated base (`0x006E0000`), connected, resolved the six offsets,
  installed the three hooks and received two custom titles; the login program logged the same from
  its side. With no server to reach, the DLL showed its message and the client did not continue.
- **The same client, logged in and in a map** (the character "Test Subject", map 148): the client
  kept running, and `Kryta.<pid>.log` records the rank hook answering the client twice during the load -
  `titles: rank 10 was given its full name` and `titles: rank 369 was given its full name`. So the
  hook that turns a custom title's short rank name into its full name works inside a running game.

- **The Hero Panel, in a second real client** (the character "Test Two" of the other account): the
  Titles tab lists the two custom titles by their full names - "Embiggener (1)" with `33/10,000`, and
  "Hog Jumper (1)" - where the wire carries only `Mini` and `Jump`. Hovering Embiggener shows its
  description, "Make Minipets real size", and the log records
  `titles: title 49 at rank 369 was given its description`. So all three hooks do in the running
  client what the file patch did. Seen by the assistant through a capture of that client's own
  window, 2026-11-23; the owner has not yet looked himself.

**Confirmed by the owner, 2026-11-23:** "tested, everything seems to be working" - after his own
test of the new client, which he had said would cover the Hero Panel and one login per account with
two clients. On that word the old path was removed: `title_patch.py`, `title-runtime.cpp`,
`patch-gw.py --legacy-titles`, and the portal's `/kryta/custom-titles.json` endpoint. The setting
that names the world pack the title text comes from moved with it, from `Portal.TitleWorldConfig` to
`ClientLink.TitleWorldConfig`.

A crash met during this test is not the DLL's: the character "Player One" ends the client on
entering a map with the client's own assertion `id < s_items.Count()` (`CpsData.cpp(432)`), and it
did so with the old `Gw.custom.exe` as well (`docs/port/20-open-issues.md` item 10d).

### Choices made while building, for the owner to confirm or change

- **The crates live in `tools/client-link/` and `tools/client-dll/`.**
- **A client build is not named at all.** The offsets are byte patterns, and one document serves
  every build its patterns fit; a document can hold several attempts for builds that differ. So
  `config/Client/offsets/` has no folder per build. The DLL sends the build stamp of the client's
  file (the `TimeDateStamp` of its PE header) so the logs say which build a failure was in.
- **The DLL finds the server** at the `-portal` host of the client's command line (then `-authsrv`),
  on port 6650. A file `Kryta.json` beside the client, `{ "link": "host:port" }`, names another
  address.
- **When start-up fails** - the server is not reached in ten tries, refuses, or its offsets do not fit
  the build - the player sees a message box that says why and the client ends. The game's own window
  is up while the DLL tries, about half a minute at most.
- **When the link drops during play** the DLL opens it again, every three seconds, and the client
  keeps running.
- **The title hooks are the old three**, at the same three places, doing the same thing; only how
  they get there changed. Two of them are short assembly routines inside the DLL that read the
  client's registers, as the Python-written machine code did. A client update that changes those
  registers needs the DLL rebuilt; one that only moves the functions needs nothing.
- **The DLL has no callback registry yet.** With one feature, the title hooks call the title code
  directly. Reforged's event callbacks (`include/base/hook_types.h`) are the shape to port when a
  second feature shares a hook.
- **The unsafe code is confined to `tools/client-dll/src/runtime/`.** The workspace forbids `unsafe`;
  the DLL crate does not take that lint, and says why in its `Cargo.toml`.

## Not built

1. **The account attached to a link.** The DLL sends its process id; it does not yet read the account
   name and report it, and the server does not yet join a link to a login. Titles do not need it: the
   title text is the same for everyone, and each character's ranks travel on the game connection.
2. **The notice retail gives a displaced client** (`ENetError` 0x23): no source shows the message that
   carries it, so the connection is closed without one.
3. **Titles on the server** (items 10a and 10b).
4. **The patcher in Rust.**

## Not settled

1. **How the DLL learns the account name** is found in Reforged ("What the reading found") and not
   yet checked on a client connected to Kryta.
2. **Whether the choices above stand.**

## What the reading found (2026-11-23)

Read before any code, as Rule 0a asks. Nothing in the repository was changed by it; the experiments
were built outside the tree.

### The Reforged parts

| part | file | finding |
|---|---|---|
| the scanner | `src/base/scanner.cpp` (212 lines), `src/base/file_scanner.cpp` (370 lines) | Self-contained. It maps the executable's file from disk as an image and searches that, then converts the address to the running one, so hooks already written into memory cannot spoil a search. It finds by pattern and mask, by a use of an address, by a use of a string, by an assertion's file and message, and walks from a call to the function it calls and back to a function's start. |
| the pattern and resolver reader | `src/base/patterns.cpp` (1,033 lines), `include/base/patterns.h` | Self-contained apart from the JSON library and the logger. A resolver is a list of steps, or several attempts tried in order. Ten step kinds: `scan`, `scan_in_range`, `to_function_start`, `function_from_near_call`, `find_use_of_string`, `dereference`, `read_u32`, `add`, `divide`, `validate_section`. Each resolver states how severe a failure is and whether to continue. |
| the hook wrapper | `src/base/hooker.cpp` (102 lines) | A thin layer over MinHook: create, enable, disable, remove, and a count of calls in flight. |
| event callbacks | `include/base/hook_types.h` (`HookEntry`, `HookStatus`, `HookCallback`), used by `src/GW/stoc/` | The shape to port: a callback registered under an owner's entry, with an altitude for order and a status it can set to block the original. |
| the frame scheduler | `src/callback/callback.cpp` (246 lines) | Not ported. It schedules Python callables per frame and is built around the Python host. |

The four parts to port separate cleanly; none of them needs the Python host.

### A 32-bit DLL in Rust

Proved by a throwaway crate: a `cdylib` built for `i686-pc-windows-msvc` with a `DllMain` that starts
a thread, using the `minhook` crate (0.9.0, MIT) - a wrapper over the same MinHook library Reforged
uses. Loaded into a 32-bit process, it hooked a function and called the original through the hook.

- The target was not installed on this machine. `rust-toolchain.toml` now lists it, so `rustup`
  installs it by itself; `README.md` and `docs/port/05-tooling.md` state it.
- The crate compiles MinHook's C source, so a C compiler is still needed to build the DLL; it is the
  one Rust on Windows already requires.
- Built the default way, the DLL imports `VCRUNTIME140.dll`. `.cargo/config.toml` links the C runtime
  in for this target, so a player's machine needs nothing installed.

### The three title functions, by pattern

Each of today's three fixed addresses was turned into a byte pattern with its address operands masked,
and searched for in two different client builds on this machine. Each pattern matched exactly once in
each build, at a different address:

| function | build 39167 (`Gw.exe`) | the older build (`Gw.custom.known-good.exe`) |
|---|---|---|
| the rank-name call | function at `0x51E7C0`, call at +32 | function at `0x51E100` |
| the hover formatter call | `0x4A88C7`, call at +20 | `0x4A81D7` |
| the metadata lookup | `0x1A7540` | `0x1A74B0` |

So the method works for these three, across a client update.

### One login per account

Not enforced when this was read (the login connection enforces it since; see "What is built"). The
portal keeps a map from session to account and adds to it at each login without
looking for an earlier one (`game/src/Portal/mod.rs:163`, `:288`); the world keeps its connected
players by player id, with no account in the record (`game/src/Districts/Peers.rs`); and the login
database has no record of who is connected (`data/sql/base/db_auth/`). It is
`docs/port/20-open-issues.md` item 10c.

### The account name inside the client

Reforged reads it: `CharContext.player_email`, at `+0x3D0` of the character context
(`include/GW/context/character.h:61`), reached through `offsets/context.json`. The client sends the
same name to this tree's portal as `LoginName` (`game/src/Portal/mod.rs:278`). Not yet checked on a
client connected to Kryta.
