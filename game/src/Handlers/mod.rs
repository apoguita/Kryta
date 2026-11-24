//! `game/Handlers/` - the reference's `src/server/game/Handlers/` (36 files).
//!
//! One file per message family, and in each one the translation from a client's message to the nouns
//! that own the state: the reference writes these as `WorldSession::HandleXxxOpcode` members, so they
//! are `impl WorldSession` here too, in their own file, calling into `Characters/`, `Agents/`,
//! `Maps/` and the rest. The rule they follow is the guide's (`docs/port/10-separation-guide.md`,
//! section 2): a handler reads the message, asks a noun for the decision, and answers - it does not
//! hold the decision itself.
//!
//! Ported so far, seven families: the creation screen (`CharacterHandler`, whose rows are
//! `DatabaseCharacters` and whose block is `creation::player_data_block`), the item messages
//! (`ItemHandler`, whose decisions are
//! `characters::equip_place` and `DatabaseCharacters::move_item`), the four movement messages
//! (`MovementHandler`, answered by `AgentState`, `pathfind` and `Maps::Portals`), the travel family
//! (`TravelHandler`, answered by the travel row, `Portals` and `place_for_travel`), a spoken line
//! (`ChatHandler`, answered by `game/Chat/`), the three asks of a map load (`InstanceHandler`,
//! whose frames are `agents::Player`'s) and the displayed title (`TitleHandler`, answered by
//! `VanillaTitles`).

#[path = "CharacterHandler.rs"]
pub mod character;

#[path = "ItemHandler.rs"]
pub mod item;

#[path = "MovementHandler.rs"]
pub mod movement;

#[path = "TravelHandler.rs"]
pub mod travel;

#[path = "ChatHandler.rs"]
pub mod chat;

#[path = "InstanceHandler.rs"]
pub mod instance;

#[path = "TitleHandler.rs"]
pub mod title;
