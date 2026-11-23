# World packs

A pack is what a world contains: its maps, where a character may stand in each, how they connect, and
what a character is created with. `Vanilla` is the faithful base. A second pack is a folder beside it,
and the setting that names the pack is what selects it - `WorldPack` in `worldserver.json`, a path,
with `config/World/Vanilla` the default.

| pack | is |
|---|---|
| `Vanilla/` | the base game's own values: the map catalogue, the doorway geometry, the creation screen's content. Every number in it is either the client's (it decides what a number means) or a measurement, and the document that records the source is `docs/` |

Nothing else lives at this level. A pack is a folder with a README and its own subfolders, and no file
here is shared between packs: a pack that inherits nothing is a pack whose numbers can be checked
against a capture without asking which layer overrode what.

The maps are here: `maps/<campaign>/<area>/<kind>/<map id>/` holds one folder per map (a map with no
campaign is `maps/unplaced/<map id>/`), and the world
reads that tree at start-up. What a world *is* is these files; a database row is for what a server
writes - an instance, a character, an item a character holds
(`docs/decisions/0014-the-world-content-is-a-pack.md`).
