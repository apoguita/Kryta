-- characters.visibility_flags - the inventory panel's display mode for the cape, the headgear and
-- the two costume slots.
--
-- One byte, two bits per slot kind (the cape 0x03, the headgear 0x0C, the costume body 0x30, the
-- costume head 0xC0): the low bit of a pair is "shown in an explorable area", the high bit "shown
-- in a town or outpost" (`research/Rurik/toolkit/authsrv/visstatus.py`). The client changes it with
-- `0x8057 [value, mask]`, the server answers `0x00EF [value, mask]`, and every load states the
-- whole byte in `0x00EF`.
--
-- Measured on the retail client, 2026-11-24:
-- `recordings/client/raw/creation-armor-in-game-setting-unfiltered.jsonl` seq 482-1164, the
-- headgear's four choices and the server's answer to each.
--
-- NULL means "never chosen": the load then sends `0xFF`, every slot Always Show, which is what the
-- retail server sends a new character
-- (`recordings/client/raw/creation-armor-head-hidden-unfiltered.jsonl` seq 2383).

ALTER TABLE "characters" ADD COLUMN IF NOT EXISTS "visibility_flags" BIGINT NULL;

COMMENT ON COLUMN "characters"."visibility_flags" IS 'The display-mode byte the client last set (0x0057), or NULL for never (every slot Always Show)';
