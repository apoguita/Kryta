# Flavors

A flavor is one server's build and setup, and everything it defines is in one folder here
(`docs/decisions/0016-a-server-is-a-flavor.md`). `Vanilla` is the faithful base. A second flavor is
a folder beside it, and a flavor is the unit an operator runs and shares: copy the folder and the
other machine has that server.

| flavor | is |
|---|---|
| `Vanilla/` | the base game's own values: the map catalogue, the doorway geometry, the creation screen's content, the command surface, and what a patched client's DLL is given. Every number in it is either the client's (it decides what a number means) or a measurement, and the document that records the source is `docs/` |

A flavor's name is a plain one: 1 to 64 ASCII letters, digits, `_` and `-`. The server tells its
clients the name, and a client keeps that flavor's maps in a folder of that name
(`docs/port/24-custom-maps.md`), so a name that is not plain stops the login program at start-up.

What a flavor's folder holds:

| folder | holds | who reads it |
|---|---|---|
| `World/` | the world pack: `maps/<campaign>/<area>/<kind>/<map id>/` with one folder for each map (a map with no campaign is `maps/unplaced/<map id>/`), the creation screen's content, the title catalogue and the campaign start maps (`docs/decisions/0014-the-world-content-is-a-pack.md`) | `worldserver`, at start-up, through `WorldPack` in `worldserver.json`; `authserver` reads the pack's `titles.json` and map files through the same `WorldPack` (`ClientLink.TitleWorldConfig`); the panel (`tools/manage`) reads the pack named there |
| `Server/` | the chat command surface, `commands.json` (`docs/decisions/0011-the-command-surface-is-content.md`) | `worldserver`, at start-up, through `CommandsFile` |
| `Client/` | what `Kryta.dll` inside a patched client is given over the client link: the offset documents its features find the client's functions by | `authserver`, at start-up, through `ClientLink.Directory`; it checks each document and sends them to every client that connects |

Nothing is shared between flavors: a flavor that inherits nothing is one whose numbers can be
checked against a capture without asking which layer overrode what.

Beside those folders are the two programs' settings in this flavor, `authserver.json` and
`worldserver.json`. They hold everything about a program that is not one machine's own, and a path
in them that names this flavor's content is relative to the flavor's folder (`"WorldPack": "World"`),
so a copied flavor reads its own files. What one machine alone knows - database logins, addresses,
ports - is not in a flavor: it is `config/machine.json`, laid over the flavor's settings
(`config/README.md`).

To run another flavor: copy `Vanilla/` under a new name, edit the copy, name it in
`config/flavor.json`, and restart.
