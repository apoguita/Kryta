# Research code feature inventory

This is the canonical inventory of what the local research trees implement in code. It is not a
reading list, a milestone list, or a port plan. A row is included only after reading an executable
path: a request dispatcher or client encoder, response builder or client decoder, and the state
owner where one exists. Tests strengthen a row but do not replace the path.

The corresponding code-derived baseline for Kryta itself is
`docs/port/18-kryta-code-feature-inventory.md`. Compare a research feature with that baseline before
starting a contract extraction; do not infer Kryta support from a packet-table row.

The comparison unit is a client-visible packet contract:

```text
client action/event -> C2S messages -> server state -> ordered S2C messages -> accepted client state
```

Language, process layout, client patch, build, and adapter are not criteria for including or
excluding evidence. They can explain field translation, but they cannot make a contract irrelevant.
The governing criterion and the required record for a future port are in
`docs/research-sources.md`, "Packet-sequence completeness criterion". Under Rule 0 Kryta ports
client-facing behavior from the source whose contract is the most complete; where no project and no
recording has it, the owner is asked.

## Evidence labels

| label | meaning |
|---|---|
| full server path | Code dispatches a client request, changes game/account state, and emits the client response sequence. |
| server response path | Code emits an ordered response sequence and owns enough state to use it, but the request side has not yet been traced in that tree. |
| client contract | Code encodes the request and/or decodes/applies the response and therefore proves the client half, not a server implementation. |
| protocol only | Packet layouts or builders exist without a traced stateful path. It is not a feature claim. |
| excluded | Code has no Guild Wars client packet contract. This is a scope result, never an adapter judgement. |

## Feature-family inventory

"Common" means the feature has an implementation path in two or more independently maintained
research trees. "Source-specific" means it was found in the named tree in this pass; it does not
assert that no other tree can implement the same function under a different name.

| feature family | code-derived sources | evidence and notable source-specific coverage | Kryta comparison today |
|---|---|---|---|
| Auth handshake, encryption, login, character list and game hand-off | ASURA, ASURA_testfork, OpenTyria, Rurik, Preforged, Preservation, Entice, GameRevision, Eden, Headquarter | Common. Full server paths are in `ASURA_testfork/crates/asura-auth/src/lib.rs:222-836`, `OpenTyria/code/OpenTyria/AuthSrv.c:625-1397`, `Rurik/toolkit/authsrv/authsrv.py:44129-48553`, `preservation-main/authservice/conn.go` plus `handlers.go:70-407`, and GameRevision's login controllers/views. Eden has the client state machine in `eden-client/src/session.rs:987-1545` and wire code in `eden-network/src/auth.rs`; Headquarter has client-side auth and portal code. | Implemented and tested end-to-end. The audit records the remaining thin parts of the handshake and placement path; this row is not a claim of parity beyond it. |
| Character creation, appearance, confirm, list selection, deletion and settings | ASURA, ASURA_testfork, OpenTyria, Rurik, Preforged, Preservation, Entice, GameRevision, Eden | Common. Preservation handles creation, profession choice, confirmation and dye in `gameservice/handlers.go:164-329`; Entice has `CharacterCreationController.cs`; GameRevision has `CharacterCreation.java` and `CharacterCreationView.java`. Rurik covers creation and persisted inventory in `authsrv.py:48236-48336` and database tests. | Kryta implements creation, selection, deletion and settings. Appearance variants and every source's complete rejection/update branch still require per-contract comparison. |
| Instance load, player data, spawn point, manifest, item stream and ready-to-play transition | ASURA, ASURA_testfork, OpenTyria, Rurik, Preforged, Preservation, Entice, GameRevision, Eden | Common and the richest cross-source contract. Preservation's ordered load helpers are `gameservice/player.go:219-555` and are exercised by `headless_test.go:296-310`. Entice's request handlers and reply order are in `InstanceLoadController.cs:18-151`. GameRevision has `InstanceLoad.java` and `InstanceLoadView.java:44-285`. Eden models the client acceptance side in `eden-network/src/game/session.rs:1207-1658`. | Kryta loads maps and has an end-to-end test that plays the client's conversation against the binary; the real-client proof is the manual procedure (`docs/port/07-manual-test.md`). The audit records the ASURA sequence as ported; source-specific additions must be compared as complete sequences, not appended because they exist elsewhere. |
| Map catalog, outpost travel, portal crossings, district/instance selection and map transfer | ASURA, ASURA_testfork, Rurik, Preforged, Preservation, Entice, GameRevision, Eden | Common. Rurik has transfer issuance, arrival and reachability in `authsrv.py:2147-2404`; Preservation has `TransferPlayerToNewMap` in `gameservice/instance.go:715-758` and transition tests; Eden encodes map and mission actions in `eden-network/src/game/ctos.rs:682-2073` and applies map state in its client. | Kryta implements map loading, travel, portal crossing, districts and transfer. Optional deferred map loading is not a completeness criterion because the normal map-loading contract already exists. |
| Movement: click, keyboard/directional movement, rotation, cancellation, correction, simulation ticks and path validation | ASURA, ASURA_testfork, OpenTyria, Rurik, Preforged, Preservation, Entice, GameRevision, Eden | Common. Preservation provides a particularly clear full path: handlers at `gameservice/handlers.go:89-109`, state/simulation at `movement.go:40-289`, and acceptance/validation tests in `movement_test.go` and `move_validation_test.go`. Rurik contains click/keyboard/cancel paths at `authsrv.py:45190-47922`; GameRevision has `MoveRotateClick.java`, `MovementSystem.java`, and `EntityMovementView.java`. | Kryta has click movement, position update, cancellation and last-position handling backed by its graph pathfinder. Keyboard/directional details remain an explicit contract-comparison subject rather than assumed coverage. |
| Agent lifecycle: spawn, despawn, visibility, appearance/equipment, position and generic property updates | ASURA, ASURA_testfork, Rurik, Preforged, Preservation, Entice, GameRevision, Eden, Headquarter | Common. Preservation sends active agents and peer introductions in `gameservice/instance.go:616-696`; GameRevision's `EntitySpawningView.java` emits spawn, NPC model/name and appearance; Eden applies world/agent events in `eden-client/src/world/mod.rs:414-622`; Headquarter has client object and event decoders. | Kryta has player/peer lifecycle and its current load agent path. NPC/monster lifecycle and source-specific appearance branches need a separate audit subject. |
| Chat, command parsing, local broadcast and emotes | ASURA, ASURA_testfork, OpenTyria, Rurik, Preforged, Preservation, Entice, GameRevision, Eden, Headquarter | Common. Preservation's local broadcast is `gameservice/instance.go:464-504` with headless tests; GameRevision connects `Chat.java`, `ChatSystem.java`, and `ChatMessageView.java`; Eden has both client UI state and game chat wire encoders. Rurik dispatches chat at `authsrv.py:48366-48376`. | Kryta implements chat plus the recorded chat commands. Emotes, channels, whispers and social chat are not established by that narrower implementation. |
| Party membership, invitations, leave/kick, ready state, leader and party UI | Rurik, Preforged, Preservation, Entice, Eden | Common outside the thin early servers. Rurik dispatches party, hero and henchman requests at `authsrv.py:45759-45853`; Entice's `PartyController.cs` changes party state; Eden has party request encoders and client application/UI. Preservation exposes a party invite handler but does not by itself prove a complete party contract. | Kryta has the one-player load party scaffolding, not a proven general party contract. This is a high-value comparison subject. |
| Heroes, henchmen, flags, target lock and hero AI | Rurik, Preforged, Eden | Rurik has explicit dispatches for hero AI, lock, add/kick, skill toggle and flags at `authsrv.py:13106-13238` and `45759-45797`. Eden has client encoders for hero AI/prioritization in `eden-network/src/game/ctos.rs:562-599`. | No complete Kryta contract has been identified in this inventory. |
| Inventory stream, bags, item creation, move, equip, destroy/drop, weapon sets and persistent item state | ASURA, ASURA_testfork, OpenTyria, Rurik, Preforged, Preservation, Entice, GameRevision, Eden, Headquarter | Common. ASURA_testfork has persistent item state and movement in `asura-db/src/inventory.rs:329-902`. Preservation handles load/equip/destroy/dye and validates its item manager. GameRevision's load view sends inventory pages, gold and weapon set packets. Preforged has inventory/ground/gold executable tests registered by `CMakeLists.txt`. | Kryta implements loading, move, equip and a silent drop around its character items. Destruction, gold, storage, transaction/refusal, and weapon-set selection are absent and must not be inferred from the load stream. |
| Dye and visual equipment coloring | Preservation, Preforged, Eden, Headquarter | Source-specific implemented paths: Preservation's `onDyeEquipment` calls `applyDyeToItem` (`handlers.go:296-329`, `player.go:199-217`); Preforged has `gw_dye_transactions`; Eden has client inventory/equipment application. OpenTyria is deliberately excluded: its color handler is a no-op at `GameSrv.c:1684-1691`. | No Kryta dye contract has been identified. This is a concrete candidate because there are real stateful sources, not merely an opcode. |
| Storage, trade, merchant, collector and crafter transactions | Rurik, Preforged, Eden, Headquarter | Preforged contains distinct merchant, collector, crafter, storage, transaction and trade modules plus registered tests. Eden encodes trade/item orders and applies inventory trade state; Headquarter observes client trade and merchant objects. | No complete Kryta contract has been identified. Separate each transaction type; a panel-opening packet alone is not a transaction. |
| Skills: skill bar edit, attributes, profession/secondary profession, cast admission, activation/recharge and effect publication | Rurik, Preforged, Preservation, Entice, GameRevision, Eden, Headquarter | Common. Rurik dispatches skill use, skillbar, attributes and secondary profession at `authsrv.py:45446-45464` and `48338-48352`. Preforged supplies the largest implementation set: skill policy, cast lifecycle, effects, cooldowns, conditions, interrupts, health/energy, weapon and projectile delivery, with executable tests registered in its `CMakeLists.txt`. Eden provides strongly typed C2S encoders and client state for skill/attribute/party messages. | Kryta sends load-time skill/profession data but has no demonstrated general skill-use/skillbar/attribute-edit contract. This is a family, not a claim that all named Preforged skills are individually ready to port. |
| Combat: targeting, attacks, damage/healing, conditions, enchantments, hexes, death/resurrection and AI-driven effects | Rurik, Preforged, Entice, GameRevision, Eden | Preforged is the main executable implementation: the dispatcher binds stateful combat callbacks in `src/apps/gw_auth.cpp`, while its runtime contains separate effect families and many registered tests (attack, wounds, degeneration, energy drain, interrupts, weapon hits, area effects and more). Rurik has attack/interact/target branches and quest-linked combat state. | No complete Kryta combat contract has been identified. Packet table rows and agent spawning do not amount to combat. |
| NPC interaction, dialogue, services and quests/objectives/rewards | Rurik, Preforged, Preservation, Entice, Eden, Headquarter | Rurik has the fullest stateful quest path: acceptance, markers, dialogue, objectives, rewards and turn-in at `authsrv.py:148-1704`, then request dispatch at `45634-45660`. Preforged has dialogue/quest service request paths and named quest implementations. Preservation has interaction handlers but does not alone prove a complete quest flow. Eden models NPC dialogue, quest log and client acceptance. | No complete Kryta NPC/quest contract has been identified. This is not an authorization to invent it; future work starts at the source with the most complete contract. |
| Guilds, friends, account social state, permissions, titles, factions, cartography and map unlocks | Rurik, Preforged, Preservation, Entice, GameRevision, Eden, Headquarter | Common as data/update paths, not uniformly full interactions. Preservation sends unlock, faction, title-adjacent, cartography and map state during load. GameRevision has faction, skills, attributes and private-profession views. Preforged has guild/friends/account sessions and policy modules. Eden applies friends, guilds, titles and map state client-side. | Kryta implements a narrow character/map unlock and cartography load slice, and a partial title display (load-time tracks and ranks, display/hide, peers told; no earning or persistence). It does not claim a complete social, guild, title or faction contract. |
| Client protocol tooling: typed codecs, captures, replay, hooks, packet tables and packet inspection | ASURA, ASURA_testfork, Rurik, Eden, Entice, GameRevision, Headquarter, OpenTyria | Common research infrastructure. Eden's typed game codecs, C2S encoders and tests are in `eden-network/src/game/`; Rurik validates codec round trips against captures in `toolkit/schema/test_codec.py`; Entice hooks original traffic; Headquarter has packet/event decoding; GameRevision has Netty codecs and serialization tests. | Kryta has its table, capture writer and reader/diff tools. It does not yet have an independent reference-client conversation test; the audit records that as missing. |
| Map/archive, navigation, terrain, model/material and client rendering research | ASURA, ASURA_testfork, Eden, Headquarter, offline prototype | This is client/data work rather than a network feature. Eden and the offline prototype have executable map/asset decoding and rendering; ASURA/Kryta own the server pathfinding source. Headquarter supplies archive and client-object inspection. | Kryta already has an offline archive reader (used by `tools/manage`), the map parser and the server pathfinder. These sources can improve data fidelity, but do not substitute a packet contract. |
| Operator, capture and process supervision | ASURA_testfork, Rurik, Preforged, Kryta | This is not client gameplay. ASURA_testfork contains capture and supervisor components; Rurik has a local orchestrator; Preforged has host/test tools. | Kryta's operator surfaces are tracked separately in `docs/port/14-operator-surfaces.md`; do not mix them into packet parity. |

## Source-by-source code ledger

The paths below are the code inspected for the table above. They are the starting points for a
line-by-line contract extraction, not citations to project prose.

| source | implementation inventory | evidence class |
|---|---|---|
| ASURA | Auth and game session dispatch, frame builders, creation, items, movement, travel, chat, map/instance load, peers, cartography, pathfinding and capture. | Full server path; the project Kryta was ported from, to parity. |
| ASURA_testfork | The same major server surfaces plus persisted account/login/character/inventory state, item move/drop/equip, map transfer, live movement variants, party/peer load frames, cartography, capture and supervisor. Key code: `crates/asura-auth/src/lib.rs`, `crates/asura-world/src/{agent,chat,item,map,player,travel}.rs`, `crates/asura-db/src/inventory.rs`, and database tests. | Full server path. |
| OpenTyria | Auth login/select/instance hand-off and game load, creation, chat, click movement, cancellation, last position and drop dispatches. `AuthSrv.c:1348-1397` and `GameSrv.c:1842-1904` are the dispatch roots. It also proves that a declared dye handler can be a no-op. | Mixed full server paths and no-op/protocol-only paths; inspect each branch. |
| Rurik | Stateful auth/game server in `toolkit/authsrv/authsrv.py`: login, persistence, instance load, movement, transfer, quest chains, dialogue, combat, skillbar/attributes, inventory, merchant transactions, party/heroes/henchmen, chat and commands. Code tests assert dispatch and packet ordering. Its imported `schema/messages.json` is not counted as independent OpenTyria proof. | Full server path, with unusually detailed contract/test evidence. |
| Preforged | `src/apps/gw_auth.cpp` binds auth, loading, quest, inventory, party, trade, merchant/crafter/collector, character, skills and combat to runtime state. `src/runtime/gw_*.{h,inc}` splits hundreds of concrete skill/effect/transaction families. `CMakeLists.txt` registers executable tests across wire, inventory, party, travel, combat and individual effects. | Full server paths and the broadest combat/skill implementation set. |
| Preservation | Auth and game finite-state connection handling; creation, load, agent lifecycle, movement/path validation, map transitions, chat, equip/destroy/dye, item manager, party invite, interaction, commands and DB state. Key code: `authservice/handlers.go`, `gameservice/{handlers,player,instance,movement}.go`; tests cover handshake, load sequence, movement, pathing and items. | Full server path for its implemented slices. |
| Entice | Old direct server controllers cover auth, creation, instance load, inventory, movement, party, skills, vendors, attack and chat. Its separate Elixir logic/client has entity state, maps, movement, group, skills, casting, vitals and Web channels. Original-client hooks and protocol classes expose the client boundary. | Full paths in the C# controller server, plus client-contract and observation evidence elsewhere. |
| GameRevision GWLP:R | Java login/map shards implement login/dispatch, character creation, instance load, agents, movement, chat, ping/timing, skills, attributes, factions and views built from typed packet classes. Many packet classes remain named `Unknown`, so named classes alone are not feature proof; the controller/view path is the evidence. | Server response and full paths for selected controllers; broad protocol material elsewhere. |
| Eden | A client implementation and research suite: auth/game/file sessions, typed packet decode/encode, C2S encoders for movement, skills, heroes, party, trade, items, chat and missions, and state application for world, agents, inventory, quests, skills, friends, titles and UI. The rich tests prove encoding/application but not a server state transition. | Client contract and acceptance evidence. |
| Headquarter | Original-client inspection/injection code: auth, networking, packet/event decoding and client object models for agents, items, inventory, party, guild, trade, titles, quests, skills and world, plus plugins. | Client observation/contract evidence, not a server feature implementation. |
| offline prototype | Archive/model/material/terrain/navigation extraction plus a browser map viewer. Its code has no game socket, client packet dispatcher or server simulation. | Excluded from packet-feature inventory; valuable only for offline data/render research. |
| WowWarband | AzerothCore structural reference, not Guild Wars protocol code. | Excluded from Guild Wars feature inventory; it remains the Kryta structure authority. |

## What is common, and what is genuinely useful next

The broad common core is now demonstrated in code, not assumed: login and hand-off; character
creation; map/instance load; travel; movement; agents; chat; inventory; and at least some
skills/party support. That says where multiple sources can corroborate a contract.

The strongest source-specific pools for expanding Kryta are:

| pool | why it matters | first contract to extract, not implement |
|---|---|---|
| Rurik quest/dialogue and heroes/henchmen | It couples C2S dispatch, persisted state, ordered responses and automated sequence assertions. | One quest accept -> objective -> reward/turn-in path; one hero add/remove/AI path. |
| Preforged transactions and combat/skills | It has explicit stateful mechanics plus a large executable test set, rather than just packet naming. | One merchant/collector transaction; one normal attack; one skill cast and recharge path. |
| Preservation dye and movement validation | Its small Go code makes packet-to-state-to-response ordering easy to audit, and tests isolate branches. | Dye a currently equipped item; keyboard move/cancel/correction across a map boundary. |
| Entice and GameRevision load paths | They provide independent ordered load/agent/party/message sequences. | Compare their instance load phases against ASURA and Kryta's `WorldSession::load_map`. |
| Eden typed client behavior | It can say precisely which fields/client updates an action needs, including many actions no current server implements. | One typed request/reply pair for party, trade, skills or NPC dialogue, followed by the client state it applies. |

None of those rows authorizes a port by itself. The next action for any selected row is a new
contract record with every packet and state transition, an ASURA/capture comparison, and a Kryta
line-by-line verdict. That prevents both false negatives (discarding evidence because its vessel
differs) and false positives (calling a packet name or an unconnected builder a feature).

## Inventory limits and next update rule

This pass inventories implemented feature families. It intentionally does not pretend that every
individual Preforged skill, every numbered GameRevision `Unknown`, or every Eden UI component is a
separately verified server contract. Those names remain discoverable in their source trees, but enter
the port ledger only when their complete packet/state path has been extracted.

When a feature is reduced to that path, add its contract record to
`docs/port/16-packet-completeness-research.md`; update `docs/port/09-parity-audit.md` in the same
commit when Kryta's client-facing behavior changes. Do not add a second feature checklist.
