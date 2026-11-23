# Open issues

Things that are not settled, in one place. Each one is the owner's to decide or to schedule. An item
leaves this file when it is settled, and the document it belongs to is corrected in the same change.

Written on 2026-11-23, after the documentation was read and corrected in full.

## The game rules against the code

| # | issue | where it is recorded |
|---|---|---|
| 1 | **Travel gating.** Rule R12 says a character can only travel to maps it has unlocked. The code refuses nothing: it marks the map unlocked when the character arrives. The owner marked this to resolve later. | `docs/game-rules.md`, "Travel gating"; `docs/port/08-rules-coverage.md`, R20/R12 |
| 2 | **Surname.** Rule R65 says a first name and a surname are both required. The evidence (E229) says the client's own name rule does not require a surname. | `docs/game-rules.md`, R65 and E229 |
| 3 | **Outpost teardown.** Rules R37 and R38 give an outpost a 15-minute empty timeout and no reconnection hold. The code holds every empty copy for 900 seconds, outposts and explorables alike. | `docs/port/08-rules-coverage.md`, R11 and R37 rows |
| 4 | **Pairs of rules the ledger itself lists as unreconciled:** R14 and R13 (last-outpost placement), R17 and R26 (disband or truncate a party), R29 and R34 (when a map's population exists), R32 (instance reuse), R2 and R35 (instance scope). | `docs/game-rules.md`, "Questions carried from the later ASURA review" |
| 5 | **Ledger tidying.** The rule rows are out of numeric order, and R66 writes "Melandrus Accord" where the evidence writes "Melandru's Accord". | `docs/game-rules.md` |

## The JSON policy

| # | issue | where it is recorded |
|---|---|---|
| 6 | **The game's numbers are in a file that cannot be shared.** Spawn scatter, tick interval, party cap, movement limits and the rest of the `World` section are in `worldserver.json`, beside the machine's database password and ports. ASURA kept them in separate files under `config/Server/` (`spawns.json`, `movement.json`, `intervals.json`, `portals.json`), and rule R13 names `config/Server/spawns.json`. | `docs/port/04-configuration.md`; `docs/port/08-rules-coverage.md`, R13 |
| 7 | **Values a person could want to change are still in the code.** Two known ones: the twenty `/tp` travel aliases (`game/src/Maps/TravelTargets.rs`) and the `/maps` text. The code has not been searched for the rest. | `docs/port/04-configuration.md` |
| 8 | **Some settings have a default in the code**, for example `Console.Enable` and `Control.Port`. Whether a setting may fall back to a code default is not decided. | `docs/port/04-configuration.md` |
## Protocol and content

| # | issue | where it is recorded |
|---|---|---|
| 10 | **Custom titles: the DLL is the right approach, and the way it is done today is not.** Custom titles work, and the Hero Panel displays them, through a DLL beside the client (`KrytaTitles.dll`). Using a DLL stays. What needs to change is how it is done now: how the DLL gets into the client and how the client's title functions are patched to use it (`tools/client/patch-gw.py`, `tools/client/title_patch.py`, `tools/client/title-runtime.cpp`). What the better way is has not been worked out. To be addressed later, outside the current scope; how it works today is described in `docs/port/21-code-assessment.md`, section 10. | `docs/port/19-title-research.md`, "Client builds on this machine"; `docs/port/05-tooling.md`, "Building and running the client" |
| 10b | **The title files mix two things, because of a bad implementation.** The titles are loaded from the JSON files, and the values are loaded from them as well (`config/World/Vanilla/titles.json`, `config/World/Vanilla/vanilla-title-wire.json`: the files carry points, thresholds and display states beside the title definitions). This needs to be fixed. | `config/World/Vanilla/README.md`; `game/src/Characters/Titles.rs` |
| 10a | **Titles are not finished.** Title earning and persistence do not exist: every metric is zero and nothing is recorded per character. | `docs/port/19-title-research.md`; `docs/port/09-parity-audit.md`, section 5 |
| 11 | **NPCs and monsters.** No map population is sent. The retail recordings under `recordings/client/raw/` record other agents being created; ASURA and GWPS send none. Not started. | `docs/port/09-parity-audit.md`, "What is not ported" |
| 12 | **The district-count query.** The login connection answers `0x800B`; ASURA does not handle it. It is in none of the audit's per-message tables and names no source. | `docs/port/09-parity-audit.md`, section 6 |
| 13 | **Map 148 has a third entry point** that ASURA's pack does not have. Nothing records where it came from. | `docs/decisions/0014-the-world-content-is-a-pack.md` |
| 14 | **The placement rule lacks one fact.** The login does not know the kind of the map a client asks for, so the outpost rule is ported but not in force. Three ways to supply it: the login reads the world's content, the character row carries the map's kind, or the rule is applied in the world. | `docs/port/09-parity-audit.md`, section 7a |
| 15 | **Profession change.** On `0x8060` this tree omits GWPS's `0x015A` and `0x014B`, as ASURA does. Adopting them is open. | `docs/port/02-wire.md`, "The creation screen's conversation" |
| 16 | **Smaller items the audit lists:** the unread `createFlags` in the creation confirm, the creation error code the client has no text for, the seven armour-colour messages nobody reads, the missing reference client, and three unported ASURA tests. | `docs/port/09-parity-audit.md`, section 8 |

## The repository

| # | issue | where it is recorded |
|---|---|---|
| 17 | **Six unsanitised client recordings are committed** under `recordings/client/raw/`. Each one's header says `"sanitised":false` and "do not commit". | `recordings/README.md` |
| 18 | **No folder is assigned to this server's own recordings.** `recordings/asura/` holds ASURA's. | `recordings/README.md`; `docs/port/05-tooling.md`, "Capturing the server" |
| 19 | **Three questions in the structure doc's open table:** the five unassigned game directories, `game/Modules/` against `modules/`, and where a pack's configuration lives (decision 0014 may already answer the last). | `docs/port/01-structure.md`, "Open, and needing a word" |
| 20 | **One phrase could not be verified:** the audit says a test loads map 163, "not the map the schema seeded by hand". Nothing is seeded now. | `docs/port/09-parity-audit.md`, section 9 |
| 21 | **The documentation changes of 2026-11-23 are not committed.** | - |

## The code's design

The whole code was read on 2026-11-23 and assessed for style, paradigm, how well its structures are
delimited, and patched functionality. The findings, by file and line, and the order they suggest for
changing the code, are `docs/port/21-code-assessment.md`. None of it is fixed.

## Code defects found while reading the documents

Not fixed; the documents were the subject.

| # | defect | where |
|---|---|---|
| 22 | A comment says the load sends "the first tick" and that the party stream is not sent; neither is true. | `game/src/Server/WorldSession.rs:1205-1211` |
| 23 | Comments about item destroy sit above the drop code; no destroy handler exists. | `game/src/Server/WorldSession.rs:111`, `:1665-1666` |
| 24 | A doc comment still says item templates are seeded and read back. | `game/src/DataStores/mod.rs` |
| 25 | The comment above the `admin.json` bootstrap cites decision 0013 and calls the row "the first administrator"; it is the first game account. | `authserver/src/main.rs` |
| 26 | The dependency comment says `authserver` links only `shared`; it also links `game`. The note in the stack file says only a missing program is built; every one is. | `Cargo.toml`; `tools/supervisor/stack.json` |
| 27 | The panel prints "All 48 ... Jump Master"; the pack has 47 title rows and the track is Hog Jumper. | `tools/manage/src/pages/players.rs:322-323` |
| 28 | Two tests still read the map tables, which are empty by design. | `game/src/World/mod.rs` (`every_hosted_map_walks_from_one_of_its_own_spawn_points`); `worldserver/tests/end_to_end.rs:464` |
| 29 | The capture reader refuses most committed recordings unless given `--assume-layer`, because it does not know the tool names in their headers. | `tools/capture/capture_file.py` |
| 30 | The recordings index covers 18 of 54 recordings, and three cited `.log` recordings are ignored by Git. | `recordings/index.jsonl`; `.gitignore` |
| 31 | The code has not been searched for values written as literals inside logic. | - |
