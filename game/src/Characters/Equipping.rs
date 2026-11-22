//! Where a character's item would be worn: the decision, and nothing else.
//!
//! Reference: `src/server/game/Entities/Player/Player.cpp`, `Player::CanEquipItem` - the question a
//! handler asks before it moves anything. The handler owns the packet, the read and the reply; this
//! owns the rule, takes plain values, and prints nothing (the reference's handler logs its own
//! refusals, and `ItemHandler.cpp:155-265` is where it does so).
//!
//! Guild Wars behaviour: `ASURA/crates/asura-world/src/item.rs:314-436`, `handle_equip_item`, whose
//! first three refusals are this decision - the item must be this character's, a template must
//! describe it, its kind must be one that is worn, and the content must name an equipped bag.

use crate::characters::HeldItem;
use crate::creation::{Bag, WornSlot};
use crate::data_stores::ItemDefinition;

/// Why an item cannot be worn.
///
/// One variant per refusal `ASURA/crates/asura-world/src/item.rs:314-436` makes before it moves
/// anything, so the handler can say the same thing about each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotWorn {
    /// No row of this character has that item id.
    NotTheCharacters,
    /// No template describes it, by row id or by file id.
    NoTemplate,
    /// Its kind is not one that is worn.
    KindNotWorn {
        /// The kind the item states.
        item_type: u8,
    },
    /// The content names no equipped bag, so there is nowhere to wear anything.
    NoEquippedBag,
}

/// The slot an item of this character would be worn in: the equipped bag and the slot its kind
/// occupies in it.
///
/// `held` is the character's own rows and `templates` the world's item table; the equipped bag and the
/// worn-slot table are the caller's, because which bag a character wears things in and which slot a
/// kind occupies both come from the content, not from the item (`config/World/Vanilla/creation/inventory.json`,
/// `config/World/Vanilla/creation/worn_slots.json`).
pub fn equip_place(
    held: &[HeldItem],
    templates: &[ItemDefinition],
    equipped_bag: Option<&Bag>,
    worn: &[WornSlot],
    item_id: u32,
) -> Result<(u16, u8), NotWorn> {
    let Some(row) = held.iter().find(|row| row.item_id == item_id) else {
        return Err(NotWorn::NotTheCharacters);
    };
    // The template by row id, or - for an item written from a file id alone - by the file it draws.
    let template = templates
        .iter()
        .find(|template| row.template_id != 0 && template.id == row.template_id)
        .or_else(|| {
            templates
                .iter()
                .find(|template| template.file_id == row.file_id)
        });
    let Some(template) = template else {
        return Err(NotWorn::NoTemplate);
    };
    let Some(slot) = crate::creation::worn_slot(worn, template.item_type) else {
        return Err(NotWorn::KindNotWorn {
            item_type: template.item_type,
        });
    };
    let Some(bag) = equipped_bag else {
        return Err(NotWorn::NoEquippedBag);
    };
    Ok((bag.id, slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(item_id: u32, template_id: i64, file_id: u32) -> HeldItem {
        HeldItem {
            template_id,
            file_id,
            item_type: 4,
            dye_tint: 0,
            dye_colors: 0,
            materials: 0,
            unk1: 0,
            flags: 0,
            value: 0,
            model_id: 0,
            item_id,
            bag_id: 1,
            slot: 0,
            quantity: 1,
            profession: 0,
            name: Vec::new(),
            modifiers: Vec::new(),
        }
    }

    fn template(id: i64, file_id: u32, item_type: u8) -> ItemDefinition {
        ItemDefinition {
            id,
            file_id,
            item_type,
            dye_tint: 0,
            dye_colors: 0,
            materials: 0,
            unk1: 0,
            flags: 0,
            value: 0,
            model_id: 0,
            quantity: 1,
            profession: 0,
            name: Vec::new(),
            modifiers: Vec::new(),
        }
    }

    fn worn() -> [WornSlot; 1] {
        [WornSlot {
            bag_slot: 5,
            wire_slot: 3,
            name: "boots".to_string(),
            kinds: vec![4],
        }]
    }

    fn bag() -> Bag {
        Bag {
            id: 7,
            model: 0,
            // `BagType_Equipped`, the kind `creation::equipped_bag` looks for.
            kind: 2,
            slots: 9,
            assoc_item: 0,
        }
    }

    #[test]
    fn an_item_the_character_does_not_hold_is_not_worn() {
        let rows = [held(1, 10, 100)];
        assert_eq!(
            equip_place(&rows, &[template(10, 100, 4)], Some(&bag()), &worn(), 2),
            Err(NotWorn::NotTheCharacters)
        );
    }

    #[test]
    fn a_row_id_names_the_template_and_a_file_id_answers_when_it_does_not() {
        let rows = [held(1, 10, 100), held(2, 0, 200)];
        let templates = [template(10, 100, 4), template(11, 200, 4)];
        assert_eq!(
            equip_place(&rows, &templates, Some(&bag()), &worn(), 1),
            Ok((
                7,
                crate::creation::worn_slot(&worn(), 4).expect("a worn kind")
            )),
            "the template its row id names"
        );
        assert_eq!(
            equip_place(&rows, &templates, Some(&bag()), &worn(), 2),
            Ok((
                7,
                crate::creation::worn_slot(&worn(), 4).expect("a worn kind")
            )),
            "and the one its file id names, for a row written from a file alone"
        );
    }

    #[test]
    fn a_kind_that_is_not_worn_says_which_kind_it_is() {
        let rows = [held(1, 10, 100)];
        assert_eq!(
            equip_place(&rows, &[template(10, 100, 200)], Some(&bag()), &worn(), 1),
            Err(NotWorn::KindNotWorn { item_type: 200 })
        );
    }

    #[test]
    fn no_equipped_bag_means_nowhere_to_wear_it() {
        let rows = [held(1, 10, 100)];
        assert_eq!(
            equip_place(&rows, &[template(10, 100, 4)], None, &worn(), 1),
            Err(NotWorn::NoEquippedBag)
        );
    }

    #[test]
    fn no_template_at_all_is_its_own_refusal() {
        let rows = [held(1, 10, 100)];
        assert_eq!(
            equip_place(&rows, &[], Some(&bag()), &worn(), 1),
            Err(NotWorn::NoTemplate)
        );
    }
}
