//! The map pack: a world's maps read from its own folders, not from database rows.
//!
//! Reference: `ASURA/crates/asura-config/src/world.rs` (`pack`, `load`, `catalogue`) and
//! `ASURA/crates/asura-world/src/map_spawns.rs` / `portals.rs`. The arrangement is ASURA's and the
//! owner's direction for this tree: what a world *is* is a file, and a row is what a server writes
//! (`docs/decisions/0014-the-world-content-is-a-pack.md`).
//!
//! ```text
//! <pack>/maps/<campaign>/<area>/<kind>/<map id>/
//!   basic.json     the map's record: its id, kind, blob, name and instance-table value
//!   spawns.json    where a character may stand on entry
//!   portals.json   the walls a step crosses, and where an arrival from another map lands
//! ```
//!
//! The last two are optional: a map with no `spawns.json` has no measured entry point, and a map with
//! no `portals.json` has no doorway. A folder whose name is not the `map_id` its record states is an
//! error, not a warning - the number is how a client names a map, and two answers cannot both be
//! right. Catalogue readers walk recursively, so where a map sits in the tree decides nothing but
//! where a reader finds it.

use std::path::{Path, PathBuf};

use common::configuration::json::Document;
use common::logging::{Log, LogLevel};

use crate::maps::{Arch, Arrival, MapEntry, MapSpawn};

/// A world read from a pack folder: every map record, every entry point, every wall and arrival.
#[derive(Debug, Clone, Default)]
pub struct Pack {
    /// The maps, by id.
    maps: Vec<MapEntry>,
    /// Every spawn point, in the order the files state them.
    spawns: Vec<MapSpawn>,
    /// Every arch, and the map it leads to.
    arches: Vec<Arch>,
    /// Every arrival point, keyed by the map the character arrived on and the map it came from.
    arrivals: Vec<Arrival>,
}

impl Pack {
    /// Read the pack at `root`: the folder that holds `maps/`.
    ///
    /// A pack with no `maps/` folder is an error rather than an empty world, because a world with no
    /// maps hosts nothing and serving an empty catalogue hides that until a client asks to travel.
    pub fn load(root: impl AsRef<Path>, log: &Log) -> Result<Self, String> {
        let root = root.as_ref();
        let maps_root = root.join("maps");
        if !maps_root.is_dir() {
            return Err(format!(
                "no map pack at {}: a world with no maps hosts nothing",
                maps_root.display()
            ));
        }

        let mut pack = Pack::default();
        let mut folders = Vec::new();
        walk(&maps_root, &mut folders)?;
        folders.sort();
        for folder in folders {
            pack.read_map(&folder, log)?;
        }

        if pack.maps.is_empty() {
            return Err(format!(
                "the map pack at {} holds no map record",
                maps_root.display()
            ));
        }
        log.out(
            LogLevel::Info,
            "server.worldserver",
            format!(
                "Map pack {}: {} map record(s), {} entry point(s), {} arch(es), {} arrival(s).",
                maps_root.display(),
                pack.maps.len(),
                pack.spawns.len(),
                pack.arches.len(),
                pack.arrivals.len()
            ),
        );
        Ok(pack)
    }

    /// Every map record, and every spawn point.
    pub fn load_maps(&self) -> Result<(Vec<MapEntry>, Vec<MapSpawn>), String> {
        Ok((self.maps.clone(), self.spawns.clone()))
    }

    /// Every arrival point.
    pub fn arrivals(&self) -> Result<Vec<Arrival>, String> {
        Ok(self.arrivals.clone())
    }

    /// Every arch.
    pub fn arches(&self) -> Result<Vec<Arch>, String> {
        Ok(self.arches.clone())
    }

    /// One map's folder: its record, its spawn points and its doorways.
    fn read_map(&mut self, folder: &Path, log: &Log) -> Result<(), String> {
        let id = folder
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
            .ok_or_else(|| {
                format!(
                    "{} is not a map folder: a map is a folder named by its id",
                    folder.display()
                )
            })?;

        let basic = folder.join("basic.json");
        let document = Document::read(&basic).map_err(|error| error.to_string())?;
        let record = document.record("")?;

        // The folder is the id, and the record must say the same one. A disagreement is the file
        // lying about which map it describes, and a reader cannot pick a winner.
        let stated = number(&record, "map_id")?
            .ok_or_else(|| format!("{} states no map_id", basic.display()))?;
        if stated != u64::from(id) {
            return Err(format!(
                "{} is folder {} but states map_id {}; a map's folder is its id",
                basic.display(),
                id,
                stated
            ));
        }

        let name = record.text("name")?.unwrap_or_default();
        self.maps.push(MapEntry {
            id,
            // The record's kind, when it has one: a record with none is a destination a client can
            // name and this world cannot load, which is what the rows call `NULL`.
            kind: number(&record, "map_type")?.map(|value| value as u8),
            file_id: number(&record, "map_file_id")?.unwrap_or(0) as u32,
            name: if name.is_empty() {
                format!("map {id}")
            } else {
                name
            },
            instance_value: number(&record, "instance_value")?.map(|value| value as u32),
            name_id: number(&record, "name_id")?.map(|value| value as u32),
        });

        let spawns = folder.join("spawns.json");
        if spawns.exists() {
            let document = Document::read(&spawns).map_err(|error| error.to_string())?;
            let record = document.record("")?;
            for point in record.rows("spawns")? {
                self.spawns.push(MapSpawn {
                    map_id: id,
                    index: number(&point, "idx")?.unwrap_or(0) as i16,
                    x: float(&point, "x")?,
                    y: float(&point, "y")?,
                    plane: number(&point, "plane")?.unwrap_or(0) as i16,
                    angle: optional_float(&point, "angle")?,
                });
            }
        } else {
            log.out(
                LogLevel::Debug,
                "server.worldserver",
                format!(
                    "Map {id} has no spawns.json: it has no measured entry point, and a character cannot be placed in it before one is measured."
                ),
            );
        }

        let portals = folder.join("portals.json");
        if portals.exists() {
            let document = Document::read(&portals).map_err(|error| error.to_string())?;
            let record = document.record("")?;
            for arch in record.rows("arches")? {
                self.arches.push(Arch {
                    from_map: id,
                    idx: number(&arch, "idx")?.unwrap_or(0) as u32,
                    to_map: number(&arch, "to_map")?.map(|value| value as u32),
                    from: (float(&arch, "from_x")?, float(&arch, "from_y")?),
                    to: (float(&arch, "to_x")?, float(&arch, "to_y")?),
                });
            }
            for arrival in record.rows("arrivals")? {
                // The file's own key is `from` (`ASURA/crates/asura-world/src/portals.rs:16-22`,
                // `Arrival { from, x, y, angle }`): the receiving map states where a character
                // arriving *from* that map stands. Reading a different name here left every arrival
                // with source map 0, so a doorway crossing could not find its landing and a map
                // with no spawn list (an explorable such as Lakeside County) refused its load.
                self.arrivals.push(Arrival {
                    map_id: id,
                    from_map: number(&arrival, "from")?.unwrap_or(0) as u32,
                    x: float(&arrival, "x")?,
                    y: float(&arrival, "y")?,
                    plane: number(&arrival, "plane")?.unwrap_or(0) as i16,
                    angle: optional_float(&arrival, "angle")?,
                });
            }
        }
        Ok(())
    }
}

/// The pack answers the world's map questions: it is one of the two sources a world can be built from.
impl crate::maps::MapSource for Pack {
    fn load_maps(
        &self,
    ) -> Result<(Vec<crate::maps::MapEntry>, Vec<crate::maps::MapSpawn>), String> {
        Pack::load_maps(self)
    }

    fn arrivals(&self) -> Result<Vec<crate::maps::Arrival>, String> {
        Pack::arrivals(self)
    }

    fn arches(&self) -> Result<Vec<crate::maps::Arch>, String> {
        Pack::arches(self)
    }
}
/// Every folder under `root` that holds a `basic.json`, in one pass.
fn walk(root: &Path, folders: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("{} cannot be listed: {error}", root.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("{} cannot be listed: {error}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, folders)?;
        } else if path.file_name().and_then(|name| name.to_str()) == Some("basic.json") {
            if let Some(folder) = path.parent() {
                folders.push(folder.to_path_buf());
            }
        }
    }
    Ok(())
}

/// One scalar number from a record.
fn number(
    record: &common::configuration::json::Record<'_>,
    field: &str,
) -> Result<Option<u64>, String> {
    let Some(text) = record.text(field)? else {
        return Ok(None);
    };
    text.trim().parse::<u64>().map(Some).map_err(|_| {
        format!(
            "> Config: {}.{field} is '{}', which is not a whole number",
            record.at(),
            text
        )
    })
}

/// One scalar float from a record that must state it.
fn float(record: &common::configuration::json::Record<'_>, field: &str) -> Result<f32, String> {
    optional_float(record, field)?.ok_or_else(|| {
        format!(
            "> Config: {} states no '{}', and a point needs both of its axes",
            record.at(),
            field
        )
    })
}

/// One scalar float from a record that may leave it out.
fn optional_float(
    record: &common::configuration::json::Record<'_>,
    field: &str,
) -> Result<Option<f32>, String> {
    let Some(text) = record.text(field)? else {
        return Ok(None);
    };
    text.trim().parse::<f32>().map(Some).map_err(|_| {
        format!(
            "> Config: {}.{field} is '{}', which is not a number",
            record.at(),
            text
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Pack {
        let log = Log::new();
        Pack::load("../config/World/Vanilla", &log).expect("the shipped pack reads")
    }

    #[test]
    fn the_shipped_pack_is_the_whole_catalogue() {
        let pack = shipped();
        let (maps, spawns) = pack.load_maps().expect("the maps");
        assert_eq!(maps.len(), 265, "every map record the client can resolve");
        // 467, not 466: the third entry point of map 148 left the SQL seed and joined the pack in
        // `9d508ba`, and the count is the pack's own statement of what it states.
        assert_eq!(spawns.len(), 467, "and every entry point the pack states");
        assert_eq!(pack.arches().expect("the arches").len(), 28);
        assert_eq!(pack.arrivals().expect("the arrivals").len(), 28);

        // A record that states a kind and a blob is a map this world can host; one that states only
        // an instance value is a destination the client can name and this world cannot load.
        let hostable = maps.iter().filter(|map| map.kind.is_some()).count();
        assert_eq!(hostable, 145, "the hostable records");
    }

    /// An arrival names the map it comes from, under the file's own key (`from`, ASURA's
    /// `portals.rs` `Arrival { from, x, y, angle }`). A reader that looks for another name leaves
    /// every arrival with source map 0, and then a doorway crossing finds no landing: the map it
    /// leads to (an explorable such as Lakeside County, which states no spawn list) refuses its load.
    #[test]
    fn an_arrival_names_the_map_it_comes_from() {
        let pack = shipped();
        let arrivals = pack.arrivals().expect("the arrivals");
        let lakeside = arrivals
            .iter()
            .find(|arrival| arrival.map_id == 146 && arrival.from_map == 148)
            .expect("map 146 states an arrival from 148 in its own portals.json");
        assert!(
            (lakeside.x - 6602.0).abs() < 0.01 && (lakeside.y - 4485.0).abs() < 0.01,
            "the arrival is the file's own point, {lakeside:?}"
        );
        assert!(
            arrivals.iter().all(|arrival| arrival.from_map != 0),
            "an arrival with source map 0 is a read that missed the file's `from` key"
        );
    }

    #[test]
    fn a_map_is_named_and_blobbed_as_its_record_states() {
        let pack = shipped();
        let (maps, _) = pack.load_maps().expect("the maps");
        let city = maps
            .iter()
            .find(|map| map.id == 148)
            .expect("Ascalon City (pre-Searing)");
        assert_eq!(city.name, "Ascalon City (pre-Searing)");
        assert_eq!(city.file_id, 113_021, "its blob in the archive");
        assert!(
            city.instance_value.is_some(),
            "and its instance-table value"
        );
    }

    #[test]
    fn a_folder_that_disagrees_with_its_record_is_refused() {
        let root = std::env::temp_dir().join("kryta-pack-test");
        let folder = root.join("maps").join("Prophecies").join("148");
        std::fs::create_dir_all(&folder).expect("the folder");
        std::fs::write(
            folder.join("basic.json"),
            "{\"map_id\": 149, \"map_type\": 0, \"map_file_id\": 1, \"name\": \"not 148\"}",
        )
        .expect("the record");
        let error = Pack::load(&root, &Log::new()).expect_err("a lie is refused");
        assert!(error.contains("folder 148"), "{error}");
        std::fs::remove_dir_all(&root).ok();
    }
}
