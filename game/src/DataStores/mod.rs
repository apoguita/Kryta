//! `game/DataStores/` - the reference's `src/server/game/DataStores/` (5 files).
//!
//! What is here now: the item definitions - what an item is, once - built from the world pack's
//! creation content at start-up and held in memory (`Items.rs`), and the unlockable PvP item table
//! the load sends (`PvpItems.rs`). The reference's own `ItemTemplate` store is read once at
//! `SetInitialWorldSettings` and then held in memory; this is that pass, from files instead of rows.
//!
//! Not ported yet: the reference's other stores (creature templates, quests, the DBC data).

#[path = "Items.rs"]
pub mod items;

#[path = "PvpItems.rs"]
pub mod pvp_items;

pub use items::{definitions_as_templates, ItemDefinition};
