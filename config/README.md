# What this folder is

The game's content and this machine's values, as files: a `Server/` folder for what one machine
chooses, a `World/<pack>/` folder for what a world contains, and a `Client/` folder for what a
patched client's DLL is given, with a README in each folder saying what belongs there. The rule is `AGENTS.md` rule 7 and `docs/port/04-configuration.md`: a program's
settings file is its own (`worldserver/worldserver.json`), and every other value that describes the
world or a service is a file here, named by a setting that only says where the file is.

| folder | holds | who reads it |
|---|---|---|
| `Server/` | what a machine chooses for itself and what its services answer - the chat command surface, the first game account, and later whatever else belongs to the server rather than to the world | `worldserver` (`commands.json`, through `CommandsFile`) and `authserver` (`admin.json`, through `AdminAccountFile`), at start-up |
| `Client/` | what `Kryta.dll` inside a patched client is given over the client link: the offset documents its features find the client's functions by | `authserver`, at start-up, through `ClientLink.Directory` in `authserver.json`; it checks each document and sends them to every client that connects |
| `World/` | the packs: the map catalogue, entry lists, doorways, the creation screen's content, the title catalogue and the campaign start maps. `Vanilla` is the faithful base - the client's own vocabulary and the values a faithful world runs on | `worldserver`, at start-up, through `WorldPack` in `worldserver.json`; `authserver` reads the pack's `titles.json` through the same `WorldPack` (`ClientLink.TitleWorldConfig`); the panel (`tools/manage`) reads the pack named there, including `character_spawn.json` |

**Every start reads these files, and nothing seeds a database.** Edit a file, restart the program, and
the change is live - maps, spawns, items, every feature - with no database step. A database row is for
what a server writes while it runs; it is never where content comes from, and `data/sql` is schema only
(a schema file that writes a row fails `worldserver/tests/schema_has_no_content.rs`).
`docs/port/04-configuration.md` states the rule, and the tests named there enforce it.

Two consequences, and they are the point of the split:

- **A pack is a folder, not a code change.** A second world pack is `World/<name>/` beside `Vanilla`,
  and the setting that names the pack is what selects it.
- **A file that has no reader does not exist here.** A setting arrives with the stage that reads it,
  so this folder holds only what the programs actually load today.

`data/` is not this folder, and the difference is where the content came from. `data/sql/` is the
schema only; `data/path-graphs.bin` is extracted from the client's archive; `data/realms.json` is the
login's world list, merged over the `realmlist` table (`docs/port/04-configuration.md`). Everything in `config/` is a value this tree states,
readable and editable by hand.

Evidence and acceptance are not here. A file answers *what the value is*; the document that records
where it came from is `docs/`, and it names the recording or the source beside the value.
