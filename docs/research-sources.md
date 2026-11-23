# Local research sources

## Why these sources exist

Kryta grew from ASURA: ASURA worked as a Guild Wars server, but its structure combined
responsibilities the owner wanted separated. Kryta ports that server's client-facing behaviour
into Rust while following AzerothCore's server structure and segmentation, as measured from
WowWarband. The goal is AzerothCore's arrangement serving Guild Wars, not ASURA's arrangement
under a different name.

The copies live in `research/`, which `.gitignore` excludes: a fresh clone of Kryta has no
`research/` folder, and nothing in the build reads it. The two authority checkouts AGENTS.md names,
`C:\Users\Apo\Documents\GitHub\ASURA` and `C:\Users\Apo\Documents\GitHub\WowWarband`, are the ones
to open and cite; `research/ASURA/` and `research/WowWarband/` are snapshots of them. Today both
pairs are at the same commit: ASURA `ca9f532a54f55f51e80076b656f489fc9f734f4c`, WowWarband
`30a36ec4d56dacee0ac89d7b54d5551093e7c08f`.

A relative citation such as `Eden/...` or `Rurik/...` in `docs/game-rules.md` names the original
checkout under `C:\Users\Apo\Documents\GitHub\` (`Eden`, `Rurik`). The copies under `research/`
carry the same sources: `research/Rurik/` is at the original's commit
(`0710df3b5cb068362c61120820753e59a36dd149`), and `research/Eden/` omits only the build output.

The projects in this folder include servers and clients, at different stages, with different designs.
Their documentation can conflict. Read a statement in its original project and revision as evidence
about that project, not as a current Kryta decision or proof of Kryta's implementation.

| Question | Source to use |
|---|---|
| Server structure, services, ownership and segmentation | AzerothCore as measured from WowWarband, with Kryta's recorded decisions for deviations; see `docs/port/01-structure.md` and `docs/decisions/`. ASURA's architecture is not Kryta's design authority. |
| Client-facing protocol and behaviour | The project or live recording that carries the most complete packet contract for the feature (Rule 0 in `AGENTS.md`). ASURA was the source of truth until parity with it was reached and is now one source among the others; `docs/port/09-parity-audit.md` records its port. Cite the source. |
| What the client actually does | The game client and Eden are the client sources (owner, 2026-11-23): Eden is a reverse-engineered game client and can be used for testing too. The running client and recordings are evidence; report a disagreement with the port. Interpret each capture by its layer as described in `docs/port/02-wire.md`. |
| Guild Wars rules and owner decisions | `docs/game-rules.md` and the applicable active Kryta documents; trace facts to their stated evidence. |
| Other implementations, prototypes and historical documents | Research and leads, not automatic authority for Kryta's structure, behaviour, current status or implementation. Resolve conflicts against the applicable sources above and ask the owner when the rules require it. |

This division does not make ASURA proof of complete live-game parity. A working ASURA behaviour,
a recording and an observation from a real client are distinct evidence. Kryta's current status is
recorded in `docs/port/01-structure.md`, and client-visible work is judged with a real client.

## Packet-sequence completeness criterion

Every client-visible function is a packet contract. It starts with a client request or event, continues
through every required server packet in order, changes the necessary state, and ends when the client
accepts the result. A function is complete only when Kryta fulfils that entire contract. Packet count,
program structure, language, transport adapter, process layout, and the project that supplied the
evidence are irrelevant to that question.

Any research project can supply packet evidence. Its adapter, client build, and implementation model
are provenance to record, not a reason to dismiss the evidence. Where a source's packet numbering or
shape differs, map it to Kryta's active protocol using the source's fields, packet order, and a current
capture; do not discard the function because it came through a different vessel.

For every candidate function, make one contract record containing:

1. The client trigger or event.
2. Every client-to-server and server-to-client packet, with the fields that matter and their required
   order.
3. The state change and the client-visible completion condition.
4. Kryta's corresponding handlers, builders, and ordered sequence, with a verdict: complete, partial,
   or absent.
5. The evidence source and its provenance, including build or adapter where known.

A declared table row, a no-op handler, or a feature-list bullet is not a contract record. Conversely,
a complete sequence remains relevant even when its source used a different adapter or server design.
For client-facing implementation, Rule 0 requires opening the sources that implement the feature and
porting from the one whose contract is the most complete. If no project and no recording has it, say
so and ask the owner rather than inventing a Kryta response. `docs/port/17-research-code-feature-inventory.md` is the canonical
code-derived feature inventory; `docs/port/16-packet-completeness-research.md` records the source
survey and the contract-comparison method.

## Copies and origins

The source copies live under `research/`, which is ignored by Kryta's Git repository. They are
research materials, not Kryta dependencies. Check each source's own license and provenance before
reusing its material. Recorded revisions identify the copies initially placed here; check the local
HEAD before citing a file because a checkout can change.

| Source | Local path | Origin and role | Initial revision |
|---|---|---|---|
| WowWarband / AzerothCore | `research/WowWarband/` | [WowWarband](https://github.com/apoguita/WowWarband); [AzerothCore reference](https://github.com/mod-playerbots/azerothcore-wotlk). Server infrastructure and design authority. | `30a36ec4d56dacee0ac89d7b54d5551093e7c08f` |
| ASURA | `research/ASURA/` | [ASURA](https://github.com/apoguita/ASURA), the server Kryta was ported from; the source of truth until parity, one source among the others since. | `ca9f532a54f55f51e80076b656f489fc9f734f4c` |
| Superjack fork | `research/ASURA_testfork/` | [ASURA_testfork](https://github.com/aeratemark/ASURA_testfork), including its [milestones](https://github.com/aeratemark/ASURA_testfork/blob/main/MILESTONES.md). | `eff172b7e5b6f149c3b953c482e26e4f04dd5489` |
| Preforged | `research/preforged-source/` and `research/preforged/` | [preforged](https://github.com/CmonMeow/preforged). Source, headers, documentation, tests, config and third-party files were extracted to `preforged-source/`. The Git repository is in `preforged/`, but its working tree is incomplete. | `9183090ac02047ea8fcb2bb8bdc2b665dee4f75c` |
| OpenTyria | `research/OpenTyria/` | [OpenTyria](https://github.com/ldufr/OpenTyria). | `53acb8e984784d0e219e4e4ade6a4925562e3fe6` |
| Headquarter | `research/Headquarter/` | [Headquarter](https://github.com/ldufr/Headquarter), a client and plugin project useful for Guild Wars research; see its `LICENSE`. | `223c9ee33ce3c34bd733505cabee47f103ce92cd` |
| Eden | `research/Eden/` | Copy of the unpacked `C:\Users\Apo\Documents\GitHub\Eden` custom client for live Guild Wars servers; its workspace identifies [Eden](https://github.com/berlic/eden). Its source, documentation and vendored material are included; build output is omitted. See `NOTICE` and `docs/licenses/` before reuse. | Unpacked source; revision not established. |
| Rurik | `research/Rurik/` | [Rurik](https://github.com/GameJawnsInc/Rurik), a Guild Wars server emulator and mod-kit research project. Its skill and combat implementation was reviewed in `docs/port/12-research-and-control-panel.md`. See its `LICENSE` and `THIRD-PARTY-NOTICES.md` for licensing. | `0710df3b5cb068362c61120820753e59a36dd149` |
| Entice | `research/entice/` | Extracted from `Downloads/entice-repos.zip`: several related repositories described in its [project documentation](https://github.com/entice/entice), including client, server logic, web and entity projects. | ZIP SHA-256 `E919661DA870D4B5AEEDF49E9FCBCE870EB72E638F91ECF5DAF6FDFDE32073EB`; individual revisions not established. |
| GameRevision GWLP:R | `research/GameRevision-GWLP-R-4208ffa/` | Extracted from `Downloads/GameRevision-GWLP-R-4208ffa.zip`; its README identifies [GWLP-R](https://github.com/GameRevision/GWLP-R) as a Java Guild Wars server emulator and names Entice as its successor. | ZIP SHA-256 `D1A3FC25AB746D46685C0CC3834D951816441D342A7E4EECFCE3641FDBC99F41`; `4208ffa` appears in the archive name. |
| Preservation | `research/preservation-main/` | Extracted from `Downloads/preservation-main.zip`; a Go Guild Wars server source tree containing protocol and pathing material. The archive does not establish its repository URL or revision. | ZIP SHA-256 `B7E2597D13DFE886682AD3B0BB00B510E25008F95EAFC524302756CA047A92C1`. |
| GWPS (PreSearing) | `C:\Users\Apo\Documents\GitHub\PreSearing` only; no copy under `research/` | [GWPS](https://github.com/apoguita/GWPS), a C server for Pre-Searing Ascalon forked from [apoguita/OpenTyria](https://github.com/apoguita/OpenTyria) (its `upstream` remote). The checkout holds `code/OpenTyria/`, `portal/`, `docs/`, `tools/`, `data/` (`gwdat-maps.csv`, `packet-catalog.csv`), `db/` and `captures/`, with an ignored `Gw.dat`, `captures/` and `db/database.db`. The server whose traces are `recordings/gwps/`. | `d246a2a811191644d02d1ad7c285a6615622153f` |
| Offline prototype | `research/prototype-offline-20261007-v1-apoguita-en/` | Extracted from `C:\Users\Apo\Downloads\prototype-offline-20261007-v1-apoguita-en.zip`. | ZIP SHA-256 `A97AA8C2320C30B8F6B7310FE0ECE7D81F412B5EC85441E5A8A5D1989B47AD6B` |

The existing Git checkouts were cloned locally, not moved. Local clones contain tracked Git
content and history, not ignored or untracked files; `_fork_asura_test/_code_diff.txt` remains only
in the original checkout. The original ASURA and WowWarband paths remain in use by AGENTS.md and
existing documentation. Creating these copies does not change source authority or replace those paths.
The prototype, Entice, GameRevision and Preservation ZIPs remain in Downloads. Extracted archives
are snapshots, not Git checkouts; use their archive hashes for provenance where revisions are unknown.

Preforged's full checkout is approximately 113 GB, mainly capture data, and was stopped with about
19 GB free on the drive. Its compressed Git objects are present locally, but the full working tree
was not checked out. The source extraction omits `data/` and `bin/`; additional storage is needed to
expand the complete checkout. Do not treat the partial working tree as complete.
