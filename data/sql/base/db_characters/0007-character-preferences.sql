-- characters.preferences - the character's own preferences blob, kept so every load hands it back.
--
-- The client keeps a few settings per character (the four weapon-set checks, the spend-attributes
-- button; `research/Eden/docs/re/options-save.md`). When one changes it sends the whole blob to the
-- game server in `0x0045` (wire `0x8045`: a `u16` count, then that many bytes, at most 256), and the
-- server answers nothing. Every map load then carries the blob back in
-- `GAME_SMSG_PREFERENCES_BLOB` (`0x00B5`), in the same form, directly before
-- `GAME_SMSG_UPDATE_UNLOCKED_SKILLS` (`0x00DB`).
--
-- Measured on the retail client, 2026-11-24:
--   * `recordings/client/feature/preferences/weapon-sets-toggle-20261124.jsonl` seq 476 and 696: two uploads of 50 bytes
--     that differ in one byte (offset 15, `00` with the weapon sets unticked, `3c` with them ticked),
--     and no message from the server in answer to either;
--   * `recordings/client/feature/preferences/weapon-sets-travel-20261124.jsonl` seq 466 and 1608: an upload with
--     `24` at offset 15, and the next load's `0x00B5` carrying the same 50 bytes.
-- The same message in a whole login is `recordings/client/session/login-to-spawn-20261124-180308.jsonl`
-- seq 1253.
--
-- No research project implements this store. NULL means "never uploaded": the load then sends no
-- `0x00B5`, which is what this tree did before the column existed. What the retail server sends a
-- character that has never uploaded is not recorded (`docs/port/09-parity-audit.md`, row `0x00B5`).
-- The bytes are the client's and are never decoded here.

ALTER TABLE "characters" ADD COLUMN IF NOT EXISTS "preferences" BYTEA NULL;

COMMENT ON COLUMN "characters"."preferences" IS 'The per-character preferences blob the client last uploaded (0x0045), or NULL for never';
