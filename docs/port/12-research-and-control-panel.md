# Research and control-panel requirements

The ASURA documentation recorded research and a proposed operator workflow that remain relevant to
Kryta. This page retains the decisions, questions and evidence needed to resume that work. It does
not claim the proposed features exist. Kryta's actual tools and commands are in
`docs/port/05-tooling.md`; client testing is in `docs/port/07-manual-test.md`.

## Control panel

The shared API, configuration-manager requirements and research on existing applications are in
`docs/port/14-operator-surfaces.md`. That plan studies external tools; it does not select their code
for reuse.

The owner requested a control panel for operating the server, capturing and inspecting the client,
guiding tests, reviewing recordings, and editing configuration and game content. The ASURA plan is
`docs/evidence/asura-history/docs/work/control-panel-plan.md`; its ASURA commands, file paths and
implementation status are historical. Kryta's existing administration page and supervisor are
listed in `docs/port/05-tooling.md`. Reuse the capabilities that Kryta actually has, and identify
missing ones before deciding implementation steps.

The intended workflows include starting and stopping services, viewing status and logs, selecting a
client process, recording packets or reading supported client state, taking snapshots, associating
observations with a run, finding previous recordings, and editing supported content. An operator can
read client state without starting packet capture, and can capture a live client without starting the
private server. A run should preserve its target, client identity, timestamps, requested and actual
capture channels, artifact locations, observations and results, including partial failures.

Client-side capture, server-side wire capture and state snapshots measure different things. The
client capture reports message structures, while the server capture records decrypted wire messages;
`docs/port/02-wire.md` defines the three capture kinds. Preserve the capture layer and provenance and
keep original recordings separate from notes and derived files. A successful tool run, an automated
check, and a player's observation are distinct results. The player's actions remain manual unless
otherwise requested.

The panel should identify the process and environment being inspected and the profile being edited
or run once profile selection exists. Pin capture and inspection tasks to the selected process;
if it exits, do not silently switch to another client. The recorded first content-editing example
is default map spawns, portal destinations and arrival points, verified with the real client. Kryta
stores those in the world pack's per-map files (`docs/decisions/0014-the-world-content-is-a-pack.md`),
and the panel's content page reads them without editing them. A saved content change does not establish a live reload. The
client research runner operates on the client machine and should not be a dependency of ordinary
server operation. ASURA's historical plan documents capture tool and hook limitations; check the
current tools before carrying any specific command or capability forward.

The owner selected a web application for the comprehensive server manager, accessible locally and
remotely and delivered as part of the server stack. The local panel exists: `kryta-management-service`
serves `tools/manage` as a stack service (`docs/port/14-operator-surfaces.md`, section 2). Its remote
hosting and the possible use of the existing portal remain to be designed; `14` records the access
decisions.
Initial capture concurrency, first reader coverage, storage format and profile packaging remain open.
The requested
research and content workflows are part of the intended panel, while their implementation order and
initial scope require a concrete plan. Profile isolation and script ownership are described in
`docs/port/11-content-and-behaviour.md`.

## Earlier service-operation work

The owner approved ASURA work on a supervisor, service registry and rolling deployment; the decisions
and their dated ASURA evidence are in `docs/evidence/asura-history/docs/history/
service-supervisor-delivery-plan.md`. The recorded rolling-deploy intent starts a new world beside the
old, sends new arrivals to the selected build, and lets the old world drain existing players before
it exits. ASURA's one-machine second-address experiment and its unperformed real-client and real-host
checks are in `docs/evidence/asura-history/docs/research/multi-host-routing-experiment.md`.

These are historical ASURA decisions and observations, not proof that Kryta implements or accepts
rolling deployment, registry reconciliation or multi-host operation. `docs/port/05-tooling.md` owns
what Kryta's supervisor does today. The current owner-selected separation, profile isolation and
AzerothCore structure must be reconciled when scheduling those capabilities for Kryta.

## Skill and combat research

ASURA's `docs/research/skill-mechanics-and-behavior.md` records a source review of Rurik at
`0710df3b5cb068362c61120820753e59a36dd149` and illustrative wiki research. It reports cast queues,
activation and recharge, projectiles, effects, attack reactions, NPC selection and targeting. It also
records important limits: Rurik's queued-cast behavior explicitly differs from its stated retail
behavior; its round-robin skill choice and healing policy are authored policies; and some examples
are incomplete. These observations are source research, not proof of live-game parity or authority
for Kryta packet layouts. ASURA's own implementation and the ledger govern a port.

The examples illustrate why the eventual engine needs operations capable of expressing distinct
effects. Projectile impact, timed regeneration, eligible damage limits and attack-triggered reactions
are different operations. Their ordering, exact values and interaction with other effects require
appropriate evidence. The owner's Lua/Rust boundary and custom-skill acceptance case are maintained
in `docs/port/11-content-and-behaviour.md`.

## HeroAI workload research

The source inspection in `docs/evidence/asura-history/docs/research/heroai-server-port.md` traces
headless initialization, tree execution, build dispatch, the generic skill evaluator, target and
effect checks, and action requests. The reviewed code obtains actor state through client globals,
shared memory and observation. Server reuse needs an explicit acting agent and instance, authoritative
state queries and supported world actions. More complete server information does not itself establish
what an NPC should perceive or how quickly it should react.

The review identifies repeated skill preparation, target scans and sorts, effect lookups and interrupt
observation as candidates for measurement. It describes potential costs and cache/index ideas, not
benchmarked bottlenecks. Any reuse of prepared data must preserve selection order, eligibility,
range, tie-breaking and invalidation. The workload example of 100 instances with 200 agents each is
a capacity target to test, not an established result.

The behaviour-tree source comparison and migration concerns are maintained with the chosen design in
`docs/port/11-content-and-behaviour.md`. Earlier Lua timing reports in the historical research lack a
retained reproducer and do not establish server capacity.

## Client and world research

Eden (`research/Eden/`) is a custom Guild Wars client for live servers. The owner wants it
considered as another research client: investigate patching or configuring it to connect to Kryta,
as the official client is patched, and using it for debugging and automated research. Its source may
also help inspect client behaviour. The connection changes, debugging interfaces and automation
capabilities have not been assessed or implemented for Kryta. Observations from Eden and the
official client must retain their source and be compared when they differ; they do not replace
Rule 0 or real-client verification. Its provenance and reuse notices are in `docs/research-sources.md`.

The later ASURA world-model review reports client-context observations in
`docs/evidence/asura-history/docs/reference/game-world-model.md`, "Client-context findings recorded
2026-10-30". It describes progress arrays, map spawn lists and portal props, and corrects an earlier
agent classification. That review did not identify an ASURA capture for those context readings.
Treat them as source-only findings when selecting further measurements, not as a new wire or trigger
specification. Current asserted rules and unresolved travel and spawn questions are in
`docs/game-rules.md`.

The recovered documents on initialization, surrounding packets, unnamed headers and parity contain
historical captures, interpretations and queues. Use their cited recordings to investigate gaps,
then compare against `docs/port/02-wire.md` and the current protocol ledger in
`docs/port/09-parity-audit.md`. Captured structures and wire frames must be interpreted by their own
capture layers. Do not take historical ASURA progress tables or old commands as Kryta status.

## Recorded initialization observations

ASURA's historical `docs/reference/instance-initialization.md` lists a live map-148 load and its
capture, grouped by the sequence of load, inventory, agent, quest, guild, title and instance-table
messages. Its counts describe that capture. Another recorded ASURA run reported different population
counts between a district change and full loads. Neither result establishes a universal count or a
reason to discard messages that are absent from a particular flow.

The historical comparison also reports that changing `is_explorable` on a redirect changed the time
between loading steps on the tested client. Its cause was not established. Missing population is a
separate completeness gap; it is not a proven explanation for the delay. The capture paths, timing
and experimental limitations remain in the historical source. Current ASURA-parity status and Kryta
message sequences are owned by `docs/port/09-parity-audit.md` and `docs/port/02-wire.md`.

ASURA's former `docs/26-packet-gaps.md` (archived at
`docs/evidence/asura-history/docs/26-packet-gaps.md`) contains a dated frequency-ranked list of
live-client struct observations. It is a lead for examining recordings, not the current missing-packet
ledger and not a wire-size table. `docs/evidence/asura-history/docs/work/unnamed-packets.md` records 17
historical naming candidates, their observed
neighbors and possible client actions to investigate them. Confirm each against the current catalogue
and the evidence before treating its historical status as current.

## Recovery coverage

The historical source index in `docs/evidence/asura-history/README.md` lists the last recovered version
of each documentation path and its commit. Earlier versions remain in ASURA's Git history. The index
is a research aid; the active Kryta documents own current status and instructions. The design and
research above preserve major material identified so far. A claim-by-claim comparison of all historical
files and earlier revisions has not been completed. Which recovered document went where is
`docs/port/13-documentation-audit.md`.
