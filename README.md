# Kryta

A GW private server.

## Requirements

Windows, Rust stable with MSVC build tools, PostgreSQL, Python 3, and a Guild Wars client installation.
The client's DLL is built for 32-bit Windows; `rust-toolchain.toml` lists that target
(`i686-pc-windows-msvc`), so `rustup` installs it the first time cargo runs here.
Run the commands below in PowerShell from the repository root.

## Setup

Create three empty PostgreSQL databases: `kryta_login`, `kryta_characters`, and `kryta_world`.
Create a role with access to them. Copy the example configurations if you do not already have local
ones:

```powershell
Copy-Item authserver/authserver.json.dist authserver/authserver.json
Copy-Item worldserver/worldserver.json.dist worldserver/worldserver.json
Copy-Item tools/manage/manage.json.dist tools/manage/manage.json
```

Set the database connection strings in those files, then build:

```powershell
cargo build --workspace
```

On a fresh database, run the world once to create the schema, then stop it with Ctrl+C:

```powershell
cargo run -p worldserver --bin worldserver -- -c worldserver/worldserver.json
```

## Start Kryta

```powershell
cargo run -p kryta-supervisor
```

Open the Control Panel at [http://127.0.0.1:6602/](http://127.0.0.1:6602/). To run the management
service and panel without the stack:

```powershell
cargo run -p kryta-management-service
```

The management service prints the panel address on startup. Ctrl+C stops whichever program is running
in that terminal.

## Patch and start the client

Replace the example path with your Guild Wars folder:

```powershell
python -m pip install pefile
python tools/client/patch-gw.py "C:\Games\Guild Wars\Gw.exe" tools/client/keys.txt --output "C:\Games\Guild Wars\Gw.custom.exe"
& "C:\Games\Guild Wars\Gw.custom.exe" -authsrv 127.0.0.1 -portal 127.0.0.1
```

Start the stack before launching the client: the patched client does not start without it. The
patch creates a separate executable and leaves `Gw.exe` intact. It also builds `Kryta.dll` with
cargo and writes it beside `Gw.custom.exe`; the client loads that DLL by itself when it starts. What the patcher needs and its options are in
[Tools](docs/port/05-tooling.md#building-and-running-the-client).

Use `Gw.custom.exe` for regular play and title testing. How the client's DLL works is in
[the client platform](docs/port/22-client-platform.md).

## Documentation

- [Configuration and database settings](docs/port/04-configuration.md)
- [Tools, stack controls, and client patching](docs/port/05-tooling.md)
- [Manual client test](docs/port/07-manual-test.md)
- [Operator API and Control Panel status](docs/port/14-operator-surfaces.md)
- [What runs today, and what proves it](docs/port/01-structure.md)
- [The protocol audit](docs/port/09-parity-audit.md)
- [The game rules](docs/game-rules.md)
- [Research sources](docs/research-sources.md)
- [Project instructions](AGENTS.md)
