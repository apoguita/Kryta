# 0016 - A server is a flavor, and a flavor is one folder

Status: accepted (owner, 2026-11-27). It extends `0014-the-world-content-is-a-pack.md`, whose
`config/World/<pack>/` becomes `config/flavors/<flavor>/World/`, and it will replace the
reference's "one configuration file beside each program" when its second stage lands.

## The question

Where does everything that makes one server what it is live, so that an operator can run a
different server by naming it, and share one by copying it?

Before this record the answer was split by kind: `config/World/<pack>/` for the world's content,
`config/Client/` for what a patched client's DLL is given, `config/Server/` for the command surface
and the first account, and each program's settings beside the program
(`authserver/authserver.json`, `worldserver/worldserver.json`). Only the world's content was under a
name, so "a different server" was not one thing to point at.

## The decision

A server runs one **flavor**: its build and setup. The owner's examples are "vanilla" and
"fantasy_modded". Everything a flavor defines is under one folder, and that folder is the unit an
operator configures, runs and distributes. In the owner's words: "every setting needs to be gated
under a flavor folder", and "the server configs can also be distributed".

```text
config/
  flavor.json              which flavor this machine runs
  machine.json             this machine only: never distributed
  flavors/
    Vanilla/
      authserver.json      the login program's settings for this flavor
      worldserver.json     the world program's settings for this flavor
      Server/              the command surface
      World/               maps, creation, titles: the world pack of 0014
      Client/              what a patched client's DLL is given
    <another flavor>/
```

- **A flavor's name is a plain name**: 1 to 64 ASCII letters, digits, `_` and `-`
  (`client_link::is_flavor_name`). The server tells its clients the name, and a client keeps that
  flavor's maps in a folder of that name (`docs/port/24-custom-maps.md`), so a name is a folder name
  on machines the operator does not control.
- **What only one machine knows stays outside the flavor**: database logins, the addresses and
  ports it binds, the first account, credentials. A flavor that is copied to another operator must
  not carry them.
- **The control panel configures all of it** (owner, 2026-11-27): which flavor is active, a flavor's
  settings, and the machine's.

## Why this leaves the reference

The reference keeps one `.conf` beside each program and has no notion of a named server setup;
its content is rows in a world database. This tree already left the rows for files (`0014`). A
flavor is the same step for the rest: the reference's arrangement cannot be copied to another
machine as one thing, and the owner's requirement is that it can.

## What is built, and what is not

| stage | what | state |
|---|---|---|
| 1 | The content is under the flavor: `config/flavors/Vanilla/World`, `.../Client`, `.../Server/commands.json` | built 2026-11-27; the owner's client run that day loaded a map from it |
| 2 | The programs' settings are in the flavor's folder; `config/flavor.json` names the active flavor; `config/machine.json` holds the machine's own values and is laid over the flavor's (`Config::load_json`, `Flavor`, `Config::flavor_path` in `common/src/Configuration/mod.rs`) | built 2026-11-27; the owner ran the stack, a client and the panel on it that day |
| 3 | The control panel chooses the flavor and edits a flavor's settings and the machine's | built 2026-11-27 (`tools/manage/src/areas/flavors.rs`, `settings.rs`): a Flavors page lists the flavors, names the one the machine runs and makes a new one as a copy; the Settings page says which file states each value and changes it there. Run against this machine's files through the API; not yet used by the owner in a browser. It does not rename or remove a flavor, and it does not move a value between a flavor and the machine |
| 4 | The supervisor starts the programs on the active flavor | built 2026-11-27: `tools/supervisor/stack.json` names no settings file for the two programs, so each reads `config/flavor.json` |

What is left - running a second flavor, whose the characters are, renaming and removing, packing a
flavor - is a later phase (owner, 2026-11-27): `docs/port/20-open-issues.md`, 37 to 40.

## How the settings are read

- A program started without `-c` reads `config/flavor.json` and then
  `config/flavors/<flavor>/<program>.json`.
- A settings file that is in a flavor's folder belongs to that flavor, whoever reads it and
  however it was named. The section of `config/machine.json` named as the program is laid over
  it, and a key stated there wins. The reference's environment override still wins over both.
- A path in a flavor's settings that names the flavor's own content is relative to the flavor's
  folder (`"WorldPack": "World"`), so a flavor that is copied or renamed reads its own files.
- A settings file anywhere else has no flavor and nothing laid over it.

`config/Server/admin.json`, the first account, is the machine's and stays where it was; the
machine's file names it (`AdminAccountFile`).

## What moved, for a machine that ran the earlier layout

`authserver/authserver.json` and `worldserver/worldserver.json`, with their `.dist` files, are gone.
What they held is now the flavor's two files plus `config/machine.json`: copy
`config/machine.json.dist` and put the database connection strings in the copy.
