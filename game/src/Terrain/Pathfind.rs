//! The map's walkable graph, and what a walk request is answered with.
//!
//! Reference: the reference's path finder over a map's pmap (`GmPaths.h`, `PathFindQuery`), and the
//! working implementation's port of it, `ASURA/crates/asura-world/src/pathfind.rs`. The types a caller
//! passes and receives are the working implementation's own (`pathfind::Pos`, `pathfind::Waypoint`,
//! `Walk` in `agent.rs:570`), so the handlers that use this file are that file's handlers.
//!
//! **The graph itself is not here yet.** It is read out of the client's own archive - the path tables
//! and the bloated map blob, `gwdat.rs`, `fa_compress.rs`, `map_data.rs` and `path_tables.rs` in the
//! working implementation - and this tree's `game/Pack/` is not built. So every lookup answers
//! `Walk::NoGraph`, which is exactly what the working implementation answers for a map it holds no
//! graph for, and both its handlers already say what to do with that answer: a walk request sends
//! **nothing** (`agent.rs:858-879`), and a manual movement sends nothing either (`agent.rs:1023-1029`).
//!
//! `docs/port/09-parity-audit.md` records the graph as missing. When the archive reader lands, this
//! file gains the finder and nothing above it changes.
#![allow(dead_code)] // the ported search arrives ahead of its callers; see docs/port/09-parity-audit.md 7b
#![allow(clippy::manual_is_multiple_of)] // (len % 2) == 0 is the reference spelling in PathHeapPop
#![allow(clippy::collapsible_if)] // the nested if is the reference shape in PickNextPoint

// The directional walk, a child module as ASURA keeps it (pathfind/directional.rs).
#[path = "Pathfind/Directional.rs"]
pub mod directional;

/// A point on a map, in the game's own units, on one of the map's planes.
///
/// `ASURA/crates/asura-world/src/pathfind.rs:113`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pos {
    /// East-west.
    pub x: f32,
    /// North-south.
    pub y: f32,
    /// Which surface: the floor is 0, and slopes and bridges have their own ids.
    pub plane: u16,
}

/// One point of a walk, and the trapezoid it was crossed into.
///
/// `ASURA/crates/asura-world/src/pathfind.rs:131`. The reference stores a `PathTrapezoid *`; trapezoid
/// ids are unique across a map's planes, so the id is the same identity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Waypoint {
    /// Where the point is.
    pub pos: Pos,
    /// The trapezoid it stands on, or 0 when the point is not on one.
    pub trap_id: u32,
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/pathfind.rs, lines 33-454: the geometry the search is made of.
//
// Copied function for function, in the file's own order. The quirks are the reference's and stay:
// `closest_point_on_segment` stores the offset rather than the projected point because nothing reads
// the point, `heap_pop`'s sift-down is spelled with the extra "last full depth" case because that is
// the order equal-cost nodes pop in, and `precise_cross` treats near-parallel as parallel.
// ---------------------------------------------------------------------------------------------

use crate::terrain::map_data::{Trapezoid, Vec2f};
use crate::terrain::path_tables::{path_reciproc, path_sqrt};

/// `fclampf`.
fn clampf(value: f32, min: f32, max: f32) -> f32 {
    value.min(max).max(min)
}

/// `Vec2fIsZero`.
fn is_zero(vector: Vec2f) -> bool {
    vector.x == 0.0 && vector.y == 0.0
}

/// `Vec2fEqual`.
fn equal(left: Vec2f, right: Vec2f) -> bool {
    left.x == right.x && left.y == right.y
}

/// `Vec2fAdd`.
fn add(left: Vec2f, right: Vec2f) -> Vec2f {
    Vec2f {
        x: left.x + right.x,
        y: left.y + right.y,
    }
}

/// `Vec2fSub`.
fn sub(left: Vec2f, right: Vec2f) -> Vec2f {
    Vec2f {
        x: left.x - right.x,
        y: left.y - right.y,
    }
}

/// `Vec2fDot`.
fn dot(left: Vec2f, right: Vec2f) -> f32 {
    (left.x * right.x) + (left.y * right.y)
}

/// `Vec2fCross`: negative for "left of", positive for "right of" (`GmMath.h:25`).
fn cross(left: Vec2f, right: Vec2f) -> f32 {
    (left.x * right.y) - (left.y * right.x)
}

/// `Vec2fDiv`.
fn div(vector: Vec2f, scalar: f32) -> Vec2f {
    Vec2f {
        x: vector.x / scalar,
        y: vector.y / scalar,
    }
}

/// `PathVec2fDist`.
fn vec2f_dist(left: Vec2f, right: Vec2f) -> f32 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    path_sqrt((dx * dx) + (dy * dy))
}

/// `PathVec2fUnit`.
fn vec2f_unit(vector: Vec2f) -> Vec2f {
    let length = path_sqrt(dot(vector, vector));
    div(vector, length)
}

impl Pos {
    /// The two floats, as the reference's `pos.v2` sees them.
    fn v2(self) -> Vec2f {
        Vec2f {
            x: self.x,
            y: self.y,
        }
    }
}

/// `PathFindPoint`: a position and the trapezoid it was crossed into. The reference stores a
/// `PathTrapezoid *`; trapezoid ids are unique across a map's planes, so the id is the same identity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct FindPoint {
    /// Where the point is.
    pos: Pos,
    /// The trapezoid it was crossed into, by trap id.
    trap: u32,
}

/// `PathFindNode`: one per trapezoid, filled in as the search reaches it.
#[derive(Debug, Clone, Copy, Default)]
struct FindNode {
    /// Whether the search has finished with it.
    closed: bool,
    /// The cost of the best route to it so far.
    cost_to_node: f32,
    /// The point the route entered it at.
    point: FindPoint,
    /// The next node of the chain back to the start.
    next: Option<u32>,
}

/// `PathHeapNode` (`GmPathHeap.h`).
#[derive(Debug, Clone, Copy)]
struct HeapNode {
    /// The cost the node was pushed with.
    cost: f32,
    /// The trapezoid it names.
    trap_id: u32,
}

/// `PathBuildStep` (`GmPaths.h:107`): a corridor edge the waypoint builder may cut back to.
#[derive(Debug, Clone, Copy)]
struct BuildStep {
    /// The point on the edge.
    pos: Vec2f,
    /// The direction the walk crosses it in.
    dir: Vec2f,
    /// The plane it is on.
    plane: u16,
    /// The trapezoid it leads into.
    next_trap: u32,
}

/// `PathHeapPush` (`GmPathHeap.c:3`).
fn heap_push(heap: &mut Vec<HeapNode>, node: HeapNode) {
    heap.push(node);
    if heap.len() < 2 {
        return;
    }
    let mut hole = heap.len() - 1;
    while hole > 0 {
        let parent = (hole - 1) >> 1;
        if heap[parent].cost < node.cost {
            break;
        }
        heap[hole] = heap[parent];
        hole = parent;
    }
    heap[hole] = node;
}

/// `PathHeapPop` (`GmPathHeap.c:24`): a binary heap whose sift-down is spelled with the extra "last full
/// depth" case rather than a plain two-children comparison. Kept as written, because the order it pops
/// equal-cost nodes in is the order the search visits them in.
fn heap_pop(heap: &mut Vec<HeapNode>) -> Option<HeapNode> {
    if heap.is_empty() {
        return None;
    }
    let len = heap.len() - 1;
    let last_node = heap[len];
    let popped = heap[0];
    if len == 0 {
        heap.clear();
        return Some(popped);
    }

    let mut idx = 0usize;
    let mut hole = 0usize;
    let last_full_depth_idx = len >> 1;
    while idx < last_full_depth_idx {
        idx = (idx * 2) + 2;
        if heap[idx - 1].cost < heap[idx].cost {
            idx -= 1;
        }
        if last_node.cost <= heap[idx].cost {
            break;
        }
        heap[hole] = heap[idx];
        hole = idx;
    }

    if idx == last_full_depth_idx && (len % 2) == 0 && heap[len - 1].cost < last_node.cost {
        heap[hole] = heap[len - 1];
        hole = len - 1;
    }

    heap[hole] = last_node;
    heap.truncate(len);
    Some(popped)
}

/// `PathReversePathFindNode`: the chain of `next` links, reversed in place.
fn reverse_path_find_node(nodes: &mut [FindNode], head: u32) -> Option<u32> {
    let mut previous = None;
    let mut current = Some(head);
    while let Some(id) = current {
        let next = nodes[id as usize].next;
        nodes[id as usize].next = previous;
        previous = Some(id);
        current = next;
    }
    previous
}

/// `IsPointInTrap`.
fn is_point_in_trap(pos: Pos, trap: &Trapezoid) -> bool {
    if pos.y <= trap.yb || trap.yt <= pos.y {
        return false;
    }

    let top = Vec2f {
        x: trap.xtl,
        y: trap.yt,
    };
    let bottom = Vec2f {
        x: trap.xbl,
        y: trap.yb,
    };
    if cross(pos.v2(), sub(top, bottom)) < 0.0 {
        return false;
    }

    let top = Vec2f {
        x: trap.xtr,
        y: trap.yt,
    };
    let bottom = Vec2f {
        x: trap.xbr,
        y: trap.yb,
    };
    if 0.0 < cross(pos.v2(), sub(top, bottom)) {
        return false;
    }

    true
}

/// `GetClosestPointOnLineSegment`.
///
/// The reference stores `oproj` - the offset from the projected point back to `pos` - rather than the
/// projected point itself. Nothing reads the point afterwards, only the distance it travels with, so the
/// quirk is invisible; it is reproduced rather than corrected.
fn closest_point_on_segment(
    pos: Pos,
    p1: Vec2f,
    p2: Vec2f,
    distance: &mut f32,
    closest: &mut Vec2f,
) {
    let p = sub(p2, p1);
    let v = sub(pos.v2(), p1);

    if p.x == 0.0 && p.y == 0.0 {
        let norm = dot(v, v);
        if norm < *distance {
            *distance = norm;
            *closest = p1;
        }
        return;
    }

    let coeff = dot(v, p) / dot(p, p);
    let projection = if (0.0..=1.0).contains(&coeff) {
        Vec2f {
            x: (coeff * p.x) + p1.x,
            y: (coeff * p.y) + p1.y,
        }
    } else if 1.0 <= coeff {
        p2
    } else {
        p1
    };

    let offset = sub(pos.v2(), projection);
    let norm = dot(offset, offset);
    if norm < *distance {
        *distance = norm;
        *closest = offset;
    }
}

/// `DistToTrap`: squared distance to the trapezoid's edges, or zero inside it.
fn dist_to_trap(pos: Pos, trap: &Trapezoid) -> f32 {
    if is_point_in_trap(pos, trap) {
        return 0.0;
    }

    let mut distance = f32::INFINITY;
    let mut point = Vec2f::default();

    let tl = Vec2f {
        x: trap.xtl,
        y: trap.yt,
    };
    let tr = Vec2f {
        x: trap.xtr,
        y: trap.yt,
    };
    let bl = Vec2f {
        x: trap.xbl,
        y: trap.yb,
    };
    let br = Vec2f {
        x: trap.xbr,
        y: trap.yb,
    };
    closest_point_on_segment(pos, tl, tr, &mut distance, &mut point);
    closest_point_on_segment(pos, tr, br, &mut distance, &mut point);
    closest_point_on_segment(pos, br, bl, &mut distance, &mut point);
    closest_point_on_segment(pos, bl, tl, &mut distance, &mut point);

    distance
}

/// `FindPointOnNextTrap`: where the line from the current position to the destination crosses the
/// neighbour's edge at `next_y`, clamped into the shared span.
fn find_point_on_next_trap(
    min_left_x: f32,
    max_right_x: f32,
    next_y: f32,
    src_pos: Vec2f,
    dst_pos: Vec2f,
) -> Vec2f {
    let mut percent = -1.0f32;

    if src_pos.y != dst_pos.y {
        let divisor = path_reciproc(dst_pos.y - src_pos.y);
        percent = (next_y - src_pos.y) * divisor;
    }

    if percent < 0.0 {
        let src_x = clampf(src_pos.x, min_left_x, max_right_x);
        let dst_x = clampf(dst_pos.x, min_left_x, max_right_x);
        // 0.9 and 0.1: the reference spells them as the doubles nearest those floats
        // (`0.8999999761581421f`, `0.1000000238418579f`), which is what these are.
        Vec2f {
            x: (src_x * 0.9) + (dst_x * 0.1),
            y: next_y,
        }
    } else {
        let new_x = ((dst_pos.x - src_pos.x) * percent) + src_pos.x;
        Vec2f {
            x: clampf(new_x, min_left_x, max_right_x),
            y: next_y,
        }
    }
}

/// `FindIntersectionPoint`: `t1`/`t2` such that `start1 + t1*dir1 == start2 + t2*dir2`.
fn find_intersection_point(
    start1: Vec2f,
    dir1: Vec2f,
    start2: Vec2f,
    dir2: Vec2f,
) -> Option<(f32, f32)> {
    let determinant = cross(dir1, dir2);
    if determinant == 0.0 {
        return None;
    }
    let between = sub(start1, start2);
    Some((
        cross(dir2, between) / determinant,
        cross(dir1, between) / determinant,
    ))
}

/// `PickNextPoint`: where a walk crosses the segment from `point1` to `point2`.
fn pick_next_point(point1: Vec2f, point2: Vec2f, cur_pos: Vec2f, dst_pos: Vec2f) -> Vec2f {
    let cur_to_dst = sub(dst_pos, cur_pos);
    let point1_to_point2 = sub(point2, point1);

    if let Some((_, t2)) = find_intersection_point(cur_pos, cur_to_dst, point1, point1_to_point2) {
        if 0.0 <= t2 {
            let t2 = t2.min(1.0);
            return Vec2f {
                x: point1.x + (point1_to_point2.x * t2),
                y: point1.y + (point1_to_point2.y * t2),
            };
        }
    }

    let norm2 = dot(point1_to_point2, point1_to_point2);
    let divisor = path_reciproc(norm2);

    let t1 = clampf(dot(point1, point1_to_point2) * divisor, 0.0, 1.0);
    let t2 = clampf(dot(point2, point1_to_point2) * divisor, 0.0, 1.0);
    let t = (t2 * 0.1) + (t1 * 0.9);
    Vec2f {
        x: point1.x + (point1_to_point2.x * t),
        y: point1.y + (point1_to_point2.y * t),
    }
}

/// `PathVec2fPreciseCross`: a cross product that treats near-parallel as parallel, using unit vectors to
/// decide when the raw product is too small to be meaningful.
fn precise_cross(left: Vec2f, right: Vec2f) -> f32 {
    if left.x == 0.0 && left.y == 0.0 {
        return 0.0;
    }
    if right.x == 0.0 && right.y == 0.0 {
        return 0.0;
    }

    let product = cross(left, right);
    if -0.01 < product && product < 0.01 {
        return 0.0;
    }
    if product <= -1.0 || 1.0 <= product {
        return product;
    }

    let product = cross(vec2f_unit(left), vec2f_unit(right));
    if product <= -0.01 || 0.01 <= product {
        return product;
    }
    0.0
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/pathfind.rs, lines 456-557: the context a search runs in, and the two
// lookups it answers with. `PathFinding` itself (line 779 on) is the next piece.
// ---------------------------------------------------------------------------------------------

use crate::terrain::map_data::{PathStaticData, Portal};

/// A path finder over one map's imported graph.
///
/// The node array, the priority queue and the corridor steps are kept between searches, because the
/// reference keeps them in its context: a second search on the same map does not reallocate, and the
/// node array is cleared rather than rebuilt.
///
/// The graph itself is shared (`Arc`) rather than borrowed: a world keeps one finder per map for as long
/// as the map is served, and the graph is the same object for every walk. That is the reference's
/// `srv->map_context` with one difference - it has one context per server because it runs one server per
/// map, while this hosts several maps in one process.
pub struct PathContext {
    pub(crate) data: std::sync::Arc<PathStaticData>,
    /// Trap id to `(plane id, position in that plane's trapezoids)`. Trap ids are dense and assigned in
    /// plane order by the importer, so this is the reference's `array_at(&nodes, trap->trap_id)` and its
    /// `PathTrapezoid *` in one table.
    index: Vec<(u16, u32)>,
    nodes: Vec<FindNode>,
    prioq: Vec<HeapNode>,
    steps: Vec<BuildStep>,
}

impl PathContext {
    /// A finder over one map's graph, with the trap index built once.
    pub fn new(data: std::sync::Arc<PathStaticData>) -> PathContext {
        let mut index = vec![(0u16, 0u32); data.traps_count as usize];
        for plane in &data.planes {
            for (position, trapezoid) in plane.trapezoids.iter().enumerate() {
                if let Some(slot) = index.get_mut(trapezoid.trap_id as usize) {
                    *slot = (plane.plane_id, position as u32);
                }
            }
        }
        PathContext {
            data,
            index,
            nodes: Vec::new(),
            prioq: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// The trapezoid a trap id names.
    pub(crate) fn trap(&self, trap_id: u32) -> Trapezoid {
        let (plane, position) = self.index[trap_id as usize];
        self.data.planes[usize::from(plane)].trapezoids[position as usize]
    }

    /// The plane a trap id lives on.
    pub(crate) fn plane_of(&self, trap_id: u32) -> u16 {
        self.index[trap_id as usize].0
    }

    /// A trapezoid's neighbour field (`Option<usize>`, a position in its own plane) as a trap id.
    pub(crate) fn neighbour(&self, trap_id: u32, local: Option<usize>) -> Option<u32> {
        local.map(|position| {
            let plane = usize::from(self.plane_of(trap_id));
            self.data.planes[plane].trapezoids[position].trap_id
        })
    }

    /// A portal of the plane `plane`.
    pub(crate) fn portal(&self, plane: u16, portal_id: usize) -> Option<Portal> {
        self.data
            .planes
            .get(usize::from(plane))?
            .portals
            .get(portal_id)
            .copied()
    }

    /// The trapezoids a portal leads to, as trap ids: the reference walks `portal->pair->traps` and
    /// dereferences each entry, so an entry the importer left NULL is skipped here rather than crashing.
    pub(crate) fn portal_pair_traps(&self, portal: &Portal) -> Vec<u32> {
        let Some(plane) = self.data.planes.get(usize::from(portal.portal_plane_id)) else {
            return Vec::new();
        };
        let mut traps = Vec::with_capacity(portal.traps_count as usize);
        for offset in 0..portal.traps_count as usize {
            let entry = plane
                .portals_traps
                .get(portal.traps + offset)
                .copied()
                .flatten();
            if let Some(position) = entry {
                traps.push(plane.trapezoids[position].trap_id);
            }
        }
        traps
    }

    /// `PathFindTrapezoid`, as a trap id.
    pub fn find_trapezoid(&self, pos: Pos) -> Option<u32> {
        let (plane, position) = self.data.trapezoid_at(pos.v2(), pos.plane)?;
        Some(self.data.planes[usize::from(plane)].trapezoids[position].trap_id)
    }

    /// Which plane a point lies on, if any does: the first plane whose search places it.
    ///
    /// This global lookup does not establish a connected surface for directional movement.
    pub fn plane_at(&self, point: Vec2f) -> Option<u16> {
        let count = u16::try_from(self.data.planes.len()).ok()?;
        (0..count).find(|plane| self.data.trapezoid_at(point, *plane).is_some())
    }

    // -----------------------------------------------------------------------------------------
    // ASURA/crates/asura-world/src/pathfind.rs, lines 559-777: one expansion of the search - the
    // node it queues, the neighbour it costs through a crossing, and the three ways a neighbour is
    // reached (above, bellow, through a portal). `PathFinding` itself (line 779 on) is next.
    // -----------------------------------------------------------------------------------------

    /// `PathAddNode`: record the cost of reaching `trap` and queue it.
    fn add_node(
        &mut self,
        parent: Option<u32>,
        trap: u32,
        pos: Pos,
        current_cost: f32,
        estimated_cost: f32,
    ) {
        let node = &mut self.nodes[trap as usize];
        node.closed = false;
        node.cost_to_node = current_cost;
        node.point.pos = pos;
        node.point.trap = trap;
        node.next = parent;
        heap_push(
            &mut self.prioq,
            HeapNode {
                cost: estimated_cost,
                trap_id: trap,
            },
        );
    }

    /// `PathVisitTrap`: cost a neighbour reached through a crossing point.
    fn visit_trap(
        &mut self,
        curr_node: u32,
        dst_pos: Vec2f,
        neighbour: u32,
        cross_pos: Pos,
        max_cost: f32,
    ) {
        let current = self.nodes[curr_node as usize];
        let distance = vec2f_dist(current.point.pos.v2(), cross_pos.v2());
        let cost_to_trap = current.cost_to_node + distance;

        if max_cost <= cost_to_trap {
            return;
        }

        let trap = self.trap(neighbour);

        let bottom_point = Vec2f {
            x: clampf(dst_pos.x, trap.xbl, trap.xbr),
            y: trap.yb,
        };
        let estimated_dest_bottom = vec2f_dist(bottom_point, dst_pos);

        let top_point = Vec2f {
            x: clampf(dst_pos.x, trap.xtl, trap.xtr),
            y: trap.yt,
        };
        let estimated_dest_top = vec2f_dist(top_point, dst_pos);

        let mut best_estimate = estimated_dest_bottom.min(estimated_dest_top);
        if trap.yb <= dst_pos.y && dst_pos.y <= trap.yt {
            // The reference subtracts `0.009999999776482582`, the double nearest the float 0.01.
            best_estimate -= 0.01;
        }

        let estimated_cost = cost_to_trap + best_estimate;
        self.add_node(
            Some(curr_node),
            neighbour,
            cross_pos,
            cost_to_trap,
            estimated_cost,
        );
    }

    /// `PathVisitAbove`.
    fn visit_above(&mut self, curr_node: u32, dst_pos: Vec2f, neighbour: u32, max_cost: f32) {
        let trap = self.trap(self.nodes[curr_node as usize].point.trap);
        let next = self.trap(neighbour);
        let xl = next.xbl.max(trap.xtl);
        let xr = next.xbr.min(trap.xtr);
        let src = self.nodes[curr_node as usize].point.pos;
        let crossing = find_point_on_next_trap(xl, xr, next.yb, src.v2(), dst_pos);
        self.visit_trap(
            curr_node,
            dst_pos,
            neighbour,
            Pos {
                x: crossing.x,
                y: crossing.y,
                plane: src.plane,
            },
            max_cost,
        );
    }

    /// `PathVisitBellow`.
    fn visit_bellow(&mut self, curr_node: u32, dst_pos: Vec2f, neighbour: u32, max_cost: f32) {
        let trap = self.trap(self.nodes[curr_node as usize].point.trap);
        let next = self.trap(neighbour);
        let xl = next.xtl.max(trap.xbl);
        let xr = next.xtr.min(trap.xbr);
        let src = self.nodes[curr_node as usize].point.pos;
        let crossing = find_point_on_next_trap(xl, xr, next.yt, src.v2(), dst_pos);
        self.visit_trap(
            curr_node,
            dst_pos,
            neighbour,
            Pos {
                x: crossing.x,
                y: crossing.y,
                plane: src.plane,
            },
            max_cost,
        );
    }

    /// `PathVisitPortalLeft` (and, with `left` false, `PathVisitPortalRight`): the trapezoids on the far
    /// side of a portal, entered where the walk's line crosses the portal's own edge.
    fn visit_portal(
        &mut self,
        curr_node: u32,
        dst_pos: Vec2f,
        portal_id: u16,
        left: bool,
        max_cost: f32,
    ) {
        let src = self.nodes[curr_node as usize].point.pos;
        let Some(portal) = self.portal(src.plane, usize::from(portal_id)) else {
            return;
        };
        // The reference dereferences `portal->pair` and asserts it is the mirror; a portal without one is
        // a crash there, so refusing here is the closest thing to parity that does not abort.
        let Some(pair) = portal.pair else {
            return;
        };
        let Some(pair_portal) = self.portal(pair.0, pair.1) else {
            return;
        };
        debug_assert_eq!(pair_portal.portal_plane_id, portal.neighbor_plane_id);
        debug_assert_eq!(pair_portal.neighbor_plane_id, portal.portal_plane_id);

        if (portal.flags & 0x4) != 0 {
            return;
        }

        let traps = self.portal_pair_traps(&pair_portal);
        let curr_trap = self.trap(self.nodes[curr_node as usize].point.trap);

        for portal_trap in traps {
            if self.nodes[portal_trap as usize].closed {
                continue;
            }
            let trap = self.trap(portal_trap);

            // The top point is the higher of the two trapezoids' top edges on the side the portal is on;
            // the bottom point is the lower of the two bottom edges. `left` picks which corner.
            let (top, bottom) = if left {
                let top = if trap.yt <= curr_trap.yt {
                    Vec2f {
                        x: trap.xtr,
                        y: trap.yt,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xtl,
                        y: curr_trap.yt,
                    }
                };
                let bottom = if curr_trap.yb <= trap.yb {
                    Vec2f {
                        x: trap.xbr,
                        y: trap.yb,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xbl,
                        y: curr_trap.yb,
                    }
                };
                (top, bottom)
            } else {
                let top = if trap.yt <= curr_trap.yt {
                    Vec2f {
                        x: trap.xtl,
                        y: trap.yt,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xtr,
                        y: curr_trap.yt,
                    }
                };
                let bottom = if curr_trap.yb <= trap.yb {
                    Vec2f {
                        x: trap.xbl,
                        y: trap.yb,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xbr,
                        y: curr_trap.yb,
                    }
                };
                (top, bottom)
            };

            if bottom.y <= top.y {
                let crossing = pick_next_point(top, bottom, src.v2(), dst_pos);
                self.visit_trap(
                    curr_node,
                    dst_pos,
                    portal_trap,
                    Pos {
                        x: crossing.x,
                        y: crossing.y,
                        plane: pair_portal.portal_plane_id,
                    },
                    max_cost,
                );
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // ASURA/crates/asura-world/src/pathfind.rs, lines 779-918: `PathFinding` (the A* itself) and
    // `PathCreateWaypoints`, which hands the chain to the corridor walk ported above.
    // -----------------------------------------------------------------------------------------

    /// `PathFinding` (`GmPaths.c:944`): the A* itself.
    ///
    /// Returns whether a walk exists; `waypoints` receives the reduced path, which is one waypoint
    /// naming the destination when both positions are in the same trapezoid.
    pub fn path_finding(&mut self, src: Pos, dst: Pos, waypoints: &mut Vec<Waypoint>) -> bool {
        // 10,000 is the ingame constant.
        const MAX_COST: f32 = 10_000.0;

        let Some(src_trap) = self.find_trapezoid(src) else {
            return false;
        };
        let Some(dst_trap) = self.find_trapezoid(dst) else {
            return false;
        };

        if src_trap == dst_trap {
            waypoints.push(Waypoint {
                pos: dst,
                trap_id: src_trap,
            });
            return true;
        }

        self.prioq.clear();
        self.nodes = vec![FindNode::default(); self.data.traps_count as usize];
        self.steps.clear();

        let mut closest_trap: Option<u32> = None;
        let mut closest_distance = f32::INFINITY;

        self.add_node(None, src_trap, src, 0.0, f32::INFINITY);

        while let Some(top) = heap_pop(&mut self.prioq) {
            let curr_node = top.trap_id;
            self.nodes[curr_node as usize].closed = true;

            let curr_trap = self.trap(curr_node);
            if curr_node == dst_trap {
                reverse_path_find_node(&mut self.nodes, curr_node);
                self.create_waypoints(
                    FindPoint {
                        pos: src,
                        trap: src_trap,
                    },
                    FindPoint {
                        pos: dst,
                        trap: dst_trap,
                    },
                    waypoints,
                );
                return true;
            }

            let distance = dist_to_trap(dst, &curr_trap);
            if distance < closest_distance {
                closest_distance = distance;
                closest_trap = Some(curr_node);
            }

            for id in [
                self.neighbour(curr_node, curr_trap.top_left),
                self.neighbour(curr_node, curr_trap.top_right),
            ]
            .into_iter()
            .flatten()
            {
                if !self.nodes[id as usize].closed {
                    self.visit_above(curr_node, dst.v2(), id, MAX_COST);
                }
            }
            for id in [
                self.neighbour(curr_node, curr_trap.bottom_left),
                self.neighbour(curr_node, curr_trap.bottom_right),
            ]
            .into_iter()
            .flatten()
            {
                if !self.nodes[id as usize].closed {
                    self.visit_bellow(curr_node, dst.v2(), id, MAX_COST);
                }
            }

            if curr_trap.portal_left != u16::MAX {
                self.visit_portal(curr_node, dst.v2(), curr_trap.portal_left, true, MAX_COST);
            }
            if curr_trap.portal_right != u16::MAX {
                self.visit_portal(curr_node, dst.v2(), curr_trap.portal_right, false, MAX_COST);
            }
        }

        // Nothing reached the destination's trapezoid: the reference still builds a path to the closest
        // trapezoid it did reach, if the destination is inside that one, and reports failure either way.
        if let Some(trap) = closest_trap {
            if is_point_in_trap(dst, &self.trap(trap)) {
                let closest = FindPoint { pos: dst, trap };
                reverse_path_find_node(&mut self.nodes, trap);
                self.create_waypoints(
                    FindPoint {
                        pos: src,
                        trap: src_trap,
                    },
                    closest,
                    waypoints,
                );
            }
        }

        false
    }

    /// `PathCreateWaypoints` (`GmPaths.c:822`): walk the chain of trapezoids the search chose and reduce
    /// it to waypoints.
    fn create_waypoints(
        &mut self,
        src_point: FindPoint,
        dst_point: FindPoint,
        waypoints: &mut Vec<Waypoint>,
    ) {
        self.steps.clear();

        let mut helper = BuildHelper {
            context: self,
            left_bound: Bound {
                point: src_point,
                step_id: 0,
                vec: Vec2f::default(),
            },
            right_bound: Bound {
                point: src_point,
                step_id: 0,
                vec: Vec2f::default(),
            },
            curr_start_point: src_point,
            current_plane: src_point.pos.plane,
            last_pos: Pos::default(),
            dst_point,
            waypoints,
        };
        helper.run(src_point.trap);
    }
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/pathfind.rs, lines 921-940: the corridor the waypoint builder follows,
// declared here so `PathFinding` (line 783) and `PathCreateWaypoints` (line 891) arrive together with
// the helper they hand themselves to. `BuildHelper`'s own `impl` (line 941 on) is the next piece.
// ---------------------------------------------------------------------------------------------

/// `PathFindBound` (`GmPaths.c:552`): one side of the corridor being followed, and the step it began at.
#[derive(Debug, Clone, Copy)]
struct Bound {
    /// The point on this side.
    point: FindPoint,
    /// The step of the corridor it began at.
    step_id: usize,
    /// The direction out of this side.
    vec: Vec2f,
}

/// `PathBuildHelper` (`GmPaths.c:558`).
struct BuildHelper<'c> {
    /// The finder whose steps and waypoints are being walked.
    context: &'c mut PathContext,
    /// The corridor's left side.
    left_bound: Bound,
    /// The corridor's right side.
    right_bound: Bound,
    /// Where the corridor stands.
    curr_start_point: FindPoint,
    /// The plane the corridor is on.
    current_plane: u16,
    /// The last point emitted, for the direction check.
    last_pos: Pos,
    /// Where the walk is going.
    dst_point: FindPoint,
    /// The waypoints being written.
    waypoints: &'c mut Vec<Waypoint>,
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/pathfind.rs, lines 941-1286: the corridor walk. Every step is the
// reference's, including the two places where its behaviour is undefined or surprising and the port
// says so: the uninitialised `t`/`s` of a parallel step (`add_waypoint_and_reduce`) and the swap of
// the edges when the corridor goes down into the next trapezoid (`run`).
// ---------------------------------------------------------------------------------------------

impl BuildHelper<'_> {
    /// `PathBuildHelper`: walk the chain of trapezoids the search chose.
    fn run(&mut self, src_trap: u32) {
        const MAX_LOOP: i32 = 0xBB8;

        let mut curr_node = src_trap;
        let mut loop_count = 0i32;
        loop {
            loop_count += 1;
            assert!(
                loop_count < MAX_LOOP,
                "the reference asserts the walk terminates"
            );

            let curr_trap_id = self.context.nodes[curr_node as usize].point.trap;
            let curr_trap = self.context.trap(curr_trap_id);
            let next_node = self.context.nodes[curr_node as usize].next;

            if next_node.is_none() || curr_trap_id == self.dst_point.trap {
                if self.add_last(&mut curr_node, self.dst_point) {
                    break;
                }
                continue;
            }

            let next = self.context.nodes[next_node.expect("checked above") as usize];
            let next_trap_id = next.point.trap;
            let next_trap = self.context.trap(next_trap_id);

            let top_left = self.context.neighbour(curr_trap_id, curr_trap.top_left);
            let top_right = self.context.neighbour(curr_trap_id, curr_trap.top_right);
            let bottom_left = self.context.neighbour(curr_trap_id, curr_trap.bottom_left);
            let bottom_right = self.context.neighbour(curr_trap_id, curr_trap.bottom_right);

            if Some(next_trap_id) == top_left || Some(next_trap_id) == top_right {
                let left = Vec2f {
                    x: curr_trap.xtl.max(next_trap.xbl),
                    y: curr_trap.yt,
                };
                let right = Vec2f {
                    x: curr_trap.xtr.min(next_trap.xbr),
                    y: curr_trap.yt,
                };
                self.process_next(left, right, next_trap_id, &mut curr_node);
            } else if Some(next_trap_id) == bottom_left || Some(next_trap_id) == bottom_right {
                let left = Vec2f {
                    x: curr_trap.xbl.max(next_trap.xtl),
                    y: curr_trap.yb,
                };
                let right = Vec2f {
                    x: curr_trap.xbr.min(next_trap.xtr),
                    y: curr_trap.yb,
                };
                // Note the swap: going down, the reference passes `right` as the left edge.
                self.process_next(right, left, next_trap_id, &mut curr_node);
            } else if let Some(portal) = self.portal_adjacent(curr_trap.portal_right, next_trap_id)
            {
                let top = if next_trap.yt <= curr_trap.yt {
                    Vec2f {
                        x: next_trap.xtl,
                        y: next_trap.yt,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xtr,
                        y: curr_trap.yt,
                    }
                };
                let bottom = if curr_trap.yb <= next_trap.yb {
                    Vec2f {
                        x: next_trap.xbl,
                        y: next_trap.yb,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xbr,
                        y: curr_trap.yb,
                    }
                };

                self.current_plane = portal.neighbor_plane_id;
                self.context.steps.push(BuildStep {
                    pos: top,
                    dir: sub(bottom, top),
                    plane: self.current_plane,
                    next_trap: next_trap_id,
                });
                self.process_next(top, bottom, next_trap_id, &mut curr_node);
            } else if let Some(portal) = self.portal_adjacent(curr_trap.portal_left, next_trap_id) {
                let top = if next_trap.yt <= curr_trap.yt {
                    Vec2f {
                        x: next_trap.xtr,
                        y: next_trap.yt,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xtl,
                        y: curr_trap.yt,
                    }
                };
                let bottom = if curr_trap.yb <= next_trap.yb {
                    Vec2f {
                        x: next_trap.xbr,
                        y: next_trap.yb,
                    }
                } else {
                    Vec2f {
                        x: curr_trap.xbl,
                        y: curr_trap.yb,
                    }
                };

                self.current_plane = portal.neighbor_plane_id;
                self.context.steps.push(BuildStep {
                    pos: top,
                    dir: sub(bottom, top),
                    plane: self.current_plane,
                    next_trap: next_trap_id,
                });
                self.process_next(bottom, top, next_trap_id, &mut curr_node);
            } else {
                // The reference asserts "shouldn't be possible" here and stops.
                break;
            }
        }
    }

    /// `PathBuildAddWaypointAndReduce`: flush the corridor steps to waypoints, dropping repeats.
    fn add_waypoint_and_reduce(&mut self, new_point: FindPoint, from_pos: Vec2f) {
        let steps = std::mem::take(&mut self.context.steps);

        for step in &steps {
            let best_pos = if !is_zero(step.dir) {
                let to_point = sub(new_point.pos.v2(), from_pos);
                if let Some((t, _s)) =
                    find_intersection_point(step.pos, step.dir, from_pos, to_point)
                {
                    if 1.01 < t {
                        add(step.pos, step.dir)
                    } else if t < -0.01 {
                        step.pos
                    } else {
                        Vec2f {
                            x: step.pos.x + (t * step.dir.x),
                            y: step.pos.y + (t * step.dir.y),
                        }
                    }
                } else {
                    // The reference reads its uninitialised `t` here; leaving it at zero reproduces the
                    // 11-waypoint answer to the one parallel case measured, byte for byte.
                    step.pos
                }
            } else {
                step.pos
            };

            if !equal(self.last_pos.v2(), best_pos) || self.last_pos.plane != step.plane {
                self.last_pos = Pos {
                    x: best_pos.x,
                    y: best_pos.y,
                    plane: step.plane,
                };

                if let Some(previous) = self.waypoints.last() {
                    if equal(best_pos, previous.pos.v2()) {
                        self.waypoints.pop();
                    }
                }

                self.waypoints.push(Waypoint {
                    pos: self.last_pos,
                    trap_id: step.next_trap,
                });
            }
        }

        if !equal(self.last_pos.v2(), new_point.pos.v2()) {
            self.last_pos = new_point.pos;

            if let Some(previous) = self.waypoints.last() {
                if equal(new_point.pos.v2(), previous.pos.v2()) {
                    self.waypoints.pop();
                }
            }

            self.waypoints.push(Waypoint {
                pos: new_point.pos,
                trap_id: new_point.trap,
            });
        }
    }

    /// `PathBuildAddWaypoint`: the corridor has to be cut at one of the two bounds.
    fn add_waypoint(&mut self, left: Vec2f, right: Vec2f, node: &mut u32) {
        let bound_used;
        if equal(self.left_bound.point.pos.v2(), left) {
            bound_used = self.left_bound;
            self.right_bound.point = self.left_bound.point;
            self.right_bound.step_id = self.left_bound.step_id;
            *node = self.left_bound.point.trap;
        } else if equal(self.right_bound.point.pos.v2(), right) {
            bound_used = self.right_bound;
            self.left_bound.point = self.right_bound.point;
            self.left_bound.step_id = self.right_bound.step_id;
            *node = self.right_bound.point.trap;
        } else {
            let mut dx = left.x - self.curr_start_point.pos.x;
            let mut dy = left.y - self.curr_start_point.pos.y;
            let left_value = (dy * dy) + (dx * dx);

            dx = right.x - self.curr_start_point.pos.x;
            dy = right.y - self.curr_start_point.pos.y;
            let right_value = (dy * dy) + (dx * dx);

            bound_used = if left_value < right_value {
                self.left_bound
            } else {
                self.right_bound
            };
        }

        self.context.steps.truncate(bound_used.step_id);

        let from_pos = self.curr_start_point.pos.v2();
        self.add_waypoint_and_reduce(bound_used.point, from_pos);
        self.curr_start_point = bound_used.point;
        self.right_bound.vec = Vec2f::default();
        self.left_bound.vec = Vec2f::default();
        self.current_plane = bound_used.point.pos.plane;
    }

    /// `PathBuildProcessNext`: extend the corridor across the edge into `next_trap`, or cut it.
    fn process_next(&mut self, left: Vec2f, right: Vec2f, next_trap: u32, node: &mut u32) {
        let mut to_left = sub(left, self.curr_start_point.pos.v2());
        let mut to_right = sub(right, self.curr_start_point.pos.v2());

        let new_left_bound;
        if precise_cross(to_left, self.left_bound.vec) < 0.0 {
            // `to_left` is left of the left bound, so the bound is tighter and stays.
            to_left = self.left_bound.vec;
            new_left_bound = self.left_bound;
        } else {
            new_left_bound = Bound {
                vec: to_left,
                point: FindPoint {
                    pos: Pos {
                        x: left.x,
                        y: left.y,
                        plane: self.current_plane,
                    },
                    trap: next_trap,
                },
                step_id: self.context.steps.len(),
            };
        }

        let new_right_bound;
        if 0.0 < precise_cross(to_right, self.right_bound.vec) {
            to_right = self.right_bound.vec;
            new_right_bound = self.right_bound;
        } else {
            new_right_bound = Bound {
                vec: to_right,
                point: FindPoint {
                    pos: Pos {
                        x: right.x,
                        y: right.y,
                        plane: self.current_plane,
                    },
                    trap: next_trap,
                },
                step_id: self.context.steps.len(),
            };
        }

        if is_zero(self.left_bound.vec) || precise_cross(to_left, to_right) <= 0.0 {
            self.left_bound = new_left_bound;
            self.right_bound = new_right_bound;
            *node = self.context.nodes[*node as usize]
                .next
                .expect("the walk follows the chain it was given");
        } else {
            let left = new_left_bound.point.pos.v2();
            let right = new_right_bound.point.pos.v2();
            self.add_waypoint(left, right, node);
        }
    }

    /// `PathAddLast`: the chain has run out; close the corridor at the destination.
    fn add_last(&mut self, node: &mut u32, new_point: FindPoint) -> bool {
        let to_new_point = sub(new_point.pos.v2(), self.curr_start_point.pos.v2());
        if 0.0 <= precise_cross(to_new_point, self.left_bound.vec) {
            if precise_cross(to_new_point, self.right_bound.vec) <= 0.0 {
                let from_pos = self.curr_start_point.pos.v2();
                self.add_waypoint_and_reduce(new_point, from_pos);
                return true;
            }

            self.right_bound.vec = Vec2f::default();
            self.left_bound.point = self.right_bound.point;
            *node = self.right_bound.point.trap;
            self.context.steps.truncate(self.right_bound.step_id);
            let from_pos = self.curr_start_point.pos.v2();
            let point = self.right_bound.point;
            self.add_waypoint_and_reduce(point, from_pos);
            self.curr_start_point = self.right_bound.point;
            self.current_plane = self.right_bound.point.pos.plane;
            false
        } else {
            self.left_bound.vec = Vec2f::default();
            self.right_bound.point = self.left_bound.point;
            *node = self.left_bound.point.trap;
            self.context.steps.truncate(self.left_bound.step_id);
            let from_pos = self.curr_start_point.pos.v2();
            let point = self.left_bound.point;
            self.add_waypoint_and_reduce(point, from_pos);
            self.curr_start_point = self.left_bound.point;
            self.current_plane = self.left_bound.point.pos.plane;
            false
        }
    }

    /// `GetPortalAndCheckTrapAdjacent`: the portal of the current plane on the given side, if it leads
    /// to `trap`.
    fn portal_adjacent(&self, portal_id: u16, trap: u32) -> Option<Portal> {
        if portal_id == 0xFFFF {
            return None;
        }
        let portal = self
            .context
            .portal(self.current_plane, usize::from(portal_id))?;
        let pair = portal.pair?;
        let pair_portal = self.context.portal(pair.0, pair.1)?;
        for candidate in self.context.portal_pair_traps(&pair_portal) {
            if candidate == trap {
                return Some(portal);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/pathfind.rs, lines 1484-1700 and 1624-1699: the graphs a world serves.
// `load` and `import` (lines 1507-1592), which read a graph out of a `Gw.dat` through the importer,
// and `walk_direction` (1639), which needs `directional.rs`, are the remaining pieces.
// ---------------------------------------------------------------------------------------------

use crate::world::MapEntry;

/// The graphs a world serves: one per `Gw.dat` blob id, and one finder per map.
pub struct PathGraphs(std::sync::Arc<PathGraphsInner>);

/// What [`PathGraphs`] holds, behind the one `Arc` a world clones.
#[derive(Default)]
pub struct PathGraphsInner {
    /// One graph per blob id, behind a lock because a demand-loaded map adds its graph after startup.
    /// Reads clone the `Arc` out and drop the lock, so a search never holds it.
    data: std::sync::Mutex<std::collections::HashMap<u32, std::sync::Arc<PathStaticData>>>,
    /// map id -> the blob id its graph is stored under, for the maps this world can path in.
    pub bound: std::sync::Mutex<std::collections::HashMap<i32, u32>>,
    /// One finder per map, behind one lock. The finder keeps scratch arrays between searches, so
    /// sharing it is the point.
    pub finders: std::sync::Mutex<std::collections::HashMap<i32, PathContext>>,
}

impl Default for PathGraphs {
    fn default() -> Self {
        PathGraphs::empty()
    }
}

impl PathGraphs {
    /// Import every map's graph out of a `Gw.dat`, which is what `GameSrv_LoadMap` does with the blob its
    /// map config names.
    ///
    /// A map whose blob is missing or does not import is left out, with a line saying so: the world still
    /// serves that map, it just cannot say anything about where an agent in it may walk. Only an archive
    /// that cannot be opened at all is an error, and the caller decides what to do about that.
    /// `ASURA/crates/asura-world/src/pathfind.rs:1507`, `PathGraphs::load`.
    pub fn load(archive_path: &str, maps: &[MapEntry]) -> std::io::Result<PathGraphs> {
        let mut archive = crate::pack::GwDat::open(archive_path)?;
        let mut data = std::collections::HashMap::new();

        for map in maps {
            if let Some(graph) = import_graph(&mut archive, archive_path, map) {
                data.entry(map.file_id).or_insert(graph);
            }
        }

        let graphs = PathGraphs(std::sync::Arc::new(PathGraphsInner {
            data: std::sync::Mutex::new(data),
            bound: std::sync::Mutex::new(std::collections::HashMap::new()),
            finders: std::sync::Mutex::new(std::collections::HashMap::new()),
        }));
        graphs.bind(maps);
        Ok(graphs)
    }

    /// Make one map pathable: bind it to a graph already loaded (a dump), or import its blob from the
    /// archive. An archive that will not open is the only error; a blob that is missing or does not import
    /// is logged and leaves the map without pathfinding, exactly as at startup.
    /// `ASURA/crates/asura-world/src/pathfind.rs:1562`, `PathGraphs::import`.
    pub fn import(&self, map: &MapEntry, archive_path: &str) -> std::io::Result<()> {
        let already_loaded = self
            .0
            .data
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .contains_key(&map.file_id);
        if already_loaded {
            self.0
                .bound
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .insert(map.id as i32, map.file_id);
            return Ok(());
        }
        let mut archive = crate::pack::GwDat::open(archive_path)?;
        if let Some(graph) = import_graph(&mut archive, archive_path, map) {
            self.0
                .data
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .entry(map.file_id)
                .or_insert(graph);
        }
        self.0
            .bound
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(map.id as i32, map.file_id);
        Ok(())
    }

    /// Load the graphs from a dump written by the dumper, so no `Gw.dat` is needed at runtime.
    ///
    /// Nothing is bound yet: call [`PathGraphs::bind`] with the maps this world serves.
    pub fn from_dump(path: &str) -> std::io::Result<PathGraphs> {
        let bytes = std::fs::read(path)?;
        let dumped = crate::terrain::graph_dump::decode(&bytes)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        let mut data = std::collections::HashMap::new();
        for graph in dumped {
            data.insert(graph.map_file_id, std::sync::Arc::new(graph.data));
        }
        Ok(PathGraphs(std::sync::Arc::new(PathGraphsInner {
            data: std::sync::Mutex::new(data),
            bound: std::sync::Mutex::new(std::collections::HashMap::new()),
            finders: std::sync::Mutex::new(std::collections::HashMap::new()),
        })))
    }

    /// Bind every map whose blob is among the loaded graphs. Returns how many were bound.
    pub fn bind(&self, maps: &[MapEntry]) -> usize {
        let data = self.0.data.lock().unwrap_or_else(|err| err.into_inner());
        let mut bound = self.0.bound.lock().unwrap_or_else(|err| err.into_inner());
        let mut count = 0;
        for map in maps {
            if data.contains_key(&map.file_id) {
                bound.insert(map.id as i32, map.file_id);
                count += 1;
            }
        }
        count
    }

    /// A world with no graphs: every walk is refused with [`Walk::NoGraph`].
    pub fn empty() -> PathGraphs {
        PathGraphs(std::sync::Arc::new(PathGraphsInner::default()))
    }

    /// How many graphs are loaded - from an archive or a dump - whether or not a map is bound.
    pub fn imported(&self) -> usize {
        self.0
            .data
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .len()
    }

    /// The maps this world can path in, for a startup line and for tests.
    pub fn maps(&self) -> Vec<i32> {
        let mut maps: Vec<i32> = self
            .0
            .bound
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .keys()
            .copied()
            .collect();
        maps.sort_unstable();
        maps
    }

    /// Ask the finder for a walk from `from` to `to` on `map_id`.
    pub fn walk(&self, map_id: i32, from: Pos, to: Pos) -> Walk {
        match self.with_finder(map_id, |finder| {
            let mut waypoints = Vec::new();
            if finder.path_finding(from, to, &mut waypoints) {
                Walk::Found(waypoints)
            } else {
                Walk::NoPath
            }
        }) {
            Some(walk) => walk,
            None => Walk::NoGraph,
        }
    }

    /// Follow manual input through adjacent walkable cells, rather than finding a route.
    ///
    /// `ASURA/crates/asura-world/src/pathfind/directional.rs`, `walk`, whose own header marks it
    /// **PROVISIONAL / NOT ACCEPTED** - the caveat travels with the port.
    pub fn walk_direction(
        &self,
        map_id: i32,
        from: Pos,
        offset: (f32, f32),
        settings: crate::terrain::pathfind::directional::DirectionalSettings,
    ) -> Walk {
        match self.with_finder(map_id, |finder| {
            crate::terrain::pathfind::directional::walk(finder, from, offset, settings)
        }) {
            Some(Some(points)) => Walk::Found(points),
            Some(None) => Walk::NoPath,
            None => Walk::NoGraph,
        }
    }

    /// Find a route to a point on an unspecified plane. Manual movement uses `walk_direction`: choosing
    /// a destination plane globally can send a player off a bridge to reach that point.
    pub fn walk_any_plane(&self, map_id: i32, from: Pos, to: (f32, f32)) -> Walk {
        match self.with_finder(map_id, |finder| {
            let Some(plane) = finder.plane_at(Vec2f { x: to.0, y: to.1 }) else {
                return Walk::NoPath;
            };
            let mut waypoints = Vec::new();
            let destination = Pos {
                x: to.0,
                y: to.1,
                plane,
            };
            if finder.path_finding(from, destination, &mut waypoints) {
                Walk::Found(waypoints)
            } else {
                Walk::NoPath
            }
        }) {
            Some(walk) => walk,
            None => Walk::NoGraph,
        }
    }

    /// Run `use_it` against the finder for `map_id`, or `None` when this world has no graph for it.
    fn with_finder<R>(&self, map_id: i32, use_it: impl FnOnce(&mut PathContext) -> R) -> Option<R> {
        let file_id = self
            .0
            .bound
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(&map_id)
            .copied();
        let data = file_id.and_then(|file_id| {
            self.0
                .data
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .get(&file_id)
                .map(std::sync::Arc::clone)
        });
        let data = data?;

        let mut finders = self.0.finders.lock().unwrap_or_else(|err| err.into_inner());
        let finder = finders
            .entry(map_id)
            .or_insert_with(|| PathContext::new(data));
        Some(use_it(finder))
    }
}

// ---------------------------------------------------------------------------------------------
// ASURA/crates/asura-world/src/pathfind.rs, lines 1746-1794: the blob a map's path graph lives at.
// `PathGraphs::load`/`import` (1507-1592) and `import_graph` (1703-1744) wait on the importer in
// `map_data.rs`, which is the last piece of this subject.
// ---------------------------------------------------------------------------------------------

/// Read one map's blob out of an open archive and import it, or say why it is absent.
///
/// `ASURA/crates/asura-world/src/pathfind.rs:1703`, `import_graph`: every line it prints is the working
/// implementation's own.
pub fn import_graph(
    archive: &mut crate::pack::GwDat,
    archive_path: &str,
    map: &MapEntry,
) -> Option<std::sync::Arc<PathStaticData>> {
    match find_path_blob(archive, map.file_id, map.id as i32) {
        Ok(Some((blob_id, content))) => {
            if blob_id != map.file_id {
                eprintln!(
                    "world: map {} ({}) path graph comes from blob {blob_id:#X} rather than \
                     {:#X}: the archive binds the plain logical id (the bit-31 replacement flag, \
                     docs/03-roadmap.md)",
                    map.id, map.name, map.file_id
                );
            }
            match PathStaticData::import(&content) {
                Ok(imported) => Some(std::sync::Arc::new(imported)),
                Err(err) => {
                    eprintln!(
                        "world: map {} ({}) has no path graph: blob {blob_id:#X} did not import: {err}",
                        map.id, map.name
                    );
                    None
                }
            }
        }
        Ok(None) => {
            eprintln!(
                "world: map {} ({}) has no path graph: blob {:#X} is not in {archive_path}",
                map.id, map.name, map.file_id
            );
            None
        }
        Err(err) => {
            eprintln!(
                "world: map {} ({}) has no path graph: blob {:#X} could not be read: {err}",
                map.id, map.name, map.file_id
            );
            None
        }
    }
}

/// The blob a map's path graph actually lives at, tolerating the archive's bit-31 replacement rule.
///
/// The reference's catalogue names some blobs with bit 31 set (`0x8005EA96` for The Northlands) while a
/// client archive may bind the plain logical id (`0x5EA96`): the bit is the archive's "replacement
/// pending" flag, not part of the map's identity. So the candidates are the configured id, then the
/// plain one, then the flagged one, and the caller logs when a substitution happened.
///
/// Returns the id that answered and its bytes. `Ok(None)` means none of the three is in the archive; the
/// caller decides whether that is a skip or an error. `ASURA/crates/asura-world/src/pathfind.rs:1756`.
pub fn find_path_blob(
    archive: &mut crate::pack::GwDat,
    wanted: u32,
    map_id: i32,
) -> std::io::Result<Option<(u32, Vec<u8>)>> {
    let mut candidates = vec![wanted];
    let plain = wanted & 0x7FFF_FFFF;
    if plain != wanted {
        candidates.push(plain);
    }
    let flagged = wanted | 0x8000_0000;
    if flagged != wanted {
        candidates.push(flagged);
    }
    let mut first_error = None;
    for candidate in candidates {
        match archive.file(candidate) {
            Ok(Some(content)) => {
                if candidate != wanted {
                    eprintln!(
                        "world: map {map_id} path graph found at blob {candidate:#X} \
                         (the catalogue names {wanted:#X})"
                    );
                }
                return Ok(Some((candidate, content)));
            }
            Ok(None) => {}
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }
    match first_error {
        Some(err) => Err(err),
        None => Ok(None),
    }
}

/// What the finder said about a walk.
///
/// `ASURA/crates/asura-world/src/agent.rs:570`.
#[derive(Debug, Clone)]
pub enum Walk {
    /// A path, in the reference's reduced form. It may hold a single waypoint naming the destination.
    Found(Vec<Waypoint>),
    /// The finder ran and refused: the destination is not reachable from where the agent stands.
    NoPath,
    /// This world has no graph for the map, so nothing can be said about reachability. That happens when
    /// the map's blob could not be imported from `Gw.dat` -- a server whose archive is missing or older
    /// than the catalogue refuses walks rather than inventing them.
    NoGraph,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two stacked trapezoids, one plane: the lower from y 0 to 5, the upper from 5 to 10, x 0 to 10,
    /// each naming the other as its top/bottom neighbour, and a y-node at y = 5 whose two halves are
    /// their leaves. That is the smallest graph the reference's own structures can describe.
    fn two_stacked_traps() -> std::sync::Arc<PathStaticData> {
        use crate::terrain::map_data::{NodeRef, Plane, SinkNode, YNode};
        let lower = Trapezoid {
            trap_id: 0,
            top_left: Some(1),
            top_right: Some(1),
            yt: 5.0,
            yb: 0.0,
            xtl: 0.0,
            xtr: 10.0,
            xbl: 0.0,
            xbr: 10.0,
            portal_left: u16::MAX,
            portal_right: u16::MAX,
            ..Trapezoid::default()
        };
        let upper = Trapezoid {
            trap_id: 1,
            bottom_left: Some(0),
            bottom_right: Some(0),
            yt: 10.0,
            yb: 5.0,
            xtl: 0.0,
            xtr: 10.0,
            xbl: 0.0,
            xbr: 10.0,
            portal_left: u16::MAX,
            portal_right: u16::MAX,
            ..Trapezoid::default()
        };
        let plane = Plane {
            plane_id: 0,
            trapezoids: vec![lower, upper],
            ynodes: vec![YNode {
                pos: Vec2f { x: 0.0, y: 5.0 },
                above: Some(NodeRef::Sink(1)),
                bellow: Some(NodeRef::Sink(0)),
            }],
            sink_nodes: vec![SinkNode { trap: 0 }, SinkNode { trap: 1 }],
            root_node: Some(NodeRef::Y(0)),
            ..Plane::default()
        };
        std::sync::Arc::new(PathStaticData {
            planes: vec![plane],
            traps_count: 2,
        })
    }

    fn pos(x: f32, y: f32) -> Pos {
        Pos { x, y, plane: 0 }
    }

    /// The A* walks from one trapezoid to the one above it: `PathFinding` answers true and the last
    /// waypoint is the destination, in the trapezoid it stands in.
    #[test]
    fn a_walk_between_two_trapezoids_is_found_and_ends_at_the_destination() {
        let mut context = PathContext::new(two_stacked_traps());
        let src = pos(5.0, 2.0);
        let dst = pos(5.0, 8.0);
        assert_eq!(
            context.find_trapezoid(src),
            Some(0),
            "the start is in the lower one"
        );
        assert_eq!(
            context.find_trapezoid(dst),
            Some(1),
            "the destination in the upper"
        );
        let mut waypoints = Vec::new();
        let found = context.path_finding(src, dst, &mut waypoints);
        assert!(found, "the walk exists");
        assert!(!waypoints.is_empty(), "and it has waypoints");
        let last = waypoints.last().expect("a waypoint");
        assert_eq!(last.pos, dst, "the last waypoint is the destination");
        assert_eq!(last.trap_id, 1, "in the trapezoid it stands in");
    }

    /// Two positions in the **same** trapezoid are one waypoint naming the destination, and the search
    /// does not run at all.
    #[test]
    fn a_walk_inside_one_trapezoid_is_a_single_waypoint() {
        let mut context = PathContext::new(two_stacked_traps());
        let src = pos(2.0, 2.0);
        let dst = pos(8.0, 3.0);
        let mut waypoints = Vec::new();
        assert!(context.path_finding(src, dst, &mut waypoints));
        assert_eq!(waypoints.len(), 1);
        assert_eq!(waypoints[0].pos, dst);
        assert_eq!(waypoints[0].trap_id, 0);
    }

    /// A graph with no plane at all answers `None` from the split walk, so the search refuses the walk
    /// rather than inventing a path - and a point outside a trapezoid's own edges is **not** that case:
    /// the split walk is the BSP's answer, not a containment test, so the reference still walks to the
    /// leaf it lands in.
    #[test]
    fn a_position_on_no_plane_finds_no_walk() {
        let mut context = PathContext::new(std::sync::Arc::new(PathStaticData::default()));
        let mut waypoints = Vec::new();
        assert!(!context.path_finding(pos(5.0, 99.0), pos(5.0, 2.0), &mut waypoints));
        assert!(waypoints.is_empty());
        assert!(!context.path_finding(pos(5.0, 2.0), pos(5.0, 99.0), &mut waypoints));
        assert!(waypoints.is_empty());

        // And on the two-trapezoid graph, a point above both still lands in a leaf, so it walks.
        let mut context = PathContext::new(two_stacked_traps());
        let mut waypoints = Vec::new();
        assert_eq!(context.find_trapezoid(pos(5.0, 99.0)), Some(1));
        assert!(context.path_finding(pos(5.0, 99.0), pos(5.0, 2.0), &mut waypoints));
    }

    /// A dump written by `graph_dump::encode`, loaded by `PathGraphs::from_dump`, bound to a map, and
    /// walked: the round trip ASURA's own server does instead of reading a 3.6 GB archive. A map that
    /// is not bound answers `NoGraph`, which is the only thing that should ever answer it.
    #[test]
    fn a_graph_loaded_from_a_dump_walks_and_an_unbound_map_has_no_graph() {
        let data = (*two_stacked_traps()).clone();
        let bytes =
            crate::terrain::graph_dump::encode(&[crate::terrain::graph_dump::DumpedGraph {
                map_id: 148,
                map_file_id: 0x1B97D,
                data,
            }]);
        let path = std::env::temp_dir().join("kryta-path-graph-test.dump");
        std::fs::write(&path, &bytes).expect("write the dump");
        let graphs = PathGraphs::from_dump(&path.to_string_lossy()).expect("load the dump");
        assert_eq!(graphs.imported(), 1, "one graph is loaded");
        assert!(graphs.maps().is_empty(), "nothing is bound yet");

        let maps = vec![
            MapEntry {
                id: 148,
                kind: Some(0),
                file_id: 0x1B97D,
                name: "Ascalon City (pre-Searing)".to_string(),
                instance_value: Some(0xBB14_F539),
                name_id: None,
            },
            MapEntry {
                id: 146,
                kind: Some(1),
                file_id: 0x1B97E,
                name: "Lakeside County".to_string(),
                instance_value: None,
                name_id: None,
            },
        ];
        assert_eq!(
            graphs.bind(&maps),
            1,
            "only the map whose blob is loaded binds"
        );
        assert_eq!(graphs.maps(), vec![148]);

        assert!(
            matches!(
                graphs.walk(148, pos(5.0, 2.0), pos(5.0, 8.0)),
                Walk::Found(_)
            ),
            "the bound map walks"
        );
        assert!(
            matches!(
                graphs.walk(146, pos(5.0, 2.0), pos(5.0, 8.0)),
                Walk::NoGraph
            ),
            "a map with no graph answers NoGraph"
        );
        assert!(
            matches!(
                graphs.walk_any_plane(148, pos(5.0, 2.0), (5.0, 8.0)),
                Walk::Found(_)
            ),
            "a destination on an unspecified plane is found too"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Manual movement follows the direction it is given through adjacent cells. ASURA's own proof of
    /// this walker is a live capture over a real map's dumped graph; this is the structural one.
    #[test]
    fn a_manual_offset_walks_the_direction_it_is_given() {
        let data = (*two_stacked_traps()).clone();
        let bytes =
            crate::terrain::graph_dump::encode(&[crate::terrain::graph_dump::DumpedGraph {
                map_id: 148,
                map_file_id: 0x1B97D,
                data,
            }]);
        let path = std::env::temp_dir().join("kryta-directional-test.dump");
        std::fs::write(&path, &bytes).expect("write the dump");
        let graphs = PathGraphs::from_dump(&path.to_string_lossy()).expect("load the dump");
        graphs.bind(&[MapEntry {
            id: 148,
            kind: Some(0),
            file_id: 0x1B97D,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: None,
            name_id: None,
        }]);
        let settings = crate::terrain::pathfind::directional::DirectionalSettings::default();
        // Up from the lower trapezoid: the walk follows the direction across the edge above it.
        let walked = graphs.walk_direction(148, pos(5.0, 2.0), (0.0, 1.0), settings);
        let Walk::Found(points) = walked else {
            panic!("an upward offset walks: {walked:?}");
        };
        assert!(!points.is_empty(), "and it says where");
        let last = points.last().expect("a point");
        assert!(last.pos.y > 2.0, "it moved up: {:?}", last.pos);
        // A map with no graph has nothing to say, whatever the direction.
        assert!(matches!(
            graphs.walk_direction(146, pos(5.0, 2.0), (0.0, 1.0), settings),
            Walk::NoGraph
        ));
        let _ = std::fs::remove_file(&path);
    }

    /// `PathHeapPush`/`PathHeapPop` pop the cheapest node first, and keep doing it as the heap shrinks -
    /// the order is the order the search visits trapezoids in, which is why the reference's own
    /// sift-down is kept rather than a tidy one.
    /// The whole archive path: `PathGraphs::load` reads map 148's own blob out of this machine's
    /// `Gw.dat`, binds it, and the map walks from the point its config spawns on. Skipped when there is
    /// no archive to read.
    #[test]
    fn a_map_loaded_from_the_archive_walks_where_its_spawn_stands() {
        let Some(path) = std::env::var("KRYTA_GW_DAT").ok().or_else(|| {
            let candidate = std::path::PathBuf::from("Gw.dat");
            candidate
                .exists()
                .then(|| candidate.to_string_lossy().into_owned())
        }) else {
            eprintln!("skipped: no local Gw.dat");
            return;
        };
        let map = MapEntry {
            id: 148,
            kind: Some(0),
            file_id: 0x1B97D,
            name: "Ascalon City (pre-Searing)".to_string(),
            instance_value: Some(0xBB14_F539),
            name_id: None,
        };
        let graphs = PathGraphs::load(&path, std::slice::from_ref(&map)).expect("open the archive");
        assert_eq!(graphs.imported(), 1, "map 148s blob imported");
        assert_eq!(graphs.maps(), vec![148], "and it is bound");
        // The spawn point the map's own config names, and a walk a few dozen units away from it.
        let spawn = pos(9541.87, 4796.46);
        let walked = graphs.walk(148, spawn, pos(9600.0, 4850.0));
        assert!(
            matches!(walked, Walk::Found(_)),
            "the map walks from its own spawn point: {walked:?}"
        );
        // A map this world does not bind has no graph, whatever the archive holds.
        assert!(matches!(
            graphs.walk(146, spawn, pos(9600.0, 4850.0)),
            Walk::NoGraph
        ));
    }

    #[test]
    fn the_heap_pops_in_cost_order() {
        let mut heap = Vec::new();
        for (cost, trap_id) in [
            (5.0f32, 5u32),
            (1.0, 1),
            (3.0, 3),
            (2.0, 2),
            (4.0, 4),
            (0.5, 6),
        ] {
            heap_push(&mut heap, HeapNode { cost, trap_id });
        }
        let mut popped = Vec::new();
        while let Some(node) = heap_pop(&mut heap) {
            popped.push(node.cost);
        }
        assert_eq!(popped, vec![0.5, 1.0, 2.0, 3.0, 4.0, 5.0], "cheapest first");
        assert!(heap.is_empty());
    }

    /// `PathVec2fPreciseCross` calls near-parallel parallel: a product under a hundredth is zero, and a
    /// perpendicular pair keeps its sign.
    #[test]
    fn a_near_parallel_cross_is_treated_as_parallel() {
        let along = Vec2f { x: 1.0, y: 0.0 };
        assert_eq!(
            precise_cross(along, Vec2f { x: 1.0, y: 0.0 }),
            0.0,
            "parallel"
        );
        let nearly = Vec2f { x: 1.0, y: 0.001 };
        assert_eq!(precise_cross(along, nearly), 0.0, "nearly parallel");
        let across = Vec2f { x: 0.0, y: 1.0 };
        assert_eq!(precise_cross(along, across), 1.0, "perpendicular");
        assert_eq!(precise_cross(across, along), -1.0, "and the other order");
        assert_eq!(
            precise_cross(Vec2f::default(), across),
            0.0,
            "a zero vector has no direction"
        );
    }
}
