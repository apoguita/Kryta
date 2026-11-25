-- account.character_slots - how many character slots this account has.
--
-- The client is told an account's slots in the login's `AUTH_SMSG_ACCOUNT_INFO` (0x0011), as the count
-- of account feature 1, to which it adds 2 (`research/Eden/docs/re/char-select.md:316-317`). The
-- research servers send a fixed count of 4, six slots, for every account
-- (`research/OpenTyria/code/OpenTyria/AuthSrv.c:515`). Here the number is the account's own, from 2
-- to 32, and every account states it (owner, 2026-11-24: "the character cap is per account, each
-- account can have different number of slots"; "all accounts are created with 6 slots").
--
-- There is no number beside the account's: an account is created with 6, the research servers' own,
-- and it is changed on the account.
--
-- Added as its own file rather than by editing `account.sql`: the updater records a file by name and
-- sha1 and refuses a file that changed, so a schema grows by a new file.

ALTER TABLE "account" ADD COLUMN IF NOT EXISTS "character_slots" INTEGER NOT NULL DEFAULT 6
  CHECK ("character_slots" BETWEEN 2 AND 32);

COMMENT ON COLUMN "account"."character_slots" IS 'The character slots this account has, 2 to 32; an account is created with 6';
