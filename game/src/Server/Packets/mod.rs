//! The message families: one file per family, as the reference has it.
//!
//! Reference: `src/server/game/Server/Packets/`, whose `AllPackets.h` includes 17 family files.
//! One file holding every message in every direction is the arrangement this project exists to
//! avoid, so a family arrives as its own file and its own subject.

#[path = "AgentPackets.rs"]
pub mod agent_packets;
#[path = "CharacterPackets.rs"]
pub mod character_packets;
#[path = "ChatPackets.rs"]
pub mod chat_packets;
#[path = "GuildPackets.rs"]
pub mod guild_packets;
#[path = "InstancePackets.rs"]
pub mod instance_packets;
#[path = "ItemPackets.rs"]
pub mod item_packets;
#[path = "MiscPackets.rs"]
pub mod misc_packets;
#[path = "PartyPackets.rs"]
pub mod party_packets;
#[path = "TitlePackets.rs"]
pub mod title_packets;

pub use instance_packets::{
    CHAR_CREATION_START, INSTANCE_LOAD_HEAD, INSTANCE_PLAYER_DATA_DONE, INSTANCE_PLAYER_DATA_START,
    char_creation_start, creation_load_block, instance_load_head, instance_player_data_done,
    instance_player_data_start,
};
