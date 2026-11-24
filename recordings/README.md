# Recordings - public research evidence

This folder is the project's archive of **faithful recordings of the client and the servers
talking to it**. It is public on purpose: the recordings are the research material that lets anyone
check how the game actually behaves, without needing a server, the client, or another project's
checkout.

Most of it was copied from ASURA's `recordings/` folder (`C:\Users\Apo\Documents\GitHub\ASURA\recordings\`):
every file there is here. The five files under `client/raw/` that ASURA does not hold were made in this
tree with `tools/client/sniff-capture.py`. No capture made by Kryta's own `worldserver` is committed.

They are **evidence, not instructions.** A recording tells you what happened; it never dictates
what to build. The real client remains the acceptance test for any change.

## The layout

```
recordings/
  asura/     recordings made by ASURA's world server, copied from ASURA (18 files)
  gwps/      recordings of the GWPS server (historical, frozen), copied from ASURA (9 files)
  client/    client-side recordings (the sniff tool, capturing Gw.exe directly) (27 files)
```

Which layer each folder holds - the packed wire frame or the client's message struct - is
`docs/port/02-wire.md`, "Three kinds of capture". A capture the current tools write states its layer in
its header (`layer`, `header_len`); most of the copied files predate that field.

Each of those contains the **kind**, and every kind has an optional **subject** (only for
`feature/`):

| kind | what it is |
|---|---|
| `login/` | the auth session: handshake, login batch, character list |
| `world/` | one world process's message trace, one file per run |
| `session/` | a whole client session (login -> travel -> maps -> exit) |
| `feature/<subject>/` | one subject per recording: `movement`, `items`, `travel`, `chat`, `maps`, `character_creation` |
| `raw/` | `client/` only: unfiltered sniffer captures not yet sorted into a kind (login, title, fresh character, the Northlands) |

## Naming: the subject first, the date only to disambiguate

The name says **what the recording is**, in words a person would use. The date is a disambiguator,
never the name:

```
<subject>.<ext>                      the only recording of its subject in its folder
<subject>-<YYYYMMDD>.<ext>           a second one shares the subject with another day
<subject>-<YYYYMMDD>-<HHMMSS>.<ext>  several share the subject on the same day
```

Examples:

```
live-client-session-20261027-220258.jsonl        one of seven sessions against ASURA's world, 27 Oct
live-client-session-20261027-223528.jsonl
load-stream.log                                   GWPS's load stream (unique subject)
login-travel-session.log                          GWPS's login -> travel session
movement-sniff.log                                the client-side movement sniff
```

- Tools that name their own output (the sniff tool, `tools/client/sniff-capture.py`) include the full
  `YYYYMMDD-HHMMSS`, because they cannot check the folder for collisions.
- The three `.log` recordings (`gwps/world/load-stream.log`, `gwps/session/login-travel-session.log`,
  `client/feature/movement/movement-sniff.log`) match the repository's `*.log` ignore rule, so they are
  not committed and are absent from a fresh clone; the copies are in ASURA's `recordings/`.
- **Never edit a recording.** A re-record is a new file.
- When a new recording collides with an undated one, the old one is renamed to the dated form and
  `index.jsonl` is updated - the date appears exactly when it earns its place.
- The full time, timezone, build and tool live in `index.jsonl`; the filename only carries what a
  human needs.
- Client-side captures must be **sanitised** before they land here (the capture tool masks names
  and account data, offsets preserved); the index says what was sanitised. **The six files under
  `client/raw/` break this rule**: each one's header says `"sanitised":false` and
  `RAW: contains names/UUIDs -- do not commit`, and all six are committed.

## The index

`index.jsonl` is one JSON line per indexed recording - the way to search at thousands of files. It
does not cover the folder: it holds 19 lines (7 `asura`, 10 `client`, 2 `gwps`) for 55 recordings, so
a file without a line is read from its own header and its folder:

```json
{"source":"asura","kind":"world","subject":"live-client-session","path":"asura/world/live-client-session-20261027-220258.jsonl",
 "recorded":"2026-10-27T22:02:58Z","tool":"asura-world --capture","world":8001,
 "client_build":39112,"tags":["load","session"],"note":"..."}
```

- Add a line when you add a recording; the fields are `source`, `kind`, `subject`, `path`,
  `recorded` (ISO-8601), `tool`, and then whatever the recording knows: `world`, `client_build`,
  `maps`, `tags`, `note`.
- The index is the search layer; the folders are for humans.

## The sources, honestly

- `asura/` - made by ASURA's world server recording its own socket (`asura-world --capture`), and
  copied from ASURA. Kryta's `worldserver` writes the same layer when its `CaptureFile` setting names
  a file (`game/src/Server/Capture.rs`, header `"tool":"worldserver"`); none of those is committed.
- `gwps/` - made by the GWPS server (`C:\Users\Apo\Documents\GitHub\PreSearing`), copied from ASURA.
  Historical: GWPS is not the specification; these files are kept as the record of what it did, not
  as expected answers.
- `client/` - made by looking at the client itself (the sniff tool, `Gw.exe` both directions):
  ASURA's `asura/tools/sniff-capture.py` for the copied files, this tree's
  `tools/client/sniff-capture.py` for the five newer files in `raw/`. The server it talked to is not
  part of the capture; the index note says so when it is unknown.
- `tools/capture/read.py` reads a capture in the layer its header states, and refuses a file that
  states none unless it is given `--assume-layer`.
