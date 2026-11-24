//! What a map is, as the world holds it: where a character may stand on entry, where a crossing lands,
//! and the rules and tables that belong to maps - placement, doorways, travel, the instance table.
//!
//! The content itself is the world pack's files, read at start-up by [`pack::Pack`]
//! (`docs/decisions/0014-the-world-content-is-a-pack.md`), and held in memory for the life of the
//! program, because a map's definition does not change while the world serves.
//!
//! What is **not** here: an instance. A map is a definition; a running copy with a district, a population
//! and a clock is `game/Districts/`.

#[path = "Travels.rs"]
pub mod travels;

#[path = "Placement.rs"]
pub mod placement;

#[path = "TravelTargets.rs"]
pub mod travel_targets;

#[path = "Cartography.rs"]
pub mod cartography;

#[path = "InstanceTable.rs"]
pub mod instance_table;

#[path = "Portals.rs"]
pub mod portals;

#[path = "Pack.rs"]
pub mod pack;

pub use portals::{Arch, Crossed, Portals};

pub use travel_targets::{TRAVEL_TARGETS, TravelTarget};
pub use travels::{DatabaseTravels, Travel};


/// One place a character may stand on entry (R13).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapSpawn {
    /// The map's id.
    pub map_id: u32,
    /// Which point of that map's list this is.
    pub index: i16,
    /// The point, in the game's own units.
    pub x: f32,
    /// The point's other axis.
    pub y: f32,
    /// Which plane it is on.
    pub plane: i16,
    /// The facing, in radians, a character entering at this point is created with, when the row states
    /// one (`ASURA/crates/asura-world/src/agent.rs:460-476`).
    pub angle: Option<f32>,
}

impl MapSpawn {
    /// The direction vector the agent is created facing: the angle as `(cos, sin)`, which is the
    /// unit-vector form `GAME_SMSG_WORLD_CREATE_AGENT` carries. A point with no angle keeps `(1, 0)`,
    /// which is what `ASURA/crates/asura-world/src/map_spawns.rs:44-53` answers.
    pub fn direction(&self) -> (f32, f32) {
        match self.angle {
            Some(angle) => (angle.cos(), angle.sin()),
            None => (1.0, 0.0),
        }
    }
}

/// Where a portal crossing puts a character: the destination's entry for the map it came from (R21).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arrival {
    /// The map the character arrives on.
    pub map_id: u32,
    /// The map it left.
    pub from_map: u32,
    /// The point, in the game's own units.
    pub x: f32,
    /// The point's other axis.
    pub y: f32,
    /// Which plane it is on.
    pub plane: i16,
    /// The facing, in radians, when the map states one
    /// (`ASURA/crates/asura-world/src/portals.rs:129-148`, `Arrival::angle`).
    pub angle: Option<f32>,
}

impl Arrival {
    /// The direction vector the agent is created facing: `(cos, sin)`, or the default `(1, 0)`.
    ///
    /// `ASURA/crates/asura-world/src/portals.rs:140-147`, `Arrival::direction`.
    pub fn direction(&self) -> (f32, f32) {
        match self.angle {
            Some(angle) => (angle.cos(), angle.sin()),
            None => (1.0, 0.0),
        }
    }
}
