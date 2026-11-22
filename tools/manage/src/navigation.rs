//! The manager's offline navigation-cache operations.
//!
//! A serving world reads the cache only. This module is deliberately owned by the management tool:
//! its explicit archive argument is an import source, never a world-server dependency. The selected-map
//! export follows ASURA's `dump_path_graphs` importer at
//! `ASURA/crates/asura-worldserver/src/main.rs:571-665`.

use game::pack::GwDat;
use game::terrain::graph_dump::{DumpedGraph, decode, encode};
use game::terrain::map_data::PathStaticData;
use game::terrain::pathfind::find_path_blob;
use game::world::MapEntry;

/// One graph the cache holds, with the counts an operator needs to inspect it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapGraph {
    /// The configured map id, or `-1` for an archive-wide graph whose map is resolved by file id.
    pub map_id: i32,
    /// The source map blob id.
    pub file_id: u32,
    /// The number of planes in the graph.
    pub planes: usize,
    /// The number of trapezoids in the graph.
    pub trapezoids: u32,
}

/// A decoded cache and its on-disk size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    /// The path that was inspected.
    pub path: String,
    /// Bytes in that file.
    pub bytes: usize,
    /// Every graph the cache carries.
    pub maps: Vec<MapGraph>,
}

/// The result of adding or replacing one map graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpResult {
    /// The map that was exported.
    pub map_id: u32,
    /// The archive blob the graph came from.
    pub file_id: u32,
    /// The cache that was updated.
    pub cache: String,
    /// Graph count after the update.
    pub graph_count: usize,
    /// Bytes written.
    pub bytes: usize,
}

/// Read the portable cache without touching a client archive.
pub fn inspect(path: &str) -> Result<Inspection, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let graphs = decode(&bytes).map_err(|error| format!("cannot decode {path}: {error}"))?;
    Ok(Inspection {
        path: path.to_string(),
        bytes: bytes.len(),
        maps: graphs
            .into_iter()
            .map(|graph| MapGraph {
                map_id: graph.map_id,
                file_id: graph.map_file_id,
                planes: graph.data.planes.len(),
                trapezoids: graph.data.traps_count,
            })
            .collect(),
    })
}

/// Import one selected map and replace only its graph in the portable cache.
pub fn dump_map(
    cache_path: &str,
    archive_path: &str,
    map: &MapEntry,
) -> Result<DumpResult, String> {
    if map.file_id == 0 {
        return Err(format!("map {} has no path-data file id", map.id));
    }
    let existing = std::fs::read(cache_path)
        .map_err(|error| format!("cannot read cache {cache_path}: {error}"))?;
    let mut graphs =
        decode(&existing).map_err(|error| format!("cannot decode {cache_path}: {error}"))?;

    let mut archive = GwDat::open(archive_path)
        .map_err(|error| format!("cannot open archive {archive_path}: {error}"))?;
    let (file_id, bytes) = find_path_blob(&mut archive, map.file_id, map.id as i32)
        .map_err(|error| format!("cannot read map {} from {archive_path}: {error}", map.id))?
        .ok_or_else(|| {
            format!(
                "map {} blob {:#X} is absent from {archive_path}",
                map.id, map.file_id
            )
        })?;
    let data = PathStaticData::import(&bytes)
        .map_err(|error| format!("map {} blob {file_id:#X} is not path data: {error}", map.id))?;

    graphs.retain(|graph| graph.map_file_id != file_id && graph.map_id != map.id as i32);
    graphs.push(DumpedGraph {
        map_id: map.id as i32,
        map_file_id: file_id,
        data,
    });
    let output = encode(&graphs);
    std::fs::write(cache_path, &output)
        .map_err(|error| format!("cannot write cache {cache_path}: {error}"))?;
    Ok(DumpResult {
        map_id: map.id,
        file_id,
        cache: cache_path.to_string(),
        graph_count: graphs.len(),
        bytes: output.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_cache_is_an_inspection_error() {
        let error = inspect("no-such-path-graphs.bin").unwrap_err();
        assert!(error.contains("cannot read"));
    }

    #[test]
    fn the_shipped_cache_is_a_decodable_partial_map_set() {
        let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("path-graphs.bin");
        let inspection = inspect(&cache.to_string_lossy()).expect("the shipped cache decodes");
        assert!(inspection.bytes > 1_000_000);
        assert!(!inspection.maps.is_empty());
        assert!(inspection.maps.iter().all(|map| map.trapezoids > 0));
    }
}
