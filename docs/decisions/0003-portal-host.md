# 0003 - The auth program serves the portal

Status: accepted, taken to unblock a client test. The owner may overrule it; if he does, this record
is corrected rather than extended.

## The question

The client takes its identity from four HTTP endpoints on `localhost:6601` before it logs in, and the
client cannot send a login message without them. The endpoints exist in this tree as
`game/src/Portal/`, which is where the naming puts that code. Nothing in the naming is a *third
program*, and the reference has two.

## The decision

The auth program hosts the portal: it opens the listener on 6601 beside its own on 6112.

## Why

1. **It is one conversation from the client's side.** The portal's entire job is to hand out the pair
   the auth program then resolves; splitting them puts two halves of one exchange in two processes
   that must agree about a session the client is holding.
2. **A third program costs more than it explains.** It would be a third thing to configure, start,
   supervise and health-check before a single client could log in, and the reference's own shape -
   two programs - would stop being the shape.
3. **The reference already has a program owning a second interface.** Its world program hosts a SOAP
   endpoint inside the game library's world socket layout (`RemoteAccess/`), so "one program, more
   than one interface" is not a new idea in that tree; what is new here is only that the portal's
   interface is HTTP and arrives before login.

## What it costs

`authserver` links `game`, which the reference's dependency graph does not: its auth program links
`shared` and nothing above it. This is the deviation, and it is the reason this record exists.

`game` is a library and nothing in it reaches back into the auth program. Auth also uses game
account and character types, map placement, travel records and the gameplay version parser.
`docs/decisions/0010-auth-and-the-game-library.md` inventories those uses and decides that the edge
stays.

## What it does not decide

- **Who an account is.** The portal's `Accounts` is still supplied by its caller; the portal is told
  who a login name is rather than knowing. What backs it is the login database's `account` table -
  `docs/decisions/0006-accounts-are-rows.md`, which corrects the file this line first named.
- The portal's other two duties in the naming - party and storage over HTTP - which are not written.
