# Evidence

This directory holds measurements this tree relies on, copied here so a citation points at something
in the repository rather than at another project's working copy.

| file | is | came from |
|---|---|---|
| `client-protocol.md` | one client's protocol, measured: framing, the handshake, the login batch, the portal request, the gameplay version message, and the map and item families | ASURA `docs/reference/client-protocol.md` |
| `packet-coverage.md` | the header-to-name-and-size table that record produced, with its own note on what a recorded size is | ASURA `docs/work/packet-coverage.md` |
| `ui-settings-persistence.md` | the client's account settings upload (`0x8021` then `0x8020` chunks), measured from a live auth log, and why the blob is the UI layout | ASURA `docs/46-ui-settings-persistence.md` at `071735e` |
| `asura-history/README.md` | recovered historical documentation, source commits, and a subject index | ASURA Git history |

`recordings/` is the other half: the captures themselves, copied whole from ASURA's `recordings/` -
`gwps/` (the reference server's own send trace), `asura/` (a working server's wire), and `client/`
(the client sniffer, which records the client's message structs rather than the wire).

## How to read these

- **They are evidence, not rules.** A document here says what someone measured; it does not decide how
  this server is arranged. That is `AGENTS.md` and `docs/port/01-structure.md`.
- **A newer measurement wins.** Where a file here disagrees with a capture this tree made later, the
  later one is right, and the correction is written where the fact is used.
- **A recorded size is not always a wire size.** `recordings/client/` holds struct dumps, two or more
  bytes wider than the frame; `docs/port/02-wire.md`, "Three kinds of capture", says which of the
  three kinds holds the wire and how to tell them apart.
