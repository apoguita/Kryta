//! `game/DataStores/` - the reference's `src/server/game/DataStores/` (5 files).
//!
//! What is here now: the item templates - what an item is, once - seeded from the creation record
//! lists at start-up and read back as rows (`Items.rs`, `data/sql/base/db_world/0004-item-templates.sql`).
//! The reference's own `ItemTemplate` store is the same arrangement: one pass at
//! `SetInitialWorldSettings`, then rows in memory.
//!
//! Not ported yet: the reference's other stores (creature templates, quests, the DBC data).

#[path = "Items.rs"]
pub mod items;

#[path = "PvpItems.rs"]
pub mod pvp_items;

pub use items::{DatabaseItems, ItemDefinition, definitions_as_templates};
