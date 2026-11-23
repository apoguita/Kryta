# 0008 - The administration tool is a program of its own, outside the two servers

Status: updated by the owner's direction to run the panel and management API as one management service.
The existing database-backed panel routes remain in place during API migration.

## The question

Every piece of game data is now a row or a content file: accounts and their EULA version, characters and
their settings, the world list, the maps and the creation sets. A person needs to change them - create an
account, make a client accept the EULA again, correct a character's level, look at what a set contains.
Where does that live?

The reference has no such program. What it has is a worldserver console and a SOAP surface for in-game
operations, and everything else through the database by other means. Its `authserver` answers clients and
nothing more.

## The decision

The management service `tools/management-service/` serves the panel implemented in `tools/manage/`.
It can be independently started or supervised with the stack. Account and character API operations
are not implemented yet: existing panel routes access repositories directly and do not coordinate
active sessions.

## Why

1. **Separate operator service.** The management API is independent of the client-facing listeners.
   The portal keeps its client routes; operator actions must coordinate with the relevant server owners.
2. **The data access moves into the noun, not the tool.** `DatabaseAccounts` and `DatabaseCharacters` are
   library code, so the tool, the auth program and the world program ask one implementation what an
   account or a character is. A second copy of the SQL in the tool would drift from the schemas it
   describes.
3. **A page, because the state is spread out.** Three databases and the world pack's content files, with a live account
   on one side and a client on the other; a wrong `UPDATE` typed from memory is not a workflow.
4. **Loopback and a token.** The tool binds `127.0.0.1` and allows this machine by default; a `Token` in
   its settings makes it demand one. It has no authorisation model beyond that, which is why it is not
   reachable from anywhere else.

## What it costs

- Two cargo members of its own - `tools/manage`, the panel, and `tools/management-service`, the program
  that serves it - and the rule that keeps them honest: it may read and write data, never behaviour.
  A page that changes what the server *does* belongs in the server, behind a setting.
- One more settings file (`tools/manage/manage.json`), on the same JSON reader as the two programs.
- The EULA version becomes a column (`data/sql/base/db_auth/account_eula.sql`), because a flag an operator
  toggles is state, and state is a row.

## What it does not decide

Where a spawn or an instance record lives: `0014-the-world-content-is-a-pack.md` puts spawns in the
world pack, and instances are rows (`data/sql/base/db_world/0003-instances.sql`). In-game operations act
on a running world, which is the worldserver's business, not a page's: the panel's console page names a
registered operation and the world's loopback control endpoint performs it (`0011`).
