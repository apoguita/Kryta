//! The character's own row as a load reads it: what a connection needs from it before it draws
//! anything.
//!
//! Reference: `src/server/game/Entities/Player/Player.cpp`, `Player::LoadFromDB` - the row a session
//! reads to build the player it is about to add to a map. This tree's session read the row itself
//! until 2026-11-14; the shape of what it reads, and the sheet a row that cannot be read is replaced
//! by, are the noun's.

use crate::server::packets::character_packets;

/// What a load needs from the character's row: the name it shows, the campaign and profession whose
/// starting set the item stream carries, the id its items are recorded under, the gold it carries
/// (R45: gold is per character), the look it is drawn with, its level and the professions it has
/// unlocked.
#[derive(Debug, Clone)]
pub struct CharacterSheet {
    /// What the client calls it.
    pub name: String,
    /// Which campaign it was made in, 0 for Prophecies.
    pub campaign: u8,
    /// Its primary profession.
    pub profession: u8,
    /// Its secondary profession, or zero when it has chosen none.
    pub secondary_profession: u8,
    /// The id its own rows are keyed by.
    pub uuid: [u8; 16],
    /// The gold it carries (R45).
    pub gold: u32,
    /// The packed look the client builds the model from, or 0 when the row records none.
    pub appearance: u32,
    /// The campaigns this character has finished, as bit 0 Prophecies, bit 1 Factions, bit 2
    /// Nightfall. Hard mode is unlocked by a completed campaign, not by the outpost.
    pub campaigns_finished: u8,
    /// The last outpost the character stood in: where a defeated party's return button sends it.
    pub last_outpost: u16,
    /// The character's level, which the progress record and the agent's own property carry.
    pub level: u8,
    /// The professions the character has unlocked, as a bitmap.
    pub unlocked_professions: u32,
    /// Total experience: the first dword of the progress record (`0x00E9`).
    pub experience: u32,
    /// Skill points held now: the progress record's fourteenth dword
    /// (`ASURA/crates/asura-db/src/game.rs:144-145`).
    pub skill_points: u32,
    /// Skill points earned in total: its fifteenth (`ASURA/crates/asura-db/src/game.rs:146-147`).
    pub skill_points_total: u32,
}

impl CharacterSheet {
    /// The sheet a connection is loaded with when its row cannot be read - or when the program opened
    /// no character database: a fresh Prophecies Warrior named "Player".
    ///
    /// A load gives the client a body either way, and the row is what makes the name and the set its
    /// own. The two defaults are the creation screen's own (`character_packets::DEFAULT_PROFESSION`,
    /// `CREATION_UNLOCKED_PROFESSIONS`: Warrior, and bit 1 as the only profession unlocked).
    pub fn missing(uuid: [u8; 16]) -> Self {
        Self {
            name: "Player".to_string(),
            campaign: 0,
            profession: character_packets::DEFAULT_PROFESSION,
            secondary_profession: 0,
            uuid,
            gold: 0,
            appearance: 0,
            campaigns_finished: 0,
            last_outpost: 148,
            level: 1,
            unlocked_professions: character_packets::CREATION_UNLOCKED_PROFESSIONS,
            experience: 0,
            skill_points: 0,
            skill_points_total: 0,
        }
    }

    /// The progress this sheet states, as `Characters/Experience.rs` reasons about it.
    pub fn progress(&self) -> crate::characters::experience::Progress {
        crate::characters::experience::Progress {
            experience: self.experience,
            level: self.level,
            skill_points: self.skill_points,
            skill_points_total: self.skill_points_total,
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
        assert_eq!(
            (
                sheet.experience,
                sheet.skill_points,
                sheet.skill_points_total
            ),
            (0, 0, 0),
            "a fresh character has no progress"
        );
    }
}
