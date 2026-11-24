# The client platform

How Kryta adds functionality to the game client: what is done today, why it changes, and the direction
the owner gave on 2026-11-23. This page records a discussion so that it is not lost. It is a direction
and not yet a design: nothing here is built. What the owner ruled is under "Settled"; what is left is
under "Not settled". Statements marked *recommended* are the assistant's advice, not the owner's
ruling.

Custom titles are the first and, so far, the only feature that needs any of this.

## How it is done today

Three files under `tools/client/`, all from commit `7679ba1`:

| file | what it does |
|---|---|
| `patch-gw.py` | applies the connection patch (the redirect to this server), then always calls `patch_titles` |
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
(`title_patch.py:180-183`). So building Kryta's client DLL today needs Visual Studio and a checkout of
another project.

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

If a pattern is not found, the feature that needs it switches off and says so in a log, and the client
still runs (*recommended*; today an unknown build is refused outright).

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

Not yet checked: that a 32-bit DLL builds from this workspace, and which Rust hook library fits the
32-bit client. No library is named until that is read.

The patcher (`patch-gw.py`) is the same question on a smaller scale. Once it only redirects and adds
the DLL to the import table it is small enough to be a Rust tool; it does not have to move in the same
step.

## Titles on the server

The client side is half of the titles phase. The server side is in `docs/port/20-open-issues.md`:

- item 10b: the title files mix definitions and values. Definitions belong in the JSON files; each
  character's values belong in the database.
- item 10a: title earning and persistence do not exist.

With the link, the server holds both and sends them, and the `/kryta/custom-titles.json` endpoint and
its poll go away.

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

## Not settled

1. **How the DLL learns the account name** inside the client: where it is read from.
2. **How a client build is named** inside `config/Client/`: by build number, or by a fingerprint of
   the file.
3. **What the player sees** when the DLL cannot reach the server or the server has no offsets for the
   build, and what happens when the link drops during play.
4. **Whether the patcher moves to Rust**, and when.

## Before any code

Reading only, in this order:

1. Read the Reforged parts named above and report how they separate from the rest.
2. Confirm a 32-bit DLL builds from the workspace and find the hook library.
3. Find the three title functions by byte pattern instead of by fixed address, on the client build in
   use, to confirm the method works for them.

The plan for the titles phase is written from what that reading finds.
