-- Guilds: the guild, its members, and its open invitations.
--
-- Ported from `ASURA/crates/asura-db/migrations/0029_guilds.sql`, column for column. What is stored is
-- what a guild's own messages carry and play changes: the name and tag, the cape, the standing numbers
-- `0x0121` sends (allegiance, faction, qualifier, rating, rank and the two bytes whose meaning is not
-- established), the announcement and who set it, and the counter `0x0133` sends once per change. What a
-- new guild starts with is `config/Server/guilds.json`; the row is written from it once, at creation.
--
-- Three differences of this tree's own, and no more: the ids are the `uuid` text every character table
-- here is keyed by (`characters.sql`); every number is `BIGINT` like every other number in this
-- database, because the worker pool binds one integer type (the ranges ASURA checks are kept as
-- checks); and the character columns carry no foreign key, as `character_unlock` and `item_instance`
-- carry none, because this file sorts before `characters.sql`. ASURA's `ON DELETE CASCADE` from the
-- character is `DatabaseCharacters::remove`'s two statements instead.
--
-- THE KEY IS THE ID. Every guild message names the guild by a sixteen-byte key (`0x0118`, `0x0121`).
-- ONE GUILD PER CHARACTER is the primary key of the member table. An invitation is not a membership, so
-- invitations are their own table and a character may hold several.
-- MEMBER TYPE is the client's number (`0x0127`'s last byte, `0x0118`'s role): 5 the leader.

CREATE TABLE IF NOT EXISTS "guilds" (
  "id"                  VARCHAR(36)  PRIMARY KEY,
  "name"                VARCHAR(64)  NOT NULL,
  "tag"                 VARCHAR(16)  NOT NULL,
  "feature"             BIGINT       NOT NULL DEFAULT 0 CHECK ("feature" BETWEEN 0 AND 255),
  "territory"           BIGINT       NOT NULL DEFAULT 0 CHECK ("territory" BETWEEN 0 AND 255),
  "cape_background"     BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_background" BETWEEN 0 AND 255),
  "cape_detail_color"   BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_detail_color" BETWEEN 0 AND 255),
  "cape_emblem_color"   BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_emblem_color" BETWEEN 0 AND 255),
  "cape_shape"          BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_shape" BETWEEN 0 AND 255),
  "cape_detail"         BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_detail" BETWEEN 0 AND 255),
  "cape_emblem"         BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_emblem" BETWEEN 0 AND 65535),
  "cape_trim"           BIGINT       NOT NULL DEFAULT 0 CHECK ("cape_trim" BETWEEN 0 AND 255),
  "allegiance"          BIGINT       NOT NULL DEFAULT 0 CHECK ("allegiance" BETWEEN 0 AND 255),
  "faction"             BIGINT       NOT NULL DEFAULT 0 CHECK ("faction" BETWEEN 0 AND 4294967295),
  "qualifier"           BIGINT       NOT NULL DEFAULT 0 CHECK ("qualifier" BETWEEN 0 AND 4294967295),
  "rating"              BIGINT       NOT NULL DEFAULT 0 CHECK ("rating" BETWEEN 0 AND 4294967295),
  "rank"                BIGINT       NOT NULL DEFAULT 0 CHECK ("rank" BETWEEN 0 AND 4294967295),
  "announcement"        TEXT         NOT NULL DEFAULT '',
  "announcement_author" VARCHAR(64)  NOT NULL DEFAULT '',
  "revision"            BIGINT       NOT NULL DEFAULT 0 CHECK ("revision" BETWEEN 0 AND 4294967295),
  "created_at"          TIMESTAMPTZ  NOT NULL DEFAULT now()
);

COMMENT ON TABLE "guilds" IS 'A guild, written when a character founds one';

-- Names are unique whatever their case; tags are not unique (the game reuses them).
CREATE UNIQUE INDEX IF NOT EXISTS "guilds_name_key" ON "guilds" (lower("name"));

CREATE TABLE IF NOT EXISTS "guild_members" (
  "character_uuid" VARCHAR(36)  PRIMARY KEY,
  "guild_id"       VARCHAR(36)  NOT NULL REFERENCES "guilds" ("id") ON DELETE CASCADE,
  "member_type"    BIGINT       NOT NULL CHECK ("member_type" BETWEEN 0 AND 255),
  -- `0x0127`'s first and third names: the name it was invited under, and who invited it.
  "invited_name"   VARCHAR(64)  NOT NULL,
  "invited_by"     VARCHAR(64)  NOT NULL,
  "joined_at"      TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS "guild_members_guild" ON "guild_members" ("guild_id");

CREATE TABLE IF NOT EXISTS "guild_invites" (
  "guild_id"       VARCHAR(36)  NOT NULL REFERENCES "guilds" ("id") ON DELETE CASCADE,
  "character_uuid" VARCHAR(36)  NOT NULL,
  "invited_name"   VARCHAR(64)  NOT NULL,
  "invited_by"     VARCHAR(64)  NOT NULL,
  "invited_at"     TIMESTAMPTZ  NOT NULL DEFAULT now(),
  PRIMARY KEY ("guild_id", "character_uuid")
);
