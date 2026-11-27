# 24 - Custom maps: research

Research of 2026-11-25. Nothing here is implemented, and nothing here was run: every result below
was read from a project's code or documents. A client result is that project's own report.

## The subject

A custom map is a map whose geometry is not in the game: authored outside it, in an editor, and
loaded by the client. It is not a stock map with different spawns or content.

## The owner's constraint

**`Gw.dat` is not modified** (owner, 2026-11-25). Every source that loads custom geometry today does
it by writing the map into the client's archive, so no source meets the constraint as it stands. That
is a missing source under Rule 0. A route through `Kryta.dll` is assessed below.

This tree does not write to `Gw.dat` anywhere. `worldserver` never opens it
(`docs/decisions/0015-headless-navigation-assets.md`). The control panel reads it only for the
navigation dump, and the one file that step writes is the cache
(`tools/manage/src/path_graphs.rs:106`). The client patch writes a separate `Gw.custom.exe`.

## How the client picks a map file

- The client has no table from map id to map file. Its area table, `s_missionClientData`, is in
  `Gw.exe`: 888 records of 124 bytes, holding name, region, type and campaign by map id, and no map
  file id (`research/Rurik/studies/areatable/FINDINGS.md` sections 1-3;
  `research/Rurik/studies/maprows/FINDINGS.md` sections 2a-3;
  `research/Eden/docs/re/area-info-provenance.md:14-17, 78-84, 184-200`).
- The map file id comes only from the server, in the spawn-point message: `u32 map_file_id, f32 x,
  f32 y, u16 plane, u8, u8, u8[8]`. It is `0x0195` in Rurik's build 38797
  (`research/Rurik/studies/mapload/FINDINGS.md:17-31`) and `0x0196` here, built in
  `game/src/Server/Packets/InstancePackets.rs:184` and sent from `game/src/Agents/Player.rs:84`.
- The client looks that id up in its archive with an exact 32-bit compare
  (`research/Rurik/studies/maprows/FINDINGS.md` sections 1, 2b). A file id the archive does not hold
  gives "not a valid riff type" and an assert in `Map.cpp(1762)` (`docs/port/02-wire.md:236-242`);
  Rurik records the client building a default map with rect (-3072, -3072, 3072, 3072) instead
  (`research/Rurik/toolkit/authsrv/maprect.py:10-13, 34-37`). The two observations are not reconciled.
- So a custom map adds no packet. The only wire difference from a stock map is the value of the file
  id (`research/Rurik/toolkit/authsrv/authsrv.py:48306-48337`).
- The map id sent is always a stock one, so the client shows the stock name over the custom
  geometry (`research/Rurik/content/maps.toml:245-249, 441-448`). A map id outside the 888 rows was
  never tried by any project.

## The sources

| source | what it is | state |
|---|---|---|
| Rurik, `research/Rurik/` | The map writer. `toolkit/mapdata/deploy.py` builds a "stripped" map file from a height field and donor chunks, and installs it into a copy of `Gw.dat` under a new file id (`0x5F0B0` onward). The client's own compiler turns it into the final file, walkable mesh included, on first load. | By its own report: a built map loaded and confined the character (2026-08-11), a created file id compiled (2026-08-20), a 256x256 map walked (2026-08-21), travel between a stock and a created map worked (2026-09-12). `studies/customarea/FINDINGS.md`, `studies/worldmaps/FINDINGS.md`, `studies/slice/FINDINGS.md`. |
| kryta-cs, `research/kryta-cs/` | [aeratemark/kryta-cs](https://github.com/aeratemark/kryta-cs) at `1ec7df0` (2026-11-26, MIT): a map editor (Python, PySide6) for Kryta. Sculpt, paint textures, place props and spawns, set water; then build with a verbatim copy of Rurik's writer, install into `Gw.dat`, and publish a pack folder. | Its README is its own claim. Its one published map is in the fork: "Kryta Test Vale", 64x64, map 449, file id 389376 (`0x5F100`), at `research/Kryta-aeratemark/config/World/Vanilla/maps/Custom/449/` with the project at `research/Kryta-aeratemark/data/maps/custom/kryta-test-vale.kmap.json`. |
| every other project | A map list by data or by code, stock maps only. | None authors geometry, adds a file to the archive, or pairs a map id with a non-stock file. |

## The editor's route, end to end

From `research/kryta-cs/README.md`, "Build & Deploy", and `kryta_cs/kryta.py`:

1. Build the map file with Rurik's writer; textures, sky, environment and sound are borrowed from a
   stock donor map in the same archive.
2. Back up `Gw.dat` once, then install the map into it under a new file id, with an empty walkable
   mesh.
3. Publish `config/World/<pack>/maps/Custom/<map id>/` with `basic.json` (`map_id`, `map_type`,
   `map_file_id`, `name`, and the `instance_value` of the stock map it stands in for), `spawns.json`
   and `kryta-cs.json`. The pack's own folder for that id is moved to
   `<pack>/kryta-cs-displaced/`, because one id is stated once (`game/src/Maps/Pack.rs:123`).
4. Start the world and travel to the map. The client compiles the walkable mesh into `Gw.dat`.
5. Close the client and ask the control panel to copy that mesh into `data/path-graphs.bin`. Until
   then the map is `NoGraph` and a click-walk is cancelled (`game/src/Handlers/MovementHandler.rs:98-107`).

Steps 2 and 4 write `Gw.dat`. They are what the owner's constraint excludes.

## What this tree already has, and what it lacks

- **Has:** the pack reader takes the file id from `map_file_id` in `basic.json` and sends it; a
  `maps/Custom/<id>/` folder is the layout it already reads. Not tried.
- **Stale in the editor:** it posts to `/api/navigation/dump`; this tree serves
  `/api/v1/navigation/dump` (`tools/manage/src/areas/navigation.rs:18`).
- **Lacks:**
  - a way to give the client a map file that is not in its archive;
  - a walkable graph for such a map, since today the graph is read from the mesh the client compiled
    into `Gw.dat`;
  - a map id and name of the custom map's own: it stands in a stock slot, and `/tp` only knows
    `game/src/Maps/TravelTargets.rs`;
  - `cartography::grid_for` (`game/src/Maps/Cartography.rs:53`) answers the Tyria grid for every map;
  - `docs/port/04-configuration.md` does not list `Custom/` or `kryta-cs-displaced/`.

## What the writer cannot do yet

From `research/Rurik/studies/worldmaps/FINDINGS.md` (W6, W24, W25) and
`research/Rurik/studies/customarea/FINDINGS.md`:

- no shore, and no minimap for a created map; kryta-cs adds a water level;
- textures smear on steep faces, and gates about 3 cells wide do not become walkable;
- the Zones chunk is undecoded and copied from a donor;
- Rurik's fullest map, "Ashcoil" (2026-08-22), loaded, and its owner judged it a "complete failure"
  visually;
- the editor lists as not done: NPC spawns as objects, doorways between custom maps.

The archive-space cap on created maps (one on build 38797, eight on 38833) and survival across a
client patch are limits of the archive route only.

## Delivery without touching `Gw.dat`: assessed feasible, not built (2026-11-26)

Read from the sources below; nothing was run. The route is `Kryta.dll` giving the client the map's
bytes from a file beside the game.

### What the client does, as the sources record it

- The spawn-point handler passes the file id to `FileConvertIdToPath`, which writes a two-unit name
  (`(id-1) % 0xFF00 + 0x100`, `(id-1) / 0xFF00 + 0x100`), and the map is loaded from that name
  (`research/Eden/docs/re/configuration.md:128-137`; `research/Rurik/studies/mapload/FINDINGS.md:17-47`;
  `C:\Users\Apo\Py4GW_Reforged_Native\docs\RE\map_travel_research.md:205-217`).
- `FileOpen(path, mode, err)` is a dispatcher: a two- or three-unit name with every unit above 0xFF
  is an archive entry (file id, stream) and goes to `FileOpenArchive(0, fileId, stream, mode, err)`;
  any other string is a real path opened with `CreateFileW`
  (`research/Eden/docs/re/configuration.md:139-151`). So the client's own code can open a loose file.
- A normal map load reads stream 1 of the file id, the finished ("bloated") form, whole into a
  buffer. Only when that open or parse fails does the client read stream 0 (the "stripped" form),
  compile it, write stream 1 into the archive with mode 2, and retry once
  (`research/Rurik/studies/customarea/FINDINGS.md:1305-1357, 1404-1406`;
  `research/Eden/docs/re/map-bloat.md:86-101`).
- A finished map supplied directly loads with no compile and no write-back, by Rurik's report
  (`research/Rurik/studies/customarea/FINDINGS.md:3049-3077, 3310-3368`).
- The archive layer checks an entry's CRC-32 when a read completes; no checksum over the map's
  content was found (`research/Rurik/studies/datwrite/FINDINGS.md:417-441`;
  `research/Rurik/studies/customarea/FINDINGS.md:483-487`).
- A map names its textures, models, sky and sound by file id. Every Rurik and kryta-cs map borrows
  stock ids, so one custom file per map is enough
  (`research/Rurik/studies/customarea/FINDINGS.md:6546-6550`).
- When the load fails, `PlayerLoadMap` finds the file's hard links, deletes it, and sends `0x8C`
  (`C:\Users\Apo\Py4GW_Reforged_Native\docs\RE\map_travel_research.md:212-214`).

### The functions, already found by pattern

Py4GW Reforged Native resolves these with the offset format `Kryta.dll` already reads
(`C:\Users\Apo\Py4GW_Reforged_Native\offsets\gw_dat_reader.json`,
`include\GW\textures\gw_dat_reader.h:42-49`); Stealth carries the same and records them verified on
builds 38888 and 39112 (`C:\Users\Apo\Py4GW_Stealth\docs\RESEARCH.md:3217-3254`):

| function | prototype, all `__cdecl` |
|---|---|
| open by file id | `RecObj* (u32 archive, u32 file_id, u32 stream_id, u32 flags, u32* error_out)` |
| open by name | `RecObj* (const wchar_t* name, int, int)` |
| read whole file | `u8* (RecObj* rec, int* size_out)` |
| free the buffer | `void (RecObj* rec, u8* bytes)` |
| close | `void (RecObj* rec)` |

Both projects only call them. No project hooks file access.

### Two ways to hook

1. **Swap the name.** Where the map's name is made for a custom file id, give the loader a real path
   instead. The client then opens the loose file itself. Not known: whether the loader hands the name
   to `FileOpen` as it is, or decodes it and asks for stream 1 by number, in which case a real path
   never reaches the dispatcher.
2. **Swap the bytes.** Detour the open: for a custom file id, open a stock map instead, so the client
   holds a record it made itself, and remember that record. Detour the read: for that record, return
   the custom file's bytes in a buffer the client can free. Not known: how the buffer is allocated
   and freed, and whether the map loader reads through these two functions or a neighbour of them.

Either way the file given is the finished form, so the compile and its write into the archive never
start.

### Where the finished file comes from

The writer (Rurik, kryta-cs) produces the stripped form and relies on the client's compile. Three
ways to get the finished form without writing an archive, none built:

- the DLL also detours the write: on the author's machine it serves the stripped file for stream 0,
  takes the mode-2 write of stream 1 into a loose file, and serves that file on the retry;
- Eden's port of the compile (`research/Eden/docs/re/map-bloat.md`): terrain, props and zones are
  stated landed, path and sight are stated not ready in one document and byte-identical in another;
- Rurik's direct finished-form writer (`toolkit/mapdata/mapbuild.py`), which Rurik left because
  terrain, props, zones and sight are hard to author that way.

### The server's side needs no new reader

`game/src/Terrain/MapData.rs:1043`, `pub fn import(map: &[u8])`, builds the path graph from a finished
map's bytes with no archive. The same loose file the client is given can feed
`data/path-graphs.bin`. The link from server to DLL (`tools/client-link/`) is this tree's own and can
carry the file; the game protocol does not change.

### Risks

- A record left open is reported at exit: `file '...Gw.dat' still open`, `NtFile.cpp(321)`
  (`C:\Users\Apo\Py4GW_Stealth\docs\RESEARCH.md:3217-3250`).
- A failed load deletes the file it tried. With a custom id that must reach neither the archive nor
  the loose file.
- A custom map that names a model the archive lacks asserts (`PrCollision.cpp:354`,
  `research/Eden/docs/re/map-bloat.md:753`).
- A client with its downloader enabled may ask the file server for an id it lacks; this tree serves
  no files. Not measured.
- Every address above is from builds 38797 to 39112; this tree's client is 39167, and the patterns
  have not been tried on it.

## The map format

Researched 2026-11-26, from the sources' documents and code; nothing was run.

- **It is not an open format.** There is no specification from ArenaNet. What is known is community
  reverse engineering and the client's own assertion strings.
- **The container** is `ffna`, one type byte (3 is a map), then `{u32 id, u32 size, payload}` records
  to the end of the file. A chunk id is `(stage << 28) | (kind << 24) | base`: stage 1 is the
  stripped form, stage 2 the finished form; kind 0 is data, kind 1 a list of file ids the chunk
  depends on (`research/Rurik/studies/mapdata/FORMAT.md:142-147`;
  `research/Rurik/studies/customarea/FINDINGS.md:188-195`). Compression belongs to the archive entry,
  not to the file.
- **Limits:** a cell is 96.0 units and the map's rect must equal its dimensions times 96; a side is a
  multiple of 32 cells; Rurik compiled maps up to 256x256; one texture index per cell; the walkable
  slope cut is 35 or 45 degrees, chosen by the top byte of the map flags
  (`research/Rurik/studies/customarea/FINDINGS.md:5449-5455`;
  `research/Rurik/studies/worldmaps/AUTHORING.md:19, 47-57`).
- **The references:** Eden's `docs/re/render/map-chunks.md` and `docs/re/map-bloat.md` (every chunk
  indexed, and the only account of how the client makes the finished form); Rurik's studies and
  toolkit (the only writer); GuildWarsMapBrowser's ImHex pattern (the finished form only, with errors
  the other two corrected).

| chunk | holds | can be written by |
|---|---|---|
| Terrain, and its texture list | heights, a texture index per cell | Rurik, both forms |
| Props, and their model list | placed models, with outlines that block movement | Rurik, stripped form |
| Path | the walkable mesh: planes, trapezoids, portals | Rurik, both forms; the client's compile; Eden's port of it |
| Map parameters, Environment, Sound | the rect and flags; sky, fog, light, water level; emitters | Rurik |
| Zones | flora: plant types and the areas they grow in | kryta-cs, stripped form: a donor map's plant types kept whole, the areas authored (`research/kryta-cs/kryta_cs/zones.py`); Eden's port makes the finished form from the stripped one |
| Sight | the line-of-sight tree | nobody; Rurik's directly built map loaded without it |
| Shore, Light, Mission | shore lines, lights, start points | nobody; read only |

The client's compile is what makes the finished Path, Sight, Zones and terrain lighting. Eden's port
of it is stated byte-identical on 356 of 356 retail maps for every chunk except Sight
(`research/Eden/crates/eden-assets/src/map/bloat.rs:22-48`).

## The tools

| tool | what it does | writes |
|---|---|---|
| kryta-cs, `research/kryta-cs/` | the only editor: heights, texture per cell, donor props, flora areas, spawns, placed NPCs, a water level; six generators and six brushes; checks what the client would refuse. It also opens a stock map to change its props, flora and NPCs, and edits the pack's Lua files with the world's own checks | a `.kmap.json` project; a stripped map; the pack folder; `Gw.dat` on install |
| Rurik's toolkit, `research/Rurik/toolkit/mapdata/` | the map writer the editor uses; `deploy.py --out FILE` builds a stripped map to disk with no install | a stripped map file |
| Rurik's Blender scripts, `research/Rurik/tools/blender/` | a map into Blender and a Blender mesh back to a map's heights and prop placements, through its `gwmap` files; no archive opened | `gwmap` JSON with raw height and tile arrays |
| GuildWarsMapBrowser, github.com/Jonathan-Greve/GuildWarsMapBrowser (v8.0.1, 2026-03-15) | reads stock maps and models; two Blender add-ons import what it exports | its own JSON, OBJ, heightmap PNG and TIFF, textures; no map and no archive |
| asura_cs, `research/ASURA_testfork/asura-cs/` | the editor kryta-cs replaced: NPCs, spawns, portals, quests, no terrain; only spawn points worked | profile JSON and SQL |
| Eden, the offline prototype, PreSearing's tools, Py4GW | viewers and dumps | nothing a client loads |

No public tool writes a map or takes a heightmap, OBJ or glTF into one. Texture mods (gMod, uMod,
TexMod) replace textures in graphics memory and cannot supply heights, collision or pathing.

What kryta-cs lacks for this tree: its build already makes the whole map in memory with the archive
only read (`research/kryta-cs/kryta_cs/pipeline.py:463-504`), but the file reaches disk only inside
`install`, and the file id comes from the install. It has no import or export of any outside format,
no portals, no patrol routes, and one height field with no bridges or caves. Editing a stock map's
props or flora is done by patching that map's file inside `Gw.dat`, which this tree does not use. It reads
`Gw.dat` for the donor's textures and models.

Licences: Rurik is MIT with parts derived from GuildWarsMapBrowser under that project's terms (a link
to its repository and credit to its author); Eden marks its map code "Restricted origin" before
distribution; kryta-cs is MIT. Its notice credits Rurik and not GuildWarsMapBrowser, whose terms
the two derived Rurik modules carry.

## The direction

What the owner has decided (2026-11-26, 2026-11-27): `Gw.dat` is never modified; a custom map
reaches the client as files beside it, through `Kryta.dll`; kryta-cs was to be researched. Points
1 and 3 below follow from that and are proven by the owner's runs.

**Decided by the owner, 2026-11-27: the server holds the maps and serves them.** A client gets a
custom map from the server, through `Kryta.dll`. Where a file is too large to send, the server
provides the way to reach it (the owner's example: a network shared drive), and the server controls
that. Maps are not copied to clients by hand or by a tool: "we cannot be copying maps to hundreds
of clients". A client may keep a local copy for a time, made automatically and never by its
user: the server instructs the DLL to copy or stream the file, or to fetch it another way, and
states its checksum, so a map the server has changed is known to be changed and is fetched again.
The folder `Kryta\maps\` beside the client, which phases 2 to 4 were proven with and
which a person fills, was the session's shortcut and was never agreed; it is not the design. What
those phases proved about the client stands: it loads a map it is handed outside the archive, it
compiles a stripped one, and it writes nothing into `Gw.dat`. Point 1 below is replaced by this
paragraph; phase 6 step 3 is no longer a late step but the way a map reaches a client at all, and
is to be planned again.

**Decided by the owner, 2026-11-27: tests, proofs and tooling are Rust, in this tree.** The other
projects are research and take over nothing. What this document records as proven with a Python
script or with kryta-cs's build counts as not proven until it is done again in Rust: the test map
of phase 4 and the file a client compiled from it, the walk-graph check made with that file, the
archive checks, the stock map file of phases 2 and 3, and the reading of the client's code by
script. Done again in Rust so far: the archive check
(`game/src/Pack/Gwdat.rs`, `the_archive_holds_none_of_the_named_custom_file_ids`, with
`KRYTA_GW_DAT` and `KRYTA_ABSENT_FILE_IDS=0x5F100`: 156,315 entries, 147,555 file ids, no
`0x5F100`).

**Decided by the owner, 2026-11-27: the editor is this tree's own, a Rust desktop program in the
workspace** (the owner chose it over an editor inside the control panel and over an outside 3D tool
with a converter). kryta-cs stays research. The editor has two parts: a map writer, a library that
turns a map's description into the map file, ported from its sources; and the window a person
edits in. Its plan is not written yet. The paragraph below is what stood before this decision.

**Not decided by the owner: how and with what a map is authored.** Points 2 and 4 were written by
the session as if agreed, and were not: the owner agreed to research kryta-cs, not to adopt it, and
not to a programming language (owner, 2026-11-27). kryta-cs and Rurik's map writer are Python. They
stand here as one option. Phases 5 and 7 wait for the owner's decision.

1. **The format this tree supports is the game's own map file**, given to the client as a loose file
   in its finished form. The authored source is the editor's project file.
2. **kryta-cs is the editor**, kept as a tool outside this tree, changed in two places: a build that
   saves the map to disk, and a file id given without an install. The install into `Gw.dat` is not
   used.
3. **The finished form comes from the client's own compile**, with the DLL taking the result into a
   loose file (the first option under "Where the finished file comes from").
4. **Importing is one small tool**: a heightmap (PNG, TIFF or raw) into the editor's project. Blender
   is already covered both ways by Rurik's scripts, and stock maps come out through
   GuildWarsMapBrowser.
5. **Asked of aeratemark:** the GuildWarsMapBrowser credit in the editor's notice.

## The work plan

Started on the owner's word, 2026-11-26 ("make a plan ... then proceed"). Each phase names where it
is done, what proves it, and its state. A session that resumes reads the first phase that is not
`done` and its notes. A phase's state and notes are updated in the same change that moves it.

Two repositories are involved. **Here** is this tree: the DLL, the link, the server. **Editor** is a
checkout of kryta-cs outside `research/` (the copy under `research/` is a reference and is not
edited).

Fixed for every phase: `Gw.dat` is opened for reading only, and a proof includes that its size and
modification time did not change.

| # | phase | where | state |
|---|---|---|---|
| 1 | Find the client's file functions in build 39167, and read how the map loader opens its file | here | done, 2026-11-26 |
| 2 | The loose-file proof: the client loads a map whose bytes come from a file beside it | here | done 2026-11-26: the client loaded the loose file |
| 3 | Failure safety: a missing or bad loose file ends in the client's own "failed to load", with nothing deleted and nothing written | here | done 2026-11-26: nothing is compiled or written; a file that is not a map ends the client by its own assertion, a cut one is an empty room |
| 4 | The compile captured: a stripped map goes in, the client compiles, the finished map is saved as a loose file | here | done 2026-11-27: the client compiled the stripped file and loaded what it wrote |
| 5 | The editor builds to disk | editor | not decided by the owner (see "The direction"). An experiment exists, 2026-11-27, in `research/kryta-cs`, uncommitted; its window has not been run |
| 6 | The server carries custom maps: the pack names the file, the walk graph is built from it, the link delivers it | here | steps 1 and 2 built 2026-11-27; step 3, the delivery over the link, not started |
| 7 | Import: a heightmap into an editor project; Blender both ways | editor | not decided by the owner; not started |

### Phase 1 - the functions, and the loader

1. Port Reforged's five file resolvers (`C:\Users\Apo\Py4GW_Reforged_Native\offsets\gw_dat_reader.json`:
   `open_file_by_file_id_func`, `file_hash_to_rec_obj_func`, `read_file_buffer_func`,
   `free_file_buffer_func`, `close_rec_obj_func`) into `config/Client/offsets/files.json`, and list
   the file in `config/Client/README.md` and `docs/port/04-configuration.md`.
2. Proof: `$env:KRYTA_GW_EXE = 'F:\GW\GW1\Gw.exe'; cargo test -p kryta-client -- --ignored` prints an
   address for each.
3. Read the client's code around the map loader in build 39167 (the functions the sources name at
   `0x00707650`, `0x00707BB0`, `0x00470D50`, `0x00470F50`, `0x004702B0` in older builds) and record
   here: what the loader is given, which function opens the file, with which stream and mode, which
   function reads it, and what the failure branch calls. This decides between "swap the name" and
   "swap the bytes".

Done when: the five addresses are printed, and "What phase 1 found" below answers the four questions.

### Phase 2 - the loose-file proof

1. A test file with no authoring: a stock map's finished bytes (stream 1), read from the archive with
   this tree's reader (`game/src/Pack/Gwdat.rs`), saved beside the client as
   `Kryta/maps/<file id in hex>.ffna`.
2. In the DLL, a `files` feature beside `titles` (`tools/client-dll/src/runtime/`): the hook phase 1
   chose, answering only for file ids that have a loose file. Every answer is one line in
   `Kryta.<pid>.log`.
3. A pack folder under `maps/Custom/` whose `map_file_id` is an id the archive does not hold.
4. Proof, with a real client: travel to the map; the client stands in it and walks; the log shows the
   loose file served; `Gw.dat` is unchanged. The owner looks himself.

Done when: the owner has seen it.

### Phase 3 - failure safety

1. With the loose file removed, and with a truncated one: the client reports its own failure
   (`0x8C`), nothing is deleted from the archive or the folder, no stream is written, and the client
   exits without "still open".
2. Whatever the failure branch calls that deletes or writes is answered by the DLL for custom ids.

Done when: both cases are in the log as refused, and `Gw.dat` is unchanged.

### Phase 4 - the compile captured

1. For a custom id with only a stripped file (`<id>.stripped.ffna`): the finished open fails, the
   client reads stream 0 from the DLL, compiles, and its write of stream 1 is taken into
   `<id>.ffna`; the retry is served from that file.
2. Proof: the fork's test map (`research/Kryta-aeratemark/data/maps/custom/kryta-test-vale.kmap.json`)
   built to a stripped file by the editor's build, compiled by a client, loaded, walked; `Gw.dat`
   unchanged; `game/src/Terrain/MapData.rs` `import` reads the saved file.

Done when: a map authored outside the game is walked in a client and imported by the server's reader.

### Phase 5 - the editor builds to disk

In a kryta-cs checkout: a build that saves the stripped map as a file, a file id given without an
install, and a publish that writes the pack folder with the map file beside `basic.json`. The archive
install stays out of this route. Its tests run with `python -m pytest tests`.

### Phase 6 - the server carries custom maps

1. The pack names the map file; `docs/port/04-configuration.md` lists it.
2. The walk graph of a custom map is built from its finished file, not from an archive.
3. The link sends a client the files it lacks (`tools/client-link/`; a frame is at most 1 MiB today,
   so a file travels in parts).
4. `docs/port/09-parity-audit.md` and `docs/port/22-client-platform.md` are updated in the same change.

### Phase 7 - import

A heightmap (PNG, TIFF or raw) into an editor project, and Rurik's Blender scripts wired to the
editor's project file.

### Not in the plan yet

A map id and a name of the custom map's own; the client's downloader asking for an id it lacks;
portals between custom maps.

### What phase 1 found

Read in the code of the stock client, build 39167 (`F:\GW\GW1\Gw.exe`), 2026-11-26. Addresses are
that build's, with the image at `0x400000`.

**The five functions resolve.** `cargo test -p kryta-client -- --ignored` with
`config/Client/offsets/files.json`:

| resolver | address | what it is |
|---|---|---|
| `files.open_file_by_file_id_func` | `0x470DB0` | the archive open: `handle (archive, file_id, stream, flags, error*)` |
| `files.file_hash_to_rec_obj_func` | `0x470FB0` | the open by name: `handle (name, flags, error*)` |
| `files.read_file_buffer_func` | `0x470BC0` | the whole file as bytes: `bytes (handle, size*)` |
| `files.free_file_buffer_func` | `0x471430` | `(handle, bytes)` |
| `files.close_rec_obj_func` | `0x46F9E0` | `(handle)` |

Each is 0x60 above the address Eden and Rurik give for builds 38797 and 38833.

**The archive open is the one way into the archive.** The archive's own lookup (`0x471590`) has one
caller, the archive open. The open by name decodes a name that is an encoded file id and calls the
archive open (`0x4710AE`); any other name it copies as a path of at most 0x104 units (`0x471051`)
and opens as a real file. Both return the same kind of handle.

**The map loader is `0x705880`**, called from one place (`0x705E28`, in the function that says
"Creating default map" when it returns nothing). Given the map's name, it:

1. decodes the name to a file id (`0x470360`; 0 when the name is not an encoded id);
2. opens it: a name that is not an id with the open by name, flags 1 (`0x705938`); a file id with the
   archive open, stream 1, flags 1 (`0x705955`);
3. reads the whole file (`0x70596C`), parses it (`0x712390`), frees the bytes and closes the handle;
4. on failure, and only for a file id, logs "Attempting to re-bloat", opens stream 0 with flags 1
   (`0x705A96`), reads it, compiles it (`0x705560`), and tries stream 1 once more;
5. after that says "Map '%s' failed to load" and returns nothing.

So the four questions: the loader is given a name; the archive open opens a map, stream 1, flags 1;
the read is `files.read_file_buffer_func`; the failure branch opens stream 0 of the same file id and
compiles.

**The hook chosen** is neither of the two that were weighed, and is smaller than both: a detour on
the archive open that, for a file id with a loose file, calls the client's own open by name with the
loose file's path and returns its handle. The name the client carries stays the encoded file id, so
every other user of the id is undisturbed; the handle is one the client made itself; and stream 0 or
a write of such an id is answered "not found" the way the client's own function answers
(`0x470F70`: 2 to the error argument, no handle), so the compile and its write cannot start.

**The compile is `0x705560`**, read 2026-11-26. It has two callers, the loader (`0x705BEB`) and
`0x70611B`. It calls `0x711870`, which fills a buffer (pointer at `+0`, size at `+8`) with the
finished map; then it opens the file id with the archive open, stream 1, flags 2, and a null error
argument (`0x7055BD`), writes the buffer with `0x471490 (handle, bytes, size)` and closes. When the
open gives no handle it asks `0x629070` (bit 2 of a flag word of the event context), logs "Map '%s'
could not be opened for writing" unless that bit is set, and returns 0. So an open the DLL refuses
ends the compile with a log line and no write.

**A handle is an archive file or a file on disk, and the write takes both.** The handle holds
`m_archive` at `+8`, `m_standalone` at `+0xC`, `m_fileId` at `+0x10`, the stream at `+0x14` and
`m_flags` at `+0x18` (the assertions of `0x471490`). The open by name gives its flags to the
standalone open (`0x475F30`) and stores them in the handle (`0x471133`); the write sends a
standalone handle to `0x475FB0` (`0x471572`). This is the way phase 4 would take: answer the
compile's open for writing with the open by name of a loose path, flags 2, so the client writes the
finished map to disk itself. The open of a file on disk (`0x476F10`, through `0x476DF0`) hands the next layer the write bit
of the flags (`0x476E8C`), and that layer (`NtFile.cpp`, `0x484330`) opens with read and write
access and makes the file when the bit is set (`0x4843A1`: disposition 4, else 3). It does not cut
a file that is already there.

**The loader's failure branch with the DLL's refusal** (phase 3, by reading only): the open of
stream 0 at `0x705A96` gets no handle and the branch leaves at `0x705CBD` without reaching the
compile.

From `0x705CBD` the loader asks `0x629070`, logs "Map '%s' failed to load" unless that bit is set,
and returns nothing; its caller then says "Creating default map". Nothing is deleted and nothing is
written on that way out.

Not read yet: `0x70611B`'s caller; whether anything asks the file server
for a file id the archive lacks.

### What phase 2 has, and what it waits for

Built, 2026-11-26:

- `tools/client-dll/src/map_files.rs`: the folder (`Kryta\maps\` beside the client), the file name
  (`<file id, eight hex digits>.ffna`), and the rule for one open; five tests.
- `tools/client-dll/src/runtime/file_hooks.rs`: the detour. It is installed only when the folder
  holds a file, and a build the two offsets do not fit runs without it.
- `cargo test -p kryta-client`: 22 pass. `cargo build -p kryta-client --release --target
  i686-pc-windows-msvc` builds the DLL.
- The test file: stock map file 113021 (Ascalon City, pre-Searing), its finished form, 2,925,267
  bytes, read from the archive with the archive opened for reading; the archive's size and
  modification time were the same after. In this archive the client cannot address file id
  `0x5F100` (389376), so that is the id the test names.

The owner's run, 2026-11-26, client `Gw.kryta.exe`, process 22408, with map 148's `map_file_id`
set to 389376 for the run and put back after:

- `Kryta.22408.log`: `map files: hook installed for 0x5f100 in F:\GW\GW1\Kryta\maps`, then
  `map files: file 0x5f100 stream 1 was opened from F:\GW\GW1\Kryta\maps\0005F100.ffna`. No
  `is not served` line: the client asked for nothing else of that file id.
- The owner entered the map and reported it normal.
- The archive has no file id `0x5F100` after the run (its file table, read with the archive opened
  for reading).

Two things the run does not settle:

- `Gw.dat` is not a measure. Its size went from 3,662,074,880 to 3,658,322,944 bytes and its time to
  the moment the client ended; its time before the run was the end of the previous session. The
  client writes its own archive in a run with no loose file. Not compared: the same session without
  the loose file.
- The log ends with four `FAULT 0xc0000005 executing ... (no module)` lines with the display
  driver (`AMDXN32.DLL`) on the stack, eight seconds after the map opened and in the same second
  the archive was last written. No earlier log has them. The owner closed the client after seeing the map
  (owner, 2026-11-26), and they are the log's last lines, so they are at the client's end; no
  address of the client or of `Kryta.dll` is on their stacks. Not compared: the end of a session
  without the loose file.

### What phase 3 has

The owner's run, 2026-11-26, process 15140, with the loose file cut to its first 65,536 bytes:

- `Kryta.15140.log`: the file was opened from the loose path, and there is no `is not served` line.
  The client did not ask for stream 0 and did not try to write: it took the cut file as a map.
- The owner was placed in a dark room with a white floor; no message, no crash.

- After the client closed: the archive has no file id `0x5F100`, and the same 147,555 entries as
  before. `Gw.dat` was 3,662,074,504 bytes while the client ran and 3,658,322,944 after it closed,
  the size it had after the first run closed: the client grows and trims its archive in a session,
  and the size is no measure of what this feature does.
- This log has no `FAULT` line at the client's end, so the four of the first run are not a
  consequence of the hook being installed.

So a file that is cut short is not refused by the client, and the failure branch read in phase 1 was
not taken.

The second run, 2026-11-26, process 21192, with the loose file replaced by 49,152 bytes of text:

- The client's own error log, in order: "Map '0x05f100' is not a valid riff type", "Map file
  '0x05f100' failed to load.  Attempting to re-bloat.", "Map '0x05f100' failed to load", "Creating
  default map". That is the branch read in phase 1.
- `Kryta.21192.log`: stream 1 was opened from the loose path, then `file 0x5f100 stream 0 flags 0x1
  is not served; the client was told it is not found`. No open for writing was asked.
- The client then stopped on its own assertion, `found`, `P:/Code/Engine/Map/Map.cpp(1762)`, inside
  the handling of the spawn-point message (`0x0196` and `0x5f100` are on its stack).
- The archive has no file id `0x5F100` and the same 147,555 entries.

What phase 3 establishes: for a file id the DLL holds, the client compiles nothing and writes
nothing, whether the file is a map, a cut map, or not a map. What it does not give: a soft failure.
A file that is not a map ends the client, by the client's own assertion after "Creating default
map"; a cut one shows an empty room. Neither is told to the player. So a file is checked before a
client is sent to it, which is phase 6's: the server reads the same bytes
(`game/src/Terrain/MapData.rs`, `import`) and refuses a map whose file it cannot read.

### What phase 4 has

Built, 2026-11-26:

- `tools/client-dll/src/map_files.rs`: a second file name, `<file id>.stripped.ffna`. A file id
  with a stripped file and no finished one is answered: the read of stream 0 from the stripped
  file; the open of stream 1 for writing with the path `<file id>.ffna`, which the client's own
  open makes and its compile writes; the read of stream 1 from that path once a file with bytes is
  there, and "not found" before, which is what sends the loader to its compile. A finished file
  that is there at the client's start wins and the map is not compiled again; to compile again,
  delete it. Seven tests.
- `tools/client-dll/src/runtime/file_hooks.rs`: the same detour, one more answer.
- The test file: the fork's test map (`research/Kryta-aeratemark/data/maps/custom/
  kryta-test-vale.kmap.json`, 64 by 64 cells) built by the editor's build as it stands
  (`research/kryta-cs`, `kryta_cs/pipeline.py`, `build`, which reads the archive for its donors and
  writes nothing): 12,413 bytes, twelve chunks of stage 1. The archive's size and time were the
  same after.

The owner's run, 2026-11-27, with `Kryta\maps\` holding only `0005F100.stripped.ffna` and map 148
given `map_file_id` 389376 and the test map's spawn point (4599.12, 2760.34) for the run: the owner
stood in a map they did not know, drawn with the game's own textures.
`Kryta.15100.log`, in order: stream 1 not served; stream 0 opened from the stripped file; stream 1
opened for the client's compile to write to `0005F100.ffna`; stream 1 opened from `0005F100.ffna`.
The file the client wrote is 66,510 bytes: twelve chunks of stage 2 (the path chunk `0x20000008`
is 29,742 bytes). The server's `import` reads it: 3 planes, 274 trapezoids, and the spawn point
lies in a trapezoid of plane 0. Map 148 was put back after the run.

After the client was closed, the archive's file table has 147,555 entries, as before the run, and
no file id `0x5F100`: the client compiled the map and wrote nothing of it into the archive.

### What phase 5 has

An experiment, made before the owner had decided how maps are authored, and against this plan's
own line that the copy under `research/` is not edited. In the local copy of the editor,
`research/kryta-cs` (aeratemark's repository; the changes are in its working tree and are not
committed), 2026-11-27:

- `kryta_cs/pipeline.py`: `export` builds the map and saves it as
  `Kryta\maps\<file id>.stripped.ffna` beside the chosen archive, the name and folder the DLL
  reads; `assign_file_id` gives a new map the lowest id the archive does not bind and no file in
  that folder has, without an install; the finished file a client compiled from an earlier build
  is removed, so the client compiles the new one. The archive is opened for reading only.
- `kryta_cs/kryta.py`: `publish` takes the map file and copies it beside `basic.json` in
  `maps/Custom/<map id>/`; `kryta-cs.json` names it.
- `kryta_cs/ui/deploy_window.py`: a "Build to disk" button that runs check, export and publish.
  The archive route ("Build && Deploy") is as it was.
- `README.md`: the two routes.
- `publish` also takes the finished file a client compiled (`pipeline.compiled_path`, the
  `<file id>.ffna` beside the client) and copies it into the pack folder; a publish of a new build
  that no client has compiled removes the copy of the old one.

Proof so far: the editor's tests with the real archive (`KRYTA_GW_DAT=F:\GW\GW1\Gw.dat`,
`py -3 -m pytest tests`): 95 pass, among them an export of a new 32 by 32 map whose file starts
`ffna` type 3, a second map that does not take the first one's id, the removal of a stale finished
file, and the archive's size and first megabyte the same after. Two tests do not run on this
machine because PySide6 is not installed in either Python; for the same reason the window with the
new button has not been opened. A game map opened from the archive is refused by `export`.

### What phase 6 has

Steps 1 and 2, 2026-11-27:

- The pack names a map's files by rule and not by a field: `<file id>.stripped.ffna` and
  `<file id>.ffna` in the map's folder, the file id being the record's `map_file_id` as eight
  hexadecimal digits, which is the name the DLL reads beside a client. `game/src/Maps/Pack.rs`
  (`MapFiles`, `Pack::map_files`) finds them; `docs/port/04-configuration.md` lists them.
- `worldserver/src/main.rs`, `load_map_files`, at start-up after the cache is read: each stripped
  file is checked to be a map container, and each finished file is imported
  (`PathGraphs::insert_file`, `game/src/Terrain/Pathfind.rs`, which calls the same `import` an
  archive blob goes through) and its graph bound to the map. A file that is not a map stops the
  program, which is the refusal phase 3 asked for. A map with a stripped file and no finished one
  is served, and the log says it has no graph.
- Proof: `a_map_file_that_is_not_a_map_stops_the_start`, and
  `a_finished_map_file_gives_the_map_its_walk_graph` run with the file the client wrote in the
  phase 4 run (`KRYTA_FINISHED_MAP`, `KRYTA_FINISHED_MAP_SPAWN=4599.12,2760.34`): map 148 is
  bound, and a walk from the spawn point to a point 100 units east is `Found`, in trapezoid 124.

Not done: no world has been started with a custom map in its pack and walked by click in a client.
Nothing is sent to a client: the two files are put beside it by hand or by the editor (step 3).
The finished file reaches the pack only by the editor's publish, from the folder of the client
that compiled it.

## The delivery plan (2026-11-27)

Written after the owner's decisions of 2026-11-27, and waiting for the owner's word before any code.
It replaces phase 6 step 3 and the folder beside the client. Everything in it is Rust, in this tree.

The owner's answers it is built on:

- **The server holds the maps and serves them**; it instructs the DLL how to get each file and
  states a checksum, so a changed map is fetched again.
- **A local copy is a cache, made automatically.** The user never takes part.
- **One cache for a machine, not one for each client.** Several clients run on one system, and a
  copy for each would hold every file as many times as there are clients.
- **The server requests every map, and each is copied silently in the background** (owner,
  2026-11-27), so it is ready when it is needed. This is the normal way a map arrives, and it
  happens for every map the server hosts, whether or not the player ever enters it.
- **Only a map that is missing or not ready at load is fetched in the client's load phase**, and the
  load is held until it is whole (owner, 2026-11-27: "the game streams its contents live and we
  should do the same"). This is the exception, not the way maps normally arrive.
- **The server serves finished maps.** Delivery is that and nothing more; no client is asked to make one.

| # | step | proof | state |
|---|---|---|---|
| D1 | The archive reader reads a file's other streams (`game/src/Pack/Gwdat.rs`), ported from its source, so a stock map's stripped stream is a test input made in Rust | a test that reads a stock map's stripped stream and finds it a map container of stage 1 | done 2026-11-27: `GwDat::stream`, ported from Rurik's reader; `a_map_has_a_finished_stream_and_a_stripped_one` with the real archive reads file `0x1B97D`: stream 1 is 2,925,267 bytes with records of stage 2, stream 0 is 919,891 bytes with records of stage 1, and there is no third. `KRYTA_STREAM_OUT` saves the two as files |
| D2 | Taken out of this plan (owner, 2026-11-27): the server serves finished maps, and that is the whole of delivery. How a finished map is made from an author's file is a question of authoring, which the owner has not decided, and is not asked again here. The pack holds the finished file `<file id>.ffna`; for tests it is a stock map's finished stream, read by D1 | - | not part of delivery |
| D3 | The link carries a map list: for each custom map of the pack, its file id, size, checksum and how to get it (`tools/client-link`, `authserver/src/Server/client_link.rs`) | the link's tests; the DLL's log lists what the server announced | built 2026-11-27: `Message::Maps` (link protocol 2), sent after the titles and listing each finished file of the pack with its size and SHA-256; `ClientLink::host_maps`. Tests: `a_hosted_map_is_listed_and_sent_in_parts`, `a_file_that_is_not_a_map_is_not_hosted`. Not yet seen in a client's log |
| D4 | The link carries a file in parts, because a frame is at most 1 MiB; and a file may instead be named by a path the server gives | a file larger than one frame arrives whole and its checksum matches | the link half built 2026-11-27: `Message::MapWanted` and `Message::MapPart`, parts of 256 KiB as hexadecimal text; the test sends a file of three parts and compares its bytes. A file named by a path the server gives is not built |
| D5 | The DLL's cache: one folder for the machine, a file stored under its checksum, written to a temporary name and renamed so two clients that fetch the same file do not spoil it; a background task fetches what the list names and the cache lacks | two DLL instances, one cache, one copy; a changed checksum is fetched again | built 2026-11-27: `tools/client-dll/src/map_cache.rs`, the cache at `%LOCALAPPDATA%\Kryta\maps\<sha256>.ffna`; the link's own thread fetches. Its tests cover a second store of one file, a file whose bytes no longer give its checksum, and a changed checksum. Not yet run with two clients |
| D6 | The DLL serves a map from the cache; a map entered before its file has arrived waits for it. Only finished files are served, so no client compiles | the owner's client run: a custom map loads with nothing placed by hand; the archive check in Rust passes after | done 2026-11-27, by the owner's run. For it map 148's folder held `0005F100.ffna`, the finished stream of the game's own file `0x1B97D` as D1's test saved it, and its `map_file_id` was 389376; nothing was placed on the client's side. The owner entered map 148 and saw it as it always is. `Auth.log`: "1 map file(s) hosted", "pid 23336: listed 1 map file(s)", "sent map file 0x5f100, 2925267 byte(s)". `Kryta.23336.log`: "the server lists 1 file(s), 1 of them to fetch", "file 0x5f100 fetched, 2925267 byte(s), into C:\Users\Apo\AppData\Local\Kryta\maps\5d79...7256.ffna", and twenty seconds later "file 0x5f100 was opened from" that file. `Server.log`: "Map 148: walk graph read from its map file". After the client closed, the archive check in Rust: 147,555 file ids, no `0x5F100`. Map 148 was put back. Not shown by this run: a second client using the same cache, a changed map being fetched again (both are in the Rust tests only), and whether the owner walked by click |
| D8 | A map is fetched on the load that needs it and the load is held until it is whole; every other listed map is fetched in the background, one after another | the DLL's tests and the link's test; the owner's client run: a map entered with an empty cache loads after a wait, and the log shows the rest arriving | built 2026-11-27, replacing D5's fetch of the whole list at connect. `Served::answer` looks a listed file up in the cache when a load opens it, asks the server from the load's own thread (`file_hooks::ask`) when the cache lacks it, and returns when the file is stored; the wait ends when no part has arrived for 120 seconds or the link drops. `Served::fetch_all`, on a thread started for each list, does the same for every listed file in turn. A load that comes while a background file is arriving waits for that file's remaining parts first, because the server answers requests in order. Tests: `a_load_asks_for_its_file_and_waits_until_it_is_whole`, `the_other_listed_maps_are_fetched_in_the_background`, `a_load_fetches_a_hosted_map_into_the_cache_and_a_second_client_does_not`. The owner's run, 2026-11-27, with an empty cache and the pack of D6's run: `Kryta.14748.log` has "the server lists 1 file(s)", one second later "file 0x5f100 fetched, 2925267 byte(s), into" the cache and "1 of the 1 listed file(s) are in the cache", and thirteen seconds later "file 0x5f100 was opened from" that file; the owner saw the map as it always is. That run shows the background copy; the map was whole before the load, so no load was held in a client yet. Map 148 was put back. To try a held load on a machine where a map arrives in a second, the server can be told to send slowly: `ClientLink.MapPartPauseMs` in `authserver.json` waits that long before each part (test `a_map_sent_slowly_takes_the_pauses_and_arrives_whole`). The owner's second run, 2026-11-27, with the pause at 5000 and an empty cache: `Auth.log` has "map files are sent slowly, 5000 ms before each part"; `Kryta.28936.log` has the list at second 178, "file 0x5f100 fetched" at second 238, and "file 0x5f100 was opened from" the cached file in that same second, so the load was waiting when the file became whole and went on at once; the owner saw the map load and the client stayed connected. The log does not say when the load began to wait, so the length of the hold is not measured. The pause and map 148 were put back. Not known beyond that run: whether the client's connection to the world server survives a long hold, since the hold is inside the client's file open |
| D7 | The folder `Kryta\maps\` beside the client and the code that scans it are removed | the DLL's tests | done 2026-11-27: `map_files.rs` is deleted and the DLL reads no folder beside the client. The two test files of the earlier runs are still in `F:\GW\GW1\Kryta\maps\`, read by nothing |

**Where the cache is (owner, 2026-11-27):** a folder under the Windows user's local application
data, `%LOCALAPPDATA%\Kryta\maps\`. Not a temporary folder, which would be fetched again each
time, and not a client's own folder, which would be one copy for each client.

**Windows and Linux (owner, 2026-11-27).** The project runs on both. The server's side of this
plan is portable Rust and uses no Windows location; the pack's map files are found by their exact
names, upper-case hexadecimal, because a Linux file system tells cases apart. The cache is on the
client's machine, inside the DLL, and the game client is a Windows program: on Linux it runs under
Wine or Proton, where `%LOCALAPPDATA%` is a folder of the Wine prefix
(`<prefix>/drive_c/users/<user>/AppData/Local/`). So the cache is one for each Wine prefix, as it is
one for each Windows user. Not tried on Linux. Open for the owner: whether clients that run from
different prefixes on one Linux machine should share one cache, which needs a location the
operator or the server names.

**The server orchestrates, never the client (owner, 2026-11-27).** The operator configures the
server and runs it; the maps it hosts are the maps its clients have. The list the server sends is
the whole truth for a client connected to it: the DLL fetches what the list names, serves nothing
the list does not name, and learns of a change from the list (a new checksum, a map added, a map
gone). The user chooses nothing and keeps nothing: no setting, no file to place, no say in which
maps stay. An operator may host a wholly different set of maps, and its clients follow.

**A pre-load pack is allowed (owner, 2026-11-27).** A server may offer its maps as one pack (the
owner's example: a zip file) that a user loads ahead of time. This is the one place a user acts,
and knowingly. It changes nothing above: the pack only fills the cache early, the server's list
and checksums still decide what is served, and a file from a pack whose checksum is not the one
the server states is fetched again like any other. Not in the steps below yet; it comes after D6.

**One folder for each server flavor, and no stale map in it (owner, 2026-11-27).** A flavor is the
build and setup a server runs: the owner's examples are "vanilla" and "fantasy_modded". The server
names its flavor with its map list, and the client keeps that flavor's maps in a folder of that
name under the cache, `%LOCALAPPDATA%\Kryta\maps\<flavor>\`. The folder is kept equal to the list
of the server connected to: a map stays for as long as that server lists it, a changed map
replaces its old version, and a map the server no longer lists is removed. Nothing expires by age.
Another flavor's folder is that flavor's server's to keep, and is not touched. Built 2026-11-27
(D9): `Message::Maps` carries `flavor` (link protocol 3), which today is the world pack's folder
name; `Cache::of_flavor` and `Cache::sweep` in `tools/client-dll/src/map_cache.rs`; test
`a_flavors_folder_holds_only_what_its_server_lists`. A map that two flavors both host is stored
once in each folder. Not yet run with a client.

## For the user manual

There is no user manual yet. The owner's word, 2026-11-27: the technical limits are pointed out in
it when it is written. This list is what it takes from this document; a limit found later is added
here in the same change that finds it.

For a player:

- A client needs `Kryta.dll`, and it needs the server: it does not run detached
  (`docs/port/22-client-platform.md`).
- The maps a server hosts are fetched by the client without the player doing anything, and kept in
  a cache: `%LOCALAPPDATA%\Kryta\maps\<the server's flavor>\` on Windows. A map the server stops
  hosting is removed from that folder the next time the client connects to it. Entering a map that is not in the cache yet
  holds the loading screen until the map has arrived; the other maps arrive in the background
  while the client runs.
- On Linux the client runs under Wine or Proton, and the cache is inside the Wine prefix
  (`<prefix>/drive_c/users/<user>/AppData/Local/Kryta/maps/`). Clients run from different prefixes
  each keep their own copy. Not tried on Linux yet.
- The cache is one for each Windows user, not one for the machine: two Windows users of one computer
  each fetch the maps.
- Deleting the cache loses nothing: the maps are fetched again.
- `Gw.dat` is never written by any of this.

For an operator:

- A map is served only as a finished map file, `<file id>.ffna` in the map's pack folder. A map
  that has only a stripped file is not served to clients.
- The client's DLL and the server speak one link version and refuse each other otherwise: they are
  updated together.
- A map file is held in the server's memory and sent in parts over the link. Serving a file from a
  path the server names (a shared drive) is not built, and neither is the pack a user loads ahead
  of time.
- The server reads its map files once, at start-up: a changed map reaches clients after a restart.
