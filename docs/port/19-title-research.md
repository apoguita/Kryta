# Title research: what a title is, what the projects prove, and what remains

Status: research, and the record of the title slice implemented in commit `7679ba1`. That
slice reads `config/World/Vanilla/titles.json` and `vanilla-title-wire.json`
(`game/src/Characters/Titles.rs`), sends the title messages during load
(`game/src/Server/Packets/TitlePackets.rs`), answers a title's display and hide (`0x8058`, `0x8059`)
with `0x00F4` to the player and its peers, keeps a metric catalog with every value at zero
(`game/src/Characters/Metrics.rs`), and publishes custom title text at
`GET /kryta/custom-titles.json` (`game/src/Portal/mod.rs`). `09-parity-audit.md` section 5
records the packets as ported thin from the retail and Preforged recordings, and the custom text
route as a feature with no source. Title earning and persistence are not implemented.

The short finding: every project agrees a title is a **client-visible projection
record**, not a raw metric. No project on disk implements a general metric layer from
which titles are inferred. The closest artifacts are the four working title types plus
a deprecated value, a measured title packet cluster, and a complete imported title
profile for one official character.

## Sources inspected

A location under `research/` is relative to this repository. `ASURA/`, `_fork_asura_test/`,
`Rurik/`, `Eden/`, `PreSearing/` and `WowWarband/` are under `C:\Users\Apo\Documents\GitHub\`;
`Py4GW_Stealth/` and `Py4GW_Reforged/` are under `C:\Users\Apo\`.

| project | location | title surface |
|---|---|---|
| ASURA | `ASURA/` | packet table rows for `0x00F3`, `0x00F4`, `0x00F6`; no title handler in `crates/asura-world` |
| ASURA_testfork | `research/ASURA_testfork/` | same packet table; `research/ASURA_testfork/docs/23-initialization-protocol.md:89` records titles as not sent |
| _fork_asura_test | `_fork_asura_test/` | same surface; no title implementation |
| Rurik | `Rurik/` | measured title protocol and client probe; account-level hand-authored title store |
| Eden | `Eden/` | full client decode and UI semantics for `0x00F3`-`0x00F6` and CtoS 88 |
| Headquarter | `research/Headquarter/` | client handlers, `Title` struct, `TitleModeFlags`; handlers are no-ops |
| OpenTyria | `research/OpenTyria/`, `PreSearing/code/OpenTyria/` | 48-title enum, `TitleType`, empty rank tables, dead `titles` table |
| GameRevision GWLP-R | `research/GameRevision-GWLP-R-4208ffa/` | protocol shells `P231`, `P232`, `P234`; no storage or update shell |
| preforged | `research/preforged/` | official title/faction capture and live import |
| preforged-source | `research/preforged-source/` | title profile, rank/track storage, account/character merge, restore packets |
| entice | `research/entice/` | no title code |
| preservation-main | `research/preservation-main/` | no title code |
| prototype-offline | `research/prototype-offline-20261007-v1-apoguita-en/` | no title code |
| WowWarband | `WowWarband/` | Azeroth titles and achievements, not Guild Wars titles |
| Py4GW Stealth | `Py4GW_Stealth/` | native `TitleStruct`, `TitleTierStruct`, title enum and thresholds |
| Py4GW Reforged | `Py4GW_Reforged/` | `TitleID`, `TITLE_NAME`, `TITLE_TIERS`, `TITLE_CATEGORIES` |

## What a title is

A title is a named, scoped, ranked projection over one or more server metrics, plus a
per-character display selection. It is not an achievement with a boolean completed
state, and it is not a single counter.

The parts the client needs are:

- a fixed title ID in the client's 48-row title table;
- current points;
- a current rank ID and its minimum;
- a next rank ID and its minimum, or a terminal marker;
- the maximum rank and its rank ID;
- two encoded template strings (progress text and hint/name);
- a selected title, represented to the client as a tier ID.

The parts the client does not receive are the earning rules. The protocol carries the
projection, not the source metric.

## The title packet cluster

Modern client builds use four server messages and two client messages:

| opcode | name | fields |
|---|---|---|
| `0x00F3` / 243 | `TITLE_RANK_DATA` | rank id, flags, rank number, encoded name |
| `0x00F4` / 244 | `TITLE_RANK_DISPLAY` | player number, rank id |
| `0x00F5` / 245 | `TITLE_UPDATE` | title id, new current points |
| `0x00F6` / 246 | `TITLE_TRACK_INFO` | title id, flags, points, current rank id, current minimum, extra dword, next rank id, next minimum, rank count, max rank id, two encoded strings |
| CtoS `0x0058` / 88 (`0x8058` on the wire, with the client's direction bit) | `TITLE_DISPLAY` | title id |
| CtoS `0x0059` / 89 (`0x8059` on the wire) | `TITLE_HIDE` | none |

Rurik measured the modern `0x00F6` field map and the client's behaviour
(`Rurik/studies/newopcodes/FINDINGS.md:1463-1477`,
`Rurik/studies/character/RUNS.md:188-202`):

- word 1 is the title id, indexing the client's 48-row `s_titleClientData` and the
  `ctx+0x81c` title array;
- word 2 is flags; bit 0 makes the paired value render as tenths;
- word 3 is current points, and it is the only field `0x00F5` patches;
- words 4 and 7 are rank IDs into the `0x00F3` table;
- words 5 and 8 are those ranks' minimum values;
- word 6 is unread by Rurik's analysis;
- word 9 is a rank count, unverified;
- word 10 is the max rank id;
- words 11 and 12 are printf-style templates, not captions.

The client render was observed: the row name is the current rank's `0x00F3` string, the
bar denominator is the next rank's minimum, and `0x00F5` moves the bar. A `string16(8)`
field admits at most 7 code units; at-cap is a client hangup. A `0x00F6` that references
a rank not previously sent is silent on receive and crashes at render (`Array.h(587)`).
Raising the field to `string16(16)` lets the row display a longer name, but hovering it
asserts in `TextEncode.cpp` while reading a coded string. The experiment also widened
both track strings and broke their decoded field layout, so it did not isolate the
rank-name length.

Rurik's client-side handler also names the extra word as `title_mode_flags`:

```text
TitleModeFlags_None          = 0
TitleModeFlags_DhuumCovenant = 1
TitleModeFlags_ReforgedMode  = 2
```

`research/Headquarter/code/client/title.c:46-63` and
`research/Headquarter/include/client/constants.h:258-262` carry that naming. Rurik calls
the same word unread; the preforged capture has zero in that slot for every track. It is
metadata about how a rank was earned, not a points field.

## Two protocol generations

The projects do not share one opcode numbering scheme.

- Preforged's 2022 client uses `242` rank data, `243` display, `244` update, `245` track
  status (`research/preforged-source/docs/progression-integration.txt:42-60`).
- Rurik and the ASURA captures use `243` rank data, `244` display, `245` update, `246`
  track status.
- GameRevision carries shells numbered `P231`, `P232`, `P234`; its own `getHeader()`
  values are not the modern opcodes.

The 2022 preforged track record has eight dwords after the title id. The modern record
has nine: the extra dword is at index 4, and preforged's converter drops only that field
while rejecting unknown nonzero extensions
(`research/preforged-source/docs/progression-integration.txt:128-136`,
`research/preforged-source/src/runtime/gw_progression.h:34-37`).

## The title-type classification

OpenTyria names four working types plus a deprecated type
(`research/OpenTyria/code/OpenTyria/GmTitles.h:83-89`):

```text
TitleType_Points       = 0
TitleType_Cartographer = 1
TitleType_Missions     = 2
TitleType_Deprecated   = 3
TitleType_Account      = 4
```

A preforged official-client capture (`research/preforged/bin/skills/
skills-20261007-134151-12248-47332500.jsonl`, record `progression_snapshot`) contains:

- 48 title tracks, one per title id;
- 367 rank-tier records;
- 88 named rank records;
- 31 terminal rank records with `rank = 0xFFFFFFFF`;
- 44 populated tracks.

The `flags` values in the 48 tracks are exactly the working types:

| flags | type | examples in the capture |
|---|---|---|
| `0` | points | Survivor, Drunkard, Sunspear, Lightbringer, EotN reputations, KoaBD, Party Animal, Sweet Tooth, Master of the North |
| `1` | cartographer / per-mille | Tyrian, Canthan, Elonian Cartography |
| `2` | missions / count | Protector, Guardian, Skill Hunter, Vanquisher, Legendary tracks, LDoA |
| `4` | account | Hero, Gladiator, Champion, Kurzick, Luxon, Lucky, Unlucky, Gamer, Zaishen, Treasure Hunter, Wisdom, Codex |

That is a title-type classification, not a metric system. It says how a title's points
are shaped, not where the points come from.

The tier table is separate from the tracks. A track references rank IDs; the rank table
holds the rank number, flags, and encoded name. Rurik's measured fields agree: words 4
and 7 are rank IDs, words 5 and 8 are the corresponding minimums.

## Account and character scope

The wiki and the capture support two scopes:

- account titles share progress across the account's characters;
- character titles progress separately;
- display selection is per character even for account titles.

Preforged implements the split. Its `account_title` list is
`0, 3, 4, 5, 6, 15, 16, 23, 44, 45, 46, 47`
(`research/preforged-source/src/runtime/gw_progression.h:95-97`). It extracts those
tracks into `fw_gw_account_titles`, merges them into the character profile on load, and
strips them from the character's own profile
(`research/preforged-source/src/runtime/accounts_character_progression.inc:10-11`,
`research/preforged-source/src/runtime/gw_progression.h:98-112`).

Rurik's `charstore` does not split them: `title_ranks` and `titles` both live under
`account` (`Rurik/toolkit/authsrv/charstore.py:137`, `:312-336`). Its own study states
the correct split at `Rurik/studies/character/STORAGE.md:24-40`, but the store cannot
represent a character title separately. Rurik is therefore a presentation store, not a
scope model.

OpenTyria's dead table carries both `account_id` and `char_id`
(`research/OpenTyria/db/seed.sql:159-170`), but no code reads or writes it.

Preforged's account list omits Commander (`22`), which the wiki lists as an account
title but which is retired. That is an open discrepancy, not a resolved fact.

## The raw metric families around titles

The title protocol does not carry the source metrics. The source families are separate
protocol surfaces. The following are the ones the projects expose.

| metric family | evidence | shape |
|---|---|---|
| XP, level, skill points, morale | `0x00E9` fields 0 XP, 9 level, 10 morale, 13 SP; `0x00EE` deltas (`Rurik/studies/character/FINDINGS.md:1183-1203`) | scalar + delta |
| faction current, lifetime, cap | `0x00E9` fields 1/3/5/11; caps `0x00EA`-`0x00ED` (`Rurik/studies/character/STORAGE.md:80-118`) | current + lifetime + cap |
| mission completion | `0x0094`, five arrays: primary, secondary, hard-mode primary, hard-mode secondary, unlocked (`Eden/crates/eden-network/src/game/typed/maps.rs:113-120`) | bitset |
| vanquished areas | `0x00FA`, one bit per mission (`maps.rs:169-173`) | bitset |
| challenges | `0x004B`, completed challenge bits (`maps.rs:186-189`) | bitset |
| exploration | `0x008B` dimensions + `0x008A` RLE payload + `0x008C` incremental mark (`Rurik/studies/minimap/FINDINGS.md:205-247`) | continent block grid |
| skill libraries | `0x001D` account library, `0x00DB` character library, `0x00DA` bar | sets of skill ids |
| items | `0x0144`, `0x0161`, `0x013E`, `0x013F` | item rows and events |
| deaths and morale | `0x009C`, `0x00EE` attr 10, status flags | event + scalar |
| title projection | `0x00F3` rank table, `0x00F6` track, `0x00F5` update, `0x00F4` display | points + rank ids |

These are state surfaces, not earning rules. A mission bit says a mission is complete;
it does not say Protector requires primary and secondary in normal mode. A faction
current says a number; it does not say the Kurzick or Luxon award. A fog grid says a
block is explored; it does not say the Cartographer denominator.

## What each project actually proves

### ASURA, ASURA_testfork, _fork_asura_test

The packet table names the cluster and records the captures. ASURA_testfork's world
does not send titles (`research/ASURA_testfork/docs/23-initialization-protocol.md:89`).
No handler, entity, or persistence exists in `crates/asura-world`.

### Rurik

Rurik proves the client behaviour and the field map. Its title store is hand-authored:
`title_ranks` is a dictionary of rank definitions, `titles` holds points and rank
references, and the server sends them in order (`charstore.py:312-336`,
`toolkit/authsrv/authsrv.py:43564-43594`). It does not compute points from gameplay.

### Eden

Eden proves the client side: tier table insertion, track rows, selected player tier,
`0x00F5` patch, ordering, bar arithmetic, and the display-time check
`display_tier <= current_tier`. It is a client model, not a server store.

### OpenTyria and PreSearing OpenTyria

OpenTyria contributes the 48-title enum, the five-value `TitleType`, and the dead
`titles` table with both account and character keys. Its rank arrays are all empty
(`research/OpenTyria/code/OpenTyria/GmTitleConstData.c:3-15`), the send site is
commented (`research/OpenTyria/code/OpenTyria/GameSrv.c:1473`), and nothing reads the
table. It is scaffolding and taxonomy only.

### Headquarter

Headquarter contributes the client packet structs and the `TitleModeFlags` name. Its
handlers assert the packet size and do nothing else
(`research/Headquarter/code/client/title.c:6-71`); its `Title` struct holds only title
id, current/max rank, points in current rank, and points to rank up
(`research/Headquarter/code/client/title.h:6-13`). It has no state population.

### GameRevision GWLP-R

GameRevision contributes protocol shells for rank data, rank display, and track status
(`research/GameRevision-GWLP-R-4208ffa/protocol/src/main/java/gwlpr/protocol/gameserver/outbound/P231_TitleRankData.java`,
`P232_TitleRankDisplay.java`, `P234_TitleTrackStatus.java`). It has no update shell for
`0x00F5` and no title storage. Its numbering is its own.

### preforged and preforged-source

Preforged is the most complete. It has:

- `Rank { props, number, name }` and `Track { eight dwords, progress, hint }`
  (`research/preforged-source/src/runtime/gw_progression.h:34-37`);
- validation of 48 tracks, excluded 8/11/12, required current rank, optional terminal
  next rank, and current points not below the current rank minimum
  (`research/preforged-source/src/runtime/gw_progression.h:72-79`);
- account/character split and merge
  (`research/preforged-source/src/runtime/gw_progression.h:95-112`);
- persistence in `fw_gw_factions`, `fw_gw_progression`, and `fw_gw_account_titles`
  (`research/preforged-source/src/runtime/accounts_progression.inc:24-54`);
- restore packets for stats, caps, rank definitions, tracks, and active title
  (`research/preforged-source/src/runtime/gw_progression.h:84-89`);
- select/hide handling and unearned-track refusal
  (`research/preforged-source/src/runtime/gw_progression.h:82-83`);
- an official capture import, a legacy account-title seed, and a live client render of
  the imported titles
  (`research/preforged-source/docs/progression-integration.txt:120-169`).

It still has no earning rules. `.titlepoints` sets points directly
(`research/preforged-source/src/runtime/gw_development_commands.h:29`). The
documentation says title threshold catalogs and account-bound aggregation remain
unfinished (`research/preforged-source/docs/progression-integration.txt:58-60`,
`:153-155`). The capture is reference input, not a computed catalog
(`research/preforged/bin/skills/README.txt:171-184`).

### entice, preservation-main, prototype-offline

No title code. Their absence is recorded because a search across all projects should
not leave a false impression that one of them holds a hidden implementation.

### WowWarband

WowWarband's titles are Azeroth's: `chartitles_dbc`, `player_factionchange_titles`, and
`cs_titles.cpp`. It is the port's structural reference, not a source for Guild Wars
title metrics.

### Py4GW Stealth and Reforged

These are the client-side models: `TitleStruct`, `TitleTierStruct`, `TitleID`,
`TITLE_TIERS`, `TITLE_NAME`, and `TITLE_CATEGORIES`. Reforged's `TITLE_TIERS` supplies
threshold names and values; Stealth's `TitleStruct` supplies the 0x2C native record.
Neither has a server metric layer.

## Client string experiment (2026-11-21)

Build 39167, `F:\GW\GW1\Gw.exe`. The server sends the 48 rows; every metric is zero and
the rows carry a display-only `1`. The extra title's full name in this experiment was
`Jump Master`; its client rank name is `Jump`; its next rank is `Ace` at `10,000`. The
catalog now names that track `Hog Jumper`.

What the client accepts:

- `0x00F3` is `[u32 rank id, u32 flags, u32 rank, string16(8)]`, handler `0x0091E7C0`.
- `0x00F6` is ten dwords and two `string16(8)` fields, handler `0x0091E840`.
- The handlers and descriptor addresses below are the preferred-base VAs
  `msgshape.py` prints; a running client adds its load base minus `0x400000`.
- The descriptors are zero in the file and are filled at load time. On this build the
  `0x00F3` array is at `C041A0` (`+0x10` is the string cmd), and `0x00F6` is at
  `C041CC` (`+0x2C` and `+0x30` are the strings).
- A coded literal spends three units on framing, so `string16(8)` carries four text
  units. `Jump` fits; `Jump Master` needs 14 coded units before the decoded NUL.

Failed patch 1 - the shared cap constant:

- File `0x00801D78` holds `0x817` (`string16(8)`) in the descriptor constant pool.
  Changing it to `0x1017` makes `msgshape.py` report `string16(16)` for both title
  messages, and the row displays `Jump Master`.
- It changes every field built from that constant. Hovering the row then crashes in
  `TextEncode.cpp:190` (`(value & ~WORD_BIT_MORE) >= WORD_VALUE_BASE`) while reading
  a coded string. The assert establishes an invalid coded word at that point.

Failed patch 2 - the title-only initializer:

- The `0x00F3` initializer at file `0x6467E` and the `0x00F6` initializer at `0x64704`
  each load the shared constant with `A1 78 35 C0 00`.
- Replacing those five bytes with `B8 17 10 00 00` (`mov eax,0x1017`) leaves the shared
  constant alone. `msgshape.py` reports 16 for the title messages and 8 for other
  `string16(8)` fields such as `0x0049`.
- Both original `A1` operands have PE `HIGHLOW` relocations at RVAs `0x6527F` and
  `0x65305`. The failed file retains those entries after replacing the operands with
  immediates. `ArenaNet.log` records `BaseAddr: 00EC0000` for the build 39167 failure.
  The loader adds `0x00AC0000` to both immediates, yielding `0x00AC1017`. Rebasing
  `Gw.custom.titleonly.exe` with pefile reproduces both values exactly. The crash
  log records `edi=0xAC10` at the `MsgChannel.cpp:178` receive assertion:
  `strChars && (strChars <= maxFieldBytes / sizeof(wchar))`.
- Reproduction: read little-endian dwords at RVAs `0x6527F` and `0x65305` from
  `pefile.PE(path).get_memory_mapped_image(ImageBase=0xEC0000)`. A fresh load at
  `0x400000` gives `0x1017` at both offsets.
- `msgshape.py` reads the preferred-base image. Its output cannot verify a patched
  immediate at a relocated load address.

Build 39167 static trace (preferred-base VAs, from `Gw.exe`):

| Path | Disassembly evidence |
|---|---|
| Rank receive and storage | `0x0091E7C0` passes decoded packet `+0x10` to `0x008130B0`. The worker stores the two rank dwords in the 12-byte record at `[ctx+0x82C] + 12*rank_id`. It scans for the UTF-16 NUL (`0x0046C3E0`), allocates or resizes to `2*(length+1)`, and copies the coded string into the allocation at record `+0x08` (`0x00813153-0x00813198`). The record holds a pointer. |
| Track receive and storage | `0x0091E840` reads the decoded strings at packet `+0x2C` and `+0x3C` (`0x0091E889-0x0091E898`). The worker `0x00813320` copies nine dwords into the 44-byte record at `[ctx+0x81C] + 44*title_id`, then separately copies the strings into allocations at record `+0x24` and `+0x28`. The packet pointer difference is 16 bytes, the width of the original first string slot. A descriptor-only change that widens both strings puts the second field at `+0x4C` while the handler still reads `+0x3C`. That mismatch is confirmed statically. The exact bad operand in the first crash remains unverified. |
| Rank name consumers | `0x00815650` reads rank record `+0x08` at `0x0081568A`. The nonzero-rank row calls it with track `+0x08` (current rank ID) at `0x008A97CB`. The hover path calls it with track `+0x14` (next rank ID) at `0x008A8898`. |
| Next-rank hover | `0x008A8820` contains the `AttribTitles.cpp:114 codedNextTierName` assert at `0x008A88A8`. It formats track `+0x18` (next minimum) with `0x008154A0`, reads the second track string through `0x008155A0` (record `+0x28`), and passes that template to `0x007C76D0` with the coded name in argument `0x0A` and the coded number in argument `0x0B` (`0x008A88B7-0x008A88DB`). |

The client performs the hover composition. The observed sentence showed `Ace` without
`10,000`, even though the disassembly shows that both are passed to this formatter.
Whether a stock encoded template can render both arguments on the observed surface
still needs a client result. Changing the rank name and changing the sentence are
separate experiments.

### Client builds on this machine

**This section is the record of the first title client, which was replaced on 2026-11-23.** The
client is now built by `tools/client/patch-gw.py` as described in `docs/port/05-tooling.md`: it loads
`Kryta.dll`, which finds the title functions by byte pattern and is sent the text over its own link
to the login program (`docs/port/22-client-platform.md`). `KrytaTitles.dll`, the fixed-address
hooks and the portal's `/kryta/custom-titles.json` are gone. What follows is what was measured and
built before that, and the addresses in it are where the new offsets were measured.

The catalog and the manager's title rows name the custom track `Hog Jumper`; the wire carries the
short rank name `Jump`, and the patched client's hooks substitute the full text.

| Executable | Purpose and result |
|---|---|
| `Gw.custom.exe` | Regular Kryta client: the connection patch plus the title lookup hooks, built by `tools/client/patch-gw.py`. SHA-256 `7B0D29239F06A38294240AA2590B22DBA69AEE6C9E359BDB0E3519174928BAB7`. |
| `KrytaTitles.dll` | The x86 title helper `patch-gw.py` installs beside `Gw.custom.exe`. SHA-256 `5AAAA93733FDFBA63A59E8A444DFFFCCDCE05D1A03A6E116AD9D1DDF652C1269`. |
| `Gw.custom.connection-only.exe`, `Gw.custom.before-title.exe` | The connection-only build from before the title hooks. Both are SHA-256 `4937CBF715B48EF1ECB4729A0D0DCA1C9FB46C08ACE2F8545DE802618ECC1748`. |
| `Gw.custom.known-good.exe`, `Gw.custom.working-backup.exe` | Older backups, identical to each other: SHA-256 `497EDDA7A44E63F6A9B3DEEB245B95C632FF331150BF0ED7D8625DDF4977AE35`. |
| `Gw.custom.titleonly.exe`, `Gw.custom.title-probe.exe`, `Gw.custom.title-hover.exe`, `Gw.custom.hog-jumper.exe` | Retired title experiments, no longer in `F:\GW\GW1`. `patch-gw.py --retire-title-probes` moves any of these four into a `retired-title-probes-*` folder there (`retired-title-probes-8huv14xe/`, whose permissions deny listing). The titleonly build widened the title packet strings and crashed; the title-probe build rendered `Jump Master` with the original hover; the title-hover build passed offline checks with no in-game result; the hog-jumper build was the fixed-text proof of concept, and the owner confirmed `Hog Jumper` and its custom hover in-game. |

The first version of the client-only probe redirected the call
at `0x0091E7E0` through a position-independent stub. When both Kryta's rank ID 10 and
the exact NUL-terminated coded `Jump` match, the stub passes a coded `Jump Master`
to the existing length, allocation, and copy path. All other inputs reach the worker
unchanged. The script requires the recorded connection-only SHA-256 and creates a
new file exclusively.

Offline verification executed the generated x86 stub with a stand-in worker:
the matching input was substituted and other rank IDs, coded text, and terminators
passed through. PE mapping at `0x00400000` and `0x00EC0000` resolved the same hook
RVA and preserved the stock descriptor loads. `msgshape.py` reported the stock
`string16(8)` fields for both title messages.

A client-side substitution proves a client-supplied label only. Arbitrary text chosen
by the server needs a source the client can resolve within the stock packet format,
or a separate change to the protocol. ASURA at `ca9f532`, `packets.rs:771,774`, defines
the stock caps and supplies no title handler. The receive layout, registration size
and allocation would all need checking before any wire-cap experiment proceeds.

The owner confirmed that the first probe displays `Jump Master` and opens its hover.
The hover still showed the stock sentence with `Ace`. The updated probe redirects
the hover formatter call at `0x008A88DB` for title ID 8, next rank ID 11, and next
minimum 10,000. It passes `You must jump 10,000 times.` to the existing formatter
as a single literal and continues at the function epilogue (`0x008A891F`) before
the optional stock append. Every other title/rank/threshold takes the original
formatter and continuation. Offline x86 checks verified the custom arguments,
stack cleanup and all three guard failures. Both hook RVAs and both stock title
descriptor loads were checked at the preferred and relocated bases. Rendering
the new hover sentence is pending a client run.

The Hog Jumper version retains the two hook sites and also guards on
current rank ID 10 before substituting the hover. ASURA's
`client-patcher/patch-gw.py` only patches the connection, so both hooks
are local to the separate probe. An isolated 32-bit harness mapped the
generated executable at `0x31000000` and `0x47000000`, then ran the
hover stub through the original formatter and text-encoding routines.
The game context, scratch allocator, and final validation were stand-ins.
The formatter returned the exact encoded sentence on a match and
preserved the caller's text when any of the four guards failed. The
rank hook supplied the new name only for rank 10 and coded `Jump`.
The two title descriptor initializer operands matched the original
client at both relocated bases. The owner subsequently confirmed both strings
in the Hero Panel.

The server and player sheet read custom definitions and vanilla IDs from the
configured world's `titles.json`. The captured vanilla packet records are
in the adjacent `vanilla-title-wire.json`. Stock packets carry rank IDs, short names and
progress. The portal publishes custom text and its lookup IDs from the same
catalog at `GET /kryta/custom-titles.json`. The official `patch-gw.py`
builds `Gw.custom.exe` with the lookup hooks and installs its x86
`KrytaTitles.dll` helper. The helper fetches the definitions from the
portal and substitutes only when the custom IDs and the corresponding
stock track or rank fields match.

Build 39167's title enumerator at `0x008154F0` walks the received track
array, whose size comes from the receive worker at `0x00813320`. The
separate stock metadata accessor at `0x005A7540` asserts for IDs 48 and
above. The official patch supplies a fallback metadata record for those
rows. The earned name and description still come from the text hooks.
Kryta assigns consecutive rows starting at 48 in catalog ID order so
gaps in the authored IDs do not inflate the client array.

An offline harness mapped the official hooks at two relocated bases and
ran them with the real HTTP runtime. It picked up edited text and a second
custom track from the HTTP catalog without changing the image. The portal
and gameplay tests cover the response and the stock packets. The owner
confirms the Hero Panel displays the titles with the integrated executable
(2026-11-23). The work is not finished: the title feature, including the
injection (`KrytaTitles.dll`) and the patching (`tools/client/patch-gw.py`),
is still open.

### Why vanilla descriptions are missing in Kryta

- The original Kryta test catalog gave all 47 vanilla tracks the same
  `ZERO_RANKS` and sent coded `Pts` and `Rank` in the `0x00F6` template fields.
  The client supplies the compiled name at rank zero from its own table.
- The same-build retail capture
  `recordings/client/raw/northlands-live-20261106-014922.jsonl` has 13 title tracks
  with separate progress and hover strings of four or five code units. Read each
  slot up to its first NUL in the decoded 76-byte struct, at `+0x2C` and `+0x3C`.
  Trailing words in the slot are outside the coded string.
- Preforged's `skills-20261007-134151-12248-47332500.jsonl`
  `progression_snapshot` has separate progress and hint strings for 44 slots.
  Both strings match the same-build Northlands capture for 12 of its 13 slots.
  Commander (22) is empty in that Preforged snapshot and present in Northlands.
  Preforged's private 2022 UI verification used captured ranks and tracks
  (`research/preforged-source/docs/progression-integration.txt:128-140,169`).
- Eden decodes both strings as encoded text
  (`Eden/crates/eden-network/src/game/typed/title.rs:86-100`).
  Headquarter calls the second field `description`
  (`research/Headquarter/code/client/title.c:46-63`). Rurik traces that same field
  to the next-rank composition
  (`Rurik/studies/newopcodes/FINDINGS.md:1463-1477`).

The stock-client restoration first added captured rank names, first
thresholds, and both templates for 33 vanilla tracks in
`config/World/Vanilla/titles.json`. These rows use one test point.

The early in-game checks reported missing descriptions. The server log
confirmed the world had loaded all 12 catalog rows. Offline resolution of
both templates for every row against this client's `Gw.dat` produced readable
retail text. The archive key derivation at `0x00908CC3` uses the first five
SHA-1 choose rounds, while the auth-derived routine reused by Rurik's offline
`textrec.py` differs in its last two rounds. This is an offline research check.
The Kryta runtime sends the captured references unchanged.

The first load in the Northlands capture supplies title definitions at seq
537-575, after progression and flags (534-536) and before instance load finish
(582). Kryta now sends its title definitions in that phase. The test display
selection remains after load finish. The gameplay connection test checks this
placement.

In a follow-up on the open connection-only build 39167 client, a read-only
check found matching track text in memory for IDs 2, 21, 22 and 34, including
both coded fields for Defender of Ascalon (21). All 48 title rows had tooltip
registration data. The popup the owner reported as `Rank` was allocated for
Wisdom (46). That was the literal fallback for Wisdom in the original 12-row
catalog. ASURA_testfork's client recordings contain Wisdom at the 10,000-point
rank, and Preforged's progression snapshot contains it at the 2,500-point rank.
ASURA_testfork's own server sends no title packets, and Rurik's live title
probe established rank rendering and progress updates, with hover rendering
outside that proof.

The owner moved to Legendary Defender of Ascalon and Sweet Tooth in the same
stock-client session and verified both descriptions render. Some other rows
also rendered. The `Rank` popup identifies an uncovered row. The client
result establishes that the stock title format can render captured
descriptions. It does not establish that moving the packet order was required.

The full login and traversal recordings were originally filed by activity.
ASURA's client character-creation captures contain 13 `0x00F6` records and
its travel captures contain 429. Preforged's leveling and skill checkpoint
packet logs add 21 verified starting rows to the original 12, including
Wisdom and Treasure Hunter. In
`research/preforged/data/captures/pre-searing-ranger-partial-2026-10-10/packets-7108-102222937.jsonl`,
lines 1382-1384 give Wisdom's rank IDs 360 and 361 and threshold 100.
The normal-mode Drunkard, Survivor and Party Animal rows come from
`farmer-hamnet-2026-10-30/packets-7528-73582328.jsonl`. Encoded strings end at
the first NUL in the captured slots. The catalog gives a packet line for
every imported track and rank. The 21 additions still need in-game checks.

The catalog at that point had placeholders for IDs 0, 5, 11, 12, 17, 20,
30-32, and 38-42.
IDs 11 and 12 are deprecated.

### First earned ranks for the 12 remaining active tracks

The [Guild Wars Wiki title overview](https://wiki.guildwars.com/wiki/Title)
and its linked track pages describe the ranks and their requirements. The
first earned rank names and thresholds below agree between Py4GW Stealth
(`py4gw/enums_src/title_enums.py:129`) and Py4GW Reforged
(`Py4GWCoreLib/enums_src/Title_enums.py:117`). Their tier numbers are
ordinals. The packet rank IDs came from decoding existing retail `0x00F3`
strings against `Gw.dat` and matching the decoded first-rank names.

| Track ID | Track | First earned rank | Points | Retail rank ID | Capture line |
|---|---|---|---:|---:|---|
| 0 | Hero | Hero | 25 | Unverified | |
| 5 | Kurzick | Kurzick Supporter | 100,000 | 74 | B:2168 |
| 17 | Sunspear | Sunspear Sergeant | 50 | Unverified | |
| 20 | Lightbringer | Lightbringer | 100 | 264 | A:12611 |
| 30 | Legendary Cartographer | Legendary Cartographer | 3 | 240 | A:7560 |
| 31 | Legendary Guardian | Legendary Guardian | 6 | 243 | A:7563 |
| 32 | Legendary Skill Hunter | Legendary Skill Hunter | 3 | 246 | A:7566 |
| 38 | Asuran | Not Too Smelly | 1,000 | 282 | A:133969 |
| 39 | Deldrimor | Delver | 1,000 | 294 | A:16684 |
| 40 | Vanguard | Agent | 1,000 | 306 | A:4737 |
| 41 | Norn | Slayer of Imps | 1,000 | 318 | A:17063 |
| 42 | Master of the North | Adventurer of the North | 100 | 330 | C:2793 |

- A: `research/preforged/data/captures/field-sessions/20261024-urgoz-prefix/packets-61244-363289656.jsonl`.
  Lines 7562, 7565, and 7568 also link the three Legendary track IDs to
  their earned ranks and confirm their thresholds in `0x00F6`.
- B: `research/preforged/data/captures/pre-searing-ele-warrior-2026-10-09/packets-21116-95422890.jsonl`.
- C: `research/preforged/data/captures/skill-checkpoints/20261016-celestial-cast/packets-108988-190116609.jsonl`.

The 17 distinct Preforged retail packet logs inspected contain earned-rank
track records for all 12. None contains a track record with a zero current
threshold for these tracks. To populate the stock-client rows at one test
point, the remaining evidence is the starting-state `0x00F6` and its
referenced rank records. Hero and Sunspear also still lack a verified
first earned rank ID.

### Owner-reported missing title information

The client report names Master of the North, Asura, Deldrimor, Ebon
Vanguard, Norn, Skill Hunter, Cartographer, Lightbringer, Sunspear, Wisdom,
Treasure Hunter, Luxon, Kurzick, and Hero. For Cartographer and Skill
Hunter, both the campaign tracks and the Legendary variants are included
in the investigation.

| Reported tracks | Track IDs | Packet data during the reported test |
|---|---|---|
| Hero, Kurzick, Sunspear, Lightbringer | 0, 5, 17, 20 | Generic fallback |
| Legendary Cartographer and Legendary Skill Hunter | 30, 32 | Generic fallback |
| Asura, Deldrimor, Ebon Vanguard, Norn, Master of the North | 38-42 | Generic fallback |
| Tyria, Cantha and Elona Cartographer | 1, 2, 18 | Retail rank-zero data |
| Tyrian, Canthan and Elonian Skill Hunter | 24, 26, 28 | Retail rank-zero data |
| Luxon, Treasure Hunter and Wisdom | 6, 45, 46 | Retail rank-zero data |

For the nine reported rows with rank-zero data, both encoded strings and
all other track fields except the test points match their cited retail
packets in `config/World/Vanilla/vanilla-title-wire.json`. The Cantha and Elona
Cartographer rows and the Tyrian and Canthan Skill Hunter rows even have
one point in those retail packets. This establishes the source data for
these rows. Their missing information in the owner's client test remains
unexplained.

The later retest reported the same missing descriptions and no visible
points. During that run, a read-only inspection of the connection-only
`Gw.custom.exe` (PID 21572, game build 39167) found 48 title records,
each with exactly one point stored in the client. All nine reported
rank-zero catalog rows had their expected current and next rank IDs and
thresholds. The generic fallback rows had rank IDs 1 and 2, next
threshold 2, and the short placeholder strings. This confirms delivery
of the test points. It does not explain the missing progress text or
descriptions in the panel.

The Canthan Skill Hunter comparison found a load-order difference.
Preforged's `packets-6468-5728843.jsonl:110585-110587` gives rank IDs
213 and 214 immediately before track 26 at one point. Northlands seq
567-569 gives ranks 273 and 274 immediately before Sweet Tooth (34).
Kryta previously batched every rank ahead of every track. The normal
load now sends each track directly after its new ranks, in the order
seen in those captures. The gameplay connection test checks both
sequences. Tooltip and progress rendering still need a client check.

The owner reported `1/2` on Legendary Skill Hunter (track 32) and
provided a reference screenshot showing `3/3 continents completed`.
The campaign tracks count elite skills and have goals of 90 (Tyria),
120 (Cantha), and 140 (Elona). The Legendary track counts completed
campaign Skill Hunter tracks. The
[Skill Hunter Wiki page](https://wiki.guildwars.com/wiki/Skill_Hunter)
describes both rules. Urgoz capture A:7566-7568 records Legendary Skill
Hunter at rank 1 with three points, current threshold three, and a
terminal next rank. At the time of the report, Kryta's track 32 used the shared
`ZERO_RANKS` test fallback with a threshold of two. The retail rank-zero record for
this track remains unverified.

The owner's next unfiltered recording,
`recordings/client/raw/fresh-character-20261121-151849-756.jsonl`, used a
retail character with some titles earned. It retained 1,309 packets,
including rank and display packets but no `0x00F6`. During the same
session, a read-only check of the client found no populated track data
for Legendary Skill Hunter or the three campaign Skill Hunter tracks.
The owner confirmed those rows were hidden. The next useful sample is a
character with one or two completed campaign Skill Hunter tracks, where
Legendary Skill Hunter has positive progress and remains unearned.

The partial-progress character's recording
`recordings/client/raw/fresh-character-20261121-152136-829.jsonl` retained
714 packets including load finish, with no `0x00F6`. The open retail
client did have populated tracks 24, 26, and 28, each at rank zero with
two skills and goals of 90, 120, and 140. A read-only comparison matched
their numeric metadata, both encoded templates, and both rank names to
the Kryta catalog. Track 32 was empty in that same client.

A follow-up read of the open retail client covered all 20 title IDs in
the owner's report. It found Canthan Skill Hunter at exactly one
point, with its rank-zero track fields and both templates matching
Kryta's catalog. Tyria and Elona Cartographer and Elonian Skill Hunter
were also below their first rank and matched the catalog at their
respective point values. Cantha Cartographer, Luxon, Wisdom, and
Treasure Hunter were at earned ranks and had matching templates.
Tyrian Skill Hunter was absent on the follow-up character but had
matched at two points on the prior character.

Of the 11 reported rows that used Kryta's generic fallback, nine had
earned-rank data in the follow-up client and two were absent
(Legendary Cartographer and Legendary Skill Hunter). No new
starting-rank records were found for those 11. The stock and
connection-only executables also match byte for byte in the title
packet handlers, the row update, and the next-rank hover routine
listed above. The reason for the rendering failure in the covered
rows has yet to be verified.

The test display now replaces all 12 active placeholders with complete
retail earned states. Each track identifies the captured `0x00F6` and
its rank records in the world pack. Most rank-zero rows retain the
one-point test value. Legendary Skill Hunter sends the captured
completed state at three points, pending a rank-zero capture for the
one-point case. Hero uses captured rank ID 3 as its display threshold.
The synthetic rank 3 is omitted when the fallback does not refer to it. The change affects
packets for the display test only. Client rendering of the new states
and the nine previously reported rendering failures remain unverified.

The next client report named Jump, Kind of a Big Deal, Treasure Hunter
and Wisdom. A read-only check of that run found the expected stored
records for all four. Jump's progress and hover fields were literal
`Pts` and `Jump`. It now uses the count-track flag evidenced by
Preforged's Canthan Skill Hunter packet and two build 39167 archive
templates, `%str1%` (record 2729) and `%str1%: %str2%` (record 2442).
These give the client slots for the count and the next-rank hover.
Kind of a Big Deal, Treasure Hunter and Wisdom now replay the exact
rank-zero points from their existing capture references: 3 of 5,
3 of 100, and 53 of 100. The remaining task is to verify how the Hero
Panel renders those four packets.

Preforged's retail client snapshot at
`research/preforged/bin/skills/skills-20261007-134151-12248-47332500.jsonl:3`
contains both encoded templates for every unresolved active track,
including Legendary Guardian (31). All are at earned ranks. For 26 of the
already sourced tracks with a different earned rank in that snapshot,
both templates match the corresponding rank-zero catalog templates
word for word. This gives a source for the missing template text. The
snapshot does not contain the missing starting-rank records, and its
display rank ID field cannot be used as the first earned rank ID for
Hero or Sunspear.

### Unfiltered retail map load and title selection

Local raw evidence: `recordings/client/raw/login-unfiltered-17072.jsonl`
(2026-11-21, build 39167). The sniffer ran for 30 seconds in both directions
with no opcode filter. It recorded 822 packets across 71 direction/header
pairs, including the map load head (seq 1) and load finish (seq 692).

- Two StoC `0x00F4` rank-display packets arrived during load (seq 489 and 497).
  After load, all nine CtoS `0x0058` requests carrying track IDs were answered
  by StoC `0x00F4` carrying the selected tier ID. Client-clock deltas were
  94-125 ms. For example, track 17 (Sunspear) selected tier 261 at seq 792-793,
  and track 46 (Wisdom) selected tier 367 at seq 806-807.
- This window had no `0x00F3` rank definitions, `0x00F6` track definitions, or
  faction messages (`0x00E9` through `0x00EE`). A read-only inspection of the same live
  client found 42 tracks with both templates. The title table was available
  during the test without its definitions appearing in the recording.
- The live table includes the 12 uncovered active tracks, all at earned
  ranks. This verifies their presence and gives earned-rank metadata.
  Their rank-zero transitions are still unresolved.

The owner repeated the test on a newly launched retail client before signing
in. Local raw evidence: `recordings/client/raw/login-unfiltered-9292.jsonl`.
The 30-second unfiltered run recorded 776 packets (753 StoC, 23 CtoS) in
65 direction/header pairs. It includes the map load head (seq 3), the load
finish (seq 545), and 16 title selection requests. All 16 got the rank
display reply in 62-94 ms by the capture's tick values. As in the first
unfiltered run, it recorded no title definitions or faction totals/caps.

The server now handles `0x8058` and `0x8059` on a loaded map. It replies
with one `0x00F4` containing the character's selected tier, or rank zero
when hidden (Preforged `packets-1360-15821281.jsonl:36252`). The district
keeps one displayed rank per player for peer updates. The load skips the
deprecated Treasure Hunter and Wisdom slots 11 and 12, which were the
source of the duplicate rows. The active account tracks are 45 and 46.

Five inbound groups in the fresh run have exactly 64 records at the same
tick (seq 8-71, 72-135, 139-202, 265-328 and 452-515). Py4GW Stealth's
client bridge has a 64-entry event queue (`py4gw/game_thread/shared_block.py:75`),
and the capture stubs skip logging when it is full
(`py4gw/game_thread/payload.py`, checks against `EVENT_DEPTH`;
`docs/PACKET_SNIFFER_PORT.md`, "The log"). The JSONL summary reports retained
packets and has no drop count. The full-size groups indicate that the queue
can saturate during the load. The comparison cannot distinguish packets
lost there from packets delivered outside the observed gameplay handler
path. Neither unfiltered recording establishes that retail skipped title
or faction initialization. Repeating the same capture setup would leave
that ambiguity.

## What no project has

- No project has a general metric store or event model that titles consume.
- No project computes title points from XP, missions, exploration, skills, items, or
  PvP.
- No project has PvP title sources. Champion, Gladiator, Hero, Codex, Gamer, Zaishen,
  and Commander have no capture or event stream.
- No project has item-driven title sources. Chests, sweets, alcohol, party items, and
  rare-item identification are not modeled as title metrics.
- No project persists Cartographer from exploration. Rurik sends synthetic fog; the
  client stores `mapBits`; no project records which blocks a character explored.
- No project derives Survivor. XP deltas and deaths exist separately; the counted-death
  rule and high-water behaviour are not implemented.
- No project has a complete threshold catalog sourced from the client. Preforged imports
  the ranks it observed; Py4GW has a wiki table; OpenTyria's tables are empty.
- No project settles whether `Account` is a scope orthogonal to the points/cartographer/
  missions shapes or a fourth metric type. The capture's `flags = 4` supports the
  orthogonal reading; OpenTyria collapses it into one enum.

## Open questions

1. Is `Account` a scope applied to a points/cartographer/missions shape, or is it a
   fourth shape as OpenTyria treats it?
2. Settled for this client: build 39167 uses the modern numbering (243-246), which is what ASURA's
   table and this tree's slice use. The 2022 numbering (242-245) matters only when reading
   Preforged's captures.
3. For the target client, is the ninth track dword `title_mode_flags` (Dhuum/Reforged)
   or the unread word Rurik measured?
4. Which raw metrics are authoritative server state, and which are client-reported
   state? XP, faction, mission bits, and exploration do not all arrive by the same path.
5. Which titles are character-scoped and which are account-scoped in the target rule
   set, given that Rurik stores all titles on the account and preforged splits them?
6. Which title earning rules are known from a capture or the owner, and which remain
   unknown? The current ledger names the gap directly: the record, tiers, display rules,
   and Prophecies thresholds are evidenced; how points are earned per track is not.

## One-sentence conclusion

The projects collectively define the **shape of a title** and the **shape of a few raw
state families**, but none defines the **metric layer** that would infer a title from
those families; that layer has to be derived from the raw families and the earning
rules, not copied from a project.
