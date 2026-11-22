# Vanilla

The faithful base pack: the client's own vocabulary and the values a faithful world runs on.

```text
Vanilla/
  titles.json         vanilla title IDs and editable custom titles
  party_sizes.json    how many players a party may hold, per map (ASURA's own file; every other map
                      takes the world's PartySizeCap)
  vanilla-title-wire.json   captured vanilla title packet records
  experience.json     the experience curve and the skill points a level grants (ASURA's own file;
                      game/Characters/Experience.rs)
  maps/               one folder per map, by campaign, area, kind and id:
                        <campaign>/<area>/<kind>/<map id>/basic.json    the map's own record
                        <campaign>/<area>/<kind>/<map id>/spawns.json   where a character may stand
                        <campaign>/<area>/<kind>/<map id>/portals.json  arches and arrival points
  creation/
    inventory.json    the bags and the items a creation screen is given, before a profession is chosen
    equipment.json    what a new character wears, one set per (campaign, profession)
    worn_slots.json   where a worn item sits: the cell inside the equipped bag, and its position in the
                      agent's nine-dword visual list
```

- `maps/` is the world itself: 265 map records, 138 entry lists and 17 doorway files, copied from
  ASURA's pack of the same name. The world reads the tree at start-up (`WorldPack`) and refuses a
  folder whose name is not the `map_id` its record states. A record that states a `map_type` and a
  `map_file_id` is a map this world can host; one that states only an `instance_value` is a destination
  the client can name and this world cannot load (`docs/decisions/0014-the-world-content-is-a-pack.md`).
- `creation/` is one subject: everything the character-creation screen is built from and everything a
  new character leaves with. Its three files are read together at start-up (`InventoryFile`,
  `EquipmentFile` and `EquippedSlotsFile` in `worldserver.json`), and the world logs what it read.
- `inventory.json` and `equipment.json` are imported, not written by hand: `tools/import-creation-inventory.py`
  regenerates them from ASURA's generated equipment table, which came from the reference's own
  `GetDefaultEquipments`. `worn_slots.json` is written by hand, because its order is a measurement and
  its two numberings must agree with a capture (`docs/game-rules.md` E234).
- A file that would describe something the world does not read yet is not here. `character_spawn.json`
  (the outpost each campaign's new character starts in, `docs/game-rules.md` E232) arrives with the
  stage that reads it, not before.

Every file carries a `note` and an `information` block saying what a row means and where the numbers
came from, so a reader does not have to open the code to know what they are looking at.

`titles.json` lists vanilla title IDs. Kryta reads their captured rank
references, thresholds, and stock text references from `vanilla-title-wire.json`.
Vanilla records without explicit points use one test point at rank zero.
Records with points and a current threshold replay a captured display state.
The `custom_tracks` list defines custom names, descriptions, metric keys,
short wire names, and thresholds. Hog Jumper uses ID 48 and Embiggener uses ID 49.
The test does not write character progress. The portal publishes only custom
text and its lookup keys, and the official client retrieves it at runtime.
Edit this file and restart Kryta to change the text without rebuilding the client.

To create another custom title, add a row under `custom_tracks`. Use a
unique ID of 48 or higher and two rank IDs that do not appear in
`vanilla-title-wire.json`'s `ranks` array or another custom row. Set `name` and `description`
to the text players should see, `short_name` and `next_short_name` to stock
wire fallbacks of at most four characters, and `metric` to the server metric
key. `points`, `current_minimum`, and `next_minimum` describe the display
test state. Set `selected` to 1 on at most one row for the initial
displayed title. Reload the character after restarting the stack.
Both the auth and world services validate the title files before listening.
If a title record is invalid, its startup error appears in `Auth.log` or
`Server.log`.
