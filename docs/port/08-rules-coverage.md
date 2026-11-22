# Rule coverage: where each of the owner's rules stands in this tree

The owner's ledger (`docs/game-rules.md`) is what this port is measured against: 67 asserted rules. This
file answers, for each one, one question - what is there today? It is a statement of today, and it is
updated in the same commit as the change that moves a rule.

The states are:

- **in** - implemented and proven (the test or the live run is named).
- **partial** - some behavior exists, but the whole rule or its acceptance is outstanding.
- **data** - the rows or the setting exist, and the behaviour that consumes them waits on a capture or
  on a later step.
- **blocked** - the next piece needs a measurement this tree does not have; `docs/port/06-world-plan.md`
  says which piece and why it is not invented.
- **not started** - nothing exists.

## Districts and instances

| rule | state | where, and what proves it |
|---|---|---|
| R1 a map is a definition, an instance is a running copy | in | `game/Districts/`: `Districts::enter` opens and hands out a copy; `game/Maps/` holds the definition |
| R2 outposts are shared copies | in | the same copy is handed out while it has room (`a_filled_district_is_left_for_the_first_with_room`) |
| R3 explorables are private per party | in | an explorable entry opens a fresh copy and the party's members are sent into that same copy (`World::queue_travel`, `World::hold_party`); `an_explorable_gets_a_copy_of_its_own` |
| R4 an outpost has numbered districts | in | copies are numbered and the load reports the number in `0x019A` |
| R5 a new district opens at the allotment | in | `World.DistrictAllotment` (100) and the first-with-room rule; `a_filled_district_is_left_for_the_first_with_room` |
| R16 one district is enough to begin | in | the first entry opens district 1 and the login answers the district list with exactly one (`authserver/tests/end_to_end.rs` checks the 29-byte answer at the program level: the transaction echoed, eleven language pairs, language 0 with one district) |
| R22 default assignment takes the first district with room | in | `Districts::enter` with no named district |
| R57 a named district is honoured | in | `enter` with a hint opens or joins that number; `a_named_district_is_honoured_even_before_it_exists` |
| R58 a full district cannot be entered | data | `enter` refuses and the connection closes; the refusal's own wire is not captured |
| R37/R38/R39/R40 teardown and the disconnect hold | partial | an empty copy is held 900 s and disposed by the world's tick; a socket that ends is a disconnect (`a_copy_is_held_for_the_hold_and_then_disposed`). The reconnection itself is not built |
| R32 an instance lives while a party is inside | partial | empty copies are disposed by the sweep; full party and re-entry behavior still needs implementation and verification |
| R33 ticking is configurable | partial | `World.InstanceTick` controls the world's instance sweep; `World.SimulationTickMs` sets the session tick interval (100 ms in the distributed configuration). The session loop uses its own interval, so this setting does not establish that all instance simulation can be disabled. `World.SweepIntervalMs` sets the sweep interval |
| R20/R12 world-map travel reaches unlocked outposts; the walk sets the unlock | partial | `character_unlock` rows with the starting unlock written at the confirm, and the map a character **arrives in** unlocked when its players ask is answered (`ASURA/crates/asura-world/src/player.rs:442-471`: "the walk sets the bit, the bit never gates the walk", which is R12's own "a scripted/forced travel unlocks at arrival"). The server never refuses a load for an unlock - it did until 2026-11-20, and that refusal is what a doorway crossing met as the client's `007`. Proved by `a_step_through_ascalon_citys_arch_crosses_to_lakeside_county` (a walk through 148's arch redirects to 146, the arriving connection stands on map 146's arrival and unlocks it) and by `an_arrival_unlocks_the_map_it_reaches` (a load of a map outside the set proceeds and writes the bit). R20's own travel list still needs the answer's wire |
| R21 a portal crossing lands on the destination's arrival point | in | `map_arrival` is read into the world; `a_step_through_ascalon_citys_arch_crosses_to_lakeside_county` exercises the crossing and destination arrival |
| R13 entry takes a random spawn point, scattered | in | `WorldSession::entry_point` and the load's `0x0196`; `An outpost connection is loaded with its entry point` |
| R14 a load places the character at its last outpost | partial | an **outpost** writes `last_outpost` (an explorable must not overwrite it, ASURA `player.rs:447`) and the login's list reports it; a map that is not an outpost resumes the saved position instead, written when the connection ends and cleared by a travel that is not a doorway crossing (`ASURA` `character.rs:213-294`, `agent.rs:531-545`) - proved by `a_reconnect_to_an_explorable_resumes_where_the_character_left` |

## The travel unit: party, heroes, henchmen

| rule | state | where, and what proves it |
|---|---|---|
| R6, R7, R8, R9, R10 the party travels as one, the leader drives it | in | a leader's world-map travel holds the roster, sends the lock and the countdown, and moves every present member (`TravelHandler::answer_party_travel`); a non-leader is asked `0x01E7 {4}` and travels alone; a doorway crossing sends every present member the same redirect (`World::queue_travel`). The countdown window itself still needs a client run |
| R17, R18, R19 party persistence and leadership | partial | the roster crosses a map in memory (`World::hold_party`, `Table::adopt`) and a merge picks the leader by the rule in `Parties/Rules.rs`; there is no database ledger, so a restart loses the party, and a smaller destination cap does not disband it. A disconnected member keeps a shadow row, but heroes do not exist |
| R23, R24, R25, R27, R28 heroes and henchmen | not started | `game/Heroes/` is a stub |
| R15 the party cap is not a constant | in | `World.PartySizeCap` (8), a map's own `party_sizes.json` row, and `rules::may_fill` on every invite and accept |
| R26 an arrival at a smaller cap truncates the party | partial | `rules::travelling_count` states and tests the rule; the travel handler leaves the tail in the old party instead of splitting it into parties of one |

## Character creation

The owner's description of the live creation screens is `docs/game-rules.md` R59-R67, with its own
caveat that the screens are the client's and may send only once. `docs/port/02-wire.md`, "The creation
screen's conversation", is the frames.

| rule | state | where, and what proves it |
|---|---|---|
| R59 a character is created from an empty slot | in | the confirm writes the character against the account, and the list the slot belongs to is answered at login (`a_creation_confirm_writes_a_character_row`); the slot screen is the client's |
| R60 role-playing or PvP is the first choice | partial | `characters.is_pvp` is written as the campaign byte being 0, which is E225's own split (a PvP create's `0x60` chapter byte is 0 and its three flags are absent); nothing else branches on it - no level 20, no Great Temple of Balthazar start (E225) |
| R61 a role-playing character chooses a campaign | in | `0x8060`'s first byte is read into the creation state and written to `characters.campaign` |
| R62 the campaign decides the profession list | data | `config/World/Vanilla/creation/equipment.json` holds exactly the lists R62 names - campaign 1 (Prophecies) with professions 1-6, 2 (Factions) with 1-8, 3 (Nightfall) with 1-6, 9 and 10 (Paragon and Dervish), and the PvP campaign 0 with 1-10 - imported from the reference's `GetDefaultEquipments` (`tools/import-creation-inventory.py`); the list the screen draws is the client's own table, and its rule is "profession chapter 0 or the chosen chapter" (`docs/game-rules.md` E221) |
| R63 the screens run in one order | not started | the server sees `0x8089`, `0x8060`, seven `0x8084` and `0x808B` and nothing else; the order is the client's, so nothing here enforces or observes it |
| R64 two appearance screens, five armour colours and height | partial | the packed appearance word is read and stored whole (`sex`, `height`, `skin`, `hair colour`, `face`, `profession`, `campaign` - the height a signed -8..7, E228); **the five armour colours are lost**: the client sends them as seven `0x8084` after every profession preview (E226-E227) and this tree does not read that message, so the armour keeps the colours the equipment data names |
| R65 the name is unique, 19/3, letters, first name and surname | partial | a taken name is refused and no row is written (the `characters.name` unique index, `game/Handlers/CharacterHandler.rs:30-37,302-310`), but the code sent is `0x0106` (262), which is in no reading of the client's own error table, so the client shows its generic network error (E230); the length, alphabet and capitalisation rule is the client's (`E229`) and the server does not re-check it, and no source states that a surname is required on the wire |
| R66 three flags beside the name | not started | the bits are known - Reforged `0x1`, Dhuum `0x2`, Melandru `0x4` in the confirm's last dword (E222-E223) - and no column stores them and nothing reads the dword |
| R67 a cinematic, then the starter map its campaign history dictates | not started | the confirm answers `0x0188` and then `0x0099`, and that map is `World.StartingOutpost`, or the lowest-id outpost the world hosts when the row is absent (`WorldSession::starting_outpost`) - the same map for every campaign. The per-campaign table is evidenced and absent here: 148 / 212 / 449 (or 490, contested) / 642 (E232) |

## Characters, skills, movement

| rule | state | where, and what proves it |
|---|---|---|
| R29, R30, R31 what is in a map before anyone arrives | blocked | live initialization captures contain agent-creation observations (`docs/port/12-research-and-control-panel.md`); the map NPC population and its spawn rows are not implemented |
| R34 the compass range gates spawns and ticks | blocked | needs agents; the range is a setting (`World.CompassRange` 10,000) |
| R43 aggro is a bubble | not started | needs agents |
| R44 death and the death penalty | not started | `game/Combat/` is a stub |
| R46 pre-Searing level convention | not started | no levelling beyond the row's `level` 1 |
| R48 level 20 and experience | not started | `characters.level`/`experience` columns exist; nothing changes them |
| R50 skills: quests, trainers, capture | not started | `game/Skills/` is a stub |
| R51 attribute points from level-ups | not started | |
| R55 a quest affects only the log | not started | `game/Quests/` is a stub |
| R45 gold is per character; the bank holds gold and materials | data | `characters.gold` is written 0 at creation and drawn into the load's `0x0141` from the row (`a_travel_record_decides_the_map_and_the_district`, gold 1234); nothing changes it yet, and no bank rows exist |
| R49 Xunlai storage is per account | not started | |

## Items and commerce

| rule | state | where, and what proves it |
|---|---|---|
| R45 items a character holds are the character's | in | `item_instance` rows written at the confirm, drawn by the load (`a_creation_confirm_writes_a_character_row`) |
| moving, wearing and destroying an item | in | `0x8072`/`0x8030`/`0x8069` move or delete the row and answer with the change (`a_travel_record_decides_the_map_and_the_district`); the slot a kind is worn in is content (`config/World/Vanilla/creation/worn_slots.json`, E234), not a compiled table |
| item definitions are world data | in | `item_template` seeded at start-up and read back (`definitions_are_seeded_once_and_read_back`) |
| R54 merchant NPCs hold a fixed list | not started | `game/Trader/` is a stub |

## Missions, quests, chat, guilds, titles

| rule | state | where, and what proves it |
|---|---|---|
| R52 missions are instances with success/failure | not started | |
| R56 a mission outpost presents the Enter Mission button | data | `World.MissionButton` setting; the button is the client's |
| R55 quests | not started | |
| the chat and social wire | the channel rule and the commands in; guilds, friends and alliance not started | `game/Chat/` holds the reference's three commands and `/maps`' table, and the connection answers `0x8064` with ASURA's pair (`0x005D`, `0x0061`) - `ASURA/crates/asura-world/src/chat.rs`, and `worldserver/tests/end_to_end.rs` speaks a line and types the commands against the program; `game/Guilds/` is a stub |
| guilds, friends, alliance | not started | `game/Guilds/` is a stub |
| titles and character slots | partial - the test display sends 33 captured rank-zero rows and 12 captured earned states. Metrics report zero. Character slots have not started | `titles.json` lists the vanilla IDs and custom definitions. `vanilla-title-wire.json` holds captured metadata for the stock packets. The patched client fetches the custom text from Kryta. [Title research](19-title-research.md) |

## Every number

| rule | state | where, and what proves it |
|---|---|---|
| R53 every number and threshold stays configurable, with the game's value as the default | partial | `game/World/WorldConfig.rs` reads the implemented world settings from `worldserver.json`; this does not establish coverage for every future mechanic |
