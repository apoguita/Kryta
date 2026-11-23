# 0006 - Accounts are rows, not a file

Status: accepted. It corrects 0003's account source and closes a stopgap; it is not a deviation from
the reference, whose accounts also live in its login database.

## The question

The portal answers who a login name is. When it was written there was no account schema and no
identity decision, so the pairs lived in `data/accounts.json` - a file this tree then carried into the
JSON configuration format, where it looked like configuration and was not.

## The decision

An account is a **row in the login database's `account` table** (`data/sql/base/db_auth/account.sql`).
The table holds the login name the client types and the identifiers the portal issued for it; the
portal reads the row on every login. There is no file.

## Why

1. **It is state, and state is the database's.** An account is created once, used many times, and its
   columns change as a person plays; a configuration file is a thing an operator writes and shares.
   `AGENTS.md` rule 7 keeps a database row for what a server writes while it runs, and an account is
   exactly that; the difference between that and a file is what this record exists to fix.
2. **The reference does the same.** `AuthSocket::HandleAuthSession` asks `LoginDatabase` for the row
   and reads the answer; there is no account file in that tree either.
3. **A file cannot be the authority for something many processes share.** The auth program reads an
   account now; a character's owner will be read by the world program next. Two programs reading one
   file is a cache that disagrees with itself; one table is a fact.

## What it changes

- `data/accounts.json` and its reader are gone. The `Accounts` trait stays in `game/src/Portal/` -
  the portal is still told who an account is rather than knowing - and the implementation that asks
  the database is the auth program's, `authserver/src/Server/accounts.rs`, because that program owns
  the login pool.
- The query takes the login name **as a parameter**, never as text: it arrived on a socket.
  `Connection::query_with` exists for that and mirrors the reference's prepared statements.
- The lookup runs on a **pool worker**, not on the session's thread: the blocking client starts a
  runtime of its own, and a runtime inside the socket layer's runtime panics and takes the network
  thread with it (measured, not feared - a login against the first version did exactly that). So the
  pool grew a `Query` operation with a reply channel, which is the reference's `Query` with its
  callback by another name.

## What it does not decide

How a password is checked. An operator creates an account in the panel (`tools/manage`, `/accounts`,
through `game::accounts`), and the login program inserts the first one from `config/Server/admin.json`
when the table is empty; the pair on the row is what the portal hands out, and a name the table does
not hold is GWPS's own refusal: `400 who are you?`. A client cannot create an account.
