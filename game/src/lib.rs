//! The game library: one directory per noun of the game, and the socket layer inside
//! `Server/` - the reference's own arrangement, which is why one process owns one
//! listener and still keeps the network as a subsystem.
//!
//! Reference: `src/server/game/`, 50 directories at 30a36ec4d. This tree has 41:
//!
//! - 24 keep the reference's name and content.
//! - 9 are the same directory renamed for this game:
//!   Agents <- Entities, Terrain <- Grids, Districts <- Instances, Parties <- Groups,
//!   Skills <- Spells, Drops <- Loot, Behaviour <- AI, Trader <- AuctionHouse,
//!   Support <- Tickets, plus Maps, Guilds and Quests keeping their own names.
//! - 5 have no reference directory: FileCheck, Portal, Pack, Heroes, Cinematics.
//!
//! Nothing in here is ported yet. The directory exists so the boundary exists: a change
//! to one noun moves one directory, and no game behaviour is written into `shared/`.

#[path = "Accounts/mod.rs"]
pub mod accounts;
#[path = "Achievements/mod.rs"]
pub mod achievements;
#[path = "Addons/mod.rs"]
pub mod addons;
#[path = "Agents/mod.rs"]
pub mod agents;
#[path = "Behaviour/mod.rs"]
pub mod behaviour;
#[path = "Cache/mod.rs"]
pub mod cache;
#[path = "Calendar/mod.rs"]
pub mod calendar;
#[path = "Chat/mod.rs"]
pub mod chat;
#[path = "Cinematics/mod.rs"]
pub mod cinematics;
#[path = "Combat/mod.rs"]
pub mod combat;
#[path = "Conditions/mod.rs"]
pub mod conditions;
#[path = "DataStores/mod.rs"]
pub mod data_stores;
#[path = "Districts/mod.rs"]
pub mod districts;
#[path = "Drops/mod.rs"]
pub mod drops;
#[path = "Events/mod.rs"]
pub mod events;
#[path = "FileCheck/mod.rs"]
pub mod file_check;
#[path = "Globals/mod.rs"]
pub mod globals;
#[path = "Guilds/mod.rs"]
pub mod guilds;
#[path = "Handlers/mod.rs"]
pub mod handlers;
#[path = "Heroes/mod.rs"]
pub mod heroes;
#[path = "Maps/mod.rs"]
pub mod maps;
#[path = "Misc/mod.rs"]
pub mod misc;
#[path = "Modules/mod.rs"]
pub mod modules;
#[path = "Motd/mod.rs"]
pub mod motd;
#[path = "Movement/mod.rs"]
pub mod movement;
#[path = "Pack/mod.rs"]
pub mod pack;
#[path = "Parties/mod.rs"]
pub mod parties;
#[path = "Portal/mod.rs"]
pub mod portal;
#[path = "Pools/mod.rs"]
pub mod pools;
#[path = "Quests/mod.rs"]
pub mod quests;
#[path = "Reputation/mod.rs"]
pub mod reputation;
#[path = "Scripting/mod.rs"]
pub mod scripting;
#[path = "Server/mod.rs"]
pub mod server;
#[path = "Skills/mod.rs"]
pub mod skills;
#[path = "Support/mod.rs"]
pub mod support;
#[path = "Terrain/mod.rs"]
pub mod terrain;
#[path = "Texts/mod.rs"]
pub mod texts;
#[path = "Time/mod.rs"]
pub mod time;
#[path = "Tools/mod.rs"]
pub mod tools;
#[path = "Trader/mod.rs"]
pub mod trader;
#[path = "World/mod.rs"]
pub mod world;
