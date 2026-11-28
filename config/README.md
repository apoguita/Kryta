# What this folder is

The game's content and this machine's values, as files: a `flavors/<flavor>/` folder for everything
one server setup defines, and a `Server/` folder for what one machine chooses for itself, with a
README in each folder saying what belongs there. The rules are `AGENTS.md` rule 7,
`docs/port/04-configuration.md` and `docs/decisions/0016-a-server-is-a-flavor.md`: every value that
describes the world or a service is a file here, named by a setting that only says where the file
is.

| folder | holds | who reads it |
|---|---|---|
| `flavors/` | one folder for each flavor - a server's build and setup. `Vanilla` is the faithful base: its `World/` is the world pack (the map catalogue, entry lists, doorways, the creation screen's content, the title catalogue and the campaign start maps), its `Server/` is the chat command surface, and its `Client/` is what `Kryta.dll` inside a patched client is given (`flavors/README.md`) | each program reads its own settings file in the flavor (`worldserver.json`, `authserver.json`); `worldserver` then reads the pack and the commands through `WorldPack` and `CommandsFile`; `authserver`, the client files through `ClientLink.Directory` and, for the pack's `titles.json` and map files, `ClientLink.TitleWorldConfig`; the panel (`tools/manage`) reads the pack named in the flavor's `worldserver.json` |
| `Server/` | what a machine chooses for itself and does not share: the first game account | `authserver` (`admin.json`, through `AdminAccountFile`), at start-up |

**Every start reads these files, and nothing seeds a database.** Edit a file, restart the program, and
the change is live - maps, spawns, items, every feature - with no database step. A database row is for
what a server writes while it runs; it is never where content comes from, and `data/sql` is schema only
(a schema file that writes a row fails `worldserver/tests/schema_has_no_content.rs`).
`docs/port/04-configuration.md` states the rule, and the tests named there enforce it.

Two consequences, and they are the point of the split:

- **A flavor is a folder, not a code change.** A second flavor is `flavors/<name>/` beside `Vanilla`,
  and the settings that name its folders are what select it.
- **A file that has no reader does not exist here.** A setting arrives with the stage that reads it,
  so this folder holds only what the programs actually load today.

Three files at this level are not content:

| file | is |
|---|---|
| `flavor.json` | the flavor this machine runs: the name of a folder under `flavors/`. A program started without a settings file reads its own from that folder |
| `machine.json` | what one machine alone knows, for every flavor it runs: database logins, the addresses and ports the programs bind, the paths of the machine's own files. One section for each program; a key stated there wins over the flavor's. Never committed and never part of a flavor |
| `machine.json.dist` | the template of `machine.json`: copy it, and put this machine's values in the copy |

`data/` is not this folder, and the difference is where the content came from. `data/sql/` is the
schema only; `data/path-graphs.bin` is extracted from the client's archive; `data/realms.json` is the
login's world list, merged over the `realmlist` table (`docs/port/04-configuration.md`). Everything in `config/` is a value this tree states,
readable and editable by hand.

Evidence and acceptance are not here. A file answers *what the value is*; the document that records
where it came from is `docs/`, and it names the recording or the source beside the value.
