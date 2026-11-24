# What a patched client is given

A client patched for Kryta loads `Kryta.dll` when it starts, and the DLL opens its own connection to
the server - the client link (`docs/port/22-client-platform.md`). This folder holds what the server
sends the DLL over that link. It is named by `ClientLink.Directory` in `authserver/authserver.json`,
and `authserver` reads it once, at start-up.

| folder | holds |
|---|---|
| `offsets/` | the offset documents: every `*.json` file is one, and all of them are sent to each client |

## An offset document

A document says how to find places in the client by what their bytes look like, so that nothing in
the DLL is an address. It is Py4GW Reforged's offset format, unchanged, so a document written for one
project works in the other:

- `namespace`: the prefix of every name in the file (`titles` makes `titles.rank_call`).
- `patterns`: named byte patterns. `pattern` is the bytes (`\xNN`), `mask` says which are compared
  (`x`) and which are not (`?`), `offset` is added to the address of a match, and `section` is where
  to search (`text`, `rdata` or `data`).
- `resolvers`: named lists of `steps` that lead from a match to an address. A step has an `op`
  (`scan`, `scan_in_range`, `to_function_start`, `function_from_near_call`, `find_use_of_string`,
  `dereference`, `read_u32`, `add`, `divide`, `validate_section`), reads earlier outputs by name
  (`in`, `start`, `end`) and writes one (`out`). The output named `final` is the resolver's answer.
  A resolver may hold several `attempts` instead of `steps`; the first that works is used, which is
  how one file serves more than one client build.

The reader is `tools/client-dll/src/patterns.rs`.

## When the client is updated

A new client build moves the client's functions. The DLL finds them again by pattern, so most
updates need nothing. When a pattern no longer matches, the client's `Kryta.<pid>.log` and the login
program's log both name the offset that was not found; correct the pattern here (or add an attempt
for the new build beside the old one) and restart `authserver`. Players download nothing.

`cargo test -p kryta-client -- --ignored`, with `KRYTA_GW_EXE` naming an unpatched `Gw.exe`, resolves
every offset in this folder against that file and prints the addresses.

| file | concern |
|---|---|
| `offsets/titles.json` | the three places the custom title feature hooks: the title rank message's call to its worker, the hover text's call to its formatter, and the title table lookup |
