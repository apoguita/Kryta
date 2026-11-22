//! `game/Agents/` - the reference's `src/server/game/Entities/` (84 files).
//!
//! Renamed: the reference calls this directory `Entities/`.
//!
//! What is here: a player's own agent - where it stands, which way it faces, and the walk it is in.
//! It is the working implementation's `AgentState`, ported whole: `ASURA/crates/asura-world/src/agent.rs:89`
//! for the state, `:111` for the walk, `:339` for the position report, `:390` for a correction, and
//! `:293` for a tick. Every field and every branch is that file's; the citations are above each one.
//!
//! The three numbers it runs by are the working implementation's too: `POSITION_UPDATE_TOLERANCE`
//! (`agent.rs:59`), `POSITION_UPDATE_MAX_ELAPSED_MS` (`agent.rs:63`) and the correction ceiling, which
//! is its setting `config/Server/movement.json` (`max_allowed_correction`).

#[path = "Entry.rs"]
pub mod entry;
pub use entry::{Entering, Landing, entry_point};

#[path = "Player.rs"]
pub mod player;
pub(crate) use player::Player;

use std::time::{Duration, Instant};

use crate::terrain::pathfind::{Pos, Waypoint};

/// How far over its speed budget a client's position report may travel, in game units
/// (`ASURA/crates/asura-world/src/agent.rs:59`).
pub const POSITION_UPDATE_TOLERANCE: f32 = 50.0;

/// The longest gap between two reports that still counts as elapsed time; a longer one is measured at
/// this, because a client that was not moving cannot have travelled further
/// (`ASURA/crates/asura-world/src/agent.rs:63`).
pub const POSITION_UPDATE_MAX_ELAPSED_MS: u32 = 2_000;

/// What the agent's state did with a position the client reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adopted {
    /// The report was within its budget and is now the server's own copy.
    Accepted,
    /// The report was farther than its budget and was walked back to the budget's edge.
    Clamped,
    /// The report changed the plane and was farther than the budget: refused outright.
    Refused,
}

/// One agent's body: where it is, and the walk it is in.
///
/// `ASURA/crates/asura-world/src/agent.rs:89`.
#[derive(Debug, Clone)]
pub struct Agent {
    /// The id every message about this body names.
    pub agent_id: u32,
    /// Where it stands, in the game's own units.
    pub position: (f32, f32),
    /// Which plane it stands on.
    pub plane: u16,
    /// Which way it faces while it is not walking, as the unit vector the world announced when it
    /// created the agent. The spawn point's angle lands here.
    pub facing: (f32, f32),
    /// When it last reported where it was, for the budget below.
    pub last_report: Option<Instant>,
    /// Its walking speed, which is what the budget is computed from.
    pub speed_base: f32,
    /// The walk in progress, if any.
    pub motion: Motion,
    /// Last announced movement type: 1 forward, 4 backward, 7/8 strafe, 9 stopped.
    pub move_type: u8,
    /// The movement speed factor sent with the type; separate from the base speed.
    pub move_speed: f32,
    /// Last announced manual direction, for suppressing unchanged direction replies.
    pub move_direction: (f32, f32),
}

/// The walk an agent is in: one segment being walked, and the waypoints after it.
///
/// The reference's `GmAgent` walking state (`ASURA/crates/asura-world/src/agent.rs:111`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Motion {
    /// The path the server chose. The segment being walked has already been taken off the front of it,
    /// exactly as `waypoint_idx` does in the reference.
    pub waypoints: Vec<Waypoint>,
    /// The next waypoint to take when the current segment ends.
    pub waypoint_idx: usize,
    /// Where the current segment ends, plane included.
    pub destination: Pos,
    /// A unit vector from the agent toward `destination`.
    pub direction: (f32, f32),
    /// Zero when the agent is standing still.
    pub speed: f32,
}

impl Agent {
    /// A body standing where the connection placed it, facing the spawn point's direction.
    ///
    /// `ASURA/crates/asura-world/src/agent.rs:129`.
    pub fn new(
        agent_id: u32,
        position: (f32, f32),
        plane: u16,
        facing: (f32, f32),
        speed_base: f32,
    ) -> Self {
        Self {
            agent_id,
            position,
            plane,
            facing,
            last_report: None,
            speed_base,
            motion: Motion::default(),
            move_type: 1,
            move_speed: 1.0,
            move_direction: (0.0, 0.0),
        }
    }

    /// The agent's position as a point on the map's graph (`agent.rs:151`).
    pub fn here(&self) -> Pos {
        Pos {
            x: self.position.0,
            y: self.position.1,
            plane: self.plane,
        }
    }

    /// Begin walking a path the finder produced (`agent.rs:164`).
    ///
    /// The reference takes the first waypoint as the destination and leaves the rest in the list; this
    /// is that, and it returns the waypoint so the caller can broadcast it. `None` when the path is
    /// empty, which the reference also ignores (`if (0 < agent->waypoints.len)`).
    pub fn begin_walk(&mut self, waypoints: Vec<Waypoint>) -> Option<Waypoint> {
        self.motion.waypoints = waypoints;
        self.motion.waypoint_idx = 0;
        let first = *self.motion.waypoints.first()?;
        self.motion.waypoint_idx = 1;
        self.set_destination(first.pos);
        Some(first)
    }

    /// Point at `destination` and start moving (`agent.rs:180`).
    ///
    /// The reference divides by the distance without checking it, so a destination equal to the
    /// current position produces a NaN direction and then a NaN position - which would be saved and
    /// reloaded on the next visit. This keeps the direction it had instead, which makes such a walk a
    /// no-op that the arrival test clears on the next tick. That is the working implementation's own
    /// deliberate difference and it is kept.
    pub fn set_destination(&mut self, destination: Pos) {
        self.motion.speed = self.speed_base * self.move_speed;
        self.motion.destination = destination;
        let dx = destination.x - self.position.0;
        let dy = destination.y - self.position.1;
        let norm = ((dx * dx) + (dy * dy)).sqrt();
        if norm > 0.0 {
            self.motion.direction = (dx / norm, dy / norm);
        }
    }

    /// Stand where you are: the destination collapses to the position and the speed to zero
    /// (`agent.rs:193`).
    pub fn cancel_movement(&mut self) {
        self.motion.destination = self.here();
        self.motion.speed = 0.0;
    }

    /// Take a position the client reported as the agent's own, exactly (`agent.rs:205`).
    pub fn adopt_exact(&mut self, position: (f32, f32), plane: u16) {
        self.position = position;
        self.plane = plane;
    }

    /// Start walking straight at `destination`, replacing any walk in progress (`agent.rs:214`).
    pub fn walk_to(&mut self, destination: Pos) {
        self.motion.waypoints.clear();
        self.motion.waypoint_idx = 0;
        self.set_destination(destination);
    }

    /// Replace the previous order and announce a manual walk (`agent.rs:222`).
    ///
    /// The reply order is the live capture's (direction, type, destination), and the types carry the
    /// speed factors the reference's own table does: 4-6 walk at 0.66, 7-8 at 0.75, everything else at
    /// 1.0.
    pub fn begin_manual_walk(
        &mut self,
        waypoints: Vec<Waypoint>,
        direction: (f32, f32),
        move_type: u8,
    ) -> Vec<(u16, Vec<u8>)> {
        self.cancel_movement();
        self.motion.waypoints.clear();
        self.motion.waypoint_idx = 0;
        if waypoints.is_empty() {
            return Vec::new();
        }
        let speed = match move_type {
            4..=6 => 0.66,
            7..=8 => 0.75,
            _ => 1.0,
        };
        let mut reply = Vec::new();
        let dx = direction.0 - self.move_direction.0;
        let dy = direction.1 - self.move_direction.1;
        if dx * dx + dy * dy >= 0.0003 || self.move_type != move_type {
            reply.push(
                crate::server::packets::agent_packets::agent_update_direction(
                    self.agent_id,
                    direction,
                    move_type,
                ),
            );
        }
        if self.move_type != move_type || self.move_speed != speed {
            reply.push(crate::server::packets::agent_packets::agent_update_speed(
                self.agent_id,
                speed,
                move_type,
            ));
        }
        self.move_type = move_type;
        self.move_speed = speed;
        self.move_direction = direction;
        if let Some(first) = self.begin_walk(waypoints) {
            reply.push(crate::server::packets::agent_packets::agent_move_to_point(
                self.agent_id,
                (first.pos.x, first.pos.y),
                first.pos.plane,
                self.plane,
            ));
        }
        reply
    }

    /// The keyboard stop that carries its stopping point: type 9, then a destination at the point
    /// (`agent.rs:267`).
    pub fn stop_manual_walk(&mut self) -> Vec<(u16, Vec<u8>)> {
        self.stand_still();
        vec![
            crate::server::packets::agent_packets::agent_update_speed(self.agent_id, 1.0, 9),
            crate::server::packets::agent_packets::agent_move_to_point(
                self.agent_id,
                self.position,
                self.plane,
                self.plane,
            ),
        ]
    }

    /// The walk is over, at rest, with no waypoints left, and nothing goes on the wire
    /// (`agent.rs:280`).
    ///
    /// Used when the stopping point is refused: a refused correction still means the agent is not
    /// walking, so the server's copy must stop - otherwise it keeps advancing along the abandoned path
    /// and tells every client to keep walking it.
    pub fn stand_still(&mut self) {
        self.cancel_movement();
        self.motion.waypoints.clear();
        self.motion.waypoint_idx = 0;
        self.move_type = 9;
        self.move_speed = 1.0;
    }

    /// Walk `delta` seconds of the current segment (`agent.rs:293`).
    ///
    /// Returns the waypoint that became the new destination, if the agent crossed one during the tick -
    /// the caller broadcasts it, because every client is drawing this agent from the last destination it
    /// was told. Arrival clears the walk, as the reference does.
    pub fn advance(&mut self, delta: f32) -> Option<Waypoint> {
        if self.motion.speed == 0.0 {
            return None;
        }

        let mut distance = delta * self.motion.speed;
        let destination = self.motion.destination;
        let to_destination = distance_between(self.position, (destination.x, destination.y));

        let mut crossed = None;
        if to_destination <= distance {
            distance -= to_destination;

            self.position = (destination.x, destination.y);
            self.plane = destination.plane;

            if self.motion.waypoint_idx < self.motion.waypoints.len() {
                let next = self.motion.waypoints[self.motion.waypoint_idx];
                self.motion.waypoint_idx += 1;
                self.set_destination(next.pos);
                crossed = Some(next);
            } else {
                distance = 0.0;
            }
        }

        if 0.0 < distance {
            let (dx, dy) = self.motion.direction;
            self.position.0 += dx * distance;
            self.position.1 += dy * distance;
        }

        if self.plane == destination.plane
            && self.position.0 == destination.x
            && self.position.1 == destination.y
        {
            self.motion.destination = Pos::default();
            self.motion.speed = 0.0;
            self.motion.waypoint_idx = 0;
            self.motion.waypoints.clear();
        }

        crossed
    }

    /// Adopt a position the client reported (`agent.rs:339`).
    ///
    /// The first report of a session gets the full budget rather than none: the client has been standing
    /// still while it loaded, and the reference makes the same choice. A report farther than the budget
    /// is walked back to the budget's edge along its own direction; a report that changes the plane and
    /// is farther than the budget is refused.
    pub fn adopt_report(&mut self, position: (f32, f32), plane: u16, now: Instant) -> Adopted {
        let elapsed_ms = match self.last_report {
            None => POSITION_UPDATE_MAX_ELAPSED_MS,
            Some(last) => {
                let elapsed = now.saturating_duration_since(last).as_millis() as u32;
                if elapsed == 0 || elapsed > POSITION_UPDATE_MAX_ELAPSED_MS {
                    POSITION_UPDATE_MAX_ELAPSED_MS
                } else {
                    elapsed
                }
            }
        };
        self.last_report = Some(now);

        let allowed = self.speed_base * (elapsed_ms as f32 / 1000.0) + POSITION_UPDATE_TOLERANCE;
        let distance = self.distance_to(position);

        if plane != self.plane {
            // A plane change is legitimate when it is close by and suspicious when it is not.
            return if allowed < distance {
                Adopted::Refused
            } else {
                self.plane = plane;
                self.position = position;
                Adopted::Accepted
            };
        }

        if distance <= allowed {
            self.position = position;
            return Adopted::Accepted;
        }

        // Out of budget: move as far as the budget allows, along the reported direction.
        let step = if distance > 0.0 {
            allowed / distance
        } else {
            0.0
        };
        self.position = (
            self.position.0 + (position.0 - self.position.0) * step,
            self.position.1 + (position.1 - self.position.1) * step,
        );
        Adopted::Clamped
    }

    /// Adopt a correction the client sends after cancelling a walk (`agent.rs:390`).
    ///
    /// Only a short correction on the same plane counts; how short is the world's
    /// `MaxAllowedCorrection` setting, which is the working implementation's
    /// `config/Server/movement.json`.
    pub fn adopt_correction(
        &mut self,
        position: (f32, f32),
        plane: u16,
        max_allowed_correction: f32,
    ) -> bool {
        if plane != self.plane || max_allowed_correction < self.distance_to(position) {
            return false;
        }
        self.position = position;
        true
    }

    /// How long ago the client last reported, for the trace (`agent.rs:399`).
    pub fn since_last_report(&self, now: Instant) -> Option<Duration> {
        self.last_report
            .map(|last| now.saturating_duration_since(last))
    }

    /// The straight-line distance to a point (`agent.rs:404`).
    pub fn distance_to(&self, other: (f32, f32)) -> f32 {
        distance_between(self.position, other)
    }
}

/// The real square root, which is what the reference's movement uses (`agent.rs:411`).
pub fn distance_between(left: (f32, f32), right: (f32, f32)) -> f32 {
    let dx = right.0 - left.0;
    let dy = right.1 - left.1;
    ((dx * dx) + (dy * dy)).sqrt()
}

/// How far apart two announced points of a walk are, in game units (`agent.rs:625`).
///
/// The live capture's points are 145-158 units apart - a copy walking at the character's own 288 u/s
/// and re-announcing the next point about every half second. That cadence, rather than the corner of a
/// trapezoid, is what keeps the client and the server's copy together.
pub const LIVE_CADENCE_STEP: f32 = 145.0;

/// Sample a walk so no two points are farther apart than `step`, keeping every point the finder
/// produced and adding the intermediate ones on the same straight segment (`agent.rs:646`).
///
/// The added points carry the plane the segment **starts** on, and only the finder's own point carries
/// its plane: that is the reference's own stamping, so a plane change is announced once, at the corner.
pub fn densify(waypoints: Vec<Waypoint>, from: Pos, step: f32) -> Vec<Waypoint> {
    if waypoints.is_empty() || step <= 0.0 {
        return waypoints;
    }
    let mut dense = Vec::with_capacity(waypoints.len() * 4);
    let mut at = from;
    for waypoint in waypoints {
        let dx = waypoint.pos.x - at.x;
        let dy = waypoint.pos.y - at.y;
        let distance = ((dx * dx) + (dy * dy)).sqrt();
        let pieces = (distance / step).ceil();
        let pieces = if pieces.is_finite() && pieces > 1.0 {
            pieces as usize
        } else {
            1
        };
        for piece in 1..pieces {
            let fraction = piece as f32 / pieces as f32;
            dense.push(Waypoint {
                pos: Pos {
                    x: at.x + dx * fraction,
                    y: at.y + dy * fraction,
                    plane: at.plane,
                },
                trap_id: waypoint.trap_id,
            });
        }
        dense.push(waypoint);
        at = waypoint.pos;
    }
    dense
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walker() -> Agent {
        Agent::new(1, (0.0, 0.0), 0, (0.0, 1.0), 288.0)
    }

    #[test]
    fn a_report_inside_its_budget_is_taken_exactly() {
        let mut agent = walker();
        let now = Instant::now();
        assert_eq!(agent.adopt_report((10.0, 0.0), 0, now), Adopted::Accepted);
        assert_eq!(agent.position, (10.0, 0.0));

        // One second later the budget is speed * 1 s + the tolerance: 288 + 50.
        let later = now + Duration::from_secs(1);
        assert_eq!(
            agent.adopt_report((300.0, 0.0), 0, later),
            Adopted::Accepted
        );
        assert_eq!(agent.position, (300.0, 0.0));
    }

    #[test]
    fn a_report_past_its_budget_is_walked_back_to_the_edge() {
        let mut agent = walker();
        let now = Instant::now();
        assert_eq!(agent.adopt_report((0.0, 0.0), 0, now), Adopted::Accepted);
        // A single millisecond of budget at 288 u/s plus the 50-unit tolerance.
        let later = now + Duration::from_millis(1);
        assert_eq!(
            agent.adopt_report((1000.0, 0.0), 0, later),
            Adopted::Clamped
        );
        assert!(agent.position.0 < 1000.0, "the report was not taken whole");
        assert!(agent.position.0 > 0.0, "and the agent did move toward it");
    }

    #[test]
    fn a_plane_change_is_refused_when_it_is_far_and_taken_when_it_is_near() {
        let mut agent = walker();
        let now = Instant::now();
        assert_eq!(agent.adopt_report((0.0, 0.0), 0, now), Adopted::Accepted);
        let later = now + Duration::from_millis(1);
        assert_eq!(
            agent.adopt_report((9000.0, 0.0), 1, later),
            Adopted::Refused
        );
        assert_eq!(agent.plane, 0, "the refused report changed nothing");

        let later = later + Duration::from_millis(1);
        assert_eq!(agent.adopt_report((5.0, 0.0), 1, later), Adopted::Accepted);
        assert_eq!(agent.plane, 1);
    }

    #[test]
    fn a_walk_advances_by_its_speed_and_stops_on_arrival() {
        let mut agent = walker();
        agent.set_destination(Pos {
            x: 100.0,
            y: 0.0,
            plane: 0,
        });
        assert!(agent.motion.speed > 0.0);
        // Half a second at 288 u/s is 144 units: past the destination.
        assert_eq!(agent.advance(0.5), None);
        assert_eq!(agent.position, (100.0, 0.0));
        assert_eq!(agent.motion.speed, 0.0, "an arrival clears the walk");
    }

    #[test]
    fn a_walk_crosses_a_waypoint_and_reports_it() {
        let mut agent = walker();
        let first = Waypoint {
            pos: Pos {
                x: 10.0,
                y: 0.0,
                plane: 0,
            },
            trap_id: 7,
        };
        let second = Waypoint {
            pos: Pos {
                x: 20.0,
                y: 0.0,
                plane: 0,
            },
            trap_id: 8,
        };
        // `begin_walk` takes the first point as the current destination and leaves the rest in the list,
        // so the point a tick crosses is the next one's (`agent.rs:164`, `:309`).
        let started = agent
            .begin_walk(vec![first, second])
            .expect("a first point");
        assert_eq!(started.trap_id, 7, "the walk starts at the first point");
        assert_eq!(agent.motion.waypoint_idx, 1);

        // A tenth of a second at 288 u/s is 28.8 units: past the first point, not the second.
        let crossed = agent.advance(0.1).expect("the first point is crossed");
        assert_eq!(crossed.trap_id, 8);
        assert_eq!(agent.motion.waypoint_idx, 2);
    }

    #[test]
    fn a_correction_is_taken_within_the_ceiling_and_refused_beyond_it() {
        let mut agent = walker();
        assert!(agent.adopt_correction((100.0, 0.0), 0, 250.0));
        assert_eq!(agent.position, (100.0, 0.0));
        assert!(!agent.adopt_correction((1000.0, 0.0), 0, 250.0));
        assert_eq!(
            agent.position,
            (100.0, 0.0),
            "the refused one changed nothing"
        );
        assert!(
            !agent.adopt_correction((100.0, 0.0), 3, 250.0),
            "another plane"
        );
    }
}
