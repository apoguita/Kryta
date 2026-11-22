//! The character: what one is, and the 62 bytes a character list says about it.
//!
//! Reference: `src/server/game/Characters/` for the noun, and `GmChar.c`'s
//! `CharacterSettings_FromDbCharacter` for the record the login sends with every character in the list.
//! A character in this game is a name, a profession pair, a campaign, the outpost it stands in and up to
//! five armour pieces; the login's list message carries exactly that, packed into 62 bytes.
//!
//! The blob is built here rather than in either program because both need it: the login sends it, and
//! the world writes what it is made of at a character's creation
//! (`docs/decisions/0007-character-schema.md`). The repository that reads the row is here for the
//! same reason: two programs and the administration tool read the same table.

#[path = "Repository.rs"]
pub mod repository;

#[path = "Equipping.rs"]
pub mod equipping;

#[path = "Sheet.rs"]
pub mod sheet;

#[path = "Metrics.rs"]
pub mod metrics;

#[path = "Titles.rs"]
pub mod titles;

#[path = "Experience.rs"]
pub mod experience;

pub use equipping::{NotWorn, equip_place};
pub use sheet::CharacterSheet;

pub use repository::{
    BACKPACK_MODEL, DatabaseCharacters, EQUIPPED_ITEMS_MODEL, HeldItem, LAST_CARRYING_MODEL,
    MoveOutcome, Refused,
};

use std::fmt;

/// Where a character last stood, and the map the position belongs to.
///
/// `ASURA/crates/asura-store/src/state/character.rs:213-224`, `Position`: a position is only usable on
/// the map it was taken in, and `None` from the repository means the character has never been saved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// The map the position belongs to.
    pub map_id: u32,
    /// The coordinate the client reports.
    pub x: f32,
    /// The coordinate that goes with it.
    pub y: f32,
    /// The plane inside the map blob, the client's third coordinate.
    pub plane: i16,
}

/// The version byte pair `CharacterSettings` starts with.
///
/// Evidence: `ASURA`'s `character_settings`, which records the reference's own constant 6 in the first
/// two bytes and zero in the next four (`last_time_played`).
pub const CHARACTER_SETTINGS_VERSION: u16 = 6;

/// How many bytes the block is, whatever the character wears.
///
/// The reference sends `sizeof(CharacterSettings)`: five piece slots are always present, and a character
/// wearing nothing sends five empty ones. A shorter block is a client reading pieces that are not there.
pub const CHARACTER_SETTINGS_LEN: usize = 62;

/// How many armour pieces a character record carries.
pub const PIECE_SLOTS: usize = 5;

/// One armour piece as the list carries it: a file id and four colour nibbles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmourPiece {
    /// The `Gw.dat` file id of the piece's model.
    pub file_id: u16,
    /// Four nibbles of dye colour, packed as the reference packs them.
    pub colors: u16,
}

/// What a character list says about one character.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterSettings {
    /// The outpost the character was last in.
    pub last_outpost: u16,
    /// The packed appearance word: sex 1 bit, height 4, skin 5, hair colour 5, face 5, profession 4,
    /// hair style 6, race 2.
    pub appearance: u32,
    /// 0 none (a PvP character), 1 Prophecies, 2 Factions, 3 Nightfall, 4 Eye of the North.
    pub campaign: u8,
    /// The character's level, five bits on the wire.
    pub level: u8,
    /// Whether it was made for PvP, where the client hides PvE-only surfaces.
    pub is_pvp: bool,
    /// Its second profession, four bits.
    pub secondary_profession: u8,
    /// 0 none, 1 show, 2 hide.
    pub helm_status: u8,
    /// The armour it wears, body, legs, head, boots, gloves.
    pub pieces: Vec<ArmourPiece>,
}

impl CharacterSettings {
    /// The 62 bytes the login list carries for this character.
    pub fn to_blob(&self) -> Vec<u8> {
        let pieces = self.pieces.len().min(PIECE_SLOTS);
        let mut blob = Vec::with_capacity(CHARACTER_SETTINGS_LEN);

        blob.extend_from_slice(&CHARACTER_SETTINGS_VERSION.to_le_bytes());
        blob.extend_from_slice(&self.last_outpost.to_le_bytes());
        blob.extend_from_slice(&0u32.to_le_bytes()); // last_time_played: the reference zeroes it
        blob.extend_from_slice(&self.appearance.to_le_bytes());
        blob.extend_from_slice(&[0u8; 16]); // the guild hall: nothing here creates guilds

        // One packed word: campaign 4 bits, level 5, is_pvp 1, secondary profession 4, helm 2.
        let packed = (u32::from(self.campaign & 0xF))
            | ((u32::from(self.level) & 0x1F) << 4)
            | (u32::from(self.is_pvp) << 9)
            | ((u32::from(self.secondary_profession) & 0xF) << 10)
            | ((u32::from(self.helm_status) & 0x3) << 14);
        blob.extend_from_slice(&packed.to_le_bytes());

        blob.push(pieces as u8);
        blob.extend_from_slice(&0u32.to_le_bytes()); // the reference leaves these four zero

        for index in 0..PIECE_SLOTS {
            match self.pieces.get(index) {
                Some(piece) => {
                    blob.extend_from_slice(&piece.file_id.to_le_bytes());
                    blob.push(
                        (piece.colors & 0xF) as u8 | (((piece.colors >> 4) & 0xF) as u8) << 4,
                    );
                    blob.push(0); // the piece's unk, zero for every piece the reference sends
                    blob.push(
                        (((piece.colors >> 8) & 0xF) as u8)
                            | ((((piece.colors >> 12) & 0xF) as u8) << 4),
                    );
                }
                None => blob.extend_from_slice(&[0u8; 5]),
            }
        }

        blob
    }
}

/// One character, as its row holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    /// The client's id for it: 16 bytes, little-endian on the wire.
    pub uuid: [u8; 16],
    /// The name the client shows.
    pub name: String,
    /// What the list says about it.
    pub settings: CharacterSettings,
}

impl Character {
    /// What the login's list message carries for this character.
    pub fn list_blob(&self) -> Vec<u8> {
        self.settings.to_blob()
    }
}

/// Why a character could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterError {
    /// The uuid column is not the 36-character text form of 16 bytes.
    BadUuid(String),
    /// A column a character cannot be without is missing or the wrong type.
    BadRow(String),
}

impl fmt::Display for CharacterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadUuid(text) => write!(f, "'{text}' is not a character uuid"),
            Self::BadRow(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for CharacterError {}

/// A 16-byte identifier as the text form the database keeps, in the wire's little-endian order.
pub fn uuid_text(bytes: &[u8; 16]) -> String {
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_le_bytes([bytes[4], bytes[5]]),
        u16::from_le_bytes([bytes[6], bytes[7]]),
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

/// The 16 wire bytes of a uuid the database holds as text.
pub fn uuid_bytes(text: &str) -> Result<[u8; 16], CharacterError> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return Err(CharacterError::BadUuid(text.to_string()));
    }
    let mut bytes = [0u8; 16];
    for (index, byte) in hex.as_bytes().chunks(2).enumerate() {
        let pair =
            std::str::from_utf8(byte).map_err(|_| CharacterError::BadUuid(text.to_string()))?;
        bytes[index] =
            u8::from_str_radix(pair, 16).map_err(|_| CharacterError::BadUuid(text.to_string()))?;
    }
    // The text form is the wire's little-endian order, so the first four bytes are reversed back into
    // the order the client reads them.
    Ok([
        bytes[3], bytes[2], bytes[1], bytes[0], bytes[5], bytes[4], bytes[7], bytes[6], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_sixty_two_bytes_whatever_is_worn() {
        let bare = CharacterSettings::default().to_blob();
        assert_eq!(bare.len(), CHARACTER_SETTINGS_LEN);

        let dressed = CharacterSettings {
            pieces: vec![
                ArmourPiece {
                    file_id: 1,
                    colors: 0x1234
                };
                5
            ],
            ..CharacterSettings::default()
        };
        assert_eq!(dressed.to_blob().len(), CHARACTER_SETTINGS_LEN);
    }

    #[test]
    fn the_layout_is_the_references_own() {
        let settings = CharacterSettings {
            last_outpost: 146,
            appearance: 0xAABB_CCDD,
            campaign: 1,
            level: 5,
            is_pvp: false,
            secondary_profession: 2,
            helm_status: 1,
            pieces: Vec::new(),
        };
        let blob = settings.to_blob();

        assert_eq!(&blob[0..2], &6u16.to_le_bytes(), "the version");
        assert_eq!(&blob[2..4], &146u16.to_le_bytes(), "the outpost");
        assert_eq!(&blob[4..8], &[0u8; 4], "last_time_played is zero");
        assert_eq!(
            &blob[8..12],
            &0xAABB_CCDDu32.to_le_bytes(),
            "the appearance"
        );
        assert_eq!(&blob[12..28], &[0u8; 16], "the guild hall is zero");
        // campaign 1, level 5, secondary 2, helm 1 -> 1 | 5 << 4 | 2 << 10 | 1 << 14
        let packed = 1u32 | (5 << 4) | (2 << 10) | (1 << 14);
        assert_eq!(&blob[28..32], &packed.to_le_bytes(), "the packed word");
        assert_eq!(blob[32], 0, "no pieces");
        assert_eq!(&blob[33..37], &[0u8; 4], "the reference's zero");
        assert_eq!(&blob[37..], &[0u8; 25], "five empty slots");
    }

    #[test]
    fn a_uuid_round_trips_through_its_text_form() {
        let mut bytes = [0u8; 16];
        bytes[0] = 0x1E;
        bytes[1] = 0x2C;
        bytes[2] = 0x5A;
        bytes[3] = 0x0F;
        bytes[15] = 0x66;
        let text = uuid_text(&bytes);
        assert_eq!(text.len(), 36);
        assert_eq!(uuid_bytes(&text).unwrap(), bytes);
    }

    #[test]
    fn a_uuid_that_is_not_one_is_refused() {
        assert!(uuid_bytes("not-a-uuid").is_err());
        assert!(uuid_bytes("0F5A2C1E-1111-2222-3333-44445555666Z").is_err());
    }
}
