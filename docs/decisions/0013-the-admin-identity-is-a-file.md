# 0013 - The administrator identity is a file, and it is never locked out

Status: accepted, on the owner's instruction, 2026-11-19, and confirmed 2026-11-23: there is one
administrator, held in a file, who cannot be locked out by the database, and every other user is a row.
It decides where the operator's own identity lives. **It is not implemented**: the manager has no
login, and no hash library is chosen. `config/Server/admin.json`, which the login program reads to
insert a first *game account* into an empty `account` table (`authserver/src/main.rs`,
`AdminAccountFile`), is not this identity - that is a client's account, and it is a row. The owner may
correct this record; a correction replaces this text rather than being appended beside it.

## The question

The selected direction (`docs/port/14-operator-surfaces.md`, section 4) wants an initial `admin` account
with password `admin`, kept in a server-restricted file, and administrators able to change passwords.
Where does that identity live: a row in the login database beside the players' accounts, or a file?

## The decision

**A file. The administrator's identity never lives in a database row.**

The owner's reason, in the owner's terms: an administrator must never be locked out of access. A row is
one more thing that can be lost, emptied by a migration, made unreachable by a database that will not
start, or edited by someone who does not understand it - and the person it locks out is the one who owns
the machine. A file protected by the operating system can be corrected by that same person with the same
privileges they used to install the server.

Concretely:

- The administrator identity is **name, a salted password hash, and whether the password must change**,
  in a file the manager reads at start-up. It is created on first initialization; the first password is
  `admin` and must be changed before remote access is enabled.
- It is **not** an `account` row (`0006-accounts-are-rows.md` describes those rows as the clients'
  accounts) and **not** an `account_access` row. The two databases being down does not stop an operator
  logging into the manager of a server that is up.
- The file is **protected as this tree already protects its world control credential**
  (`game/src/Server/RemoteAccess/mod.rs`, `verify_permissions`): a regular file, not a symlink, readable
  only by the service user, SYSTEM and administrators, checked on every load, and a check that fails
  refuses to serve rather than serving insecurely.
- The password is **changed through the manager**, which rewrites the file under the same protection.
  Nothing else writes it. There is no "reset by editing the database" path to document because there is
  no database path at all.

## Why this is not a second account system

It is one identity, used by one surface: the web manager's login (`14`, sections 5 and 8) and, later, the
link from a game account to an operator identity for in-game operator features. The clients' accounts
stay where `0006` puts them, in rows the login reads. The two meet only through that link, and the link's
field is on the administrator side - a game account does not become an administrator because a row says
so, and an administrator is not a row.

## What it does not decide

- **The hash.** "Store salted password hashes rather than plaintext passwords" is the direction; the
  algorithm and its crate are an implementation decision to record when the login is built, because this
  tree has no hashing library yet and inventing one is worse than choosing a maintained one. The file's
  format will carry the algorithm's name so a later change can be read from the file itself.
- **Sessions, cookies, CSRF, throttling, HTTPS.** `14`, section 8, owns them and they are unchanged by
  this record.
- **Recovery beyond the file.** A person with the machine's privileges can replace or remove the file and
  the manager bootstraps again - that is the recovery story, and it should be written down when the login
  lands.
- **Where the file sits.** A path named by the manager's own settings file, as `04-configuration.md`
  requires of a setting; the sensible default is beside `manage.json` with the OS permissions above.

## How the claim gets proved

1. First start with no file: the manager creates one holding `admin` and "must change", refuses remote
   access until the password changes, and says so.
2. A wrong password is refused; a correct one opens the manager; a changed password is what the file now
   holds, and the old one no longer opens it.
3. The file with permissive permissions is refused with a reason, and the manager does not serve.
4. With both databases stopped, an administrator can still log into the manager.
