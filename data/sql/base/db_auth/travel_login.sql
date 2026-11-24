-- A travel's login: which login of the account this hand-off belongs to.
--
-- Only one login per account is ever permitted, and a new login closes any old connection. The login
-- program closes the login connection it displaced; the world has to close the displaced client's
-- gameplay connections, and the two programs do not talk to each other. So every login draws a
-- number - a later login a larger one - and every hand-off of that login carries it: the rows the
-- login program writes, and the rows the world writes when the same client travels on. A gameplay
-- connection then arrives knowing which login it belongs to (`game/src/Accounts/Logins.rs`).
--
-- 0 is a row written by a program that draws no login numbers; the world leaves such a connection
-- out of the rule.
ALTER TABLE "travel" ADD COLUMN IF NOT EXISTS "login" BIGINT NOT NULL DEFAULT 0;
COMMENT ON COLUMN "travel"."login" IS 'The number of the login this hand-off belongs to; a later login of the account has a larger one. 0 states none';
