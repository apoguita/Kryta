//! The game's own names for the numbers a row stores: campaigns, professions, the appearance word,
//! an outpost's name. Used by every area that shows a character or a map.

use std::collections::HashMap;

/// The campaign a character was made in, as the client and the row's own comment name it.
/// `characters.campaign`: 0 none (a PvP character), 1 Prophecies, 2 Factions, 3 Nightfall,
/// 4 Eye of the North (`data/sql/base/db_characters/characters.sql`).
pub const CAMPAIGNS: [(u8, &str); 5] = [
    (0, "None (PvP)"),
    (1, "Prophecies"),
    (2, "Factions"),
    (3, "Nightfall"),
    (4, "Eye of the North"),
];

/// The professions, in the client's own numbering.
pub const PROFESSIONS: [(u8, &str); 11] = [
    (0, "None"),
    (1, "Warrior"),
    (2, "Ranger"),
    (3, "Monk"),
    (4, "Necromancer"),
    (5, "Mesmer"),
    (6, "Elementalist"),
    (7, "Assassin"),
    (8, "Ritualist"),
    (9, "Paragon"),
    (10, "Dervish"),
];

/// The name of a campaign, or `Unknown` for a number this tree has no name for.
pub fn campaign_label(value: u8) -> &'static str {
    CAMPAIGNS
        .iter()
        .find(|(id, _)| *id == value)
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}

/// The name of a profession, or `Unknown` for a number this tree has no name for.
pub fn profession_label(value: u8) -> &'static str {
    PROFESSIONS
        .iter()
        .find(|(id, _)| *id == value)
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}

/// A character's profession pair, as the character page and the roster read it.
pub fn professions(primary: u8, secondary: u8) -> String {
    if secondary == 0 {
        profession_label(primary).to_string()
    } else {
        format!(
            "{} / {}",
            profession_label(primary),
            profession_label(secondary)
        )
    }
}

/// The primary profession, packed in the appearance word, bits 20-23 (`docs/game-rules.md` E74).
pub fn primary_profession(word: u32) -> u8 {
    ((word >> 20) & 0xF) as u8
}

/// Where a character is, as the roster lists it: the outpost's name, its id, or "not placed".
pub fn outpost_label(map_id: u32, names: &HashMap<u32, String>) -> String {
    if map_id == 0 {
        return "not placed".to_string();
    }
    match names.get(&map_id) {
        Some(name) => format!("{name} ({map_id})"),
        None => format!("map {map_id}"),
    }
}

/// The fields packed into the appearance word, in the order the client packs them
/// (`docs/game-rules.md` E74).
pub fn appearance_fields(word: u32) -> [(&'static str, u32); 6] {
    [
        ("sex", word & 0x1),
        ("height", (word >> 1) & 0xF),
        ("skin", (word >> 5) & 0x1F),
        ("hair colour", (word >> 10) & 0x1F),
        ("face", (word >> 15) & 0x1F),
        ("hair style", (word >> 24) & 0x3F),
    ]
}

/// The sex the appearance word's first bit names.
pub fn sex_label(word: u32) -> &'static str {
    if word & 0x1 == 1 { "female" } else { "male" }
}

/// An item's name, from the UTF-16 code units a held item carries.
pub fn item_name(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_numbers_have_their_names() {
        assert_eq!(campaign_label(1), "Prophecies");
        assert_eq!(campaign_label(3), "Nightfall");
        assert_eq!(campaign_label(9), "Unknown");
        assert_eq!(profession_label(7), "Assassin");
        assert_eq!(profession_label(10), "Dervish");
        assert_eq!(professions(1, 0), "Warrior");
        assert_eq!(professions(6, 3), "Elementalist / Monk");
    }

    #[test]
    fn the_appearance_word_is_the_clients_own_bits() {
        // sex 1, height 7, skin 3, hair colour 9, face 12, profession 6, hair style 33.
        let word = 1 | (7 << 1) | (3 << 5) | (9 << 10) | (12 << 15) | (6 << 20) | (33 << 24);
        let fields = appearance_fields(word);
        assert_eq!(fields[0], ("sex", 1));
        assert_eq!(fields[1], ("height", 7));
        assert_eq!(fields[2], ("skin", 3));
        assert_eq!(fields[3], ("hair colour", 9));
        assert_eq!(fields[4], ("face", 12));
        assert_eq!(fields[5], ("hair style", 33));
        assert_eq!(sex_label(word), "female");
        assert_eq!(primary_profession(word), 6);
    }

    #[test]
    fn an_outpost_is_named_when_the_pack_names_it() {
        let names = HashMap::from([(148, "Ascalon City".to_string())]);
        assert_eq!(outpost_label(148, &names), "Ascalon City (148)");
        assert_eq!(outpost_label(9, &names), "map 9");
        assert_eq!(outpost_label(0, &names), "not placed");
    }
}
