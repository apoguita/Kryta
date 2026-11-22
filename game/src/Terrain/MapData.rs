//! A map's path graph: the types the reference's A* consumes, and the box its trapezoids occupy.
//!
//! Ported from `ASURA/crates/asura-world/src/map_data.rs`, first section, **every type with its own
//! name, its own fields in their own order**. Nothing here is renamed, regrouped or tidied: the field
//! order is the order the reference's structs read (`GmMapDataImport.h`, `GmPaths.h`), and the dump
//! format (`graph_dump.rs`, ported next) writes exactly these fields in this order.
//!
//! The importer that builds these from a `Gw.dat` blob is a later piece of this subject. The graph
//! itself - planes, trapezoids, split nodes, portals - is what the finder searches, and this file is
//! that graph and nothing else.
//!
//! Reference anchors:
//!
//! * `Vec2f` - `Vec2fSub`, `Vec2fCross` (`GmPolygon.h`);
//! * `NodeRef` - the reference stores pointers, this stores an array position; `NodeType` plus an
//!   index is the same identity (`GmMapDataImport.c:9`);
//! * `XNode`, `YNode`, `SinkNode`, `Trapezoid`, `Portal`, `Plane` - the import's own records;
//! * `PathStaticData` - one map's planes and the trapezoid counter they share;
//! * `Bounds` - the `worldDims` an agent's position is checked against.
#![allow(dead_code)] // the importer container arrives ahead of the plane readers; see docs/port/09-parity-audit.md 7b item 4

/// `PATH_DATA_SIGNATURE` / `PATH_DATA_VERSION`: the first eight bytes of the path section.
pub const PATH_DATA_SIGNATURE: u32 = 0xEEFE_704C;

/// The version a path section states, which is the only one this reader accepts.
pub const PATH_DATA_VERSION: u32 = 12;

/// `STAGE_SHIFT`: a chunk id's stage lives in its top nibble, its `ChunkId` in the rest.
pub const STAGE_SHIFT: u32 = 28;

/// The stage a map's own data sits at: `GmImportMap` builds `(2 << STAGE_SHIFT) | ChunkId_Path`.
pub const STAGE_MAP: u32 = 2;

/// The three arrays a node id can name, in the order their numbering starts (`GmMapDataImport.c:9`).
pub const FIRST_X_NODE_INDEX: u32 = 0;

/// Where the y-node numbering starts.
pub const FIRST_Y_NODE_INDEX: u32 = 0x4000_0000;

/// Where the sink-node numbering starts.
pub const FIRST_SINK_NODE_INDEX: u32 = 0x8000_0000;

/// `NodeType`, the byte `PathImport_ReadRootNode` reads: a split along a line.
pub const NODE_TYPE_X: u8 = 0;

/// `NodeType`: a split along the horizontal.
pub const NODE_TYPE_Y: u8 = 1;

/// `NodeType`: a leaf, which is a trapezoid.
pub const NODE_TYPE_SINK: u8 = 2;

/// A 2-D position, the reference's `Vec2f`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2f {
    /// The first axis.
    pub x: f32,
    /// The second axis.
    pub y: f32,
}

impl Vec2f {
    /// `Vec2fSub`: the vector from `other` to this one.
    ///
    /// The name is the reference function's, so it stays even though Rust has an operator for it.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: Vec2f) -> Vec2f {
        Vec2f {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    /// `Vec2fCross`: the z of the cross product, which is the sign of "left or right of a direction".
    pub fn cross(self, other: Vec2f) -> f32 {
        (self.x * other.y) - (self.y * other.x)
    }
}

/// A pointer to a node, as a position in one of a plane's three node arrays. `NodeType` plus the array
/// index is what the reference's node id encodes, so this is the same identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRef {
    /// A node in the plane's x-node array.
    X(usize),
    /// A node in the plane's y-node array.
    Y(usize),
    /// A node in the plane's sink-node array.
    Sink(usize),
}

/// `XNode`: a split along a line, `left`/`right` being the halves.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct XNode {
    /// The point the line passes through.
    pub pos: Vec2f,
    /// The direction the line runs in.
    pub dir: Vec2f,
    /// The half on the left, when it holds a node.
    pub left: Option<NodeRef>,
    /// The half on the right, when it holds a node.
    pub right: Option<NodeRef>,
}

/// `YNode`: a split along the horizontal, `above`/`bellow` being the halves (the reference's spelling).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct YNode {
    /// The point the split passes through.
    pub pos: Vec2f,
    /// The half above it, when it holds a node.
    pub above: Option<NodeRef>,
    /// The half below it, when it holds a node (the reference spells it `bellow`).
    pub bellow: Option<NodeRef>,
}

/// `SinkNode`: a leaf, which is a trapezoid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SinkNode {
    /// The trapezoid's position in the plane's own array.
    pub trap: usize,
}

/// `PathTrapezoid`: a convex cell of the plane. The six floats are its corners; `portal_left` and
/// `portal_right` name portals, not trapezoids.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Trapezoid {
    /// `trap_id`: unique across **all** planes, because it comes from the static data's counter.
    pub trap_id: u32,
    /// The neighbour above-left, as an index into `portals_traps`.
    pub top_left: Option<usize>,
    /// The neighbour above-right.
    pub top_right: Option<usize>,
    /// The neighbour below-left.
    pub bottom_left: Option<usize>,
    /// The neighbour below-right.
    pub bottom_right: Option<usize>,
    /// The portal on its left side.
    pub portal_left: u16,
    /// The portal on its right side.
    pub portal_right: u16,
    /// The top edge's height.
    pub yt: f32,
    /// The bottom edge's height.
    pub yb: f32,
    /// The top-left corner's x.
    pub xtl: f32,
    /// The top-right corner's x.
    pub xtr: f32,
    /// The bottom-left corner's x.
    pub xbl: f32,
    /// The bottom-right corner's x.
    pub xbr: f32,
}

/// `PathPortal`: a way from one plane to another through a trapezoid's side.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Portal {
    /// The plane this portal belongs to.
    pub portal_plane_id: u16,
    /// The plane it leads to.
    pub neighbor_plane_id: u16,
    /// The portal's flags, as the import read them.
    pub flags: u8,
    /// The portal on the other plane with the same shared id, as `(plane, portal)`.
    pub pair: Option<(u16, usize)>,
    /// How many trapezoids name this portal.
    pub traps_count: u32,
    /// A position in the plane's `portals_traps`, which is a list of trapezoids.
    pub traps: usize,
}

/// `PathPlane`: one plane of a map's geometry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plane {
    /// Which plane this is.
    pub plane_id: u16,
    /// The plane's vectors, as the import read them.
    pub vectors: Vec<Vec2f>,
    /// The plane's convex cells.
    pub trapezoids: Vec<Trapezoid>,
    /// The splits along a line.
    pub xnodes: Vec<XNode>,
    /// The splits along the horizontal.
    pub ynodes: Vec<YNode>,
    /// The leaves.
    pub sink_nodes: Vec<SinkNode>,
    /// The ways from this plane to another.
    pub portals: Vec<Portal>,
    /// The reference's `h000C`: a count of records the tag-11 section holds, eight bytes each.
    pub h000c: u32,
    /// The trapezoids a portal leads to, `None` where the reference would store a NULL.
    pub portals_traps: Vec<Option<usize>>,
    /// Where walking the splits starts (`None` where the reference's pointer would be NULL).
    pub root_node: Option<NodeRef>,
}

/// `PathStaticData`: every plane of one map, and the trapezoid counter they share.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathStaticData {
    /// The map's planes, in the order the import read them.
    pub planes: Vec<Plane>,
    /// `traps_count`: the counter every trapezoid's `trap_id` comes from.
    pub traps_count: u32,
}

/// The box a plane's trapezoids occupy: the `worldDims` the client checks an agent against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// The lowest x.
    pub x0: f32,
    /// The lowest y.
    pub y0: f32,
    /// The highest x.
    pub x1: f32,
    /// The highest y.
    pub y1: f32,
}

impl PathStaticData {
    /// `PathFindTrapezoid` (`GmPaths.c:11`): walk a plane's splits from the root until a leaf answers.
    ///
    /// `plane` is an index into `planes` (the reference's `pos.plane`), and the answer is the trapezoid's
    /// position in that plane's array, where the reference returns a pointer.
    pub fn trapezoid_at(&self, pos: Vec2f, plane: u16) -> Option<(u16, usize)> {
        let plane_state = self.planes.get(usize::from(plane))?;
        let mut node = plane_state.root_node?;
        // A dead end in the graph is the reference's NULL node pointer, which ends the walk with no answer.
        loop {
            node = match node {
                NodeRef::X(index) => {
                    let xnode = plane_state.xnodes.get(index)?;
                    let offset = pos.sub(xnode.pos);
                    let side = xnode.dir.cross(offset);
                    if side < 0.0 {
                        xnode.left?
                    } else {
                        xnode.right?
                    }
                }
                NodeRef::Y(index) => {
                    let ynode = plane_state.ynodes.get(index)?;
                    if pos.y < ynode.pos.y {
                        ynode.bellow?
                    } else if pos.y > ynode.pos.y {
                        ynode.above?
                    } else if pos.x < ynode.pos.x {
                        ynode.bellow?
                    } else {
                        ynode.above?
                    }
                }
                NodeRef::Sink(index) => {
                    return Some((plane, plane_state.sink_nodes.get(index)?.trap));
                }
            };
        }
    }

    /// Every trapezoid of the map, as `(plane, position in that plane)`.
    pub fn trapezoids(&self) -> impl Iterator<Item = (u16, usize)> + '_ {
        self.planes.iter().flat_map(|plane| {
            let plane_id = plane.plane_id;
            (0..plane.trapezoids.len()).map(move |index| (plane_id, index))
        })
    }
}

impl Bounds {
    /// Whether a point is inside the box, edges included.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.x0 <= x && x <= self.x1 && self.y0 <= y && y <= self.y1
    }
}

impl PathStaticData {
    /// The box of one plane, as the union of its trapezoids' corners.
    ///
    /// The client asserts that an agent's position is inside the `worldDims` of the plane it is on
    /// (`agint.h:929`, `pos.y >= worldDims.y0`), so a destination outside this box is a crash rather
    /// than a walk. A plane with no trapezoids has no box at all and `None` says so.
    pub fn plane_bounds(&self, plane: u16) -> Option<Bounds> {
        let plane = self.planes.get(usize::from(plane))?;
        let mut bounds: Option<Bounds> = None;
        for trapezoid in &plane.trapezoids {
            let left = trapezoid
                .xtl
                .min(trapezoid.xbl)
                .min(trapezoid.xtr)
                .min(trapezoid.xbr);
            let right = trapezoid
                .xtl
                .max(trapezoid.xbl)
                .max(trapezoid.xtr)
                .max(trapezoid.xbr);
            let low = trapezoid.yt.min(trapezoid.yb);
            let high = trapezoid.yt.max(trapezoid.yb);
            bounds = Some(match bounds {
                None => Bounds {
                    x0: left,
                    y0: low,
                    x1: right,
                    y1: high,
                },
                Some(current) => Bounds {
                    x0: current.x0.min(left),
                    y0: current.y0.min(low),
                    x1: current.x1.max(right),
                    y1: current.y1.max(high),
                },
            });
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Vec2fSub` and `Vec2fCross` are the reference's own, sign included.
    #[test]
    fn the_vector_operations_are_the_references() {
        let a = Vec2f { x: 3.0, y: 4.0 };
        let b = Vec2f { x: 1.0, y: 1.0 };
        assert_eq!(a.sub(b), Vec2f { x: 2.0, y: 3.0 });
        assert_eq!(a.cross(b), (3.0 * 1.0) - (4.0 * 1.0));
        // The cross product's sign says which side of a direction the other vector is on.
        let right = Vec2f { x: 1.0, y: 0.0 };
        let up = Vec2f { x: 0.0, y: 1.0 };
        assert_eq!(
            right.cross(up),
            1.0,
            "up is to the left of right in this order"
        );
        assert_eq!(up.cross(right), -1.0, "and the other way round is negative");
    }

    /// A plane's splits are walked from its root to a leaf, and the leaf names the trapezoid: a point
    /// right of the split takes `right`, a dead end answers `None` as the reference's NULL pointer does.
    #[test]
    fn a_point_is_walked_down_the_splits_to_its_trapezoid() {
        let data = PathStaticData {
            traps_count: 2,
            planes: vec![Plane {
                plane_id: 0,
                trapezoids: vec![
                    Trapezoid {
                        trap_id: 0,
                        xtl: 0.0,
                        xtr: 0.0,
                        ..Trapezoid::default()
                    },
                    Trapezoid {
                        trap_id: 1,
                        xtl: 10.0,
                        xtr: 10.0,
                        ..Trapezoid::default()
                    },
                ],
                // A split at x = 5 running along -y, so the point x < 5 is on its left: the reference
                // takes `side < 0` as left, and `cross(dir, offset)` is negative there.
                xnodes: vec![XNode {
                    pos: Vec2f { x: 5.0, y: 0.0 },
                    dir: Vec2f { x: 0.0, y: -1.0 },
                    left: Some(NodeRef::Sink(0)),
                    right: Some(NodeRef::Sink(1)),
                }],
                sink_nodes: vec![SinkNode { trap: 0 }, SinkNode { trap: 1 }],
                root_node: Some(NodeRef::X(0)),
                ..Plane::default()
            }],
        };
        assert_eq!(
            data.trapezoid_at(Vec2f { x: 1.0, y: 0.0 }, 0),
            Some((0, 0)),
            "left of the split"
        );
        assert_eq!(
            data.trapezoid_at(Vec2f { x: 9.0, y: 0.0 }, 0),
            Some((0, 1)),
            "right of it"
        );
        assert_eq!(
            data.trapezoid_at(Vec2f { x: 1.0, y: 0.0 }, 3),
            None,
            "no such plane"
        );
        let rootless = PathStaticData::default();
        assert_eq!(
            rootless.trapezoid_at(Vec2f::default(), 0),
            None,
            "a plane with no root"
        );
    }

    /// The plane's box is the union of its trapezoids' corners, edges included.
    #[test]
    fn a_planes_bounds_are_the_union_of_its_trapezoids() {
        let data = PathStaticData {
            traps_count: 2,
            planes: vec![Plane {
                plane_id: 0,
                trapezoids: vec![
                    Trapezoid {
                        trap_id: 0,
                        yt: 10.0,
                        yb: 0.0,
                        xtl: 1.0,
                        xtr: 5.0,
                        xbl: 0.0,
                        xbr: 4.0,
                        ..Trapezoid::default()
                    },
                    Trapezoid {
                        trap_id: 1,
                        yt: 20.0,
                        yb: -5.0,
                        xtl: -3.0,
                        xtr: 2.0,
                        xbl: -4.0,
                        xbr: 3.0,
                        ..Trapezoid::default()
                    },
                ],
                ..Plane::default()
            }],
        };
        let bounds = data.plane_bounds(0).expect("the plane has trapezoids");
        assert_eq!(bounds.x0, -4.0);
        assert_eq!(bounds.y0, -5.0);
        assert_eq!(bounds.x1, 5.0);
        assert_eq!(bounds.y1, 20.0);
        assert!(bounds.contains(-4.0, -5.0), "edges are inside");
        assert!(!bounds.contains(-4.5, 0.0), "and past them is not");
        assert_eq!(data.plane_bounds(7), None, "a plane the map does not have");
        let empty = PathStaticData::default();
        assert_eq!(
            empty.plane_bounds(0),
            None,
            "a plane with no trapezoids has no box"
        );
    }

    /// The constants are the reference's, and the node index bases do not overlap.
    #[test]
    fn the_constants_are_the_references() {
        assert_eq!(PATH_DATA_SIGNATURE, 0xEEFE_704C);
        assert_eq!(PATH_DATA_VERSION, 12);
        assert_eq!(STAGE_SHIFT, 28);
        assert_eq!(STAGE_MAP, 2);
        assert_eq!(FIRST_X_NODE_INDEX, 0);
        assert_eq!(FIRST_Y_NODE_INDEX, 0x4000_0000);
        assert_eq!(FIRST_SINK_NODE_INDEX, 0x8000_0000);

        assert_eq!([NODE_TYPE_X, NODE_TYPE_Y, NODE_TYPE_SINK], [0, 1, 2]);
    }
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/map_data.rs, lines 145-482: the blob's container - the records a map
// file is made of - and the node-id lookup the plane readers use. The plane readers themselves
// (`read_planes` and the eleven `read_*` functions) are the next piece.
// ---------------------------------------------------------------------------------------------

/// `RIFF_SIGNATURE` (`GmMapDataImport.c:3`): the four bytes a map blob starts with.
pub const RIFF_SIGNATURE: &[u8] = b"ffna";

/// `RIFF_TYPES`: a type byte at or above this is not a container at all (`RiffCheckHeader`).
pub const RIFF_TYPES: u8 = 9;

/// Why a blob is not a map this reader can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// The first five bytes are not `"ffna"` plus a type below `RIFF_TYPES` (`RiffCheckHeader`).
    Header,
    /// The container's type byte is a known container but not one of the two map types.
    NotAMap {
        /// The type byte it carried.
        found: u8,
    },
    /// A record claims more bytes than the blob holds (`RiffParseChunks`).
    ChunkPastTheEnd,
    /// The container has no stage-2 path chunk (`GmImportMap`): no graph, so no walking.
    NoPathChunk,
    /// A section did not begin with the tag its reader requires. `found` is `None` when fewer than five
    /// bytes were left, which is how `ParseTaggedChunk` fails first.
    Tag {
        /// The tag the reader required.
        expected: u8,
        /// The tag it found, or `None` when there was not even a header.
        found: Option<u8>,
    },
    /// A section's header claims more payload than the blob has left (`ParseTaggedChunk`).
    TagPastTheEnd {
        /// The section's tag.
        tag: u8,
    },
    /// A field past the end of the bytes that were there.
    Truncated(&'static str),
    /// A payload does not have the shape its reader measured it against; the string says which.
    Shape(&'static str),
    /// An index does not name an element the plane has. The reference would read past its array here;
    /// this refuses instead of reading someone else's memory.
    Index {
        /// What the index was for.
        what: &'static str,
    },
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Header => {
                write!(f, "not a map blob: the header is not \"ffna\" and a type")
            }
            ImportError::NotAMap { found } => write!(f, "container type {found} is not a map"),
            ImportError::ChunkPastTheEnd => {
                write!(f, "a record claims more bytes than the blob holds")
            }
            ImportError::NoPathChunk => write!(f, "the container has no path chunk"),
            ImportError::Tag {
                expected,
                found: Some(found),
            } => write!(f, "expected section {expected}, found {found}"),
            ImportError::Tag {
                expected,
                found: None,
            } => write!(f, "expected section {expected}, ran out of bytes"),
            ImportError::TagPastTheEnd { tag } => {
                write!(f, "section {tag} claims more bytes than are left")
            }
            ImportError::Truncated(what) => write!(f, "ran out of bytes reading {what}"),
            ImportError::Shape(what) => write!(f, "{what}"),
            ImportError::Index { what } => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// One record of a map container: an id and the bytes it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapChunk<'a> {
    /// The record's id: a stage and a `ChunkId` in one word.
    pub chunk_id: u32,
    /// The bytes it holds.
    pub data: &'a [u8],
}

/// A map container's records, sorted by id so a lookup is a binary search (`RiffParseChunks` sorts for
/// `RiffGetChunk` for exactly that reason).
#[derive(Debug, Clone)]
pub struct Chunks<'a> {
    sorted: Vec<MapChunk<'a>>,
}

/// A reading cursor over the bytes of one section. The reference passes a `slice_uint8_t` by pointer and
/// advances it; this is that, with the bounds checks the reference leaves to the caller.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Take `count` bytes and advance past them.
    fn take(&mut self, count: usize, what: &'static str) -> Result<&'a [u8], ImportError> {
        if self.bytes.len() < count {
            return Err(ImportError::Truncated(what));
        }
        let (head, tail) = self.bytes.split_at(count);
        self.bytes = tail;
        Ok(head)
    }

    /// `ParseTaggedChunk`: a tag byte, a length, five bytes of header, and a promise that the payload is
    /// there. Returns the payload's length without consuming it - every caller reads its own.
    fn tagged(&mut self, tag: u8) -> Result<usize, ImportError> {
        let header = match self.take(5, "a section header") {
            Ok(header) => header,
            Err(_) => {
                return Err(ImportError::Tag {
                    expected: tag,
                    found: None,
                });
            }
        };
        if header[0] != tag {
            return Err(ImportError::Tag {
                expected: tag,
                found: Some(header[0]),
            });
        }
        let length = u32_at(header, 1) as usize;
        if self.bytes.len() < length {
            return Err(ImportError::TagPastTheEnd { tag });
        }
        Ok(length)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// `RiffParseAndGetType`: the container's type byte, or `None` when this is not a container.
pub fn riff_type(data: &[u8]) -> Option<u8> {
    // `RiffCheckHeader`: five bytes, the signature, and a type below the table's end.
    if data.len() < 5 || &data[0..4] != RIFF_SIGNATURE || data[4] >= RIFF_TYPES {
        return None;
    }
    Some(data[4])
}

impl<'a> Chunks<'a> {
    /// `RiffParseChunks`: split a blob into its records, then sort them by id.
    pub fn parse(data: &'a [u8]) -> Result<Chunks<'a>, ImportError> {
        if riff_type(data).is_none() {
            return Err(ImportError::Header);
        }

        let mut rest = &data[5..];
        let mut sorted = Vec::new();
        // `while (8 <= size)`: a tail shorter than a record header is not a record, and the reference
        // ignores it rather than complaining.
        while rest.len() >= 8 {
            let chunk_id = u32_at(rest, 0);
            let chunk_size = u32_at(rest, 4) as usize;
            if rest.len() < 8 + chunk_size {
                return Err(ImportError::ChunkPastTheEnd);
            }
            sorted.push(MapChunk {
                chunk_id,
                data: &rest[8..8 + chunk_size],
            });
            rest = &rest[8 + chunk_size..];
        }
        sorted.sort_by_key(|chunk| chunk.chunk_id);
        Ok(Chunks { sorted })
    }

    /// `RiffGetChunk`: the record with this id, by binary search over the sorted ids.
    pub fn get(&self, chunk_id: u32) -> Option<&MapChunk<'a>> {
        self.sorted
            .binary_search_by_key(&chunk_id, |chunk| chunk.chunk_id)
            .ok()
            .map(|index| &self.sorted[index])
    }

    /// Every record, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &MapChunk<'a>> {
        self.sorted.iter()
    }

    /// How many records the container holds.
    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    /// Whether the container holds no record at all.
    pub fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }
}

/// `GetNodeFromDatId`: a node id becomes the array it names and its position there. An id past the end
/// of its array is `None` - the reference's dangling NULL, not an error.
fn node_ref(node_id: u32, plane: &Plane) -> Option<NodeRef> {
    if node_id < FIRST_Y_NODE_INDEX {
        // `FIRST_X_NODE_INDEX` is zero, so the reference's subtraction is a no-op here.
        let index = (node_id - FIRST_X_NODE_INDEX) as usize;
        (index < plane.xnodes.len()).then_some(NodeRef::X(index))
    } else if node_id < FIRST_SINK_NODE_INDEX {
        let index = (node_id - FIRST_Y_NODE_INDEX) as usize;
        (index < plane.ynodes.len()).then_some(NodeRef::Y(index))
    } else {
        let index = (node_id - FIRST_SINK_NODE_INDEX) as usize;
        (index < plane.sink_nodes.len()).then_some(NodeRef::Sink(index))
    }
}
impl Trapezoid {
    /// How many bytes one trapezoid record is: `trap_ref`'s four neighbour indices, two portals and six
    /// corner floats (`GmMapDataImport.c`).
    pub const SIZE: usize = 44;
}

/// `GetTrapHelper`: a neighbour reference, where `UINT32_MAX` is the reference's own spelling of "none".
fn trap_ref(dat_idx: u32, count: usize) -> Result<Option<usize>, ImportError> {
    if dat_idx == u32::MAX {
        return Ok(None);
    }
    if dat_idx as usize >= count {
        return Err(ImportError::Index {
            what: "a trapezoid names a neighbour the plane does not have",
        });
    }
    Ok(Some(dat_idx as usize))
}

impl PathStaticData {
    /// `PathImport_PlaneReadCounts`: eight counts, which size the plane's arrays.
    fn read_counts(&mut self, index: usize, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        let length = reader.tagged(0)?;
        if length < 32 {
            return Err(ImportError::Shape(
                "the plane's count section is shorter than its eight counts",
            ));
        }
        let head = reader.take(32, "the plane's counts")?;
        let plane = &mut self.planes[index];
        plane.h000c = u32_at(head, 0);
        plane.vectors = vec![Vec2f::default(); u32_at(head, 4) as usize];
        plane.trapezoids = vec![Trapezoid::default(); u32_at(head, 8) as usize];
        plane.xnodes = vec![XNode::default(); u32_at(head, 12) as usize];
        plane.ynodes = vec![YNode::default(); u32_at(head, 16) as usize];
        plane.sink_nodes = vec![SinkNode::default(); u32_at(head, 20) as usize];
        plane.portals = vec![Portal::default(); u32_at(head, 24) as usize];
        plane.portals_traps = vec![None; u32_at(head, 28) as usize];
        Ok(())
    }

    /// `PathImport_PlaneTag11`: a section of `h000C` eight-byte records, which the reference skips without
    /// reading. It advances `h000C * 8` of the section's `sub_length` bytes, not all of them.
    ///
    /// Measured over the seven map blobs the catalogue names: every one of their 283 planes declares
    /// `sub_length == 2 * h000C * 8`, and the next section's tag sits exactly at the midpoint. The
    /// reference reads the next section from inside this one's declared bounds, and it works because the
    /// file is written that way - not because the arithmetic is right. Advancing `sub_length` instead
    /// would step over the vector section, and every plane would fail on its next tag.
    fn skip_tag11(&mut self, index: usize, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        let length = reader.tagged(11)?;
        let skip = self.planes[index].h000c as usize * 8;
        debug_assert!(
            skip <= length,
            "the reference asserts h000C * 8 <= sub_length"
        );
        reader.take(skip, "the plane's tag-11 section")?;
        Ok(())
    }

    /// `PathImport_PlaneReadVectors`: the positions every node and trapezoid edge refers to.
    fn read_vectors(&mut self, index: usize, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        let length = reader.tagged(1)?;
        let count = self.planes[index].vectors.len();
        if length != count * 8 {
            return Err(ImportError::Shape(
                "the vector section is not eight bytes per vector",
            ));
        }
        let rows = reader.take(length, "the plane's vectors")?;
        let plane = &mut self.planes[index];
        for (slot, row) in plane.vectors.iter_mut().zip(rows.as_chunks::<8>().0) {
            *slot = Vec2f {
                x: f32_at(row, 0),
                y: f32_at(row, 4),
            };
        }
        Ok(())
    }

    /// `PathImport_PlaneReadTrapezoids`: neighbours by index, two portals, six corner floats, and a
    /// `trap_id` from the import's own counter - so ids are unique across a map's planes.
    fn read_trapezoids(
        &mut self,
        index: usize,
        reader: &mut Reader<'_>,
    ) -> Result<(), ImportError> {
        let length = reader.tagged(2)?;
        let count = self.planes[index].trapezoids.len();
        if length != count * Trapezoid::SIZE {
            return Err(ImportError::Shape(
                "the trapezoid section is not 44 bytes per trapezoid",
            ));
        }
        let rows = reader.take(length, "the plane's trapezoids")?;

        let traps_count = &mut self.traps_count;
        for (slot, row) in self.planes[index]
            .trapezoids
            .iter_mut()
            .zip(rows.as_chunks::<44>().0)
        {
            *slot = Trapezoid {
                trap_id: *traps_count,
                top_left: trap_ref(u32_at(row, 0), count)?,
                top_right: trap_ref(u32_at(row, 4), count)?,
                bottom_left: trap_ref(u32_at(row, 8), count)?,
                bottom_right: trap_ref(u32_at(row, 12), count)?,
                portal_left: u16_at(row, 16),
                portal_right: u16_at(row, 18),
                yt: f32_at(row, 20),
                yb: f32_at(row, 24),
                xtl: f32_at(row, 28),
                xtr: f32_at(row, 32),
                xbl: f32_at(row, 36),
                xbr: f32_at(row, 40),
            };
            // The reference's `static_data->traps_count++`: a plain u32, wrapping rather than panicking.
            *traps_count = traps_count.wrapping_add(1);
        }
        Ok(())
    }

    /// `PathImport_ReadRootNode`: which array's first element the descent starts from.
    fn read_root_node(&mut self, index: usize, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        let length = reader.tagged(3)?;
        if length < 1 {
            return Err(ImportError::Shape("the root-node section is empty"));
        }
        let kind = reader.take(1, "the root node")?[0];
        let plane = &self.planes[index];
        // The reference stores `&array[0]` without checking the array has one; an empty array leaves it
        // NULL there, so a graph with no nodes of the named kind has no root here either.
        let root = match kind {
            NODE_TYPE_X => (!plane.xnodes.is_empty()).then_some(NodeRef::X(0)),
            NODE_TYPE_Y => (!plane.ynodes.is_empty()).then_some(NodeRef::Y(0)),
            NODE_TYPE_SINK => (!plane.sink_nodes.is_empty()).then_some(NodeRef::Sink(0)),
            _ => {
                return Err(ImportError::Shape(
                    "the root node is not one of the three node kinds",
                ));
            }
        };
        self.planes[index].root_node = root;
        Ok(())
    }

    /// `PathImport_ReadXNodes`: a position, a second position the direction points at, and the halves.
    fn read_xnodes(&mut self, index: usize, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        let length = reader.tagged(4)?;
        let count = self.planes[index].xnodes.len();
        if length != count * 16 {
            return Err(ImportError::Shape(
                "the x-node section is not sixteen bytes per node",
            ));
        }
        let rows = reader.take(length, "the plane's x nodes")?;

        for (idx, row) in rows.as_chunks::<16>().0.iter().enumerate() {
            let plane = &self.planes[index];
            let pos = vector_at(plane, u32_at(row, 0), "an x node's position")?;
            let toward = vector_at(plane, u32_at(row, 4), "an x node's direction")?;
            let left = node_ref(u32_at(row, 8), plane);
            let right = node_ref(u32_at(row, 12), plane);
            self.planes[index].xnodes[idx] = XNode {
                pos,
                dir: toward.sub(pos),
                left,
                right,
            };
        }
        Ok(())
    }
}

/// A vector reference, or an error naming what asked for it.
fn vector_at(plane: &Plane, index: u32, what: &'static str) -> Result<Vec2f, ImportError> {
    plane
        .vectors
        .get(index as usize)
        .copied()
        .ok_or(ImportError::Index { what })
}
impl PathStaticData {
    /// `PathImport_ReadYNodes`: a position and the halves above and below it.
    fn read_ynodes(&mut self, index: usize, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        let length = reader.tagged(5)?;
        let count = self.planes[index].ynodes.len();
        if length != count * 12 {
            return Err(ImportError::Shape(
                "the y-node section is not twelve bytes per node",
            ));
        }
        let rows = reader.take(length, "the plane's y nodes")?;

        for (idx, row) in rows.as_chunks::<12>().0.iter().enumerate() {
            let plane = &self.planes[index];
            let pos = vector_at(plane, u32_at(row, 0), "a y node's position")?;
            let above = node_ref(u32_at(row, 4), plane);
            let bellow = node_ref(u32_at(row, 8), plane);
            self.planes[index].ynodes[idx] = YNode { pos, above, bellow };
        }
        Ok(())
    }

    /// `PathImport_ReadSinkNodes`: which trapezoid each leaf is.
    fn read_sink_nodes(
        &mut self,
        index: usize,
        reader: &mut Reader<'_>,
    ) -> Result<(), ImportError> {
        let length = reader.tagged(6)?;
        let count = self.planes[index].sink_nodes.len();
        if length != count * 4 {
            return Err(ImportError::Shape(
                "the sink-node section is not four bytes per node",
            ));
        }
        let rows = reader.take(length, "the plane's sink nodes")?;

        let traps = self.planes[index].trapezoids.len();
        for (slot, row) in self.planes[index]
            .sink_nodes
            .iter_mut()
            .zip(rows.as_chunks::<4>().0)
        {
            let trap = u32_at(row, 0) as usize;
            if trap >= traps {
                return Err(ImportError::Index {
                    what: "a sink node names a trapezoid the plane does not have",
                });
            }
            *slot = SinkNode { trap };
        }
        Ok(())
    }

    /// `PathImport_ReadPortalTraps`: the trapezoids a portal leads to. An index past the plane's
    /// trapezoids leaves the reference's NULL, which is `None` here.
    fn read_portal_traps(
        &mut self,
        index: usize,
        reader: &mut Reader<'_>,
    ) -> Result<(), ImportError> {
        let length = reader.tagged(10)?;
        let count = self.planes[index].portals_traps.len();
        if length != count * 4 {
            return Err(ImportError::Shape(
                "the portal-trapezoid section is not four bytes per entry",
            ));
        }
        let rows = reader.take(length, "the plane's portal trapezoids")?;

        let traps = self.planes[index].trapezoids.len();
        for (slot, row) in self.planes[index]
            .portals_traps
            .iter_mut()
            .zip(rows.as_chunks::<4>().0)
        {
            let trap = u32_at(row, 0) as usize;
            *slot = (trap < traps).then_some(trap);
        }
        Ok(())
    }

    /// `PathImport_ReadPortals`: the ways between planes, paired with their counterpart.
    ///
    /// The pairing key is `(plane << 16) | shared_id`, which is why a portal records the shared id it was
    /// registered under: the counterpart is looked up as `(neighbor_plane << 16) | shared_id`.
    fn read_portals(
        &mut self,
        index: usize,
        reader: &mut Reader<'_>,
        pairs: &mut std::collections::HashMap<u32, (u16, usize)>,
    ) -> Result<(), ImportError> {
        let length = reader.tagged(9)?;
        let count = self.planes[index].portals.len();
        if length != count * 9 {
            return Err(ImportError::Shape(
                "the portal section is not nine bytes per portal",
            ));
        }
        let rows = reader.take(length, "the plane's portals")?;

        for (idx, row) in rows.as_chunks::<9>().0.iter().enumerate() {
            let traps_count = u32::from(u16_at(row, 0));
            let traps = u16_at(row, 2) as usize;
            let neighbor_plane_id = u16_at(row, 4);
            let shared_id = u32::from(u16_at(row, 6));
            let flags = row[8];

            let plane_id = self.planes[index].plane_id;
            self.planes[index].portals[idx] = Portal {
                portal_plane_id: plane_id,
                neighbor_plane_id,
                flags,
                pair: None,
                traps_count,
                traps,
            };

            let key = (u32::from(plane_id) << 16) | shared_id;
            pairs.insert(key, (plane_id, idx));

            let neighbor_key = (u32::from(neighbor_plane_id) << 16) | shared_id;
            if let Some(&(other_plane, other_idx)) = pairs.get(&neighbor_key) {
                self.planes[usize::from(plane_id)].portals[idx].pair =
                    Some((other_plane, other_idx));
                self.planes[usize::from(other_plane)].portals[other_idx].pair =
                    Some((plane_id, idx));
            }
        }
        Ok(())
    }
}
/// `ChunkId` (`GmMapDataImport.h:3`): what a record in the container holds. Only the path chunk is read
/// by this importer; the rest are named because the stage arithmetic needs the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChunkId {
    /// The map's own header record.
    Header = 0,
    /// The old editor's record.
    EditorOld = 1,
    /// The terrain.
    Terrain = 2,
    /// The zones.
    Zones = 3,
    /// The props.
    Props = 4,
    /// An obsolete record.
    Obsolete1 = 5,
    /// The water.
    Water = 6,
    /// The mission data.
    Mission = 7,
    /// The path graph: the one this importer reads.
    Path = 8,
    /// The environment.
    Environment = 9,
    /// The locations.
    Locations = 10,
    /// Another obsolete record.
    Obsolete2 = 11,
    /// The map parameters.
    MapParameters = 12,
    /// The editor's record.
    Editor = 13,
    /// The collision data.
    Collision = 14,
    /// The light data.
    Light = 15,
    /// The shore data.
    Shore = 16,
}

/// The id of the stage-2 path chunk, which is what `GmImportMap` looks for.
pub const PATH_CHUNK_ID: u32 = (STAGE_MAP << STAGE_SHIFT) | ChunkId::Path as u32;

/// `RIFF_MAP1` / `RIFF_MAP2` (`GmMapDataImport.h:24`): the two container types `GmImportMap` accepts.
pub const RIFF_MAP1: u8 = 1;

/// The second container type a map blob may carry.
pub const RIFF_MAP2: u8 = 3;

impl PathStaticData {
    /// `GmImportMap`: a whole map blob in, the graph out.
    ///
    /// The reference does the same three things: refuse a container that is not a map, split it into
    /// records, and import the stage-2 path chunk. Its `MapParameters` import is commented out in the
    /// source, so there is nothing there to be in parity with.
    pub fn import(map: &[u8]) -> Result<PathStaticData, ImportError> {
        let found = riff_type(map).ok_or(ImportError::Header)?;
        if found != RIFF_MAP1 && found != RIFF_MAP2 {
            return Err(ImportError::NotAMap { found });
        }

        let chunks = Chunks::parse(map)?;
        let chunk = chunks.get(PATH_CHUNK_ID).ok_or(ImportError::NoPathChunk)?;

        let mut data = PathStaticData::default();
        let mut reader = Reader::new(chunk.data);
        data.read(&mut reader)?;
        Ok(data)
    }

    /// `PathDataImport`: the section header, the section the reference calls `Step2`, then the planes.
    fn read(&mut self, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        // PathImport_CheckHeader.
        let header = reader.take(12, "the path section's header")?;
        let signature = u32_at(header, 0);
        let version = u32_at(header, 4);
        let sequence = u32_at(header, 8);
        // `assert(sequence != PATH_SEQUENCE_FAST_SAVE)`, where that constant is -1. The field is not read
        // for anything else, so a fast save is refused by assertion in the reference and here alike.
        debug_assert_ne!(sequence, u32::MAX, "the reference refuses a fast save");
        if signature != PATH_DATA_SIGNATURE || version != PATH_DATA_VERSION {
            return Err(ImportError::Shape(
                "the path section's signature or version is not the one the reference reads",
            ));
        }

        // PathImport_Step2. The length is read from byte 1 of five, so the u32 overlaps the tag byte that
        // follows it; that is the reference's arithmetic and the file's layout, not a typo made twice.
        if reader.len() <= 5 {
            return Err(ImportError::Truncated("the section after the path header"));
        }
        let step2_length = u32_at(reader.bytes, 1) as usize;
        reader.take(5, "the section after the path header")?;
        reader.take(step2_length, "the section after the path header")?;

        // PathImport_ReadPlanes.
        self.read_planes(reader)
    }

    /// `PathImport_ReadPlanes`: how many planes, then each plane in turn.
    fn read_planes(&mut self, reader: &mut Reader<'_>) -> Result<(), ImportError> {
        reader.tagged(8)?; // the section's length; the plane count is the first four bytes of it
        let count_bytes = reader.take(4, "the plane count")?;
        let count = u32_at(count_bytes, 0) as usize;

        // The reference keeps this hash for the whole import: a portal pairs with one on a plane that may
        // be finished (`PathImport_ReadPortals`).
        let mut pairs: std::collections::HashMap<u32, (u16, usize)> =
            std::collections::HashMap::new();
        self.planes.reserve(count);
        for _ in 0..count {
            self.read_plane(reader, &mut pairs)?;
        }
        Ok(())
    }

    /// `PathImport_ReadPlane`: a plane is its id plus ten sections, in a fixed order.
    fn read_plane(
        &mut self,
        reader: &mut Reader<'_>,
        pairs: &mut std::collections::HashMap<u32, (u16, usize)>,
    ) -> Result<(), ImportError> {
        let plane_id = u16::try_from(self.planes.len())
            .map_err(|_| ImportError::Shape("more planes than a plane id can name"))?;
        self.planes.push(Plane {
            plane_id,
            ..Plane::default()
        });
        let index = self.planes.len() - 1;

        self.read_counts(index, reader)?;
        self.skip_tag11(index, reader)?;
        self.read_vectors(index, reader)?;
        self.read_trapezoids(index, reader)?;
        self.read_root_node(index, reader)?;
        self.read_xnodes(index, reader)?;
        self.read_ynodes(index, reader)?;
        self.read_sink_nodes(index, reader)?;
        self.read_portal_traps(index, reader)?;
        self.read_portals(index, reader, pairs)?;
        Ok(())
    }
}
#[cfg(test)]
mod importer_tests {
    use super::*;

    /// `RiffCheckHeader` in three pieces: the signature, the length, and the type table - ASURA's own
    /// test (`map_data.rs:946`), which the container must pass before anything else.
    #[test]
    fn a_blob_that_is_not_a_container_is_refused_by_its_first_five_bytes() {
        assert!(riff_type(b"ffna\x01").is_some());
        assert!(
            riff_type(b"ffna\x09").is_none(),
            "type 9 is past RIFF_TYPES"
        );
        assert!(riff_type(b"ffna").is_none(), "five bytes are needed");
        assert!(riff_type(b"nope\x01").is_none());

        assert_eq!(
            Chunks::parse(b"nope\x01whatever").unwrap_err(),
            ImportError::Header
        );
        assert_eq!(
            PathStaticData::import(b"ffna\x07").unwrap_err(),
            ImportError::NotAMap { found: 7 }
        );
        assert_eq!(
            PathStaticData::import(b"ffna\x01").unwrap_err(),
            ImportError::NoPathChunk
        );
    }

    /// A record that runs past the blob is refused, and a tail too short for a record header is ignored -
    /// ASURA's own test (`map_data.rs:971`).
    #[test]
    fn a_record_that_runs_past_the_blob_is_refused_and_a_tail_that_is_not_one_is_ignored() {
        let mut blob = Vec::from(*b"ffna\x01");
        blob.extend_from_slice(&3u32.to_le_bytes());
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(b"abcd");
        blob.extend_from_slice(b"zzz");
        let chunks = Chunks::parse(&blob).expect("a container");
        assert_eq!(chunks.len(), 1);
        let chunk = chunks.get(3).expect("the record is there");
        assert_eq!(chunk.data, b"abcd");
        assert!(chunks.get(4).is_none(), "an id nobody wrote is not there");

        let mut broken = Vec::from(*b"ffna\x01");
        broken.extend_from_slice(&3u32.to_le_bytes());
        broken.extend_from_slice(&99u32.to_le_bytes());
        broken.extend_from_slice(b"abcd");
        assert_eq!(
            Chunks::parse(&broken).unwrap_err(),
            ImportError::ChunkPastTheEnd
        );
    }

    /// The node-id ranges, ASURA's own test (`map_data.rs:994`): an id names the array its range belongs
    /// to, and one node too many is a dead end.
    #[test]
    fn a_node_id_names_the_array_its_range_belongs_to() {
        let plane = Plane {
            xnodes: vec![XNode::default(); 2],
            ynodes: vec![YNode::default(); 3],
            sink_nodes: vec![SinkNode::default(); 1],
            ..Plane::default()
        };
        assert_eq!(node_ref(FIRST_X_NODE_INDEX, &plane), Some(NodeRef::X(0)));
        assert_eq!(node_ref(1, &plane), Some(NodeRef::X(1)));
        assert_eq!(
            node_ref(2, &plane),
            None,
            "one x node too many is a dead end"
        );
        assert_eq!(node_ref(FIRST_Y_NODE_INDEX, &plane), Some(NodeRef::Y(0)));
        assert_eq!(
            node_ref(FIRST_Y_NODE_INDEX + 2, &plane),
            Some(NodeRef::Y(2))
        );
        assert_eq!(
            node_ref(FIRST_SINK_NODE_INDEX, &plane),
            Some(NodeRef::Sink(0))
        );
        assert_eq!(node_ref(FIRST_SINK_NODE_INDEX + 1, &plane), None);
    }

    /// The real archive: map 148's own blob imports, the graph has planes and trapezoids, and the entry
    /// point the map's config names lands in one of them - the check ASURA uses to know its import worked.
    #[test]
    fn the_real_archive_imports_map_148_and_its_spawn_lands_in_a_trapezoid() {
        let Some(path) = std::env::var("KRYTA_GW_DAT").ok().or_else(|| {
            let candidate = std::path::PathBuf::from("Gw.dat");
            candidate
                .exists()
                .then(|| candidate.to_string_lossy().into_owned())
        }) else {
            eprintln!("skipped: no local Gw.dat");
            return;
        };
        let mut archive = crate::pack::GwDat::open(&path).expect("open the archive");
        let blob = archive
            .file(0x1B97D)
            .expect("read")
            .expect("the blob is there");
        let data = PathStaticData::import(&blob).expect("the blob imports");
        assert!(!data.planes.is_empty(), "the map has planes");
        assert!(data.traps_count > 0, "and trapezoids");
        let spawn = (9541.87f32, 4796.46f32);
        let plane = (0..u16::try_from(data.planes.len()).unwrap()).find(|plane| {
            data.trapezoid_at(
                Vec2f {
                    x: spawn.0,
                    y: spawn.1,
                },
                *plane,
            )
            .is_some()
        });
        assert!(
            plane.is_some(),
            "map 148s own spawn point lands in a trapezoid"
        );
    }
}
