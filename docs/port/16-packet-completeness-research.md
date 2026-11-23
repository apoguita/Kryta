# Packet-completeness research

This page is the work record for finding complete client packet contracts outside Kryta and comparing
them with Kryta. The canonical criterion is `docs/research-sources.md`, "Packet-sequence completeness
criterion". A function is not judged by its source's server design, adapter, process layout, language,
or client version. It is judged only by whether Kryta emits and handles the complete packet contract
that makes the function work.

`docs/port/17-research-code-feature-inventory.md` is the canonical code-derived inventory of feature
families and the local implementation evidence behind them. This page owns the next, narrower step:
the ordered contract record and Kryta comparison for one selected function. It does not repeat the
inventory.

`docs/port/09-parity-audit.md` remains the ledger of Kryta's current packet behavior. This page does
not summarize the ledger or replace Rule 0; it identifies evidence to turn into contract records.

## How a source is used

For one feature, read every relevant source implementation and extract one ordered list:

| part | record |
|---|---|
| trigger | client action or event, and its first packet |
| request side | every required client-to-server packet and meaningful field |
| response side | every required server-to-client packet and its order |
| state | the server state transition required before the next packet or reconnect |
| completion | what the client visibly accepts, or the capture/test proving acceptance |
| Kryta verdict | complete, partial, or absent, with the matching handlers/builders and audit rows |

The source's adapter or build is recorded beside the evidence only to explain packet translation. It
never rejects the source. If the source has a complete contract and Kryta does not, it is a research
subject. Client-facing code is ported from the source whose contract is the most complete (Rule 0);
where no project and no recording has the feature, ask the owner instead of inventing a response.

## Source inventory

This inventory says where packet contracts can be extracted. It does not claim a source's larger game
system is complete, and it does not dismiss a source because it uses a different vessel.

| source | packet-contract material | current use |
|---|---|---|
| ASURA | Packet table, wire framing, handlers, builders, tests, and captures. Kryta's audit currently compares 682 game and 100 auth rows plus ASURA's handled/emitted sequences. | The project Kryta was ported from; ported to parity, and now one source among the others. |
| ASURA_testfork | `MILESTONES.md` names proved user-visible functions; its forked code and tests can supply their packet contracts. | Extract a contract for each milestone before deciding whether Kryta already completes it. |
| OpenTyria | `GameSrv.c`, `AuthSrv.c`, `GameMsg.h`, `AuthMsg.h`, and the message definitions contain request/response sequences. | Packet source. A row still needs a real contract: for example, its `GAME_CMSG_ITEM_UPDATE_EQUIPPED_COLOR` handler at `GameSrv.c:1684-1691` is a no-op, so it proves no completed dye function. |
| Rurik | `schema/messages.json`, `toolkit/`, and `studies/` contain packet shapes, captures, client observations, and server flows for movement, skills, NPCs, quests, and parties. | High-value contract source. Extract each feature's ordered flow; do not mistake its imported OpenTyria table for independent corroboration. |
| Preforged | `src/apps/gw_auth.cpp`, `src/runtime/`, tests, and its feature record contain ordered packets for party, inventory/storage, skills, NPCs, quests, travel, and movement. | High-value contract source. Read packets and acceptance evidence, then compare their whole flow to Kryta. |
| Preservation | `authservice/conn.go`, `gameservice/conn.go`, generated `StoC_gen.go`, and tests define dispatch and response sequences. | Packet source awaiting feature-by-feature extraction. |
| Entice | The client middle layer intercepts original network communication (`research/entice/entice/README.md`) and `gw-interface/` holds protocol classes. | Packet-observation source: extract the original-client side of an intercepted exchange, not Entice's replacement transport. |
| GameRevision GWLP:R | The Java `protocol/` module has inbound/outbound packet classes and server shards. | Packet source awaiting contract extraction; `Unknown` class names do not themselves prove a function. |
| Eden and Headquarter | Client and plugin code can reveal client requests, handlers, and acceptance conditions. | Client-side packet-observation sources. They can complete the client half of a contract. |
| Offline prototype | Its README identifies it as a static viewer without game protocol or simulation. | No packet-contract material. |
| WowWarband | Structure reference for Kryta, not Guild Wars packet material. | No packet-contract material. |

## Initial comparison result

Kryta already matches ASURA's declared packet table exactly (`docs/port/09-parity-audit.md`, section 1),
but table equality is not function equality. The earlier source sweep incorrectly treated adapters and
client versions as a reason to reject other projects. That conclusion is withdrawn.

One feature outside ASURA has been reduced to an ordered packet contract: titles, in
`docs/port/19-title-research.md`, from retail and Preforged captures. ASURA has no counterpart, so
`docs/port/09-parity-audit.md` records the slice Kryta sends as ported thin from those recordings. No other feature has been
reduced this way. Therefore this page makes no claim that the remaining sources add
nothing, and it creates no false claim that Kryta is function-complete. The next research action is to
extract contracts by function, beginning with the richer packet sources: Rurik, Preforged, Preservation,
Entice's original-client boundary, and GameRevision.
