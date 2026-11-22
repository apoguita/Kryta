-- A character's skill points, beside the experience and level its row already holds.
--
-- Ported from `ASURA/crates/asura-db/migrations/0036_character_experience.sql`. The progress record
-- (`0x00E9`, fifteen dwords) carries total experience first, level tenth, and the skill points held
-- and earned fourteenth and fifteenth (`game/src/Server/Packets/CharacterPackets.rs`). `experience` and
-- `level` are already columns here (`characters.sql`); the two skill-point counts are not, so the load
-- sent zero for them. This is the state those dwords describe.
--
-- **ASURA's backfill is not carried.** Its migration also rewrites every existing row above level 1 to
-- the least experience its level requires. A schema file here never writes a row (`AGENTS.md`, rule 7,
-- enforced by `worldserver/tests/schema_has_no_content.rs`), so an existing character keeps the
-- experience it has; the next change of experience sets the level the curve gives its total
-- (`docs/port/09-parity-audit.md`, 7e).

ALTER TABLE "characters" ADD COLUMN IF NOT EXISTS "skill_points" BIGINT NOT NULL DEFAULT 0
  CHECK ("skill_points" >= 0);
ALTER TABLE "characters" ADD COLUMN IF NOT EXISTS "skill_points_total" BIGINT NOT NULL DEFAULT 0
  CHECK ("skill_points_total" >= "skill_points");
