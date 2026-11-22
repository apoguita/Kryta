-- account.settings - the client's own UI settings blob, kept so the next login is answered with it.
--
-- The client uploads its interface state (window positions, visibility and the other UI options) on the
-- auth connection in two messages that share one request id: `AUTH_CMSG_SETTING_UPDATE_SIZE` (0x8021)
-- announces the total, then one or more `AUTH_CMSG_SETTING_UPDATE_CONTENT` (0x8020) carry it in chunks
-- of at most 512 bytes. The login's `AUTH_SMSG_ACCOUNT_SETTINGS` (0x0016) is answered from this column.
--
-- Until this column existed every login was answered with the one 591-byte block in
-- `authserver/src/Server/account_settings.rs`, so the client's upload was acknowledged and thrown away
-- and every session started from the same layout. ASURA keeps the same blob for the same reason
-- (`ASURA/crates/asura-db/migrations/0032_account_settings.sql` at `071735e`; the measurement is
-- `docs/evidence/ui-settings-persistence.md`).
--
-- NULL means "never uploaded": the login falls back to the committed block, which is what a new account
-- gets. The bytes are the client's and are never decoded here.
--
-- Added as its own file rather than by editing `account.sql`: the updater records a file by name and
-- sha1 and refuses a file that changed, so a schema grows by a new file.

ALTER TABLE "account" ADD COLUMN IF NOT EXISTS "settings" BYTEA;

COMMENT ON COLUMN "account"."settings" IS 'The client UI settings blob it last uploaded, or NULL for never';
