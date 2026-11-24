//! One item's definition: what it is, once, however many of it exist.
//!
//! The record lists a creation screen starts from (`config/World/Vanilla/creation/inventory.json`,
//! `config/World/Vanilla/creation/equipment.json`) carry the whole item, because that is what the
//! client is told about it. A definition is the same fields without the per-instance ones - the
//! runtime id, the bag and the slot. The world builds its definitions from that content at start-up
//! and holds them in memory; nothing is written to a table (`AGENTS.md` rule 7,
//! `docs/port/04-configuration.md`).

use crate::creation::Item;

/// One item's definition, as the pack states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDefinition {
    /// The table's row id.
    pub id: i64,
    /// The `Gw.dat` file id the client draws.
    pub file_id: u32,
    /// The item's kind code.
    pub item_type: u8,
    /// Which dye slot mask applies.
    pub dye_tint: u8,
    /// The four packed dye colours.
    pub dye_colors: u16,
    /// Crafting materials, as GWPS packs them.
    pub materials: u16,
    /// GWPS's own unverified byte.
    pub unk1: u8,
    /// Item flags.
    pub flags: u32,
    /// The item's value in gold.
    pub value: u32,
    /// The model id the client draws.
    pub model_id: u32,
    /// How many are in the stack.
    pub quantity: u32,
    /// The profession the item was made for, or zero for none.
    pub profession: u8,
    /// The name, as UTF-16 code units.
    pub name: Vec<u16>,
    /// The modifier list, as dwords.
    pub modifiers: Vec<u32>,
}

/// The definitions a world runs with, built straight from the creation content's items.
///
/// The pack says what an item is; nothing copies it into a table first. The id is the definition's
/// position, which is only an identity within one start - the client's key for an item is the item's
/// own id, and the row a character holds carries its fields.
pub fn definitions_as_templates(items: Vec<Item>) -> Vec<ItemDefinition> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| ItemDefinition {
            id: index as i64 + 1,
            file_id: item.file_id,
            item_type: item.item_type,
            dye_tint: item.dye_tint,
            dye_colors: item.dye_colors,
            materials: item.materials,
            unk1: item.unk1,
            flags: item.flags,
            value: item.value,
            model_id: item.model_id,
            quantity: item.quantity,
            profession: item.profession,
            name: item.name.clone(),
            modifiers: item.modifiers.clone(),
        })
        .collect()
}
