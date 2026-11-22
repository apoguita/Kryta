//! The item message family: the inventory stream a client is given, and the items on it.
//!
//! Reference: `src/server/game/Server/Packets/ItemPackets.{h,cpp}`. The creation screen is the first
//! caller: a client with no inventory model dies drawing the character it has just made, so a
//! creation connection is sent the reference's own bags and items before anything else.
//!
//! Layouts and headers are the working implementation's: `ASURA/crates/asura-protocol/src/game/mod.rs`
//! (`item_stream_create`, `inventory_create_bag`, `create_named_item`, `item_move_to_location`,
//! `item_set_profession`, `item_set_active_weapon_set`, `item_weapon_set`, `update_gold_storage`,
//! `item_remove`), whose sizes come from the reference's message table. Each builder says which one it
//! is.

/// `GAME_SMSG_ITEM_STREAM_CREATE`: opens the item stream the inventory follows on.
pub const ITEM_STREAM_CREATE: u16 = 0x0144;
/// `GAME_SMSG_ITEM_SET_ACTIVE_WEAPON_SET`: which weapon set is in hand.
pub const ITEM_SET_ACTIVE_WEAPON_SET: u16 = 0x0148;
/// `GAME_SMSG_INVENTORY_CREATE_BAG`: one bag of the inventory model.
pub const INVENTORY_CREATE_BAG: u16 = 0x013F;
/// `GAME_SMSG_CREATE_NAMED_ITEM`: one item, with the name and modifiers the client draws.
pub const CREATE_NAMED_ITEM: u16 = 0x0161;
/// `GAME_SMSG_CREATE_UNNAMED_ITEM`: the same item without its name - a bag slot's, or one worn.
pub const CREATE_UNNAMED_ITEM: u16 = 0x015F;
/// `GAME_SMSG_ITEM_LOW_DETAIL`: another player's item, as an introduction carries what they wear.
pub const ITEM_LOW_DETAIL: u16 = 0x015E;
/// `GAME_SMSG_ITEM_MOVED_TO_LOCATION`: where an item sits.
pub const ITEM_MOVE_TO_LOCATION: u16 = 0x013E;
/// `GAME_SMSG_ITEM_SET_PROFESSION`: the profession an item was made for.
pub const ITEM_SET_PROFESSION: u16 = 0x015A;
/// `GAME_SMSG_ITEM_WEAPON_SET`: the items in one weapon slot.
pub const ITEM_WEAPON_SET: u16 = 0x0147;
/// `GAME_SMSG_UPDATE_GOLD_STORAGE`: how much gold is in storage.
pub const UPDATE_GOLD_STORAGE: u16 = 0x0141;
/// `GAME_SMSG_ITEM_REMOVE`: the client drops an item out of its inventory model.
pub const ITEM_REMOVE: u16 = 0x014D;

/// The stream every weapon set, bag and item of a player's own inventory is sent on.
///
/// Evidence: `ASURA/crates/asura-world/src/item.rs` - `ITEM_STREAM`, the one the world uses for the
/// equipped bag and the backpack alike.
pub const ITEM_STREAM: u16 = 1;

/// One item, in the field order `GAME_SMSG_CREATE_NAMED_ITEM` writes.
///
/// `name` and `modifiers` are raw wire values rather than a string and a struct: an item's name is
/// UTF-16 code units that are not necessarily characters, and its modifiers are the client's dwords.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedItem {
    /// The item's own id in the item table.
    pub id: u32,
    /// The `Gw.dat` file id the client draws the model from.
    pub file_id: u32,
    /// The item's kind code; the client groups bags and dyes by it.
    pub item_type: u8,
    /// Which dye slot mask applies; zero for an undyed item.
    pub dye_tint: u8,
    /// The four packed dye colours, four bits each.
    pub dye_colors: u16,
    /// Crafting materials the item carries or needs, as the reference packs them.
    pub materials: u16,
    /// The reference's own unverified byte; copied, not reasoned about.
    pub unk1: u8,
    /// Item flags: identified, customized, equipped and the rest.
    pub flags: u32,
    /// The item's value in gold, as the client shows it.
    pub value: u32,
    /// The model id the client draws, which for some item kinds is not `file_id`.
    pub model_id: u32,
    /// How many are in the stack; 1 for a non-stackable item.
    pub quantity: u32,
    /// The name, as UTF-16 code units, at most 64.
    pub name: Vec<u16>,
    /// The modifier list, at most 64 dwords.
    pub modifiers: Vec<u32>,
}

/// A frame: the `u16` header and then its fields.
fn frame(header: u16, body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&header.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn u8s(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn u16s(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn u32s(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// A counted list of UTF-16 units, cut to the field's capacity.
fn units(out: &mut Vec<u8>, values: &[u16], capacity: usize) {
    let used = &values[..values.len().min(capacity)];
    u16s(out, used.len() as u16);
    for value in used {
        u16s(out, *value);
    }
}

/// A **one-byte** counted list of dwords: the reference's `TYPE_NESTED_STRUCT`, which is the one
/// counted field that does not carry a `u16` count.
fn nested_dwords(out: &mut Vec<u8>, values: &[u32]) {
    let used = &values[..values.len().min(255)];
    u8s(out, used.len() as u8);
    for value in used {
        u32s(out, *value);
    }
}

/// `0x0144`: the item stream opens.
pub fn item_stream_create(stream: u16, is_hero: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    u16s(&mut body, stream);
    u8s(&mut body, is_hero);
    frame(ITEM_STREAM_CREATE, body)
}

/// `0x0148`: which weapon set is in hand.
pub fn item_set_active_weapon_set(stream: u16, slot: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(3);
    u16s(&mut body, stream);
    u8s(&mut body, slot);
    frame(ITEM_SET_ACTIVE_WEAPON_SET, body)
}

/// `0x013F`: one bag of the inventory model.
///
/// A client that gets no bag at all has no inventory to put the items into, so "no bags" is never
/// sent by accident: the reference always creates six.
pub fn inventory_create_bag(
    stream: u16,
    bag_type: u8,
    bag_model_id: u8,
    bag_id: u16,
    slot_count: u8,
    assoc_item_id: u32,
) -> Vec<u8> {
    let mut body = Vec::with_capacity(11);
    u16s(&mut body, stream);
    u8s(&mut body, bag_type);
    u8s(&mut body, bag_model_id);
    u16s(&mut body, bag_id);
    u8s(&mut body, slot_count);
    u32s(&mut body, assoc_item_id);
    frame(INVENTORY_CREATE_BAG, body)
}

/// `0x0161`: one item, with everything a client needs to draw it.
///
/// **Not a fixed size**: the fixed part is 33 bytes after the header, then two counted fields - the
/// name as `u16` units and the modifiers as a **one-byte** count of dwords.
pub fn create_named_item(item: &NamedItem) -> Vec<u8> {
    let mut body = Vec::with_capacity(31 + 2 + item.name.len() * 2 + 1 + item.modifiers.len() * 4);
    u32s(&mut body, item.id);
    u32s(&mut body, item.file_id);
    u8s(&mut body, item.item_type);
    u8s(&mut body, item.dye_tint);
    u16s(&mut body, item.dye_colors);
    u16s(&mut body, item.materials);
    u8s(&mut body, item.unk1);
    u32s(&mut body, item.flags);
    u32s(&mut body, item.value);
    u32s(&mut body, item.model_id);
    u32s(&mut body, item.quantity);
    units(&mut body, &item.name, 64);
    nested_dwords(&mut body, &item.modifiers);
    frame(CREATE_NAMED_ITEM, body)
}

/// `0x013E`: where an item sits, which is what puts it in a bag slot.
pub fn item_move_to_location(stream: u16, item_id: u32, bag_id: u16, slot: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(9);
    u16s(&mut body, stream);
    u32s(&mut body, item_id);
    u16s(&mut body, bag_id);
    u8s(&mut body, slot);
    frame(ITEM_MOVE_TO_LOCATION, body)
}

/// `0x015A`: the profession an item was made for. Only sent when it is not none.
pub fn item_set_profession(item_id: u32, profession: u8) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    u32s(&mut body, item_id);
    u8s(&mut body, profession);
    frame(ITEM_SET_PROFESSION, body)
}

/// `0x0147`: the items in one weapon slot. The reference sends four, one per slot, all empty.
pub fn item_weapon_set(stream: u16, slot: u8, leadhand: u32, offhand: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(11);
    u16s(&mut body, stream);
    u8s(&mut body, slot);
    u32s(&mut body, leadhand);
    u32s(&mut body, offhand);
    frame(ITEM_WEAPON_SET, body)
}

/// `0x0141`: how much gold is in storage.
pub fn update_gold_storage(stream: u16, gold: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(6);
    u16s(&mut body, stream);
    u32s(&mut body, gold);
    frame(UPDATE_GOLD_STORAGE, body)
}

/// `0x014D`: the client drops an item out of its inventory model.
///
/// There is no "this item's quantity changed" message, so changing a stack is this one and the item
/// re-created; and taking a set off before another goes into the same slots is this one too, which is
/// what keeps the client from asserting on an occupied slot.
pub fn item_remove(stream: u16, item_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(6);
    u16s(&mut body, stream);
    u32s(&mut body, item_id);
    frame(ITEM_REMOVE, body)
}

/// `0x015F`: the same item without its name - a bag slot's item, which the client draws from its model.
///
/// The reference sends this one for the items in a bag and for what a player wears
/// (`GameSrv_SendItemsInBag`, `GmItem.c:111-127`, and `GameSrv_BroadcastPlayerEquippedItems`,
/// `GmItem.c:129-160`) and the named one for the inventory stream. The difference is the name: an item
/// sitting in a bag slot is drawn from its model, so the client is not told a name it can already
/// resolve (`ASURA/crates/asura-protocol/src/game/mod.rs:670-688`, `create_unnamed_item`).
pub fn create_unnamed_item(item: &NamedItem) -> Vec<u8> {
    unnamed_item(CREATE_UNNAMED_ITEM, item)
}

/// `0x015E`: another player's item, as retail introduces what that player wears.
///
/// Reference: `ASURA/crates/asura-world/src/instance.rs`, `introduction`, whose `item_low_detail`
/// call is `create_unnamed_item`'s field layout under this header. `pre-searing-load-3.jsonl`
/// seq 222-226 carries one per worn piece between the agent and its visual equipment.
pub fn item_low_detail(item: &NamedItem) -> Vec<u8> {
    unnamed_item(ITEM_LOW_DETAIL, item)
}

/// The unnamed item record both `0x015F` and `0x015E` carry: nine fields, 23 bytes after the header.
fn unnamed_item(header: u16, item: &NamedItem) -> Vec<u8> {
    let mut body = Vec::with_capacity(23);
    u32s(&mut body, item.id);
    u32s(&mut body, item.file_id);
    u8s(&mut body, item.item_type);
    u8s(&mut body, item.dye_tint);
    u16s(&mut body, item.dye_colors);
    u16s(&mut body, item.materials);
    u8s(&mut body, item.unk1);
    u32s(&mut body, item.flags);
    u32s(&mut body, item.value);
    frame(header, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unnamed item is the named one without its two counted fields: 25 bytes on the wire, the size
    /// the table declares (`ASURA/crates/asura-protocol/src/game/mod.rs:676-688`).
    #[test]
    fn an_unnamed_item_is_the_named_one_without_its_counts() {
        let item = NamedItem {
            id: 3,
            file_id: 0x0004_7C34,
            item_type: 15,
            dye_tint: 0,
            dye_colors: 0,
            materials: 0,
            unk1: 0,
            flags: 0,
            value: 25,
            model_id: 22269,
            quantity: 1,
            name: vec![],
            modifiers: vec![],
        };
        let unnamed = create_unnamed_item(&item);
        assert_eq!(&unnamed[..2], &[0x5F, 0x01]);
        assert_eq!(unnamed.len(), 25, "the table's declared size");
        let low = item_low_detail(&item);
        assert_eq!(&low[..2], &[0x5E, 0x01], "the low-detail form");
        assert_eq!(
            &low[2..],
            &unnamed[2..],
            "the same fields under its own header"
        );
        assert_eq!(&unnamed[2..6], &3u32.to_le_bytes(), "the item");
        assert_eq!(&unnamed[6..10], &0x0004_7C34u32.to_le_bytes(), "the file");
        assert_eq!(unnamed[10], 15, "its kind");
        assert_eq!(&unnamed[17..21], &0u32.to_le_bytes(), "its flags");
        assert_eq!(&unnamed[21..25], &25u32.to_le_bytes(), "its value");
        // The named form is the same bytes with the two counted fields after it.
        let named = create_named_item(&item);
        assert_eq!(
            &named[2..25],
            &unnamed[2..25],
            "the same fields, then the name"
        );
        // `33 + 2n + 1 + 4m` after the header: an empty name and no modifiers is 36 bytes.
        assert_eq!(named.len(), 36, "and an empty name, and no modifiers");
    }

    fn sample() -> NamedItem {
        NamedItem {
            id: 2,
            file_id: 0x00047C34,
            item_type: 9,
            dye_tint: 0,
            dye_colors: 0,
            materials: 0,
            unk1: 0,
            flags: 0x20001000,
            value: 5,
            model_id: 22269,
            quantity: 1,
            name: vec![0x21A8, 0xD157],
            modifiers: vec![0x24481400],
        }
    }

    #[test]
    fn the_stream_opens_in_five_bytes() {
        assert_eq!(
            item_stream_create(ITEM_STREAM, 0),
            vec![0x44, 0x01, 1, 0, 0]
        );
    }

    #[test]
    fn a_bag_is_thirteen_bytes() {
        let frame = inventory_create_bag(1, 1, 0, 1, 20, 1);
        assert_eq!(frame.len(), 13, "the reference's own size");
        assert_eq!(&frame[..2], &[0x3F, 0x01]);
    }

    #[test]
    fn a_named_item_counts_its_name_in_units_and_its_modifiers_in_bytes() {
        let frame = create_named_item(&sample());
        // header 2, the fixed 31, a count and two units of name, a count and one modifier dword.
        assert_eq!(frame.len(), 2 + 31 + 2 + 4 + 1 + 4);
        assert_eq!(&frame[..2], &[0x61, 0x01]);
        let modifiers_at = 2 + 31 + 2 + 4;
        assert_eq!(frame[modifiers_at], 1, "one modifier, counted by a byte");
    }

    #[test]
    fn an_item_moved_into_a_slot_names_the_stream_the_item_and_the_slot() {
        assert_eq!(
            item_move_to_location(1, 2, 1, 0),
            vec![0x3E, 0x01, 1, 0, 2, 0, 0, 0, 1, 0, 0]
        );
    }

    #[test]
    fn the_four_empty_weapon_sets_are_thirteen_bytes_each() {
        for slot in 0..4u8 {
            let frame = item_weapon_set(1, slot, 0, 0);
            assert_eq!(frame.len(), 13);
            assert_eq!(&frame[2..4], &1u16.to_le_bytes(), "the stream");
            assert_eq!(frame[4], slot, "the slot");
        }
    }
}
