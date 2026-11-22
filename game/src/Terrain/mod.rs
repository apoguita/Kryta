//! `game/Terrain/` - the reference's `src/server/game/Grids/` (18 files).
//!
//! Renamed: the reference calls this directory `Grids/`, which is its terrain and its path finder.
//!
//! What is here is the working implementation's path finder, whole: the graph types and their importer
//! (`MapData.rs`, port of `map_data.rs`), the dumped form (`GraphDump.rs`, `graph_dump.rs`), the fixed
//! point arithmetic (`PathTables.rs`, `path_tables.rs`), and the finder with the graphs a world serves
//! (`Pathfind.rs`, `pathfind.rs` plus `pathfind/directional.rs`). A walk is answered from the map's
//! own graph, imported from the client archive or from a dump; a map without one answers `NoGraph`,
//! which is what the working implementation answers for a map it holds no graph for
//! (`ASURA/crates/asura-world/src/agent.rs:570`).

#[path = "Pathfind.rs"]
pub mod pathfind;

#[path = "GraphDump.rs"]
pub mod graph_dump;

#[path = "MapData.rs"]
pub mod map_data;

#[path = "PathTables.rs"]
pub mod path_tables;

pub use graph_dump::{DumpedGraph, decode, encode};
pub use map_data::{
    Bounds, NodeRef, PathStaticData, Plane, Portal, SinkNode, Trapezoid, Vec2f, XNode, YNode,
};
pub use path_tables::{path_reciproc, path_sqrt};
pub use pathfind::{PathGraphs, Pos, Walk, Waypoint};
