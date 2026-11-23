# 0011 - Command names map to server API functions

Status: accepted; corrected to reflect the owner's clarification. The command rows and the shared
operation registry exist; a command permission model and the Lua caller remain planned.

## Decision

`config/Server/commands.json` holds command names, aliases and their mapping to an operation. The `operation`
value names the registered Rust API function: for example, `tp` maps to the teleport function. The
same function executes when called from a GM command, console, management tool or another authorized
caller. Adding an alias to an existing mapping needs a data change, not another implementation.

The command reader (`game/src/Chat/Commands.rs`) resolves each row's `operation` through the
`Registry` in `game/src/Chat/Operations.rs` when the file is read; there is no enum of commands, and a
row naming an unregistered operation or undeclared parameters is refused at start-up. The world
program builds one registry (`worldserver/src/main.rs`) and three callers invoke it: a chat line
(`game/src/Handlers/ChatHandler.rs`), the embedded console (`worldserver/src/CommandLine/`) and the
loopback control endpoint (`worldserver/src/RemoteAccess/`), which the management panel's console
page calls from another process. The contract is `docs/port/14-operator-surfaces.md`.

Lua remains the planned authoring surface for world behaviour and can call server operations. Its
runtime, bindings and any script-defined commands remain to be designed in
`docs/port/11-content-and-behaviour.md`. Lua is not a required intermediate step for a JSON command
mapped to a Rust function.

## Constraints and status

- Command names and aliases are content; the function owns the operation. The same execution must
  not be duplicated for each caller.
- Rule 0 governs client-visible behaviour, including existing command arguments,
  refusals, order and frames; the parity audit applies to any change to them.
- The existing travel target alias table and `/maps` text remain in Rust. Their relocation is not
  complete. Client-facing changes require comparison with their source and the audit.
- Current account rows provide no command permission model, and no per-account authorization is
  implemented. The control endpoint is authenticated only by its service credential, and the `Lua`
  caller kind waits for the Lua runtime.
- `docs/decisions/0012-chat-text-is-sent.md` records the chat output and its live-client evidence.

## What proves it

A JSON alias resolves to the same registered function as its primary name; invalid mappings are
reported; and each supported caller obtains the result of the same operation. Existing chat commands
retain their ported behaviour and client-visible frames, checked against evidence and a real client.
The tests and the live runs that show each of these are in `docs/port/05-tooling.md`, "One command
surface, one registry".
