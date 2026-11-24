//! The character's own row as a load reads it: what a connection needs from it before it draws
//! anything.
//!
//! Reference: `src/server/game/Entities/Player/Player.cpp`, `Player::LoadFromDB` - the row a session
//! reads to build the player it is about to add to a map. The statement is `DatabaseCharacters::sheet`
//! (`Characters/Repository.rs`); the shape of what it reads, and the sheet a row that cannot be read
//! is replaced by, are here.

use crate::server::packets::character_packets;

/// What a load needs from the character's row: the name it shows, the campaign and profession whose
/// starting set the item stream carries, the id its items are recorded under, the gold it carries
/// (R45: gold is per character), the look it is drawn with, its level and the professions it has
/// unlocked.
#[derive(Debug, Clone)]
pub struct CharacterSheet {
    /// What the client calls it.
    pub name: String,
    /// Which campaign it was made in, the client's own chapter byte: 0 none (a PvP character),
    /// 1 Prophecies, 2 Factions, 3 Nightfall (`docs/game-rules.md`, E225).
    pub campaign: u8,
    /// Its primary profession.
    pub profession: u8,
    /// The id its own rows are keyed by.
    pub uuid: [u8; 16],
    /// The gold it carries (R45).
    pub gold: u32,
    /// The packed look the client builds the model from, or 0 when the row records none.
    pub appearance: u32,
    /// The character's level, which the progress record and the agent's own property carry.
    pub level: u8,
    /// The professions the character has unlocked, as a bitmap.
    pub unlocked_professions: u32,
}

impl CharacterSheet {
    /// The sheet a connection is loaded with when its row cannot be read - or when the program opened
    /// no character database: a level 1 Warrior named "Player" with campaign 0, which is the
    /// no-campaign (PvP) value.
    ///
    /// A load gives the client a body either way, and the row is what makes the name and the set its
    /// own. The two defaults are the creation screen's own (`character_packets::DEFAULT_PROFESSION`,
    /// `CREATION_UNLOCKED_PROFESSIONS`: Warrior, and bit 1 as the only profession unlocked).
    pub fn missing(uuid: [u8; 16]) -> Self {
        Self {
            name: "Player".to_string(),
            campaign: 0,
            profession: character_packets::DEFAULT_PROFESSION,
            uuid,
            gold: 0,
            appearance: 0,
            level: 1,
            unlocked_professions: character_packets::CREATION_UNLOCKED_PROFESSIONS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sheet a load falls back to is the reference's fresh character, field for field.
    #[test]
    fn a_missing_row_is_a_fresh_prophecies_warrior_named_player() {
        let uuid = [0x11u8; 16];
        let sheet = CharacterSheet::missing(uuid);
        assert_eq!(sheet.name, "Player");
        assert_eq!(sheet.campaign, 0, "Prophecies");
        assert_eq!(
            sheet.profession,
            character_packets::DEFAULT_PROFESSION,
            "Warrior"
        );
        assert_eq!(sheet.level, 1);
        assert_eq!(sheet.gold, 0, "the reference's default for a new character");
        assert_eq!(sheet.appearance, 0, "no look of its own");
        assert_eq!(
            sheet.unlocked_professions,
            character_packets::CREATION_UNLOCKED_PROFESSIONS,
            "and the profession the creation screen starts with"
        );
        assert_eq!(sheet.uuid, uuid, "the id its items would be recorded under");
    }
}
