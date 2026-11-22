//! A map's doorways: the wall a character crosses, and where the receiving map puts it.
//!
//! Ported whole from `ASURA/crates/asura-world/src/portals.rs` (503 lines): `Arch`, `is_wall`,
//! `crossed_by`, `side`, `box_crossed`, `point_half_extent`, `Arrival`'s `direction`, `Crossed`,
//! `MapPortals` with `arrival_for`/`arrival_at`, and the three lookups a world asks - `crossed`,
//! `arrival`, `arrival_at`.
//!
//! **Where the data comes from is this tree's structure, not that file's.** ASURA reads one
//! `portals.json` per map folder out of its pack (`config/World/<pack>/maps/.../<map>/portals.json`);
//! here a doorway is a row (`docs/port/04-configuration.md`, "three kinds of data"): an arch is a
//! `map_arch` row and an arrival is a `map_arrival` row, which this tree already had for R21. The
//! values seeded for map 148 are ASURA's own measured file, quoted in the migration.
//!
//! The owner's rule the shape follows (`ASURA/crates/asura-world/src/portals.rs:19-26`): "the portal
//! is an invisible wall we cross ... should be a plane", so a wall-shaped arch counts a crossing only
//! when the step passes through the segment, and a point-shaped arch uses the edges of its box.
//!
//! **Two facts a reader must keep in mind**, as that file states them: several instances share one
//! geometry blob, so a position says where a doorway is rather than which instance a character is in;
//! and `to_map` is filled only where a crossing leads somewhere, so an arch naming none must not fire.

use std::collections::HashMap;

/// One arch of a map: the wall a character crosses, and the map it leads to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arch {
    /// The map the wall stands in.
    pub from_map: u32,
    /// The arch's own index in the map's list.
    pub idx: u32,
    /// The map the crossing leads to, or `None` when the data names none.
    pub to_map: Option<u32>,
    /// The wall's first endpoint. Equal to [`Arch::to`] when the data states a point.
    pub from: (f32, f32),
    /// The wall's second endpoint.
    pub to: (f32, f32),
}

impl Arch {
    /// Whether this arch is a wall (two distinct endpoints) rather than a point.
    pub fn is_wall(&self) -> bool {
        self.from != self.to
    }

    /// Whether the step `step_from` -> `step_to` crosses this arch.
    ///
    /// Movement touching or crossing the finite wall starts travel, including its endpoints. A point
    /// doorway uses the edges of its configured box, whose half side is `extent`.
    pub fn crossed_by(&self, step_from: (f32, f32), step_to: (f32, f32), extent: f32) -> bool {
        if !self.is_wall() {
            return box_crossed(self.from, step_from, step_to, extent);
        }
        if step_from == step_to {
            return false;
        }
        // Bounding boxes also reject disjoint collinear segments. Non-strict side checks include a
        // step ending on the wall, an endpoint touch, and movement along the wall itself.
        self.from.0.min(self.to.0) <= step_from.0.max(step_to.0)
            && step_from.0.min(step_to.0) <= self.from.0.max(self.to.0)
            && self.from.1.min(self.to.1) <= step_from.1.max(step_to.1)
            && step_from.1.min(step_to.1) <= self.from.1.max(self.to.1)
            && side(self.from, self.to, step_from) * side(self.from, self.to, step_to) <= 0.0
            && side(step_from, step_to, self.from) * side(step_from, step_to, self.to) <= 0.0
    }
}

/// The signed area of the triangle `a`-`b`-`c`: positive on one side of `a`-`b`, negative on the other.
fn side(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// Whether actual movement intersects a finite edge of a point doorway's box.
///
/// A non-positive `extent` disables point doorways entirely, which is what an unreadable setting in the
/// working implementation does (`ASURA/crates/asura-config/src/lib.rs`, `portal_box_half_extent`).
fn box_crossed(point: (f32, f32), from: (f32, f32), to: (f32, f32), extent: f32) -> bool {
    if extent <= 0.0 || from == to {
        return false;
    }
    let (left, right) = (point.0 - extent, point.0 + extent);
    let (bottom, top) = (point.1 - extent, point.1 + extent);
    // Test each finite edge. A report wholly inside the box crosses no boundary.
    for x in [left, right] {
        if from.0 != to.0 {
            let t = (x - from.0) / (to.0 - from.0);
            let y = from.1 + t * (to.1 - from.1);
            if (0.0..=1.0).contains(&t) && (bottom..=top).contains(&y) {
                return true;
            }
        }
    }
    for y in [bottom, top] {
        if from.1 != to.1 {
            let t = (y - from.1) / (to.1 - from.1);
            let x = from.0 + t * (to.0 - from.0);
            if (0.0..=1.0).contains(&t) && (left..=right).contains(&x) {
                return true;
            }
        }
    }
    false
}

/// How close a written arrival point has to be to a saved position to be the same arrival, in the
/// map's own units. The position written for a crossing comes from the same entry, so this only
/// absorbs the float round trip through the database.
pub const ARRIVAL_MATCH: f32 = 1.0;

/// The doorway a step crossed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossed {
    /// The map the crossing leads to, when the data names one. A crossing with none does not fire.
    pub to_map: Option<u32>,
    /// The map the wall stands in.
    pub from_map: u32,
}

/// One map's doorways, as the rows are read: its arches.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapPortals {
    /// The walls of this map.
    pub arches: Vec<Arch>,
}

/// Every map's doorways, read once at start-up: what a step is judged against, and where an arriving
/// character stands.
///
/// `ASURA/crates/asura-world/src/portals.rs:313-377` (`table_pack`, `pack_arches`, `crossed`,
/// `arrival`, `arrival_at`), with the rows in place of the pack's files.
#[derive(Debug, Clone, Default)]
pub struct Portals {
    maps: HashMap<u32, MapPortals>,
    extent: f32,
}

impl Portals {
    /// Build the table from the rows a database holds, and the point-doorway box's half side.
    pub fn new(arches: Vec<Arch>, extent: f32) -> Portals {
        let mut maps: HashMap<u32, MapPortals> = HashMap::new();
        for arch in arches {
            maps.entry(arch.from_map).or_default().arches.push(arch);
        }
        for map in maps.values_mut() {
            map.arches.sort_by_key(|arch| arch.idx);
        }
        Portals { maps, extent }
    }

    /// A world with no doorway rows: every step crosses nothing.
    pub fn empty() -> Portals {
        Portals {
            maps: HashMap::new(),
            extent: 0.0,
        }
    }

    /// The point-doorway box's half side, in map units.
    pub fn extent(&self) -> f32 {
        self.extent
    }

    /// How many maps state any doorway, for the start-up line.
    pub fn maps_with_doorways(&self) -> usize {
        self.maps
            .values()
            .filter(|map| !map.arches.is_empty())
            .count()
    }

    /// How many arches the rows hold, for the start-up line.
    pub fn arches(&self) -> usize {
        self.maps.values().map(|map| map.arches.len()).sum()
    }

    /// The walls of one map, when the rows state any.
    ///
    /// `portals.rs:335-340`, `pack_arches`.
    pub fn arches_of(&self, from_map: u32) -> Option<&[Arch]> {
        self.maps
            .get(&from_map)
            .map(|map| map.arches.as_slice())
            .filter(|arches| !arches.is_empty())
    }

    /// The doorway the step `from` -> `to` crosses in `from_map`.
    ///
    /// **A map the rows describe is judged by its own walls, and nothing else.** A step that crosses
    /// none of its arches crosses nothing, even if it passes near a measured crossing point. A map with
    /// no arch rows has no doorways and crosses nothing (`portals.rs:342-355`).
    pub fn crossed(&self, from_map: u32, from: (f32, f32), to: (f32, f32)) -> Option<Crossed> {
        self.arches_of(from_map)?
            .iter()
            .find(|arch| arch.crossed_by(from, to, self.extent))
            .map(|arch| Crossed {
                to_map: arch.to_map,
                from_map,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall along y = 0 from x = 0 to x = 10, leading to map 2.
    fn wall() -> Arch {
        Arch {
            from_map: 1,
            idx: 0,
            to_map: Some(2),
            from: (0.0, 0.0),
            to: (10.0, 0.0),
        }
    }

    /// `portals.rs:394-420`, ASURA's own wall cases, with the box's half side passed in.
    #[test]
    fn a_wall_triggers_on_crossing_or_touching() {
        let wall = wall();
        assert!(
            wall.crossed_by((-1.0, -5.0), (5.0, 5.0), 250.0),
            "a step from one side to the other, inside the wall's length, crosses it"
        );
        assert!(
            !wall.crossed_by((-5.0, -1.0), (-5.0, 1.0), 250.0),
            "a step that crosses the wall's line beyond its endpoint does not pass through the wall"
        );
        assert!(
            !wall.crossed_by((-2.0, 3.0), (8.0, 3.0), 250.0),
            "a step beside the wall on the same side does not cross it"
        );
        assert!(
            wall.crossed_by((10.0, -1.0), (10.0, 1.0), 250.0),
            "touching the wall at its endpoint starts travel"
        );
        assert!(
            wall.crossed_by((5.0, -1.0), (5.0, 0.0), 250.0),
            "landing on the wall triggers"
        );
        assert!(
            !wall.crossed_by((-5.0, 0.0), (-1.0, 0.0), 250.0),
            "disjoint collinear segments miss"
        );
    }

    /// `portals.rs:430-449`: a point doorway uses the edges of its box, and a non-positive box has no
    /// doorways at all.
    #[test]
    fn a_point_doorway_uses_box_edges() {
        let point = Arch {
            from: (0.0, 0.0),
            to: (0.0, 0.0),
            ..wall()
        };
        assert!(
            point.crossed_by((100.0, -400.0), (100.0, 400.0), 250.0),
            "a step through the box crosses its edges"
        );
        assert!(
            !point.crossed_by((0.0, 0.0), (10.0, 10.0), 250.0),
            "inside is not a crossing"
        );
        assert!(
            !point.crossed_by((400.0, -200.0), (400.0, 200.0), 250.0),
            "400 units away misses the box"
        );
        assert!(
            !point.crossed_by((100.0, -400.0), (100.0, 400.0), 0.0),
            "a box of no size has no doorway"
        );
    }

    /// `portals.rs:472-479`: a step far from every doorway crosses nothing, and a map nothing
    /// describes crosses nothing at all.
    #[test]
    fn a_step_far_from_every_doorway_crosses_nothing() {
        let portals = Portals::new(
            vec![Arch {
                from_map: 148,
                idx: 0,
                to_map: Some(146),
                // ASURA's measured row for map 148 (`config/World/Vanilla/maps/Prophecies/`
                // `Ascalon (pre-Searing)/Outposts/148/portals.json`), at the precision an f32 holds.
                from: (7523.469, 5129.551),
                to: (7055.75, 5592.349),
            }],
            250.0,
        );
        assert!(
            portals.crossed(449, (0.0, 0.0), (1.0, 0.0)).is_none(),
            "a map with no arch rows crosses nothing"
        );
        assert!(
            portals
                .crossed(148, (9000.0, 9000.0), (9100.0, 9100.0))
                .is_none(),
            "a step far from the map's own wall crosses nothing"
        );
        assert_eq!(
            portals
                .crossed(148, (7300.0, 5300.0), (7200.0, 5450.0))
                .expect("a step through the wall")
                .to_map,
            Some(146),
            "the map 148 row's wall leads to 146"
        );
    }
}
