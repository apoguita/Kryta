-- Which campaigns a character has finished. Hard mode is a completion reward, not a map property:
-- completing Prophecies, Factions or Nightfall unlocks hard mode for that character, and the client's
-- party window draws its normal/hard toggle from `0x0070` only when one of these is true.

ALTER TABLE "characters"
  ADD COLUMN IF NOT EXISTS "prophecies_finished" BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS "factions_finished"   BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS "nightfall_finished"  BOOLEAN NOT NULL DEFAULT FALSE;
