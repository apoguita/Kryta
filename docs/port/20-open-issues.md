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
| 10 | **Custom titles: the client side is rebuilt and confirmed; the server side is open.** The patch on disk now only redirects the client and makes it load `Kryta.dll` by itself; the DLL finds the client's title functions by byte pattern, from offsets the server provides, and is sent the title text over its own link to the login program (`docs/port/22-client-platform.md`). The owner tested it on 2026-11-23 ("everything seems to be working"), and the first implementation was removed. What is left for titles is items 10a and 10b, and the points under "Not built" and "Choices made while building" on that page. | `docs/port/22-client-platform.md` |
| 10b | **The title files mix two things, because of a bad implementation.** The titles are loaded from the JSON files, and the values are loaded from them as well (`config/flavors/Vanilla/World/titles.json`, `config/flavors/Vanilla/World/vanilla-title-wire.json`: the files carry points, thresholds and display states beside the title definitions). This needs to be fixed. | `config/flavors/Vanilla/World/README.md`; `game/src/Characters/Titles.rs` |
| 10c | **A displaced client gets no notice.** Only one login per account is ever permitted: a new login closes any old connection (owner, 2026-11-23). Both programs enforce it (`authserver/src/Server/logins.rs`, `game/src/Accounts/Logins.rs`), and the owner tested it. The displaced client is not sent retail's notice (`ENetError` 0x23, "another client has connected using your account"), because no source shows the message that carries it; its connection is closed and it reports the loss in its own words. | `docs/port/09-parity-audit.md`, the one-login row |
| 10d | **The character "Player One" ends the client on entering a map.** The client raises its own assertion, `id < s_items.Count()` at `P:\Code\Gw\Composite\Data\CpsData.cpp(432)`, with the id `0x47C34`, while loading map 148. It happens with the old `Gw.custom.exe` and with the new client alike, and the other character of the same account ("Test Subject") loads; so it is that character's rows, not the client: the data is corrupt (owner, 2026-11-23). Its nine `item_instance` rows have `item_type` 0, `model_id` 0 and no name, where the working character's rows have all three - rows written before those columns were filled. Nothing in the server refuses or repairs such a character before sending it to a client. Found 2026-11-23; the rows are left as they are. | the client's `ArenaNet.log` under `Documents\Guild Wars` |
| 10a | **Titles are not finished.** Title earning and persistence do not exist: every metric is zero and nothing is recorded per character. | `docs/port/19-title-research.md`; `docs/port/09-parity-audit.md`, section 5 |
| 11 | **NPCs and monsters.** No map population is sent. The retail recordings under `recordings/client/raw/` record other agents being created; ASURA and GWPS send none. Not started. | `docs/port/09-parity-audit.md`, "What is not ported" |
| 12 | **The district-count query.** The login connection answers `0x800B`; ASURA does not handle it. It is in none of the audit's per-message tables and names no source. | `docs/port/09-parity-audit.md`, section 6 |
| 13 | **Map 148 has a third entry point** that ASURA's pack does not have. Nothing records where it came from. | `docs/decisions/0014-the-world-content-is-a-pack.md` |
| 14 | **The placement rule lacks one fact.** The login does not know the kind of the map a client asks for, so the outpost rule is ported but not in force. Three ways to supply it: the login reads the world's content, the character row carries the map's kind, or the rule is applied in the world. | `docs/port/09-parity-audit.md`, section 7a |
| 15 | **Profession change.** On `0x8060` this tree omits GWPS's `0x015A` and `0x014B`, as ASURA does. Adopting them is open. | `docs/port/02-wire.md`, "The creation screen's conversation" |
| 16 | **Smaller items the audit lists:** the unread `createFlags` in the creation confirm, the creation error code the client has no text for, the seven armour-colour messages nobody reads, the missing reference client, and three unported ASURA tests. | `docs/port/09-parity-audit.md`, section 8 |

## Flavors: a later phase

The owner, 2026-11-27: what is left of flavors is work for a later phase, after map support. What
is built is `docs/decisions/0016-a-server-is-a-flavor.md`.

| # | issue | where |
|---|---|---|
| 37 | **A second flavor has never been run.** Only `Vanilla` exists. Nobody has copied a flavor, chosen it, restarted the stack and connected a client, and the panel's Flavors and Settings pages have not been used in a browser. | `docs/decisions/0016-a-server-is-a-flavor.md`; `tools/manage/src/areas/flavors.rs` |
| 38 | **Characters are not a flavor's.** The databases are the machine's (`config/machine.json`), so every flavor shows the same accounts and characters, and a character saved on a map another flavor lacks or has changed is not handled. Not decided. | `config/machine.json.dist` |
| 39 | **The panel does not rename or remove a flavor**, and does not move a value between a flavor and the machine. | `docs/port/23-control-panel.md` |
| 40 | **A flavor is not packed for distribution**, and neither is the map pack a user loads ahead of time or a map served from a path the server names. | `docs/port/24-custom-maps.md`, "The delivery plan" |

## The repository

| # | issue | where it is recorded |
|---|---|---|
| 17 | **Six unsanitised client recordings are committed** under `recordings/client/raw/`. Each one's header says `"sanitised":false` and "do not commit". | `recordings/README.md` |
| 18 | **No folder is assigned to this server's own recordings.** `recordings/asura/` holds ASURA's. | `recordings/README.md`; `docs/port/05-tooling.md`, "Capturing the server" |
| 19 | **Three questions in the structure doc's open table:** the five unassigned game directories, `game/Modules/` against `modules/`, and where a pack's configuration lives (decision 0014 may already answer the last). | `docs/port/01-structure.md`, "Open, and needing a word" |
| 20 | **One phrase could not be verified:** the audit says a test loads map 163, "not the map the schema seeded by hand". Nothing is seeded now. | `docs/port/09-parity-audit.md`, section 9 |
| 21 | **The documentation changes of 2026-11-23 are not committed.** | - |

## The control panel

The panel was rebuilt on 2026-11-23 as a data service with static pages, with a recordings area and one registration point per area so it can be extended (`docs/port/23-control-panel.md`, which also lists what is still not built: the administrator login, editing settings and content, logs, online players, and item, unlock and title editing). The same page records why the database filled with orphan rows: the database tests do not clean up after themselves, and the character tables have no foreign keys.

## The code's design

The whole code was read on 2026-11-23 and assessed for style, paradigm, how well its structures are
delimited, and patched functionality. The findings, by file and line, and the order they suggest for
changing the code, are `docs/port/21-code-assessment.md`. Its steps 1 to 5 are done and confirmed by the owner's live test (2026-11-23); titles and the title client patch remain (items 10, 10a, 10b).

## Code defects found while reading the documents

Not fixed; the documents were the subject.

| # | defect | where |
|---|---|---|
| 22 | A comment says the load sends "the first tick" and that the party stream is not sent; neither is true. | `game/src/Server/WorldSession.rs:1205-1211` |
| 23 | Comments about item destroy sit above the drop code; no destroy handler exists. | `game/src/Server/WorldSession.rs:111`, `:1665-1666` |
| 24 | A doc comment still says item templates are seeded and read back. | `game/src/DataStores/mod.rs` |
| 25 | The comment above the `admin.json` bootstrap cites decision 0013 and calls the row "the first administrator"; it is the first game account. | `authserver/src/main.rs` |
| 26 | The dependency comment says `authserver` links only `shared`; it also links `game`. The note in the stack file says only a missing program is built; every one is. | `Cargo.toml`; `tools/supervisor/stack.json` |
| 28 | Two tests still read the map tables, which are empty by design. | `game/src/World/mod.rs` (`every_hosted_map_walks_from_one_of_its_own_spawn_points`); `worldserver/tests/end_to_end.rs:464` |
| 29 | The capture reader refuses most committed recordings unless given `--assume-layer`, because it does not know the tool names in their headers. | `tools/capture/capture_file.py` |
| 30 | The recordings index covers 18 of 54 recordings, and three cited `.log` recordings are ignored by Git. | `recordings/index.jsonl`; `.gitignore` |
| 31 | The code has not been searched for values written as literals inside logic. | - |
| 32 | The fallback character sheet has campaign 0, which is a PvP character; its test names and asserts it as Prophecies. | `game/src/Characters/Sheet.rs`, `CharacterSheet::missing` |
| 33 | `World.MaxAllowedCorrection` defaults to 250 and cites OpenTyria's `MAXIMUM_ALLOWED_CORRECTION`, which is 100 in `research/OpenTyria` (`GmAgent.c:4`). Which value is right has not been checked against GWPS or ASURA. | `game/src/World/WorldConfig.rs` |
| 34 | The supervisor starts the old binary when a build fails: only the file's existence is checked. | `tools/supervisor/src/main.rs`, `ensure_built` |
| 35 | Text the programs print that is wrong: "the first administrator" for the first game account; "until the map_arch table is readable" though doorways come from the pack; "needs 16" where the check is 14 bytes; the `/stuck` help says it broadcasts. | `authserver/src/main.rs`; `worldserver/src/main.rs`; `game/src/Handlers/MovementHandler.rs`; `game/src/Chat/Operations.rs` |
| 36 | The EULA version 26 is a literal in the panel (a named constant since the rebuild), where the login program takes it from the client. | `tools/manage/src/areas/accounts.rs` |
