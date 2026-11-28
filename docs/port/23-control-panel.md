# The control panel: assessment, direction and the rebuild

What the control panel (`tools/manage`, served by `tools/management-service`) is, the direction the
owner gave on 2026-11-23, what the rebuild of the same day put in place, and what is still not built.
The requirements it has always had are `docs/port/12-research-and-control-panel.md` and
`docs/port/14-operator-surfaces.md`; this page does not replace them.

The assessment was read on 2026-11-23 from every file under `tools/manage/src` and
`tools/management-service/src` as they were then, the two requirement documents, the world's control
endpoint, the supervisor's channel, and the recording tools. The findings marked **checked** were
confirmed against the code a second time.

## The direction (owner, 2026-11-23)

1. The manager is to be a full management console for the server and its features.
2. It is to let an operator drive recordings and manage them. Driving means a way to launch the
   scripts that record. Where a script cannot be launched because it needs elevation, the panel has
   an area with clear instructions and the commands to run by hand.
3. The whole site needs a facelift: a better paradigm and a more modern look. The owner's words for
   its state before the rebuild: somewhat working, and requiring a full re-engineer.
4. The database holds nothing but test data at this stage, so wiping it is not a problem. It was
   wiped completely on 2026-11-23, to start over.
5. The panel must be easy to expand, because much more will be added to it: one registration point
   per area, on both sides ("Adding an area", below).

## What is rebuilt (2026-11-23)

### The paradigm

The service answers data; the pages are static files that ask for it.

- **A JSON API under `/api/v1/`.** `GET` reads, `POST` changes and carries a JSON body. Every answer
  is JSON from one helper (`tools/manage/src/api.rs`, `answer`): `{"ok":true, ...}`, or
  `{"ok":false,"error":"<plain sentence>"}` with a fitting status.
- **Static pages with no build step.** `tools/manage/ui/` holds `index.html`, `app.css`, `app.js`
  (the shell: router, service client, building blocks) and one script per area under `ui/areas/`.
  They are embedded in the binary with `include_str!` and served at `/` and `/ui/<file>`. No
  framework, no package manager, nothing fetched from another host. The browser routes on the `#`
  part (`#/accounts`, `#/players/<uuid>`), so one `index.html` serves every address. CSS and
  JavaScript together are 118 kB.
- **The data access is unchanged.** The panel calls the game library's repositories
  (`game::accounts`, `game::characters`, `game::maps::pack`, `game::creation`), the supervisor's line
  protocol and the world's control endpoint. It writes no SQL.
- **The old HTML page layer is gone:** `pages/*.rs`, `web/components.rs`, `web/layout.rs`,
  `web/http.rs`, `style.css` and the redirects for the old addresses. There is one UI.

### The code

| file | is |
|---|---|
| `src/lib.rs` (51 lines) | the module list and the one function that answers a request: static file, `/health`, or an area's route |
| `src/api.rs` | the route type, the call a handler reads, the one answer helper, the guard, the router; generic over its state, so its tests drive it with a stub and no database |
| `src/areas/mod.rs` | the area table - the single registration point - and the tests that keep it honest |
| `src/areas/<area>.rs` | one module per area: its routes and their handlers |
| `src/ui.rs` | the embedded static files and their content types |
| `src/state.rs`, `src/clients.rs`, `src/startup.rs` | what the service answers from; the supervisor and world clients; reading `manage.json` and serving |
| `src/view.rs`, `src/path_graphs.rs` | the game's names for stored numbers; the path-graph cache operations (was `navigation.rs`) |
| `ui/app.js`, `ui/app.css`, `ui/index.html` | the shell, the stylesheet, the one page |
| `ui/areas/<area>.js` | one script per area: its page |

No function is longer than 60 lines.

### The areas and their routes

| area | page | routes |
|---|---|---|
| overview | `#/overview` | `GET /api/v1/overview` - the counts and the "needs attention" list (recordings unsanitised or not indexed, schema files changed or new, characters with unsound item rows). The page shows the services and the world's status beside it, from their own areas. |
| services | `#/services` | `GET /api/v1/services`; `POST /api/v1/services/{name}/restart`; `POST /api/v1/services/{name}/stop`; `POST /api/v1/services/stop-all`. Refreshes every four seconds. |
| flavors | `#/flavors` | `GET /api/v1/flavors` - every flavor folder under `ConfigFolder`, what each holds, and the one `flavor.json` names; `POST /api/v1/flavors/activate` (a `name`) writes that name into `flavor.json`, changing only the value's characters, and is refused for a flavor that lacks one of the two settings files; `POST /api/v1/flavors/create` (a `name` and a `from`) makes a flavor as a whole copy of another, and is refused for a name that is not a flavor's or that differs from an existing one only in case. The programs read the name when they start, so a chosen flavor runs from the next restart; the panel's own pages follow it at once. Nothing here renames or removes a flavor (`docs/decisions/0016-a-server-is-a-flavor.md`). |
| console | `#/console` | `GET /api/v1/console/operations` (the world's `world.operations`, parsed into rows); `GET /api/v1/console/status` (`world.status`); `POST /api/v1/console/run`. The form is built from the operation's own argument names; when a line of the list cannot be parsed, the free `name=value` area is offered instead. |
| accounts | `#/accounts`, `#/accounts/{id}` | `GET /api/v1/accounts`; `POST /api/v1/accounts`; `GET /api/v1/accounts/{id}`; `GET /api/v1/accounts/{id}/token`; `POST /api/v1/accounts/{id}/update`, `/active`, `/eula`, `/delete`. The update also carries `character_slots`: how many character slots the account has (2 to 32; an account is created with 6) |
| players | `#/players`, `#/players/{uuid}` | `GET /api/v1/players`; `POST /api/v1/players`; `GET /api/v1/players/{uuid}`; `POST /api/v1/players/{uuid}/rename`, `/level`, `/delete` |
| content | `#/content/{maps,creation,titles,instances,worlds}` | `GET /api/v1/content/maps`, `/creation`, `/titles`, `/instances`, `/worlds` |
| navigation | `#/navigation` | `GET /api/v1/navigation`; `POST /api/v1/navigation/dump` |
| settings | `#/settings` | `GET /api/v1/settings`; `POST /api/v1/settings/{program}/set` (`world` or `login`, with a `key` and a `value`). The settings are the two programs' in the flavor this machine runs, with the machine's own values over them, and each row says which file states it: the flavor's (`config/flavors/<flavor>/<program>.json`) or the machine's (`config/machine.json`, the section named as the program). A change is written into the file that states the value. Each row has a Change button when its value is one number, flag or text and is not a database connection string. A setting only `config/machine.json.dist` states is listed with its value there, marked "default, not in the file"; saving it adds it to the program's section of the machine's file with its `doc`. A flavor's value is not turned into a machine's from here, nor the other way. A change rewrites only the value's characters, is read back before it is written, and takes effect when the service is restarted. The panel does not know a setting's limits: the program refuses to start on a value it does not accept. |
| schema | `#/schema` | `GET /api/v1/schema` |
| recordings | `#/recordings/{client,server,library}` | `GET /api/v1/recordings`; `GET /api/v1/recordings/preflight`; `GET /api/v1/recordings/describe?path=`; `POST /api/v1/recordings/index` |

`GET /health` answers `ok`. Search and filters (accounts by state, players by account, maps by
campaign and kind, recordings by state) are done in the browser.

Three things moved while being carried over:

- **The fixed Metrics and Titles sheets** left the player page, where they were the same for every
  player, and are the Titles tab of Content, which says so.
- **A map's campaign** is the campaign folder the pack keeps it in
  (`<pack>/maps/<campaign>/...`, `game/src/Maps/Pack.rs`). The map record states none, so the panel
  reads the folder name (`areas/content.rs`, `campaigns_by_folder`).
- **The world's status** has no page of its own: it is on the overview and the console.

### A character's data check

A character whose item rows include one with `item_type` 0 is reported as unsound, in the players
list, on its own page and on the overview (`areas/players.rs`, `Soundness`). Rows like that crashed
the client when the character was loaded.

### Security

- **Other web pages cannot change anything.** Every request that is not a `GET` is refused with 403
  unless it carries the header `X-Kryta-Panel: 1`. The panel's own pages send it; a form on another
  site cannot send a custom header, and the service answers no CORS preflight (an `OPTIONS` is
  refused the same way).
- **DNS rebinding.** On a loopback bind, any request whose `Host` header is not `127.0.0.1:<port>`,
  `localhost:<port>` or `[::1]:<port>` is refused with 403. A panel bound to another address does
  not make this check.
- **The token works.** When `Token` is set, every `/api/` request needs
  `Authorization: Bearer <token>`. The static files load without it; the page shows an unlock screen,
  keeps the token in `sessionStorage` and sends it. The cookie and `?token=` mechanism is removed.
- **No script injection.** The pages build every node with `textContent`; there is no `innerHTML`, no
  inline handler and no `eval`, and a test fails if one appears. `index.html` carries a
  content-security policy that allows scripts, styles and requests from the panel's own origin only.
  Confirmations are the panel's own dialogs.
- **Secrets.** Settings are masked by the settings reader, as before. An account's portal token is
  never in a list or a detail answer (only its last four characters); the page has a Reveal button,
  which asks `GET /api/v1/accounts/{id}/token`. Saving an account with the token field blank keeps
  the token it has.

### Verified, and how (2026-11-23)

- `cargo build --workspace`: no warnings. `cargo clippy -p kryta-manage --all-targets`: clean for
  this crate. `cargo test -p kryta-manage`: 50 passed. `cargo test --workspace`: 571 passed,
  23 ignored (540 before; the panel's 19 helper tests became 50).
- The tests cover: each path reaching its own handler; a `POST` without the header (403); a wrong,
  short or missing `Host` (403) and a non-loopback bind; the token required when set and not when
  unset, and never for the static files; both shapes of the answer helper; the recordings path
  guard; the `tasklist` parser; the index-entry builder; the folder scheme and the date stripped
  from a name; the character soundness check; settings masking; the token mask; the schema
  comparison; the operation-line parser; and the registration tests of "Adding an area".
- The service was started alone against this machine's databases (one account, no characters; the
  supervisor and the world were not running). Every `GET` route answered 200 with JSON: 1 to 75 ms
  for the reads; 0.7 s for the 83 MB path-graph cache; 0.3 s for the preflight (it runs `tasklist`);
  2 s for the three that ask an owner that is not running (Windows takes that long to refuse a
  loopback connection), which the pages ask for after they are drawn.
- A `POST` without the header, a form-encoded `POST` with an `Origin`, an `OPTIONS` and a request
  with `Host: evil.example:6602` were each refused with 403.
- Through the API: an account was created, read back, deactivated, given an EULA version, renamed
  and deleted; a duplicate name was refused with 409. A character was created on a throwaway account
  (nine starting item rows, Ascalon City unlocked, data sound), read back, renamed, refused level 25,
  set to level 4, and deleted with its account. The database was left with its one account and no
  character; the account id sequence moved on.
- Headless Edge was driven through every page, the two creation dialogs, the delete confirmation,
  the recording drawer, the quick-find palette, the dark theme and a 560-pixel window, with no
  script error. The only console message was a missing `favicon.ico`, since given an empty icon.

- A second instance was started on port 6603 with a `Token` set. The static files answered 200
  without it; an `/api/` request answered 401 with no token, a wrong one, the token in `?token=` and
  the token in a cookie, and 200 with `Authorization: Bearer`. In headless Edge the unlock screen
  appeared, refused a wrong token with "That is not the token." and opened the page with the right one.

**Not verified:** anything against a running supervisor or a running world (the services table with
real rows, restart and stop, the console running a real operation), the navigation dump (no `MapArchiveFile` is set on this machine), and
`POST /api/v1/recordings/index` writing a line (it was exercised only up to its refusals, so the
committed index was not changed; the line it builds is covered by a test).

### Still not built

| area | state |
|---|---|
| administrator login (`docs/decisions/0013-the-admin-identity-is-a-file.md`) | not built; the token is the only gate |
| editing content | read-only; of the files under `config/` the panel writes only the two programs' settings, `machine.json`, `flavor.json` and the copy that makes a new flavor. Of the settings, lists (the loggers and appenders), the database connection strings and a setting inside a section the file lacks are still changed in the file (owner, 2026-11-24: the panel changes a program's settings) |
| logs and monitoring | not built; the supervisor keeps a 30-line tail per service and does not expose it |
| starting a stopped service | the supervisor has no start verb; `restart` starts a stopped service |
| online players, kick, announcements | not built; the world registers 8 operations, mostly status and exit |
| items, unlocks, gold, titles of a player | read-only |
| comparing two recordings | not built; `tools/capture/diff.py` is run by hand |
| the server capture driven from the panel | instructions only; the panel can now change `Network.Trace`, and the loggers it needs are lists, which it does not edit |
| the server's own character creation | the panel still has its own (below) |

## Adding an area

An area is one Rust module and one script, with the same name, and one word in one table. Nothing
else is edited: the router, the page's script lines, the menu, the menu groups and the Ctrl+K palette
all follow from the registrations.

1. Write `tools/manage/src/areas/<name>.rs` with `pub fn routes() -> Vec<Route<Manage>>`. Every
   pattern starts with `/api/v1/<name>`.
2. Write `tools/manage/ui/areas/<name>.js`, which calls `Kryta.area({ id: "<name>", ... })`.
3. Add `<name>` to the `areas!` list in `tools/manage/src/areas/mod.rs`. Its position is its position
   in the menu.

A minimal area, complete on both sides:

```rust
//! Motd: the message of the day.

use serde_json::json;

use crate::api::{Call, Reply, Route, bad, get, post};
use crate::state::Manage;

/// The motd area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![get("/api/v1/motd", read), post("/api/v1/motd", write)]
}

/// `GET /api/v1/motd`.
fn read(_: &Manage, _: &Call) -> Reply {
    Ok(json!({ "lines": ["Welcome."] }))
}

/// `POST /api/v1/motd`.
fn write(manage: &Manage, call: &Call) -> Reply {
    let text = call.text("text");
    if text.is_empty() {
        return Err(bad("A message needs some text."));
    }
    manage.say(format!("motd: {text}"));
    Ok(json!({ "text": text }))
}
```

```js
(function () {
  "use strict";
  var ui = Kryta.ui, api = Kryta.api;

  Kryta.area({
    id: "motd",
    label: "Message of the day",
    group: "World",            // a name no other area uses makes a new menu heading
    hint: "what a player is told on entering",
    render: async function (view) {
      var data = await api.get("/motd");
      view.body(
        ui.card("Lines", ui.table({
          columns: [{ label: "Line", value: function (row) { return row; } }],
          rows: data.lines,
          empty: { title: "No message yet", text: "Write one below." }
        }), { flush: true }),
        ui.card("Change it", ui.form({
          fields: [{ name: "text", label: "Text", full: true }],
          submit: "Save",
          onSubmit: async function (values) {
            await api.post("/motd", values);
            ui.toast("Saved.");
            view.reload();
          }
        })));
    }
  });
})();
```

What a handler is given: `call.param(n)` and `call.id(n)` for the `{name}` parts of its pattern,
`call.text`, `call.number` and `call.flag` for the JSON body, `call.query` for the query string. It
answers `Ok(json!({...}))` or one of `bad` (400), `missing` (404), `conflict` (409), `broken` (500),
`unavailable` (503), `fail(status, text)`. The guard and the JSON shape are applied by the router.

What a page is given, in `Kryta`: `api.get` and `api.post`; `h(tag, attributes, children)` for a
node; and in `ui`: `table`, `form`, `formModal`, `modal`, `confirm`, `drawer`, `toast`, `act`,
`badge`, `button`, `link`, `card`, `stat`, `grid`, `toolbar`, `tabs`, `search`, `select`, `empty`,
`skeleton`, `banner`, `kv`, `check`, `steps`, `copy`, `log`. The `view` has `body(...)`,
`head({crumb, sub, actions})`, `every(ms, fn)`, `reload()` and `route` (`parts`, `query`). An area
may also give `count` (the number beside its menu entry) and `find` (entries for the palette).

The smallest real area is `schema`: 89 lines of Rust before its test, and 31 lines of script.

The tests in `areas/mod.rs` fail on an area that declares no route, a route outside its area's
prefix, two routes that could answer the same address, an area registered twice, and a script that
does not register its area's id. An area whose script file is missing does not compile.

## What was wrong, and where it stands

### How it was built

| fault | now |
|---|---|
| Pages were long hand-built HTML strings, escaped by hand; six functions ran 120 to 270 lines. | Gone with the page layer. Handlers answer JSON; the pages build nodes. |
| Three ways of answering a form. | One: a `POST` answers JSON, and the page shows a toast and redraws. A refresh never repeats an action. |
| Errors lost the form, and a success showed nothing. | A form keeps what was typed and shows the error beside its button and as a toast; a success is a toast. |
| Metrics and Titles on a player page were one fixed sheet. | Moved to Content, where a sheet that is the same for everyone belongs. |
| Logic in the router file; two modules named `navigation`. | `lib.rs` is 51 lines. The navigation handlers are `areas/navigation.rs`, the cache module is `path_graphs.rs`, the schema comparison is `areas/schema.rs`, the clients are `clients.rs`. |
| Dead accessors, old-address redirects, placeholder tabs, stale text. | Removed. |
| 19 tests, all on small helpers. | 50, including the router, the guard and the registration. |
| **A second character creation** (was `pages/players.rs:377-523`). | **Still true.** It is now one named function, `create_character_duplicating_the_server` in `areas/players.rs`, with two helpers, and its comment says it is to be replaced by the server's own. |

### Security (checked before the rebuild)

| fault | now |
|---|---|
| The access token did not work: a cookie was promised and never set. | Replaced by a bearer token that works ("Security", above). |
| Any page open in the operator's browser could post to `127.0.0.1:6602`. | Closed by the required header and the `Host` check. |
| A script injection in the delete confirmation. | Gone: no markup is built from text. |
| The portal token was shown in a plain text field. | Masked, with a Reveal button. |

## The bad data, and where it came from (checked)

The database held 570 item rows and 45 unlock rows that belonged to no character. Neither the server
nor the panel leaks: both delete a character through `DatabaseCharacters::remove`, which removes its
items, its unlocks and its row (`game/src/Characters/Repository.rs:946-951`).

The rows came from the database tests. `a_creation_confirm_writes_a_character_row` creates a character
named "Kryta Test" with its starting items and an unlock, and at its next run deletes only the
character's row, by name (`game/src/Server/WorldSession.rs:3590`). Each run strands one character's
items. Other database tests clean up the same partial way (`game/src/Characters/Repository.rs`,
`authserver/tests/end_to_end.rs`).

Two things let it happen and are still true:

- The character database has no foreign keys, so it accepts an item that belongs to nobody.
- The delete is three statements with no transaction.

Until the tests clean up after themselves, or run on a database of their own, every run of the
ignored database tests puts orphan rows back. The panel does not look for orphan rows; its data
check is per character.

## Recordings

### What exists

| kind | made by | needs | stopped by |
|---|---|---|---|
| server wire capture | the world itself, when `CaptureFile` is set in `worldserver.json` (`game/src/Server/Capture.rs`) | a world restart to switch on; it overwrites its file | the world stopping |
| client capture | `tools/client/sniff-capture.py`, through Py4GW Stealth | an elevated shell, a 32-bit Python, the Stealth checkout, and a running 32-bit client | its `--seconds` limit, or Ctrl+C in its own console |
| fresh-character capture | `tools/client/capture-fresh-character.ps1`, which elevates itself and runs the script above with `--raw` | the same, and exactly one process named `Gw` | Ctrl+C in its window |

The files are read with `tools/capture/read.py` and compared with `tools/capture/diff.py`. The
panel lists 33 recordings (30 `.jsonl` and 3 `.log`; the 54 that `recordings/README.md` counts
include 21 `.gitkeep` placeholders), of which `recordings/index.jsonl` lists 18. The six files in
`recordings/client/raw/` are unsanitised. A capture's default output folder is the movement folder.

### Why the panel cannot simply press the button

- **Elevation.** The client capture refuses to run without it, and the panel's service is not
  elevated.
- **Stopping.** A capture ends on its time limit or on Ctrl+C in its own console. Killing it leaves
  its hooks inside the client and loses the summary record.
- **Outside dependencies.** The 32-bit Python and the Stealth checkout are at per-machine paths.

### What the panel does instead (owner, 2026-11-23) - built

It does not elevate anything. The recordings area:

- **checks what a capture needs** (`GET /api/v1/recordings/preflight`): whether a 32-bit Python is
  at one of the places the script looks (`KRYTA_PY32`, `ASURA_PY32`, then
  `%LOCALAPPDATA%\Programs\Python\Python313-32` and `Python312-32`), whether the Stealth folder
  (`PY4GW_STEALTH`, else `%USERPROFILE%\Py4GW_Stealth`) holds a `py4gw` folder, whether the capture
  script is there, and which processes named `Gw*` are running, from `tasklist /FO CSV /NH`;
- **builds the command:** the operator picks the client, the direction, the duration, the subject
  and whether names are masked (`--raw` when not, with a warning), and the page shows the exact
  PowerShell lines with
  `--out recordings/client/feature/<subject>/<subject>-<YYYYMMDD-HHMMSS>.jsonl` and a Copy button;
- **gives the instructions beside it:** open an elevated PowerShell, paste, what the script prints,
  stop with Ctrl+C in that window and never close it, come back to the library;
- **explains the server capture** on a second tab: set `CaptureFile`, restart the world, the file is
  overwritten at each start. Instructions only;
- **manages the files:** the library (`GET /api/v1/recordings`) walks `recordings/` and, for each
  `.jsonl` or `.log` file, gives its path, its source and kind from the folder scheme, its size, and
  from its header line the tool, layer, start and whether it is sanitised, and whether the index
  lists it. It reads one line of each file. Describe (`GET /api/v1/recordings/describe?path=`) reads
  one file's header record, its summary record and its message count. Add to index
  (`POST /api/v1/recordings/index`) appends one line to `recordings/index.jsonl` with the fields
  `recordings/README.md` names. A path that leaves `recordings/` is refused.

The index line's `recorded` is the header's `started_at`; a start written as a count of seconds
(`asura-world` wrote `1793251739.946451100Z`) is written as a date; a file with no header gets its
own modification time. Its `subject` is the file's name without a trailing date.

## The rebuild's order

1. The foundation: one pattern for every page and every form, working protection against other web
   pages, and the new look. **Done.**
2. Accounts and players, done properly, on the server's own character creation. **Done except the
   creation**, which is still the panel's own.
3. Services and logs. **Services done; logs not built.**
4. Recordings. **Done except comparing two.**
5. Editing configuration and content, when the owner decides the panel may write files. **Not
   started.**

The tests' clean-up and the missing foreign keys are still to be fixed with step 2's remainder,
since that is where the character rows are handled.
