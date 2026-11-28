# This machine's values

What one machine chooses for itself and does not share, as opposed to what a flavor defines
(`config/flavors/README.md`). Every file here is named by a setting in `config/machine.json`, section
`authserver`; the setting says only where the file is, and the file holds the values.

| file | concern | read by |
|---|---|---|
| `admin.json` | the first game account: the login name and the portal token. Not the administrator identity of `docs/decisions/0013-the-admin-identity-is-a-file.md`, which is a file of its own and is not built | `authserver` at every start, `AdminAccountFile` (default `config/Server/admin.json`); it writes an `account` row only when the account table is empty (`authserver/src/main.rs`) |

The chat command surface, `commands.json`, was here until 2026-11-27. It is what a server setup
defines, so it is in the flavor: `config/flavors/Vanilla/Server/commands.json`
(`docs/decisions/0016-a-server-is-a-flavor.md`).

A file arrives here with the stage that reads it. There is no placeholder for a value nobody reads:
an unread file is a claim without a reader, and the program says what it is missing instead.

What is *not* here: a program's own settings. `worldserver.json` and `authserver.json` are in the
flavor's folder, and what one machine alone knows of them is `config/machine.json`
(`config/README.md`).
