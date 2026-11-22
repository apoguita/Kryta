# Server values

What the server chooses and answers, as opposed to what a world contains. Every file here is named by
a setting in `worldserver/worldserver.json` or `authserver/authserver.json`; the setting says only
where the file is, and the file holds the values.

| file | concern | read by |
|---|---|---|
| `guilds.json` | the guild settings: a new guild's `0x0121` bytes, allegiance, rating and rank, the `0x0133` counter's start and step, and the creation and invitation prices, which are placeholders (read and logged, never charged) | `worldserver` at start-up, `GuildsFile`; copied from ASURA's file of the same name (`game/Guilds/mod.rs`) |
| `commands.json` | the chat command surface: the names a player may type, what each takes, and which registered operation it runs | `worldserver` at start-up, `CommandsFile`; a name granted with no operation behind it is refused then, not when a player types it (`docs/decisions/0011-the-command-surface-is-content.md`) |

A file arrives here with the stage that reads it. There is no placeholder for a value nobody reads:
an unread file is a claim without a reader, and the program says what it is missing instead.

What is *not* here: a program's own settings. `worldserver.json` and `authserver.json` live beside
their programs, because they are the one file each program reads and the file's own existence is not a
setting.
