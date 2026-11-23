# 0005 - Configuration is JSON, read by one handler

Status: accepted. This is an addition the owner required, not a reading of the reference; the owner
may correct it, and a correction replaces this text rather than being appended to it.

## The question

The reference's format is `Key = value` in a `.conf` file, parsed by
`src/common/Configuration/Config.cpp`. This tree ported that parser and every quirk in it, and both
programs read an `authserver.conf` / `worldserver.conf`.

The owner asked for JSON, because a settings file is something an operator edits and shares, and JSON
is what another program can read and two people can diff. Read as "every number in a file", it carried
two payloads and an account list into files as well - all three are out again, and the rule is the one
sentence in `docs/port/04-configuration.md`.

## The decision

1. **The format is JSON, for settings only.** `authserver/authserver.json` and
   `worldserver/worldserver.json` are the programs' own files; the `.dist` files are committed and the
   live ones are not, because they hold credentials. A configuration file holds what an operator
   *chooses and shares*. It holds no payload the client dictates and no row a person lives in - a
   correction to this record's first version, which was written broadly enough that two payloads and
   an account list ended up in files. Both were undone: accounts are rows (0006), and the payloads
   are constants again. `docs/port/04-configuration.md` is the classification.
2. **One reader, in `common/src/Configuration/Json.rs`.** Not one per consumer: a file is parsed
   into `Document`, flattened to the reference's `dotted.key -> text` bag, and every typed getter,
   severity policy, environment override, `describe()` and masking above it is unchanged. A record
   list - a file of rows, such as the world list, the command rows and the creation files - is read
   as rows through the same `Document`, never by a hand-written parse in the module that wants it.
3. **Key names stay the reference's.** `RealmServerPort`, `Logger.network.trace`,
   `LoginDatabaseInfo` mean what they meant in the `.conf`. The change is the file format, not the
   vocabulary, so every document and call site still says the same word.
4. **Every value carries its reason where it is stated.** JSON has no comments, so `doc` states
   where a number came from, `note` says what a file is for, and `information` holds commentary of
   any shape - an object, an array, prose - for the explanation that will not fit in one line; all
   three are read past, never treated as keys. A number the client decides rather than the server is
   marked `immutable`, and one marked immutable without a `doc` is a complaint.
5. **The reference's parser stays, and no shipped file uses it.** `Config::parse` is the
   reference's translation with its measured quirks; it is kept because it is the reference's, and
   a `.conf` handed to this server still loads. Nothing in `data/` or in a program's directory is
   `.conf` any more.

The rules for how a document becomes that bag - sections, leaves, arrays, `value`, `hex`, `null` for
"not set" - are written down where they are implemented, at the head of
`common/src/Configuration/Json.rs`, and repeated for an operator in `docs/port/04-configuration.md`.

## Precedence, which the reference already had

An environment variable (`AC_` plus the key in upper snake) wins over the file, and the file wins over
the default the caller passes. That order does not change with the format, and it is what makes a
one-off experiment possible without editing a file a user shares.

For a datum that also lives in the database, the rule is the second half of the requirement: a
configuration file can *name* it, so a user can run a world without hand-editing rows. The realm
list is the first case: `data/realms.json` is merged over the `realmlist` table - a file row overrides
a database row of the same id, a new id is added, and a field the file does not state keeps the
table's value (`docs/port/04-configuration.md`).

## What stays in code

A setting is what an operator sets, so what nobody sets stays in code: what the client dictates (message
sizes, status codes, the handshake's arithmetic, the file check and account settings payloads), what a
player makes (an account, a character - those are rows), and what is part of an algorithm or a layout
(the RC4 S-box, the key derivation's rounds and rotation widths, SHA-1's initial state, a field's offset
in the layout its builder writes).

The settings that were still constants when this record was written - the world list, the network thread
count, the connection string - are files now; `docs/port/04-configuration.md` lists where every setting
is set.

## What it costs

- A deviation from the reference's own config format, and the reference's `.conf` files replaced by
  JSON ones. Its parser survives; its files do not.
- `serde_json` joins `common`'s dependencies. It is used for its data model - `Value` and the
  parser - not for deriving a struct per module, which would have reintroduced a hand-written reader
  by the back door and made a setting's name a Rust identifier.
- Three wrong things were carried into files by the first version of this record and taken out again:
  the account list (a player's, so a row - 0006) and two payloads (the client's, so constants). What
  the format change actually costs is small; what reading the rule as "every number" cost was three
  corrections.
