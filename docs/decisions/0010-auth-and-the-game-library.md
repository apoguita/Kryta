# 0010 - Auth links the game library, and what that edge is made of

Status: accepted as a finding. It takes no code change on its own; it says what the edge is, which of
its parts both authorities agree with, and the one part they do not - so that a later move is a
decision and not a surprise. The owner may overrule it; if he does, this record is corrected rather
than extended.

## The question

`authserver` depends on `game` (`authserver/Cargo.toml`). The reference's auth program does not: its
`src/server/apps/CMakeLists.txt:130-139` links the login program to `shared`, and links `game`,
`scripts` and `modules` to the world program only. `docs/port/10-separation-guide.md` section 1 asks
for the inventory of every such use, a comparison with the reference and with the conversation ASURA
answers, and a destination for each piece.

## The inventory

Every use of `game` in the auth program, by what it is for. `game::` appears 38 times across six
files; the categories below are the whole of it, and each one is named by the first place it appears.

| what auth uses | where | is, in ASURA's layering |
|---|---|---|
| the portal: `game::portal::{Portal, serve::PortalSession, Account, Accounts}` | `main.rs:420-427`, `Server/accounts.rs:19` | a service of its own, hosted by the login program by `0003-portal-host.md` |
| the gameplay version message: `game::server::protocol::{GameVersion, GAME_CMSG_VERSION_LEN}` | `Server/front_door.rs:24`, `Server/session.rs:2139-2150` | the **protocol** layer (`asura-protocol`), which both services link |
| the character's rows: `game::characters::{DatabaseCharacters, uuid_text, CHARACTER_SETTINGS_LEN}` | `Server/characters.rs:8`, `Server/session.rs:557,652,794,810,951,988,1016` | the **store** layer (`asura-store`), which both services link |
| the travel hand-off row: `game::maps::{DatabaseTravels, Travel}` | `main.rs:301`, `Server/session.rs:110,1168-1179` | the **store** layer: a row the world claims |
| the account table: `game::accounts::DatabaseAccounts` | `main.rs:356`, `Server/session.rs:113,172` | the login's own table, read through a game noun |
| where a character enters: `game::maps::placement::{place, Requested, Character, Substituted, MAP_KINDS}` | `Server/session.rs:1075-1111` | the **rules** layer (`asura-rules`), a pure function |

The auth program opens no world database (`authserver.json` names the login and character databases
only), so nothing here reads the game's content: every use is either a protocol message, a row, a pure
rule, or the portal.

## What ASURA decided about exactly this

Its own accepted decision, `ASURA/docs/reference/decisions/layer-decisions.md` D25, is the owner's
rule from 2026-11-03:

> A service does only its own job. The auth service answers the protocol questions; a **game rule**
> that a protocol answer happens to need lives in `crates/asura-rules` as a pure function, and the
> service supplies the data.

and its reason, in the owner's words: "auth must be auth, if features beyond auth are happening, they
are on the wrong layer, we can create a new layer or use another one". The decision was taken because
the login's travel answer had grown the last-outpost rule, the creation exemption and the default
outpost - none of which are protocol - and because `asura-world` may not exist yet when the login
decides. D26 is its other half: the socket layer is its own crate (`asura-gateway`), which is what this
tree's `authserver/src/Server/front_door.rs` is, and what
`0004-gameplay-connections-arrive-at-the-front-door.md` decided.

Measured against that, `crates/asura-auth/Cargo.toml` depends on `asura-protocol`, `asura-db`,
`asura-store`, `asura-rules` and `asura-config`, and on `asura-world` **not at all**.

## The decision

**This tree keeps the edge, and it is bounded to the six categories above.** The reasons:

1. **The reference's chain has no layer for four of the six.** Its per-table repositories are game
   nouns (`game/Characters/`, `game/Maps/`), its protocol is `shared`, and it has no rules crate at all.
   A login that must compose a character list and decide a map - which AzerothCore's authserver never
   does, because in AzerothCore the world answers the character list - reaches those nouns by the
   reference's own placement of them.
2. **The alternative that stays inside the reference's chain blurs it.** Moving `placement` into
   `shared` would make the library both programs link carry a game rule, which blurs the separation
   `AGENTS.md` names as the deliverable ("Why the separation is the point"). Keeping it in `game`
   keeps the rule where the rule's nouns are.
3. **The edge is one-way and already checked.** Nothing in `game` names `authserver`; the crate graph
   in `Cargo.toml` refuses it, so the compiler holds it.

**Why D25 does not decide it here.** `AGENTS.md` gives the reference the design and gives ASURA the
conversation: "What it is *not* the authority for is this tree's structure - where a file goes, what a
program starts, how a layer is arranged - which stays the reference's", and ASURA's documents
"describe its own function; they are not rules here". The reference has no rules layer and keeps its
repositories as nouns, so neither the layer nor the store is created here. D25 is the evidence that the
conversation needs the rule - and the reason ASURA needed a layer for it, which was that its
`asura-world` may not exist when its login decides; in this tree the rule's nouns are in `game` whether
or not a world has started, so the same need is already met by the edge.

A change of mind is a change of authority, not a code edit: this record would have to be corrected
first, and it says "no" because the reference does.

**What it would not be.** It would not move the portal (`0003` decides its host), the store nouns
(they are nouns by the reference's arrangement), the protocol message (that is `shared`'s subject and a
separate move), or the front door (`0004`).

## What it does not decide

- The auth program's own conversation: what it answers, and in what order, is the parity ledger's
  subject (`docs/port/09-parity-audit.md`), and nothing here changes it.
