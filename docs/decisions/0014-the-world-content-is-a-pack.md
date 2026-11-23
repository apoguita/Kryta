# 0014 - The world's content is a pack, not rows

Status: accepted, replacing `0009-world-schema.md`. The owner's direction: map content is JSON-driven,
in ASURA's `config/World/<pack>/` arrangement, and that premise is not to be re-argued.

## The question

Where does a world's content live - the map catalogue, where a character may stand on each map, and
the walls a step crosses? `0009` answered: rows. This record answers: a pack.

## The decision

A world's content is a folder of JSON files under `config/World/<pack>/`, and the world reads it at
start-up. A database row is for what a server *writes*: an open instance, a character, an item a
character holds, an unlock, a travel record. The line is the one `docs/port/04-configuration.md` draws,
and this record moves the map half of the content onto the file side of it.

```text
config/World/Vanilla/maps/<campaign>/<area>/<kind>/<map id>/
  basic.json     the map's record: id, kind, blob, name, instance-table value
  spawns.json    where a character may stand on entry
  portals.json   the arches a step crosses, and where an arrival lands
```

- The tree is ASURA's, copied from `ASURA/config/World/Vanilla/maps/` where the same 265 records, 466
  entry points, 28 arches and 28 arrivals live. The rows this tree served from were an import of those
  files (`data/sql/base/db_world/0011-map-catalogue.sql` says so in its own header), so the pack and
  the rows state the same numbers: a copy, not a derivation. One file has since diverged: map 148's
  `spawns.json` holds a third entry point here that ASURA's does not, so this pack has 467 entry points
  against ASURA's 466.
- The reader is `game/src/Maps/Pack.rs`, and it refuses what a client could not resolve: a folder whose
  name is not the `map_id` its record states, a pack with no `maps/` folder, a record with no `map_id`.
- A world logs which source answered - "read from the map pack at ..." - so a reader can tell the pack
  from the rows.
- The database tables `map`, `map_spawn`, `map_arch` and `map_arrival` remain in the schema, empty:
  `data/sql/base/db_world/0013-content-is-the-pack.sql` clears what earlier files seeded. **They are
  no longer what the world serves**, and the manager's content page reads the pack, not the tables
  (`tools/manage/src/pages/content.rs`).

## Why the earlier decision is replaced rather than amended

`0009` was made when the tree hosted one outpost and the pack did not exist here. Its own status line
said the owner may correct it and that a correction replaces the text; this is that correction, taken
on the owner's instruction that a world's content is configured as files, in ASURA's folder structure.

## What this does not decide

Item content beyond the creation screen, spawn tables for NPCs and monsters, and quest content are not
in the pack yet. They arrive with the stage that reads them, in the same tree, and not as rows.
