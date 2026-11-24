//! Navigation: the server-owned extracted path-graph cache.
//!
//! The world reads this cache and never a client archive. A cache may omit maps; an omitted map has
//! no graph. This area inspects the cache and can replace one selected map from the explicit
//! offline archive `MapArchiveFile` names. The cache is an operator asset, not state the running
//! world owns, so neither action goes through the world's operation queue.

use serde_json::{Value, json};

use crate::api::{Call, Reply, Route, bad, broken, get, post, unavailable};
use crate::path_graphs;
use crate::state::Manage;

/// The navigation area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/navigation", inspect),
        post("/api/v1/navigation/dump", dump),
    ]
}

/// `GET /api/v1/navigation`: the graphs the cache holds.
fn inspect(manage: &Manage, _: &Call) -> Reply {
    let inspection = path_graphs::inspect(&manage.path_graphs_file).map_err(broken)?;
    let names = manage.map_names();
    let maps: Vec<Value> = inspection
        .maps
        .iter()
        .map(|graph| {
            let name = u32::try_from(graph.map_id).ok().and_then(|id| names.get(&id));
            json!({
                "map_id": graph.map_id,
                "name": name,
                "file_id": graph.file_id,
                "planes": graph.planes,
                "trapezoids": graph.trapezoids,
            })
        })
        .collect();
    Ok(json!({
        "path": inspection.path,
        "bytes": inspection.bytes,
        "maps": maps,
        "archive_configured": !manage.map_archive_file.trim().is_empty(),
    }))
}

/// `POST /api/v1/navigation/dump`: import one map's graph from the archive into the cache.
fn dump(manage: &Manage, call: &Call) -> Reply {
    let map_id = call
        .text("map_id")
        .parse::<u32>()
        .map_err(|_| bad("A numeric map id is required."))?;
    if manage.map_archive_file.trim().is_empty() {
        return Err(unavailable(
            "MapArchiveFile is empty in manage.json; the panel has no archive selected for offline extraction.",
        ));
    }
    let (maps, _) = manage
        .world_pack()
        .and_then(|pack| pack.load_maps())
        .map_err(unavailable)?;
    let map = maps
        .iter()
        .find(|map| map.id == map_id)
        .ok_or_else(|| bad(format!("Map {map_id} is not in the world pack.")))?;
    let result = path_graphs::dump_map(&manage.path_graphs_file, &manage.map_archive_file, map)
        .map_err(bad)?;
    manage.say(format!(
        "navigation: map {} dumped from blob {:#X} into {}",
        result.map_id, result.file_id, result.cache
    ));
    Ok(json!({
        "map_id": result.map_id,
        "file_id": result.file_id,
        "cache": result.cache,
        "graph_count": result.graph_count,
        "bytes": result.bytes,
    }))
}
