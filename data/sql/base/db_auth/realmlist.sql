-- realmlist - the worlds a login may be sent to.
--
-- Reference: data/sql/base/db_auth/realmlist.sql, whose columns are reproduced here with
-- PostgreSQL's types and quoted exactly as the reference spells them. The table is the one piece of
-- the reference's auth schema that transfers to the world list unchanged: a world has an id, a
-- name, an address, a port and a flag, in both games.
--
-- Reference seed row: ('AzerothCore', '127.0.0.1', '127.0.0.1', '255.255.255.0', 8085, 0, 2, 1, 0,
-- 0, 12340). Kryta's own seed is seeded by the operator, not by this file: a world row names a
-- machine.

DROP TABLE IF EXISTS "realmlist";

CREATE TABLE "realmlist" (
  "id"                   SERIAL PRIMARY KEY,
  "name"                 VARCHAR(32)  NOT NULL DEFAULT '',
  "address"              VARCHAR(255) NOT NULL DEFAULT '127.0.0.1',
  "localAddress"         VARCHAR(255) NOT NULL DEFAULT '127.0.0.1',
  "localSubnetMask"      VARCHAR(255) NOT NULL DEFAULT '255.255.255.0',
  "port"                 SMALLINT     NOT NULL DEFAULT 8085,
  "icon"                 SMALLINT     NOT NULL DEFAULT 0,
  "flag"                 SMALLINT     NOT NULL DEFAULT 2,
  "timezone"             SMALLINT     NOT NULL DEFAULT 0,
  "allowedSecurityLevel" SMALLINT     NOT NULL DEFAULT 0,
  "population"           REAL         NOT NULL DEFAULT 0 CHECK ("population" >= 0),
  "gamebuild"            INTEGER      NOT NULL DEFAULT 12340,
  CONSTRAINT "idx_name" UNIQUE ("name")
);

COMMENT ON TABLE "realmlist" IS 'Realm System';
