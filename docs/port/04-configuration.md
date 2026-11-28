# Configuration: the JSON policy

## The policy

This is a plain-language summary of the owner's intent, written after he explained it on 2026-11-23,
and he accepted the wording. His explanation was not exhaustively detailed, so the summary is
incomplete where he has not spoken, and he corrects it where it is wrong.

**This server hosts worlds, and the people who run one define it in plain JSON files.** A person
configures a server by editing files, and shares a setup by handing those files to someone else.

1. **A world's starting data and behaviour are JSON files.** Maps, spawn points, doorways, starting
   items, titles, commands and the game's numbers are files a person can open, read and edit.
2. **To change something, edit the file and restart the server.** That is the whole procedure.
3. **Anything that could be tweaked or configured is in a JSON file.** A value in the code can only be
   changed by compiling the server again; a value in a file is changed by editing it. So every value a
   person could want to change is in a file.
4. **The code keeps only what the game client dictates.** Packet sizes, message layouts and the file
   check's bytes are fixed by the client: changing them breaks it. They stay in the code, as named
   constant tables that cite the recording they came from.
5. **The database holds what happens while the server runs.** Accounts, characters, the items they
   carry, open instances. The SQL files only create the tables. A setup is shared as JSON files, with
   no SQL files to hand around.
6. **The server reads its files once, when it starts, and stops if one is missing or broken.** It names
   the file and the reason.

The rest of this page says how a file is written, which files exist, and where each setting is.
`config/README.md` describes the `config/` folders.

```text
config/flavor.json               which flavor this machine runs
config/machine.json              what this machine alone knows; laid over the flavor's settings
config/Server/                   what a machine chooses for itself (admin.json today)
config/flavors/<flavor>/         everything one server setup defines; Vanilla is the faithful base
  worldserver.json               the game program's settings
  authserver.json                the login program's settings
  World/                         what a world contains: the world pack
  Server/                        the chat command surface (commands.json)
  Client/                        what a patched client's DLL is given (offsets/titles.json, files.json and maps.json today)
data/sql/, data/path-graphs.bin  the archive's and the database's: schema (no rows), extracted graphs
```

`worldserver` reads `Console.Enable` (default `true`): when enabled and standard input is an
interactive terminal, it starts the embedded console. Supervised non-interactive input is not consumed.
`Console.QueueCapacity` (default `64`) limits pending console requests and must be greater than zero.

`worldserver` reads `Control.Enable` (default `false`), `Control.Port` (default `7878`, always bound to
`127.0.0.1`), `Control.CredentialFile` (default empty) and `Control.TimeoutSeconds` (default `5`, must be
positive): the local control endpoint a separate manager or CLI asks for world operations. It refuses to
start when enabled without a credential file or with a non-positive timeout. `tools/manage` reaches it
through its own `ControlPort`, `ControlCredentialFile` and `ControlTimeoutSeconds`; the credential file is
created on this machine with `cargo run -p worldserver --bin kryta-cli -- init-credential <file>`, which
also restricts it to the service user. Neither program logs the credential.

`tools/manage` reads `SupervisorStackFile` (default `tools/supervisor/stack.json`) and
`SupervisorTimeoutSeconds` (default `20`). The former is the supervisor's existing configuration file:
the manager reads only its `control_port`, then asks the running supervisor for `status`, `stop <service>`
or `restart <service>`. It does not copy service rows, start a missing supervisor, or own a process.

`worldserver` reads `CaptureFile` (default empty, which is off): where to record the wire, every game
message both directions as plaintext JSONL with two-byte headers (`game/Server/Capture.rs`). It is a
setting rather than a command-line flag because a diagnostic run should be able to name its file in the
same place as everything else it runs with; the file is evidence and unsanitised, so it belongs under
`recordings/` or a scratch path, and its reader takes the layer from the file's own header
(`tools/capture/read.py`).

Settings go in a JSON file: one per program, read by one handler. This file says how one is written and
which files there are. `docs/decisions/0005-json-configuration.md` records why the format is JSON rather
than the reference's `Key = value`.

## Writing a file

A configuration file is a JSON object. A nested object is a **section**, and its keys are the
reference's dotted names; a leaf is a scalar, an array of scalars, or an object holding a `value` or
a `hex`.

```json
{
  "note": "what this whole file is for",

  "information": {
    "what": "this client asks which game files it should have before anything else",
    "measured": "a live client on 2026-10-26, build 39112",
    "how to replace the bytes": "capture the reply a client accepts, never compute one"
  },

  "RealmServerPort": 6112,

  "StatusInterval": {
    "value": 5,
    "doc": "how often the program writes its own status line, in seconds"
  },

  "Network": { "Trace": true, "Threads": 1 },

  "Appender": { "Console": [1, 5, 0, "1 9 3 6 5 8"] },

  "LoginDatabaseInfo": "127.0.0.1;5432;kryta;change-me;kryta_login"
}
```

That file's keys are `RealmServerPort`, `StatusInterval`, `Network.Trace`, `Network.Threads`,
`Appender.Console` and `LoginDatabaseInfo`.

| written as | the key's text |
|---|---|
| `"BindIP": "0.0.0.0"` | `0.0.0.0` |
| `"RealmServerPort": 6112` | `6112` |
| `"Trace": true` | `1`, and `false` is `0` - the reference reads a flag as text and compares it with `1` |
| `[1, 5, 0, "1 9 3 6 5 8"]` | `1,5,0,1 9 3 6 5 8` |
| `{"hex": "f1 02 20 00"}` | the bytes, decoded by the one decoder there is |
| `null` | the key is absent - how a file says "not set" |
| `{"value": 0, "immutable": true, "doc": "..."}` | `0`, described |

Four names are read past rather than configured, at every level of a file - the root, a section, a
leaf, a row - so no configuration key here is called one of them:

| name | is |
|---|---|
| `doc` | where a value came from, in one line |
| `note` | what a whole file is for, in one line |
| `immutable` | a number the *client* decides rather than the server: editing it does not change the client. One without a `doc` is reported, because a label with no reason is a rumour |
| `information` | commentary of any shape - an object, an array, nested trees, prose - for the explanation that will not fit in one line. Its contents are never keys and are never read |

`information` is where a file writes down what it is for, how it was measured, what would replace it
and what it links to. `data/realms.json` is the worked example: a note, an `information` block saying
how the file is read and what is deliberately not in it, then the rows themselves.

Two more rules, both enforced with a message rather than silence:

- A key stated twice - which needs two spellings of one path - is reported and the first wins.
- A value that cannot be a key (`{"value": {"nested": 1}}`, an array of objects) is reported and
  skipped. A file that states keys and has every one refused is refused as a whole, with the
  reasons, rather than being called empty.

Precedence is unchanged from the reference: an environment variable `AC_<KEY IN UPPER SNAKE>` wins
over the file, and the file wins over the default the program passes. The default in the code is the
reference's `GetOption` signature and is never the effective value when a shipped file states the
key.

## The files

Configuration, and nothing else:

| file | is | read by |
|---|---|---|
| `config/flavor.json` | the flavor this machine runs: the name of a folder under `config/flavors/` | `authserver`, `worldserver` and `kryta-cli`, when started without `-c` |
| `config/flavors/Vanilla/authserver.json` | the login program's settings in the Vanilla flavor: everything about it that is not one machine's own | `authserver`; found through `config/flavor.json`, or named with `-c` |
| `config/flavors/Vanilla/worldserver.json` | the game program's settings in the Vanilla flavor, the same way | `worldserver` and `kryta-cli`; found through `config/flavor.json`, or named with `-c`. `authserver` reads its `WorldPack` (`ClientLink.TitleWorldConfig`), and the panel reads it (`WorldConfigFile`) |
| `config/machine.json` | what one machine alone knows, for every flavor it runs: database logins, the addresses and ports the programs bind, the world's id, the control endpoint, the paths of the machine's own files. One section for each program, named as its settings file is; a key stated there wins over the flavor's | every reader of a settings file that is in a flavor's folder (`Config::load_json`), after the flavor's file |
| `data/realms.json` | the worlds a login may send a player to, merged over the `realmlist` table | `authserver`, `WorldsFile` |
| `tools/manage/manage.json` | the Configuration Manager's own configuration: bind, port, token, database strings, world-control credential location, supervisor stack-file location, and `ConfigFolder`, the folder whose `flavor.json`, `machine.json` and `flavors/` its Flavors and Settings pages read and write | `kryta-manage` and `kryta-management-service`, `-c` |
| `tools/supervisor/stack.json` | the services this machine runs - each one's program, arguments and port - the terminal's owner (`console_service`) and the supervisor's `control_port` | `kryta-supervisor`, `-c`, defaulting to this path; `tools/manage` reads its `control_port` through `SupervisorStackFile` |

A flavor's files are committed and shared: they hold no secret
(`docs/decisions/0016-a-server-is-a-flavor.md`). `config/machine.json` and `tools/manage/manage.json`
are not committed, because they hold a database password; their `.dist` files are: copy the `.dist`,
edit it, keep it. `data/realms.json` and `tools/supervisor/stack.json` hold no secret and are
committed as they are.

**How a program's settings are read.** Started without `-c`, a program reads `config/flavor.json`
for the flavor's name and then its own file in that flavor's folder. Whenever the file it reads is in
a flavor's folder (`config/flavors/<flavor>/<program>.json`), the section of `config/machine.json`
named as the program is (`authserver`, `worldserver`) is laid over it, key by key, and the machine's
value wins; the environment variable of a key still wins over both. So any setting can be pinned for
one machine without touching the flavor. A settings file anywhere else is read as it is, with no
flavor and nothing laid over it. In the table below, `authserver.json` and `worldserver.json` mean a
program's settings read this way; the shipped split is that the listener addresses and ports, the
world's id and public address, the database connection strings, the control endpoint, `PidFile`,
`LogsDir`, `WorldsFile`, `CaptureFile` and `PathGraphs` are in `config/machine.json.dist`, and every
other key is in the flavor.

**A flavor's own paths are relative to its folder.** `WorldPack`, `InventoryFile`, `EquipmentFile`,
`EquippedSlotsFile`, `CommandsFile`, `ClientLink.Directory` and `ClientLink.TitleWorldConfig` name
the flavor's own content, so a flavor that is copied or renamed still reads its own files
(`Config::flavor_path`). A machine's paths (`PathGraphs`, `WorldsFile`, `LogsDir`) are relative to
where the program is started, as before.

**A setting is in a file only once the program reads it.** The file states what the ported stages use
and nothing else, so no key sits there configuring nothing: the thread pool, SOAP and the world's own
data directory arrive with the stages that read them. Both files are checked against the code that reads them - a run with every key missing from them
warns, so a pruned file that is too small fails loudly rather than silently.

The file check's bytes, the account settings block and the accounts are not settings and are not in this
list: the first two are constants in `authserver/src/Server/file_check.rs` and `account_settings.rs`, and
the accounts are rows in `kryta_login`'s `account` table. `Config` - the type a program's file is read
into - has no method that answers bytes, so a payload cannot be read through the configuration reader at
all; bytes are decoded by `common::utilities::hex_bytes`.

A file that holds **rows** is a record list and is not a set of keys: it is read as rows through the same
reader (`Document::records`), so a single file can carry both a `note` and a list without either being
squeezed into the other's shape. The command rows, the creation files and the title catalog are read
this way. `data/realms.json` is the one that is also merged over a table: a world is a setting, and it
is also a row in the `realmlist` table - same id overrides, new id is added, and a field the file does
not state keeps the table's value.

## Every setting is in a file

| setting | is set in |
|---|---|
| the login listener: port, bind address, thread count, trace | `authserver.json` |
| the portal: enabled, where it answers | `authserver.json` |
| the client link: enabled (`ClientLink.Enable`), where it answers (`ClientLink.Bind`, `127.0.0.1:6650`), the folder of what a client's DLL is given (`ClientLink.Directory`), and the world configuration whose pack supplies the custom title text it is sent (`ClientLink.TitleWorldConfig`), and a wait before each part of a map file it sends, 0 in use and above 0 only to try a slow link (`ClientLink.MapPartPauseMs`) | `authserver.json` |
| where each place the client's DLL hooks is found in the client: byte patterns and the steps from a match to an address, in Py4GW Reforged's offset format. Every `*.json` in the folder is one document, sent to each client's DLL over the link; a new client build is handled by editing these files (`docs/port/22-client-platform.md`) | `config/flavors/Vanilla/Client/offsets/titles.json` (custom titles), `config/flavors/Vanilla/Client/offsets/files.json` (the client's file functions, for the maps the server hosts) `config/flavors/Vanilla/Client/offsets/maps.json` (the client's lookup of a map's row, for the rows the pack states, and the two lists a map id past the client's own is left out of) and `config/flavors/Vanilla/Client/offsets/text.json` (the function a coded string is handed to for decoding, for a map's name and description as text of the pack's own), under `ClientLink.Directory`. A missing folder or a file that breaks the format stops the login program |
| the login database: connection string, worker and synchronous connections, trace | `authserver.json` |
| the character database: connection string, worker and synchronous connections | `authserver.json`, `worldserver.json`, under `CharacterDatabaseInfo` and `CharacterDatabase` |
| the world listener: bind address (`BindIP`), port (`WorldServerPort`, 6113 in the shipped file), thread count and byte trace (`Network.Threads`, `Network.Trace`), and the world id it serves as (`RealmID`) | `worldserver.json`, top level |
| the address a travelling client is told to reconnect to (`PublicAddress`, `127.0.0.1:6112` - the login port, which the front door copies to the world) | `worldserver.json`, the `World` section |
| the world database: connection string, worker and synchronous connections | `worldserver.json`, under `WorldDatabaseInfo` and `WorldDatabase` |
| the login database, in the world program: connection string, worker and synchronous connections | `worldserver.json`, under `LoginDatabaseInfo` and `LoginDatabase` - the world reads it for the travel record its own program cannot be told by another program |
| the world list: every field of every world, including a world the table does not hold | `data/realms.json`, merged over `realmlist` |
| the intervals this program reports and refreshes at | `authserver.json` |
| which bag the login's character list draws each character's armour from (`EquippedBag`, 5): the id the world's creation inventory gives the equipped bag (type 2, model 21 in `config/flavors/Vanilla/World/creation/inventory.json`). The two must agree, and nothing checks that they do: a different number lists every character with no armour | `authserver.json`, read by `authserver/src/main.rs` and handed to `game/src/Characters/Repository.rs` (`with_equipped_bag`) |
| where a travel request is placed: the map id that is the creation instance (`CreationMap`, 0), and the outpost a character with no map anywhere enters (`FallbackOutpost`, 148) | `authserver.json`, read by `game/src/Maps/Placement.rs` through the login session |
| the world loop's floor: how long may pass before the loop does not bother to update (`MinWorldUpdateTime`, 1 ms - the reference's own key of the same name in `worldserver.conf`, its `WorldUpdateLoop` sleeping the remainder at `Main.cpp:586-616`) | `worldserver.json`, top level |
| the world's numbers, every one a game rule with a default (`docs/game-rules.md`): a district's allotment (R5), a party's cap (R15), the spawn scatter (R13), whether instances tick (R33), the simulation tick's interval (R33, 100 ms by default, ASURA's own `intervals.json`), how often the instance table is swept (R33/R40, 1 s by default), the compass range (R34), the disconnect hold (R37), the mission button (R56), the number a ping reply carries (20, the value GWPS's own trace carries) | `worldserver.json`, the `World` section |
| the agent a load creates: its health (`AgentHealth`, 400), energy (`AgentEnergy`, 25), energy regeneration (`AgentEnergyRegen`, 0.0) and walking speed (`AgentSpeed`, 288) - and the appearance a character with no bits of its own is drawn with (`Appearance`, `0x03100000`: profession 1, hair style 3) | `worldserver.json`, the `World` section |
| how far a client may place itself when it cancels a walk and still be believed (`MaxAllowedCorrection`, 250 units - GWPS's `MAXIMUM_ALLOWED_CORRECTION`, through ASURA's `config/Server/movement.json`) | `worldserver.json`, the `World` section |
| what a redirect's `is_explorable` carries (`TravelExplorable`: `0` for every travel by default, `1` for every travel, or `auto` for the destination's own kind); ASURA reads the same three words from `ASURA_TRAVEL_EXPLORABLE` and defaults to `0` because that is the value an explorable's load completes at (`ASURA/crates/asura-world/src/travel.rs:21-69`) | `worldserver.json`, the `World` section |
| the outpost a fresh character starts in and has unlocked (`StartingOutpost`, 148 - the one unlock of R12, and the same fact the login's `FallbackOutpost` names) | `worldserver.json`, the `World` section |
| half the side of a point doorway's box (`PointBoxHalfExtent`, 250 world units): a doorway stated as a wall never uses it, and `0` disables point doorways (`ASURA/config/Server/portals.json`, `point_box_half_extent`) | `worldserver.json`, the `World` section |
| the world itself: every map record, the entry points where a character may stand, and the arches a step crosses - the tree is classified here as one unit, so a file inside it needs no row of its own | `worldserver.json`, `WorldPack`, naming `config/flavors/Vanilla/World`, whose `config/flavors/Vanilla/World/maps/` folder is the tree (`<campaign>/<area>/<kind>/<map id>/{basic,spawns,portals}.json`). The folder of a map made outside the game also carries its map files, named by the record's `map_file_id` as eight hexadecimal digits: `<file id>.stripped.ffna`, the map as its editor built it, and `<file id>.ffna`, the finished map, which the world reads the map's walk graph from and the login program hosts for its clients over the link, with a checksum (`docs/port/24-custom-maps.md`, "The delivery plan"). These two are the only files of the pack that are not JSON: they are the client's own map format. A map's `basic.json` may also state `client_row`: the fields of the row the client itself holds for that map id (its kind, region, world-map place and the rest, by the names of `MAP_ROW_FIELDS` in `tools/client-link/src/lib.rs`), which the login program sends over the link and the client's DLL answers the client with; a field it does not state keeps the client's own value, and a name the row does not have stops the login program (`docs/port/24-custom-maps.md`, "The map's row in the client"). In place of `name_id` and `description_id`, which are ids of the client's own texts, `client_row` may state `name` and `description` as text; stating both the text and its id stops the login program. A map of the pack's own takes a `map_id` from 10000 up, states its whole row, and states no `instance_value` (`docs/decisions/0017-a-maps-client-row-is-stated-in-the-pack.md`). A folder that is not its record's `map_id`, a pack with no `maps/`, or a map file that is not a map, stops the program - serving half a world is worse than not serving (`docs/decisions/0014-the-world-content-is-a-pack.md`) |
| the creation screen's content: the bags and items a client is given, and what a new character wears per campaign and profession | `worldserver.json`, `InventoryFile` and `EquipmentFile`, naming `config/flavors/Vanilla/World/creation/inventory.json` and `config/flavors/Vanilla/World/creation/equipment.json` |
| campaign starting maps for panel character creation | `config/flavors/Vanilla/World/character_spawn.json`, copied from `ASURA/config/World/Vanilla/character_spawn.json`; a null map refuses creation |
| vanilla title IDs and custom definitions | `config/flavors/Vanilla/World/titles.json` under the configured `WorldPack` |
| captured vanilla title test states, rank thresholds, and encoded progress and hover references | `config/flavors/Vanilla/World/vanilla-title-wire.json`, read with the title catalog |
| which slot of the client's equipped bag each kind of item is worn in | `worldserver.json`, `EquippedSlotsFile`, naming `config/flavors/Vanilla/World/creation/worn_slots.json`. The index is the client's own (its nine-slot equipped array, the placement inside the equipped bag, the agent's worn list), so it is content and not a table compiled in - a wrong one draws the character without the piece (`docs/game-rules.md` E234) |
| the chat command surface: the names a player may type, what each takes, and which operation it runs | `worldserver.json`, `CommandsFile`, naming `config/flavors/Vanilla/Server/commands.json`. The surface is **content**, so the setting is only where the file is; the rows themselves are data (`docs/decisions/0011-the-command-surface-is-content.md`) |
| the first game account: the login name the client types and the token the portal hands it | `config/Server/admin.json`, the file the login program reads at every start (`AdminAccountFile`) and uses to insert one `account` row only when the table states none. It is a client's account, and a row like every other user's |
| the administrator: the one operator identity, held in a file so the database can never lock it out | not built. `docs/decisions/0013-the-admin-identity-is-a-file.md` decides it: a file the manager reads, never an `account` row; `admin.json` above is not it |
| the extracted graphs a serving world answers movement from | `worldserver.json`, `PathGraphs`, defaulting to `data/path-graphs.bin`. It is a portable `ASURAPG1` cache; it may omit maps, but the serving process never reads `Gw.dat` (`docs/decisions/0015-headless-navigation-assets.md`) |
| the cache the manager inspects and the archive it uses for an explicit map export | `manage.json`, `PathGraphsFile` and manager-only `MapArchiveFile`. The latter is empty unless an operator intends an offline dump; neither key is read by `worldserver` |
| loot tables: what a source drops and how often - a setting, not a row | not built: a JSON file under `data/loot/` (no such folder exists yet), arriving with the drop stage |
| logging: directory, appenders, logger levels | `authserver.json`, `worldserver.json` |
| the PID file, the effective-config dump | `authserver.json`, `worldserver.json` |
| what the world program does not read yet - its thread pool, its map threads, SOAP, its data directory | `worldserver.json`, arriving with the stage that reads them |

Two consequences of taking that seriously, both now in the code:

- **The connection string is critical.** A program cannot fall back to a compiled-in host and open a
  database nobody named, so a file without `LoginDatabaseInfo` or `CharacterDatabaseInfo` is a fatal
  configuration error rather than a silent default (`main.rs`, `mark_critical`). The reference's parser
  still tolerates a short string and fills the rest; no shipped file is short.
- **A setting arrives with the stage that reads it.** `Network.Threads` and `WorldServerPort` are read
  because the listeners exist; the thread pool and SOAP have no key because nothing reads one. A key in
  a file is therefore a key the program uses, never a promise.

## Where each thing lives

| what it is | examples | where it lives |
|---|---|---|
| **the world** | maps, spawn points, doorways, starting items, titles | JSON files in the world pack, `config/flavors/<flavor>/World/` |
| **the server's own content** | the chat commands | JSON files in the flavor, `config/flavors/<flavor>/Server/` |
| **what one machine chooses for itself** | the first game account | JSON files in `config/Server/` |
| **what a patched client is given** | where its DLL finds the client's functions | JSON files in the flavor, `config/flavors/<flavor>/Client/` |
| **a program's settings** | ports, database addresses, logging, and today the game's numbers too | that program's JSON file in the flavor, `config/flavors/<flavor>/worldserver.json` and `authserver.json`, with `config/machine.json` over it for what one machine alone knows |
| **what happens while the server runs** | accounts, characters, the items they carry, open instances, unlocks | database rows |
| **what the game client dictates** | packet sizes and layouts, the file check's bytes, the account settings block | named constant tables in the code, each citing its recording |

A flavor is one folder and it is the whole server setup: both programs' settings, its world pack,
its command surface and what its clients' DLL is given (`config/flavors/README.md`,
`docs/decisions/0016-a-server-is-a-flavor.md`). To make a different server, copy `Vanilla` under
another name and name it in `config/flavor.json`: its settings and its paths come with it. One
flavor is loaded per start, and its name is what the server tells its clients.

## How the files are read

- **Once, at start-up.** The world reads the pack (`game/src/Maps/Pack.rs`), the creation tables
  (`Creation::Tables`) and the command table (`commands::Table`), keeps them in memory, and answers
  every player from memory. A connection reads no file.
- **Each file is parsed once.** `common/src/Configuration/Json.rs` holds the parsed documents
  (`Store::document`), and the start-up log says how many it read (`Store::held`).
- **A missing or broken file stops the program.** It names the file and the reason.
- **A value with no answer is written `null`,** and the code refuses the action with a logged reason.
- **A file holds values and what they mean.** Where a number came from is written in `docs/`. A value
  the client decides is marked `immutable` in the file, as a note to the person editing it.

## What checks the policy

| what is checked | the test or code that fails when it is broken |
|---|---|
| a SQL file only creates tables | `worldserver/tests/schema_has_no_content.rs` fails on a SQL file that writes a row |
| a connection reads no file | `worldserver/tests/content_is_held.rs` fails on a module that serves a connection and reads a file |
| a missing file stops the program | `Creation::load` fails on a missing inventory, equipment or worn-slot file; `Pack::load` fails on a pack with no `maps/` |
| every value is stated | every reader requires its keys (`number`, `float`, `required`), and a `null` reaches the caller as `None` |
| this page lists every file | `every_config_file_is_classified_in_the_configuration_doc` (`game/src/Creation/mod.rs`, run by `cargo test -p game`) fails on any `.json` file under `config/` whose path this page does not contain. `tools/manage/manage.json` and `tools/supervisor/stack.json` are listed above by hand |

## Where the tree does not meet the policy yet

The open issues are in `docs/port/20-open-issues.md`, "The JSON policy" (items 6 to 8): the game's
numbers sitting in a file that cannot be shared, values still in the code, and settings with a default
in the code.

## Related

- Scripted behaviour is planned as Lua, calling operations the server provides
  (`docs/port/11-content-and-behaviour.md`). Lua does not replace the JSON files or the database.
- A feature brought from another project is placed by this page's table, whatever layout that project
  used.

