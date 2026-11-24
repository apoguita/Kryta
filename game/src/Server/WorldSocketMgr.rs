//! The world's listener: the socket manager with a gameplay session behind it.
//!
//! Reference: `src/server/game/Server/WorldSocketMgr.{h,cpp}`, which is a `SocketMgr<WorldSocket>`
//! with the world's own socket options. This is the same object at the size this tree has reached:
//! the base manager, a factory that builds one [`WorldSession`] per connection, and the world's
//! configuration passed in rather than read here.
//!
//! Reference: `WorldSocketMgr::StartWorldNetwork(ioContext, bindIp, port, networkThreads)`.

use common::logging::Log;
use database::database::DatabaseWorkerPool;
use shared::network::socket::{Session, SessionFactory};
use shared::network::socket_mgr::SocketMgr;
use std::sync::{Arc, Mutex};

use crate::server::world_session::WorldSession;
use crate::world::World;

/// How long the shutdown waits for kicked sessions to finish their own tail.
///
/// The reference does not wait at all: `KickAll` marks the sessions and the very next
/// `UpdateSessions(1)` pass unloads them, because its sessions are updated by the same loop that
/// stops. Here a session is a task that only turns when its socket or its clock wakes, and the
/// clock's longest wake is one simulation tick, so the bound is a few of those and no more.
const KICK_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// The listener a world program opens, and the sessions it builds.
pub struct WorldSocketMgr {
    sockets: SocketMgr,
    /// Set when the world is shutting down: every session sees it and ends itself cleanly, which is
    /// what a session's tail does on its own - give the district place back, write the position.
    ///
    /// Reference: `sWorldSessionMgr->KickAll()` (`WorldSessionMgr.cpp:220`, "save and kick all
    /// players"), the first thing the reference's shutdown does (`Main.cpp:372`).
    closing: Arc<std::sync::atomic::AtomicBool>,
}

impl WorldSocketMgr {
    /// Bind and start serving gameplay connections.
    ///
    /// The world is shared with every session: each connection needs the world's id and its maps,
    /// and neither changes while the listener runs. The character pool is shared too, because a
    /// creation screen's confirm writes a character row and nothing else may open a second one. The
    /// login pool is shared so a connection can claim the travel record its login wrote.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        world: Arc<World>,
        characters: Option<Arc<Mutex<DatabaseWorkerPool>>>,
        login: Option<Arc<Mutex<DatabaseWorkerPool>>>,
        log: Log,
        bind_ip: &str,
        port: u16,
        network_threads: usize,
        trace: bool,
    ) -> Result<Self, String> {
        let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let factory: SessionFactory = {
            let log = log.clone();
            let world = Arc::clone(&world);
            let characters = characters.clone();
            let login = login.clone();
            let closing = Arc::clone(&closing);
            Arc::new(move || {
                // A session is given the login pool only together with the character pool.
                let mut session = WorldSession::new(log.clone(), &world);
                if let Some(characters) = characters.as_ref() {
                    session = session.with_characters(Arc::clone(characters));
                    if let Some(login) = login.as_ref() {
                        session = session.with_login(Arc::clone(login));
                    }
                }
                session.set_closing(Arc::clone(&closing));
                Box::new(session) as Box<dyn Session>
            })
        };

        let mut sockets = SocketMgr::new(factory);
        sockets.set_log(log.clone());
        if trace {
            sockets.set_trace(log);
        }
        sockets
            .start_network(bind_ip, port, network_threads)
            .map_err(|error| format!("Failed to initialize network: {error}"))?;

        Ok(Self { sockets, closing })
    }

    /// The port the listener bound.
    pub fn port(&self) -> Option<u16> {
        self.sockets.port()
    }

    /// How many connections have been accepted since it started.
    pub fn accepted_count(&self) -> usize {
        self.sockets.accepted_count()
    }

    /// A handle on that count, for a status writer.
    pub fn accepted_handle(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        self.sockets.accepted_handle()
    }

    /// A handle on the live count - the connections being served right now - for a status writer.
    pub fn live_handle(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        self.sockets.live_handle()
    }

    /// Kick every live session and close the listener, in the reference's own order.
    ///
    /// Reference: the shutdown guard in `Main.cpp:370-374` - `sWorldSessionMgr->KickAll()` (save and
    /// kick all players), `UpdateSessions(1)` (the pass that unloads them), then
    /// `sWorldSocketMgr.StopNetwork()`. Here the kick is the flag every session reads on its next
    /// turn, so its own tail runs - the district place is given back and the position is written -
    /// and this waits (bounded) for them to finish before the runtime stops under them. A session
    /// that does not turn in time is dropped with the runtime, as before.
    pub fn stop(mut self) {
        self.closing
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let live = self.sockets.live_handle();
        let deadline = std::time::Instant::now() + KICK_WAIT;
        while live.load(std::sync::atomic::Ordering::Relaxed) > 0
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        self.sockets.stop_network();
    }
}
