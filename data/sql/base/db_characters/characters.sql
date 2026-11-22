-- characters - a person's character, as this game makes one.
--
-- This is the first table of the character database, and it is **this tree's design, not a port**: the
-- reference's `characters` table is Azeroth's, and its columns describe a WoW character - race, class,
-- the packed appearance of a humanoid, bags, spells, talents. Tyria's character has a profession pair, a
-- campaign, an outpost it stands in, and armour pieces; the fields below are the ones the messages this
-- tree already sends actually read (docs/decisions/0007-character-schema.md).
--
-- What is deliberately *not* here: skills and their unlocked bitmap, the four completion bitmaps, the
-- per-slot armour file ids and dyes as separate columns, hero flags, inventory. Each needs a message that
-- reads it, and a column nobody loads is the same liberty in a new place. They arrive with their message.

CREATE TABLE IF NOT EXISTS "characters" (
  "id"                    SERIAL PRIMARY KEY,
  -- The client's own identifier for the character, as the little-endian uuid text of 16 wire bytes.
  "uuid"                  VARCHAR(36)  NOT NULL,
  -- The account this character belongs to, as the portal's own uuid for it - which is the identifier a
  -- gameplay connection already carries in its version message. Keeping the portal's uuid rather than a
  -- login-database integer is what lets the world write a character without a second database
  -- connection, and lets the login list them without a cross-database join.
  "account_uuid"          VARCHAR(36)  NOT NULL,
  "name"                  VARCHAR(64)  NOT NULL,
  -- The packed appearance word: sex 1 bit, height 4, skin 5, hair colour 5, face 5, profession 4,
  -- hair style 6, race 2. Stored packed exactly as the confirm carries it, so nothing repacks it.
  "appearance"            BIGINT            NOT NULL DEFAULT 0,
  "primary_profession"    BIGINT          NOT NULL DEFAULT 0,
  "secondary_profession"  BIGINT          NOT NULL DEFAULT 0,
  -- One bit per profession the character may use: `1 << primary_profession` at creation.
  "unlocked_professions"  BIGINT           NOT NULL DEFAULT 0,
  -- 0 none (a PvP character), 1 Prophecies, 2 Factions, 3 Nightfall, 4 Eye of the North.
  "campaign"              BIGINT          NOT NULL DEFAULT 0,
  -- The campaigns this character has finished. Hard mode is unlocked by a completed campaign, not by
  -- the outpost: the client's normal/hard toggle follows `0x0070`, which this row decides.
  "prophecies_finished"   BOOLEAN      NOT NULL DEFAULT FALSE,
  "factions_finished"     BOOLEAN      NOT NULL DEFAULT FALSE,
  "nightfall_finished"    BOOLEAN      NOT NULL DEFAULT FALSE,
  "level"                 BIGINT          NOT NULL DEFAULT 1,
  "is_pvp"                BOOLEAN      NOT NULL DEFAULT FALSE,
  -- 0 none, 1 show, 2 hide - the two bits the character-list blob carries.
  "helm_status"           BIGINT          NOT NULL DEFAULT 1,
  "gold"                  BIGINT           NOT NULL DEFAULT 0,
  -- The outpost the character stands in; 0 for a character that has not been placed.
  "last_outpost"          BIGINT           NOT NULL DEFAULT 0,
  "experience"            BIGINT           NOT NULL DEFAULT 0,
  "created_at"            TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "idx_characters_uuid" UNIQUE ("uuid"),
  CONSTRAINT "idx_characters_name" UNIQUE ("name")
);

COMMENT ON TABLE "characters" IS 'A character, as this game makes one';

CREATE INDEX IF NOT EXISTS "characters_by_account" ON "characters" ("account_uuid");
