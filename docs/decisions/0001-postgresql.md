# 0001 - PostgreSQL, not MySQL

Status: accepted, 2026-11-09, on the owner's word: "the reference of azerothcore will change on
tyria and we will use posgre ... they are only databases, there differences in DB motors but
basically they provide the same functionality".

## The question

The reference talks to MySQL/MariaDB. This port needs one engine, and the choice decides how much
of the reference's database layer translates literally.

## What the reference does, measured

`src/server/database/` holds 49 files. Eight name the engine:

```
Database/MySQLConnection.{h,cpp}
Database/MySQLPreparedStatement.{h,cpp}
Database/MySQLThreading.{h,cpp}
Database/MySQLHacks.h
Database/MySQLWorkaround.h
```

The other 41 are the arrangement: `DatabaseEnv`, `DatabaseLoader`, `DatabaseWorkerPool`,
`DatabaseWorker`, `Field`, `QueryResult`, `PreparedStatement`, `Transaction`, `AdhocStatement`,
`QueryHolder`, `QueryCallback`, `SQLOperation`, the three logical databases under `Implementation/`,
and `Updater/{DBUpdater,UpdateFetcher}`. None of those names an engine in its interface.

## The decision

PostgreSQL.

## Why

1. **The schema is Kryta's own.** The reference's dialect advantage only pays if its SQL is
   imported, and it is not: its `data/sql/` tree is Azeroth's schema, which does not cross this
   project. Every table here describes Tyria's nouns, so it is written once, in one dialect. There
   is no body of MySQL SQL to translate.
2. **It runs on this machine today.** PostgreSQL listens on 5432 here and MySQL or MariaDB is
   installed nowhere, so this engine is the one whose behaviour can be run and proven while the
   layer is written. An install step between the code and its proof is a step that gets skipped.
3. **One pool per program, several logical databases.** Concurrent connections with MVCC suit that
   shape better than a table-locking engine, and the reference's three logical databases are read
   and written at the same time by two programs.
4. **The failure modes are already known here.** This server's operational state ran on
   PostgreSQL before, so its locking, its connection loss and its bulk-write behaviour are a
   familiar set of problems rather than a new one.

None of this says MySQL is worse. It says the reasons to keep MySQL are about importing SQL this
project does not import, and the reasons to use PostgreSQL are about the machine and the schema
this project actually has.

## What this changes in the port

- The eight MySQL files become the PostgreSQL equivalents. That is the whole of the engine
  difference inside the layer.
- The updater reads `.sql` files written for PostgreSQL, and the schema version table is its own.
- The connection settings: the reference's `LoginDatabaseInfo`, `CharacterDatabaseInfo` and
  `WorldDatabaseInfo` keys keep their names and their role, and their values change shape.
- Prepared statements keep the reference's named form (`PreparedStatement` with a name, an index
  and a bound-parameter list); the placeholder spelling inside the SQL changes from `?` to `$1`.

## What this does not change

The arrangement, which is the part worth getting right: `DatabaseEnv`, one pool per program, one
pool per logical database, prepared statements, transactions, result sets with typed fields, the
worker pool with its callback queue, and the updater with its schema version. The reference's
structure is this port's subject; only the wire to the engine differs.

## Still open

**The pool's shape.** The reference is thread-based: a `DatabaseWorkerPool` owns N worker threads,
each with its own blocking connection, and callers hand work over a callback queue. Rust could go
async instead, and that would change the arrangement rather than the vendor. The rule in `AGENTS.md`
is that the reference decides design, so the default is to keep the threads and the queue with a
blocking client, and this is decided when the layer is written - not now, and not by accident.
