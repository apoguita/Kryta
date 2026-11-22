//! Where a character enters a map: the point, its plane, and the way it faces there.
//!
//! Reference: `ASURA/crates/asura-world/src/agent.rs` keeps this on the agent itself -
//! `Standing::at_spawn` (`:460-476`, a point of the map's own list and the angle it states),
//! `arrival`/`resume` (`:478-545`, an outpost takes a scattered point, any other map resumes where
//! the character left it and stands on the map's first point otherwise), and the doorway's own
//! landing for a crossing (`travel.rs:319-338`, `portals::arrival`). It lived in the connection
//! (`WorldSession::entry_point`) until 2026-11-14; the frames it feeds are unchanged, and the rule is
//! the same text moved.
//!
//! This is a rule over data, not a message: the caller supplies the map, its spawn points, the landing
//! a crossing states, the position the character was saved at and how far an entry may scatter
//! (`World.SpawnScatter`, R13).

use crate::characters::Position;
use crate::maps::MapSpawn;
use crate::world::{MapEntry, map_kind};

/// A doorway crossing's landing, as the receiving map states it (R21).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    /// The map the landing belongs to: a landing for another map is not this load's.
    pub map_id: u32,
    /// The point, and the plane it is on.
    pub x: f32,
    /// The point's other axis.
    pub y: f32,
    /// Which plane it is on.
    pub plane: i16,
    /// The direction the arriving agent is created facing.
    pub facing: (f32, f32),
}

/// What a load knows before the rule runs.
#[derive(Debug, Clone, Copy)]
pub struct Entering<'a> {
    /// The map being entered.
    pub map: &'a MapEntry,
    /// Every spawn point this world holds; the ones that belong to this map are the rule's list.
    pub spawns: &'a [MapSpawn],
    /// The landing a doorway crossing states, when this load is one.
    pub landing: Option<Landing>,
    /// The position the character was saved at, when it has one.
    pub saved: Option<Position>,
    /// How far around a spawn point an entry may land (R13).
    pub scatter: f32,
}

/// Where the character stands on entry, or `None` when the map has nowhere to put it.
///
/// The branches are ASURA's `Standing::arrival` (`agent.rs:478-545`), in its order: a doorway's
/// landing wins; an **outpost** is entered at a random point of its own list, scattered (R13); every
/// other map resumes where the character left it when it left from this map, and stands on the map's
/// own first point otherwise. A map that states neither a point nor a saved position cannot be
/// entered - the character-creation instance is the one exception, because it is a conversation and
/// its catalogue row states no coordinate (see below). A point with no angle keeps `(1, 0)`
/// (`map_spawns.rs:44-53`).
pub fn entry_point(entering: &Entering) -> Option<(f32, f32, u16, (f32, f32))> {
    if let Some(landing) = entering.landing
        && landing.map_id == entering.map.id
    {
        return Some((
            landing.x,
            landing.y,
            landing.plane.max(0) as u16,
            landing.facing,
        ));
    }
    let points: Vec<MapSpawn> = entering
        .spawns
        .iter()
        .filter(|spawn| spawn.map_id == entering.map.id)
        .copied()
        .collect();
    if entering.map.kind == Some(map_kind::OUTPOST) {
        if points.is_empty() {
            // `Standing::arrival`: "outpost {} has no configured random spawn list".
            return None;
        }
        let mut pick = [0u8; 8];
        let choice = if getrandom::getrandom(&mut pick).is_ok() {
            (u64::from_le_bytes(pick) % points.len() as u64) as usize
        } else {
            0
        };
        let point = points[choice];
        let scatter = entering.scatter;
        let mut angle = [0u8; 8];
        let (dx, dy) = if getrandom::getrandom(&mut angle).is_ok() {
            let raw = u64::from_le_bytes(angle);
            let radians = (raw % 360) as f32 * std::f32::consts::PI / 180.0;
            let radius = ((raw / 360) % scatter.max(1.0) as u64) as f32;
            (radius * radians.cos(), radius * radians.sin())
        } else {
            (0.0, 0.0)
        };
        return Some((
            point.x + dx,
            point.y + dy,
            point.plane.max(0) as u16,
            point.direction(),
        ));
    }
    // Every map that is not an outpost: the saved position is used only when it is a position in
    // this map (a character saved elsewhere has no coordinate here), and a map that states neither
    // a point nor a saved position refuses the load. The character-creation instance is exempt: it
    // is a conversation that loads no geometry.
    let saved_here = entering
        .saved
        .filter(|saved| saved.map_id == entering.map.id);
    if saved_here.is_none() && points.is_empty() && entering.map.kind != Some(map_kind::CREATION) {
        // `Standing::arrival`: "map {} has no configured entry spawn or doorway arrival".
        return None;
    }
    if let Some(saved) = saved_here {
        return Some((
            saved.x,
            saved.y,
            saved.plane.max(0) as u16,
            // A resumed position keeps the default direction: the facing of a saved position is not
            // stored (`agent.rs:536-545`).
            (1.0, 0.0),
        ));
    }
    match points.first() {
        Some(first) => Some((
            first.x,
            first.y,
            first.plane.max(0) as u16,
            first.direction(),
        )),
        // The character-creation instance: `Standing::from_map` stands the character on the
        // catalogue row's coordinate, and a row that states none is zero (`map_config.spawn_x`
        // defaults to 0). This load is a conversation - the creation block is sent instead of a map
        // load - so the point is never drawn.
        None => Some((0.0, 0.0, 0, (1.0, 0.0))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outpost() -> MapEntry {
        MapEntry {
            id: 148,
            kind: Some(map_kind::OUTPOST),
            file_id: 113_021,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: Some(0xBB14_F539),
            name_id: None,
        }
    }

    fn explorable() -> MapEntry {
        MapEntry {
            id: 146,
            kind: Some(map_kind::EXPLORABLE),
            file_id: 113_022,
            name: "Lakeside County".to_string(),
            instance_value: Some(0xBB14_F53A),
            name_id: None,
        }
    }

    fn spawn(map_id: u32, index: i16, x: f32, y: f32, angle: Option<f32>) -> MapSpawn {
        MapSpawn {
            map_id,
            index,
            x,
            y,
            plane: 0,
            angle,
        }
    }

    /// R13: an outpost is entered at a point of its own list, within the scatter the world sets.
    #[test]
    fn an_outpost_is_entered_at_a_scattered_point_of_its_own_list() {
        let map = outpost();
        let spawns = [
            spawn(148, 0, 1000.0, 2000.0, Some(std::f32::consts::FRAC_PI_2)),
            spawn(999, 0, 0.0, 0.0, None),
        ];
        for _ in 0..32 {
            let point = entry_point(&Entering {
                map: &map,
                spawns: &spawns,
                landing: None,
                saved: None,
                scatter: 250.0,
            })
            .expect("an outpost with a spawn point can be entered");
            assert_eq!(point.2, 0, "the point's plane");
            let dx = point.0 - 1000.0;
            let dy = point.1 - 2000.0;
            assert!(
                (dx * dx + dy * dy).sqrt() <= 250.0,
                "the entry lands within the scatter: {point:?}"
            );
            assert_eq!(
                point.3,
                (
                    std::f32::consts::FRAC_PI_2.cos(),
                    std::f32::consts::FRAC_PI_2.sin()
                ),
                "and faces the angle the point states"
            );
        }
    }

    /// A crossing's landing wins over the map's list (R21).
    #[test]
    fn a_landing_from_a_crossing_decides_the_point_and_the_facing() {
        let map = explorable();
        let spawns = [spawn(146, 0, 0.0, 0.0, None)];
        let landing = Landing {
            map_id: 146,
            x: 6602.0,
            y: 4485.0,
            plane: 0,
            facing: (-2.28, 0.0),
        };
        let point = entry_point(&Entering {
            map: &map,
            spawns: &spawns,
            landing: Some(landing),
            saved: None,
            scatter: 250.0,
        });
        assert_eq!(point, Some((6602.0, 4485.0, 0, (-2.28, 0.0))));
    }

    /// A landing for another map is not this load's, and the rule falls through to the map's own list.
    #[test]
    fn a_landing_for_another_map_is_not_this_loads() {
        let map = explorable();
        let spawns = [spawn(146, 0, 30.0, 40.0, None)];
        let point = entry_point(&Entering {
            map: &map,
            spawns: &spawns,
            landing: Some(Landing {
                map_id: 148,
                x: 6602.0,
                y: 4485.0,
                plane: 0,
                facing: (-2.28, 0.0),
            }),
            saved: None,
            scatter: 250.0,
        });
        assert_eq!(point, Some((30.0, 40.0, 0, (1.0, 0.0))));
    }

    /// R14: a map that is not an outpost resumes the character where it left, facing the default.
    #[test]
    fn a_saved_position_on_this_map_is_resumed() {
        let map = explorable();
        let spawns = [spawn(146, 0, 30.0, 40.0, None)];
        let point = entry_point(&Entering {
            map: &map,
            spawns: &spawns,
            landing: None,
            saved: Some(Position {
                map_id: 146,
                x: 123.0,
                y: 456.0,
                plane: 1,
            }),
            scatter: 250.0,
        });
        assert_eq!(point, Some((123.0, 456.0, 1, (1.0, 0.0))));
    }

    /// A map with no spawn point of its own and no saved position cannot be entered: this is an
    /// explorable entered from nowhere (`Standing::arrival`: "map {} has no configured entry spawn
    /// or doorway arrival").
    #[test]
    fn a_map_with_no_point_and_no_saved_position_has_no_entry() {
        let map = explorable();
        let point = entry_point(&Entering {
            map: &map,
            spawns: &[],
            landing: None,
            saved: None,
            scatter: 250.0,
        });
        assert_eq!(point, None);
    }

    /// A saved position in the map itself is resumed even when the map states no spawn list at all:
    /// the character is standing there, so there is nowhere else it could be. This is the ASURA rule
    /// whose check runs before the spawn list is consulted (`agent.rs:531-545`).
    #[test]
    fn a_saved_position_in_a_map_with_no_spawn_list_is_resumed() {
        let map = explorable();
        let point = entry_point(&Entering {
            map: &map,
            spawns: &[],
            landing: None,
            saved: Some(Position {
                map_id: 146,
                x: 6602.0,
                y: 4485.0,
                plane: 0,
            }),
            scatter: 250.0,
        });
        assert_eq!(point, Some((6602.0, 4485.0, 0, (1.0, 0.0))));
    }

    /// An outpost with no spawn list cannot be entered either, and says so with the outpost's own
    /// refusal (`Standing::arrival`).
    #[test]
    fn an_outpost_with_no_spawn_list_has_no_entry() {
        let map = outpost();
        let point = entry_point(&Entering {
            map: &map,
            spawns: &[],
            landing: None,
            saved: None,
            scatter: 250.0,
        });
        assert_eq!(point, None);
    }

    /// The character-creation instance is a conversation, not a place: it states no spawn point and
    /// the load never runs, and the rule answers the catalogue row's coordinate (zero when the row
    /// states none) instead of refusing (`Standing::arrival`/`Standing::from_map`).
    #[test]
    fn the_creation_instance_needs_no_spawn_list() {
        let map = MapEntry {
            id: 0,
            kind: Some(map_kind::CREATION),
            file_id: 0,
            name: "Character Creation".to_string(),
            instance_value: None,
            name_id: None,
        };
        let point = entry_point(&Entering {
            map: &map,
            spawns: &[],
            landing: None,
            saved: None,
            scatter: 250.0,
        });
        assert_eq!(point, Some((0.0, 0.0, 0, (1.0, 0.0))));
    }
}
