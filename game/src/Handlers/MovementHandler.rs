//! The movement family: the four messages that say where an agent is and where it wants to go.
//!
//! Reference: `src/server/game/Handlers/MovementHandler.cpp`, whose `HandleMovementOpcodes`-family
//! members read the message and call into the movement objects. The behaviours are ASURA's, ported
//! whole - `ASURA/crates/asura-world/src/agent.rs:710` (`handle_move_to_coord`), `:887`
//! (`handle_position_update`), `:1062` (`handle_cancel_movement`) and `:1117` (`handle_last_position`).
//!
//! What lives here is the translation: the message's fields, the answer the handler sends, and the
//! calls it makes into `Agents/` (the agent's own state and steps) and `Maps/Portals.rs` (the wall a
//! step crosses). The doorway check a report can make stays the clock's business too:
//! `WorldSession::step_map` decides it on the segment the agent's own advance walked
//! (`ASURA/crates/asura-world/src/instance.rs:290-308`), and `cross_portal` serves it.
//!
//! **No frame, order or size changed when these moved out of `WorldSession.rs`.**

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::server::packets::agent_packets;
use crate::server::world_session::WorldSession;

impl WorldSession {
    /// `GAME_CMSG_MOVE_TO_COORD` (`0x803E`): the agent is asked to walk to a point.
    ///
    /// `ASURA/crates/asura-world/src/agent.rs:710` (`handle_move_to_coord`), ported whole: the finder
    /// is asked for a path from where the agent stands; an empty path sends **nothing** (the
    /// reference checks `0 < agent->waypoints.len`), and the answer is the finder's first waypoint -
    /// not the client's destination - because the client predicts a straight line and a straight line
    /// through a wall is what this message corrects.
    pub(crate) async fn answer_move_to_coord(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        crypt: &mut AuthCrypt,
    ) {
        let body = &message[2..];
        if body.len() < 12 {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a walk request is {} bytes and its point needs 16; it cannot be read.",
                    message.len()
                ),
            );
            return;
        }
        let destination = (
            f32::from_le_bytes([body[0], body[1], body[2], body[3]]),
            f32::from_le_bytes([body[4], body[5], body[6], body[7]]),
        );
        let plane = u32::from_le_bytes([body[8], body[9], body[10], body[11]]) as u16;
        let to = crate::terrain::Pos {
            x: destination.0,
            y: destination.1,
            plane,
        };
        let Some((agent_id, from, planes)) =
            self.with_agent(|agent| (agent.agent_id, agent.here(), agent.plane))
        else {
            return;
        };
        let walked = self.graphs.walk(self.map_id() as i32, from, to);
        match walked {
            crate::terrain::Walk::Found(waypoints) => {
                let waypoints =
                    crate::agents::densify(waypoints, from, crate::agents::LIVE_CADENCE_STEP);
                let begun = self.with_agent(|agent| {
                    let speed = (agent.move_type != 1 || agent.move_speed != 1.0)
                        .then(|| agent_packets::agent_update_speed(agent_id, 1.0, 1));
                    agent.move_type = 1;
                    agent.move_speed = 1.0;
                    let first = agent.begin_walk(waypoints)?;
                    Some((
                        agent_packets::agent_move_to_point(
                            agent_id,
                            (first.pos.x, first.pos.y),
                            first.pos.plane,
                            agent.plane,
                        ),
                        speed,
                    ))
                });
                if let Some((walk, speed)) = begun.flatten() {
                    let mut reply: Vec<Vec<u8>> = speed.into_iter().map(|(_, f)| f).collect();
                    reply.push(walk.1);
                    // Everyone else is told where this agent is walking: the reference broadcasts the
                    // same message (`ASURA/crates/asura-world/src/agent.rs:849-855`,
                    // `GameSrv_BroadcastAgentMoveToPoint`), and a client that is not told keeps drawing
                    // the walker where it last stood.
                    if let Ok(peers) = self.peers.lock() {
                        peers.broadcast(self.player_id(), self.map_id(), self.district(), &reply);
                    }
                    self.send_quiet(io, peer, crypt, &reply, "the walk").await;
                }
            }
            crate::terrain::Walk::NoPath | crate::terrain::Walk::NoGraph => {
                self.with_agent(|agent| agent.cancel_movement());
                self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: agent {agent_id} cannot walk from ({:.1}, {:.1}) plane {} to ({:.1}, {:.1}) plane {plane}: standing still, and nothing is sent.",
                        from.x, from.y, planes, destination.0, destination.1
                    ),
                );
            }
        }
    }

    /// `GAME_CMSG_POSITION_UPDATE` (`0x803D`): where the client is, and the offset it is walking in.
    ///
    /// `ASURA/crates/asura-world/src/agent.rs:887` (`handle_position_update`), ported whole: the report
    /// is adopted under its elapsed-time budget, a manual walk follows the offset through the map's
    /// connected cells (`walk_direction`), and the answer is the direction, the movement type and the
    /// destination - the live capture's own order (`agent.rs:1000-1029`). A report that is not finite,
    /// or whose type is outside 1-8, is dropped exactly as it is there (`agent.rs:923-929`).
    pub(crate) async fn answer_position_update(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        crypt: &mut AuthCrypt,
    ) {
        let body = &message[2..];
        if body.len() < 24 {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a position report is {} bytes and needs 26; it cannot be read.",
                    message.len()
                ),
            );
            return;
        }
        let position = (
            f32::from_le_bytes([body[0], body[1], body[2], body[3]]),
            f32::from_le_bytes([body[4], body[5], body[6], body[7]]),
        );
        let plane = u32::from_le_bytes([body[8], body[9], body[10], body[11]]) as u16;
        let offset = (
            f32::from_le_bytes([body[12], body[13], body[14], body[15]]),
            f32::from_le_bytes([body[16], body[17], body[18], body[19]]),
        );
        let move_type = u32::from_le_bytes([body[20], body[21], body[22], body[23]]) as u8;
        if ![position.0, position.1, offset.0, offset.1]
            .iter()
            .all(|value| value.is_finite())
            || !(1..=8).contains(&move_type)
        {
            return;
        }
        let now = std::time::Instant::now();
        let Some((adopted, walk_was_running, reply)) = self.with_agent(|agent| {
            let walk_was_running = 0.0 < agent.motion.speed;
            let adopted = agent.adopt_report(position, plane, now);
            let reply = if adopted != crate::agents::Adopted::Refused {
                let from = agent.here();
                let walked = self.graphs.walk_direction(
                    self.map_id() as i32,
                    from,
                    offset,
                    crate::terrain::pathfind::directional::DirectionalSettings::default(),
                );
                let waypoints = match walked {
                    crate::terrain::Walk::Found(waypoints) => waypoints,
                    crate::terrain::Walk::NoPath | crate::terrain::Walk::NoGraph => Vec::new(),
                };
                let norm = (offset.0 * offset.0 + offset.1 * offset.1).sqrt();
                let direction = if norm > 0.0 {
                    (offset.0 / norm, offset.1 / norm)
                } else {
                    (0.0, 0.0)
                };
                agent.begin_manual_walk(waypoints, direction, move_type)
            } else {
                Vec::new()
            };
            (adopted, walk_was_running, reply)
        }) else {
            return;
        };
        let reply: Vec<Vec<u8>> = reply.into_iter().map(|(_, frame)| frame).collect();
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: a position report ({:.1}, {:.1}) plane {plane}: {adopted:?}, server walk running={walk_was_running}, {} message(s) answered.",
                position.0,
                position.1,
                reply.len()
            ),
        );
        // The step this report moved the character along: the doorway test is the wall the step
        // crosses, not the position a tick may already have advanced
        // (`ASURA/crates/asura-world/src/agent.rs:1038-1041`, `record_portal_step`).
        self.record_portal_step(position);
        if !reply.is_empty() {
            // Everyone else is told what this report made the agent do, before the walker itself is
            // (`ASURA/crates/asura-world/src/agent.rs:1041-1042`: `instance.broadcast`, then `send`).
            if let Ok(peers) = self.peers.lock() {
                peers.broadcast(self.player_id(), self.map_id(), self.district(), &reply);
            }
            self.send_quiet(io, peer, crypt, &reply, "the walk the report asked for")
                .await;
        }
    }

    /// `GAME_CMSG_CANCEL_MOVEMENT` (`0x8028`): the client stopped walking.
    ///
    /// `ASURA/crates/asura-world/src/agent.rs:1062` (`handle_cancel_movement`): the agent's own walk is
    /// cancelled - destination to where it stands, speed to zero - and every client is told it stopped
    /// with `0x0028`. Cancelling is the load-bearing half: without it the server keeps advancing its
    /// copy along the abandoned path and answers the next click from a position the client left behind.
    pub(crate) async fn answer_cancel_movement(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
    ) {
        let Some((stop, was_walking)) = self.with_agent(|agent| {
            let was_walking = 0.0 < agent.motion.speed;
            agent.cancel_movement();
            (
                agent_packets::agent_stop_moving(agent.agent_id),
                was_walking,
            )
        }) else {
            return;
        };
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: the client cancelled walking{}.",
                if was_walking {
                    " and a server-side walk was running: cancelled"
                } else {
                    ""
                }
            ),
        );
        // Every client is told it stopped, before the walker itself is
        // (`ASURA/crates/asura-world/src/agent.rs:1103-1104`).
        if let Ok(peers) = self.peers.lock() {
            peers.broadcast(
                self.player_id(),
                self.map_id(),
                self.district(),
                std::slice::from_ref(&stop.1),
            );
        }
        self.send_quiet(io, peer, crypt, &[stop.1], "the stop")
            .await;
    }

    /// `GAME_CMSG_LAST_POS_BEFORE_MOVE_CANCELED` (`0x8047`): the stopping point a cancel carries.
    ///
    /// `ASURA/crates/asura-world/src/agent.rs:1117` (`handle_last_position`), ported whole: a
    /// correction within the world's `MaxAllowedCorrection` and on the same plane is taken, and the
    /// answer is the movement type 9 followed by a destination at that point; a refused correction
    /// sends **nothing at all**, because the only position it could name is the server's own stale
    /// guess, and the client obeys that message - measured live as one refusal ordering a client 196.7
    /// units backwards (`agent.rs:1108-1116`). The server's own copy stops walking either way.
    pub(crate) async fn answer_last_position(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        crypt: &mut AuthCrypt,
    ) {
        let body = &message[2..];
        if body.len() < 12 {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a stopping point is {} bytes and needs 16; it cannot be read.",
                    message.len()
                ),
            );
            return;
        }
        let position = (
            f32::from_le_bytes([body[0], body[1], body[2], body[3]]),
            f32::from_le_bytes([body[4], body[5], body[6], body[7]]),
        );
        let plane = u32::from_le_bytes([body[8], body[9], body[10], body[11]]) as u16;
        let ceiling = self.settings.max_allowed_correction;
        let Some((taken, reply)) = self.with_agent(|agent| {
            let taken = agent.adopt_correction(position, plane, ceiling);
            let reply = if taken {
                agent.stop_manual_walk()
            } else {
                agent.stand_still();
                Vec::new()
            };
            (taken, reply)
        }) else {
            return;
        };
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: a stopping point ({:.1}, {:.1}) plane {plane} against a ceiling of {ceiling:.1}: {}.",
                position.0,
                position.1,
                if taken {
                    "taken - type 9 and a destination at that point"
                } else {
                    "refused - the agent stops and nothing is sent"
                }
            ),
        );
        // A correction the server accepted is a movement the reports lagged behind; it crosses a
        // doorway the same way a walk does (`ASURA/crates/asura-world/src/agent.rs:1175-1179`). A point
        // that is not finite leaves no segment at all.
        if position.0.is_finite() && position.1.is_finite() {
            self.record_portal_step(position);
        } else if let Ok(mut portal) = self.portal.lock() {
            portal.last_step = None;
        }
        if !reply.is_empty() {
            let reply: Vec<Vec<u8>> = reply.into_iter().map(|(_, frame)| frame).collect();
            if let Ok(peers) = self.peers.lock() {
                peers.broadcast(self.player_id(), self.map_id(), self.district(), &reply);
            }
            self.send_quiet(io, peer, crypt, &reply, "the stopping point")
                .await;
        }
    }
}
