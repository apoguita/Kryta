# Recovered ASURA documentation: coverage and disposition

This page covers the ASURA documentation recovered under `docs/evidence/asura-history/`. It is not an
audit of Kryta's own documents.

Status: source inventory and historical revision scans completed at subject level; individual
claims remain to be verified as their subjects are implemented or investigated. The audit compares
recovered ASURA documents with Kryta's maintained documentation and traces earlier revisions where
material may have been lost. No runtime behavior
is changed by this audit. `AGENTS.md` governs code and protocol work.

## Work completed

- Labeled the separate historical client-protocol reference so ASURA commands and status are not
  mistaken for current Kryta instructions.
- Corrected packet-size guidance, distinguished the live profession-change order from ASURA's
  order, and reconciled map-unlock and portal status with the current code and parity ledger.
- Corrected the script-manager and world-update descriptions and qualified partial rule coverage.
- Traced ASURA's owner-directed file content against Kryta's world schema decision; it is settled by
  `docs/decisions/0014-the-world-content-is-a-pack.md`, which supersedes `0009-world-schema.md`.
- Restored the owner's scripting, profile, HeroAI and control-panel intentions in
  `docs/port/11-content-and-behaviour.md` and `docs/port/12-research-and-control-panel.md`.
- Reconciled R11/R12, R22, R27, E199 and E220 with the later ASURA world-model review and
  recorded unresolved world-model questions in `docs/game-rules.md`. All 279 distinct R/E identifiers
  found across six historical versions of the ledger and later world model exist in Kryta's ledger.
  Identifier preservation does not prove every revision has identical wording.

## Historical revision scan

The archive has the latest recovered version of each path, not every intermediate version.
A scan of all 658 Markdown blob/path pairs from local ASURA history compared normalized lines
longer than 65 characters with every recovered snapshot. The 57 paths below have earlier lines
absent from all snapshots. This is a discovery aid, not a semantic comparison: rewording, changed
line wrapping, corrections and moved content can produce matches or differences. A low count does
not prove that a path is complete, and a high count does not prove that useful information was lost.
The table records the subject-level disposition of the identified lines against the commits, later
ASURA consolidations and Kryta's active documents. Earlier versions are retrieved from ASURA with `git log --all -- <path>` and
`git show <commit>:<path>`. Every path below has a subject-level disposition; claim-level
verification happens as each subject is implemented or investigated. A delta disposition below covers only the earlier lines identified by the scan; it does
not claim the entire source or every shorter historical line has been semantically checked. Historic commands and implementation claims apply to their recorded revisions.

| ASURA path | Distinct earlier lines absent from recovered snapshots | Review |
|---|---:|---|
| `docs/HANDOFF.md` | 683 | historical deltas reviewed: historical session status, decisions and incidents; surviving owner decisions are in game-rules, 11 and 12; deliberately withdrawn additions remain withdrawn |
| `docs/03-protocol.md` | 436 | historical deltas reviewed: superseded 4-byte wire interpretation, portal and chat variants, earlier measurements and commands; current evidence and parity are in 02, 09 and evidence |
| `docs/37-rework-layers.md` | 217 | historical deltas reviewed: dated ASURA layer checklist and earlier claims of closure; current Kryta parity and separation are in 09 and 10 |
| `docs/10-parity-queue.md` | 200 | historical deltas reviewed: dated GWPS port queue and prior pathfinding, load, item and chat results; current ASURA port and outstanding items are in 09 |
| `docs/02-architecture.md` | 178 | historical deltas reviewed: successive ASURA service and placement designs, including superseded direct-dial and prebuild claims; current front door and structure are in 01 and decisions 0003/0004 |
| `docs/09-parity-audit.md` | 123 | historical deltas reviewed: historical GWPS versus ASURA behavior snapshots and item/chat deviations; current comparison is 09 and captured observations remain in archive |
| `docs/roadmap.md` | 122 | historical deltas reviewed: successive ASURA milestone, acceptance and simulation plans; current Kryta sequence is 01, 06, 09 and 11 |
| `docs/07-status.md` | 112 | historical deltas reviewed: historical ASURA handler and switch status including contradictory earlier item/chat experiments; current Kryta status is in 01, 08 and 09 |
| `docs/16-implementation-plan.md` | 108 | historical deltas reviewed: historical supervision decisions and implementation claims; owner-approved rolling deployment preserved in 12 with current status in 05 |
| `docs/08-next-inventory-model.md` | 85 | historical deltas reviewed: historical ASURA inventory experiments and earlier conflicting move/drop observations; retained inventory reference and current parity audit distinguish them |
| `docs/work/re-engineering-plan.md` | 71 | historical deltas reviewed: earlier ASURA phase progress and direct-client route superseded by measured front-door decision; current Kryta structure and separation are in 01 and 10 |
| `docs/19-running.md` | 67 | historical deltas reviewed: historical ASURA operations and earlier stream comparisons; use current Kryta commands and capture-layer guidance in 05 and 07 |
| `docs/01-scope.md` | 66 | historical deltas reviewed: historical ASURA service design and prebuilding assumptions; current boundaries are in 01 and 10 |
| `docs/00-glossary.md` | 63 | historical deltas reviewed: historical ASURA terminology and superseded framing and prebuilding claims; current wire and world rules are in 02 and game-rules |
| `docs/39-map-list.md` | 56 | historical deltas reviewed: older labels and placement names are retained in later archived map catalogue; later subarea categories differ and require source-specific use |
| `docs/15-service-startup.md` | 55 | historical deltas reviewed: historical ASURA supervision research and proposals; Kryta tooling status is in 05 |
| `docs/22-world-model.md` | 43 | reviewed: older spawn, doorway, SQL and instance-model claims are historical; later qualifications and rules are in `docs/game-rules.md` |
| `docs/27-packet-parity-attempt.md` | 30 | historical deltas reviewed: superseded incorrect struct/wire and acknowledgement conclusions; corrected incident retained in historical record and current layer distinction in 02 |
| `docs/06-reference-session.md` | 28 | historical deltas reviewed: recorded session and earlier ASURA framing interpretations; retained capture reference and current wire guidance are in evidence and 02 |
| `docs/33-control-panel-design.md` | 26 | historical deltas reviewed: the editor section was deliberately removed in ASURA commit `bc58da1` on the owner's instruction; do not reinstate its claims as project decisions |
| `docs/04-from-gwps.md` | 25 | historical deltas reviewed: historical gateway and vendor plans, and patcher origin; current tooling and architecture are in 05 and 01 |
| `docs/21-surrounding-packets.md` | 18 | historical deltas reviewed: historical packet sightings, interpretation and task queue; evidence and limitations are retained in 12 and the archive |
| `docs/38-layer-decisions.md` | 18 | historical deltas reviewed: earlier D20/D24/D39 content-file direction adopted by Kryta decision `0014-the-world-content-is-a-pack.md`, recorded in 11; other changes are historical |
| `docs/05-gw2-lessons.md` | 16 | historical deltas reviewed: historical external-talk research and ASURA deployment proposals; no new Kryta policy follows |
| `docs/adr/0003-simulation-in-scope.md` | 15 | historical deltas reviewed: superseded external C simulation phases; current separation is in 01 and 10 |
| `docs/adr/0004-port-azerothcore-structure.md` | 15 | historical deltas reviewed: historical source structure and vendor debate; current authority and PostgreSQL decision are in 01 and decision 0001 |
| `docs/guidance/architecture.md` | 15 | historical deltas reviewed: earlier ASURA database ownership and lease description; current Kryta structure and seams are in 01 and 10 |
| `docs/11-parity-state.md` | 14 | historical deltas reviewed: historical pathfinding and generated-check status; current pathing status is in 01 and 09 |
| `docs/adr/0009-crypto-primitives.md` | 13 | historical deltas reviewed: historical fixture and crate discussion; current cryptography evidence is in 02 and source |
| `docs/adr/0010-gwps-is-the-floor.md` | 13 | historical deltas reviewed: historical ASURA auth feature snapshot; current parity authority and audit are AGENTS and 09 |
| `docs/17-running-on-linux.md` | 12 | historical deltas reviewed: historical ASURA Linux operations and no recorded Linux client acceptance; do not use commands as Kryta instructions |
| `docs/32-world-simulation-plan.md` | 12 | historical deltas reviewed: earlier phased simulation proposal; present world mapping and Lua design are in 06 and 11 |
| `docs/30-world-content-and-scripting.md` | 10 | historical deltas reviewed: earlier mechanics and Lua division retained in 11, with unresolved callback scope explicit |
| `docs/adr/0001-rust.md` | 10 | historical deltas reviewed: superseded C world direction and earlier proposal; current whole-stack Rust direction is in AGENTS and 01 |
| `docs/adr/0006-azerothcore-is-the-reference.md` | 10 | historical deltas reviewed: historical design authority rationale; current authority order is AGENTS |
| `docs/work/layer-rework-checklist.md` | 10 | historical deltas reviewed: historical layer status and earlier settings questions; current port audit is 09 |
| `docs/20-rurik-gap.md` | 9 | historical deltas reviewed: old source-specific status and skill proposal; qualifications and research are in 12 |
| `docs/28-headers-to-name.md` | 9 | historical deltas reviewed: earlier 18-header queue superseded by 17-header ASURA research, referenced in 12 |
| `docs/adr/0008-database-vendor-mysql.md` | 9 | historical deltas reviewed: superseded MySQL decision; Kryta vendor decision is 0001 |
| `docs/14-test-plan.md` | 8 | historical deltas reviewed: historical test names and GWPS acceptance; current testing and manual-client instructions are in 07 |
| `docs/23-initialization-protocol.md` | 8 | historical deltas reviewed: superseded ASURA handler descriptions and capture status; current protocol and parity are in 02 and 09 |
| `docs/adr/0005-gwps-reference-only.md` | 7 | historical deltas reviewed: historical ASURA on-demand research proposal; current authority order is AGENTS |
| `docs/adr/0008-database-vendor.md` | 7 | historical deltas reviewed: historical PostgreSQL comparison; Kryta vendor decision is 0001 |
| `docs/18-multi-machine.md` | 6 | historical deltas reviewed: historical single-machine routing observation, retained with its unperformed real-client test in archive research |
| `docs/24-stream-parity.md` | 5 | historical deltas reviewed: historical opcode naming and coverage states; current ASURA comparison is in 09 |
| `docs/36-game-rules-ledger.md` | 5 | historical deltas reviewed: superseded spawn wording and ledger references; later rules and unresolved entry scope are in game-rules |
| `docs/12-schema-parity.md` | 4 | historical deltas reviewed: historical GWPS schema comparison; current schemas and EULA are in decisions and 01 |
| `docs/13-command-parity.md` | 4 | historical deltas reviewed: historical command-generation description; current chat and travel targets are in 01 and 09 |
| `docs/31-heroai-server-feasibility.md` | 4 | historical deltas reviewed: earlier reuse and validation proposal carried into 11 and 12 |
| `docs/adr/0007-rust-translation.md` | 4 | historical deltas reviewed: historical Rust and MySQL proposal; current structure and vendor are in 01 and decision 0001 |
| `docs/adr/0014-behavior-tree-execution.md` | 4 | historical deltas reviewed: earlier Rust leaf wording qualified by the later Lua boundary in 11 |
| `docs/work/project-status.md` | 4 | historical deltas reviewed: historical ASURA test and runtime status; current Kryta status is 01 and 08 |
| `docs/29-initialization-capture.md` | 3 | historical deltas reviewed: historical ASURA capture wrapper and unnamed count; capture requirements are in 12 |
| `docs/adr/0012-eden-is-a-source.md` | 3 | historical deltas reviewed: historical unmeasured client question; source order is AGENTS |
| `docs/40-guidelines.md` | 1 | historical deltas reviewed: historical proposed value-location approval; current authority is AGENTS and 04 |
| `docs/adr/0002-separate-project.md` | 1 | historical deltas reviewed: historical world-link consequence for the earlier C design |
| `docs/reference/inventory-model.md` | 1 | historical deltas reviewed: earlier ASURA repository path renamed later; historical model retained in archive |

A second scan reviewed 698 distinct absent normalized lines of 10 to 65 characters across 48 paths.
They were headings, continuations, dated status, old commands, previously superseded architecture,
or historical observations within the subject dispositions above. The scan is a discovery aid:
line wrapping, punctuation and words shorter than ten characters are not semantic verification.
The historical source inventory remains available for a deeper investigation of a specific subject. The source inventory and blob identities are in
`docs/evidence/asura-history/README.md`. Client measurements remain scoped to their captures.
Unresolved game-rule questions remain in `docs/game-rules.md` until the owner or evidence settles them.

## Recovered source disposition

Every recovered Markdown path is assigned once by subject below. The destination identifies the
current Kryta home or the retained historical source. This is a subject-level disposition, not a
claim that all facts and historical versions were independently verified against a client.

| Subject | Kryta disposition | Recovered ASURA paths |
|---|---|---|
| Entry and handoff | Kryta AGENTS, 01, 07 and 13; earlier handoffs are dated history. | `docs/00-glossary.md`<br>`docs/HANDOFF.md`<br>`docs/README.md`<br>`docs/reference/glossary.md`<br>`docs/work/session-handoff.md` |
| Architecture and decisions | Kryta 01, 10 and decisions; the content-file direction is settled by decision `0014-the-world-content-is-a-pack.md`. | `docs/01-scope.md`<br>`docs/02-architecture.md`<br>`docs/05-gw2-lessons.md`<br>`docs/38-layer-decisions.md`<br>`docs/40-guidelines.md`<br>`docs/guidance/architecture.md`<br>`docs/reference/decisions/layer-decisions.md`<br>`docs/reference/decisions/project-decisions.md`<br>`docs/research/azerothcore-structure.md`<br>`docs/research/gw2-service-design-lessons.md` |
| Wire and captures | Kryta 02, 09 and evidence; recorded research and capture layer limits are in 12. | `docs/03-protocol.md`<br>`docs/06-reference-session.md`<br>`docs/21-surrounding-packets.md`<br>`docs/23-initialization-protocol.md`<br>`docs/24-stream-parity.md`<br>`docs/26-packet-gaps.md`<br>`docs/28-headers-to-name.md`<br>`docs/29-initialization-capture.md`<br>`docs/operations/capturing-instance-initialization.md`<br>`docs/reference/captures/login-session-2026-10-26.md`<br>`docs/reference/client-protocol.md`<br>`docs/reference/instance-initialization.md`<br>`docs/research/client-session-packet-analysis.md`<br>`docs/work/packet-coverage.md`<br>`docs/work/unnamed-packets.md` |
| Parity history and status | Kryta 01, 08 and 09; GWPS comparisons and earlier acceptance stay historical. | `docs/07-status.md`<br>`docs/09-parity-audit.md`<br>`docs/10-parity-queue.md`<br>`docs/11-parity-state.md`<br>`docs/12-schema-parity.md`<br>`docs/13-command-parity.md`<br>`docs/14-test-plan.md`<br>`docs/25-client-readiness.md`<br>`docs/27-packet-parity-attempt.md`<br>`docs/history/gwps-parity/behavior-audit.md`<br>`docs/history/gwps-parity/chat-command-comparison.md`<br>`docs/history/gwps-parity/completed-work-queue.md`<br>`docs/history/gwps-parity/database-schema-comparison.md`<br>`docs/history/gwps-parity/handler-status.md`<br>`docs/history/gwps-parity/parity-snapshot-2026-10-31.md`<br>`docs/history/gwps-parity/reference-test-coverage.md`<br>`docs/history/packet-parity-attempt-2026-10-31.md`<br>`docs/work/project-status.md` |
| Operations and deployment | Kryta 05 and 07; historical supervisor intent and limitations are in 12. | `docs/15-service-startup.md`<br>`docs/16-implementation-plan.md`<br>`docs/17-running-on-linux.md`<br>`docs/18-multi-machine.md`<br>`docs/19-running.md`<br>`docs/history/service-supervisor-delivery-plan.md`<br>`docs/history/service-supervisor-research.md`<br>`docs/operations/running-on-linux.md`<br>`docs/operations/running-the-server.md`<br>`docs/research/multi-host-routing-experiment.md` |
| World model and game rules | Kryta game-rules and 08; original catalogue remains evidence with source qualifications. | `docs/22-world-model.md`<br>`docs/36-game-rules-ledger.md`<br>`docs/39-map-list.md`<br>`docs/reference/game-world-model.md`<br>`docs/reference/map-catalogue.md` |
| Scripting, profiles and research | Kryta 06, 11 and 12; detailed research remains cited historical evidence. | `docs/20-rurik-gap.md`<br>`docs/30-world-content-and-scripting.md`<br>`docs/31-heroai-server-feasibility.md`<br>`docs/32-world-simulation-plan.md`<br>`docs/33-control-panel-design.md`<br>`docs/34-self-contained-profiles.md`<br>`docs/35-skill-mechanics-and-behavior.md`<br>`docs/guidance/game-packs.md`<br>`docs/guidance/scripting-and-behavior.md`<br>`docs/research/heroai-server-port.md`<br>`docs/research/rurik-capabilities-and-gaps.md`<br>`docs/research/skill-mechanics-and-behavior.md`<br>`docs/work/control-panel-plan.md`<br>`docs/work/world-simulation-plan.md` |
| Inventory research | Kryta 01, 09 and the archived qualified inventory reference. | `docs/08-next-inventory-model.md`<br>`docs/reference/inventory-model.md` |
| Roadmaps and layer plans | Kryta 01, 06, 09 and 10; earlier ASURA plans are historical. | `docs/37-rework-layers.md`<br>`docs/roadmap.md`<br>`docs/work/layer-rework-checklist.md`<br>`docs/work/project-roadmap.md`<br>`docs/work/re-engineering-plan.md` |
| Project history and reviews | Retained in the historical archive; current project instructions are in AGENTS. | `docs/04-from-gwps.md`<br>`docs/history.md`<br>`docs/history/development-incidents.md`<br>`docs/history/gwps-project-origins.md`<br>`docs/history/infrastructure-milestones.md`<br>`docs/history/reviews/documentation-organization-2026-11-07.md`<br>`docs/plans.md`<br>`docs/work/documentation-consistency.md` |
| Historical ADRs | Retained as ASURA decisions and history; current Kryta authority is AGENTS and docs/decisions. ADR 0014 supplies the pinned behavior-tree reference in 11. | `docs/adr/0001-rust.md`<br>`docs/adr/0002-separate-project.md`<br>`docs/adr/0003-simulation-in-scope.md`<br>`docs/adr/0004-port-azerothcore-structure.md`<br>`docs/adr/0005-gwps-reference-only.md`<br>`docs/adr/0006-azerothcore-is-the-reference.md`<br>`docs/adr/0007-rust-translation.md`<br>`docs/adr/0008-database-vendor-mysql.md`<br>`docs/adr/0008-database-vendor.md`<br>`docs/adr/0009-crypto-primitives.md`<br>`docs/adr/0010-gwps-is-the-floor.md`<br>`docs/adr/0011-rurik-is-a-source.md`<br>`docs/adr/0012-eden-is-a-source.md`<br>`docs/adr/0013-py4gw-stealth-is-our-link-to-the-client.md`<br>`docs/adr/0014-behavior-tree-execution.md`<br>`docs/reference/decisions/0001-control-plane-language.md`<br>`docs/reference/decisions/0002-project-separation.md`<br>`docs/reference/decisions/0003-simulation-scope.md`<br>`docs/reference/decisions/0004-server-structure.md`<br>`docs/reference/decisions/0005-gwps-reference-boundary.md`<br>`docs/reference/decisions/0006-structural-design-authority.md`<br>`docs/reference/decisions/0007-whole-stack-rust.md`<br>`docs/reference/decisions/0008-postgresql.md`<br>`docs/reference/decisions/0009-protocol-cryptography.md`<br>`docs/reference/decisions/0010-gwps-behavior-coverage.md`<br>`docs/reference/decisions/0011-rurik-source-policy.md`<br>`docs/reference/decisions/0012-eden-source-policy.md`<br>`docs/reference/decisions/0013-py4gw-client-research-policy.md`<br>`docs/reference/decisions/0014-behavior-tree-execution.md` |
