-- updates - the record of which schema files a database has already had.
--
-- Reference: data/sql/base/db_auth/updates.sql. Every database has its own copy of this table,
-- because every database applies its own files.
--
-- A row is one file: its name (which is also its order), the sha1 of the bytes that were applied,
-- the state it was applied in, when, and how long it took. The hash is what catches a file that was
-- edited after it ran: a name whose bytes have changed is refused rather than re-applied.
--
-- One difference from the reference's file, and it is deliberate: that file begins with
-- `DROP TABLE IF EXISTS updates`, and this one creates the table only if it is missing. Files are
-- applied in name order and each one is recorded as it runs, so a file that drops this table erases
-- the record of every file applied before it in the same pass - including the file that created it.
-- The history table is the one table whose contents have to survive being visited again.

CREATE TABLE IF NOT EXISTS "updates" (
  "name"      VARCHAR(200) PRIMARY KEY,
  "hash"      CHAR(40)     DEFAULT '',
  "state"     VARCHAR(16)  NOT NULL DEFAULT 'RELEASED'
                           CHECK ("state" IN ('RELEASED', 'CUSTOM', 'MODULE', 'ARCHIVED', 'PENDING')),
  "timestamp" TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "speed"     INTEGER      NOT NULL DEFAULT 0
);

COMMENT ON COLUMN "updates"."name" IS 'filename with extension of the update.';
COMMENT ON COLUMN "updates"."hash" IS 'sha1 hash of the sql file.';
COMMENT ON COLUMN "updates"."state" IS 'defines if an update is released or archived.';
COMMENT ON COLUMN "updates"."timestamp" IS 'timestamp when the query was applied.';
COMMENT ON COLUMN "updates"."speed" IS 'time the query takes to apply in ms.';
COMMENT ON TABLE "updates" IS 'List of all applied updates in this database.';
