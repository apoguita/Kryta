//! The manager: one acceptor, N network threads, and the choice of which thread a connection goes
//! to.
//!
//! Reference: `src/server/shared/Network/SocketMgr.h`. The reference's `StartNetwork` builds the
//! acceptor, binds it, creates the threads, starts them, and points the acceptor at
//! `GetSocketForAccept` - which picks the thread with the fewest connections. `StopNetwork` closes
//! the acceptor, stops every thread and waits, in that order.

use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::logging::{Log, LogLevel};

use super::async_acceptor::{AsyncAcceptor, Dispatch};
use super::network_thread::{NetworkThread, NetworkThreadHandle};
use super::socket::{Session, SessionFactory};

/// The socket layer for one program: its listener and the threads behind it.
pub struct SocketMgr {
    acceptor: Option<AsyncAcceptor>,
    threads: Vec<NetworkThread>,
    factory: SessionFactory,
    /// Every connection this listener has accepted since it started, whether or not it spoke.
    ///
    /// This is the number that answers "did anything reach us at all", which a session cannot: a
    /// connection that opens and closes without sending a byte never becomes one.
    accepted: Arc<AtomicUsize>,
    /// Where a connection is reported as it arrives, at the layer that accepts it.
    log: Option<Log>,
    /// Where every byte is reported, when the caller asked for a trace.
    trace: Option<Log>,
}

impl SocketMgr {
    /// A manager with no listener and no thread yet.
    pub fn new(factory: SessionFactory) -> Self {
        Self {
            acceptor: None,
            threads: Vec::new(),
            factory,
            accepted: Arc::new(AtomicUsize::new(0)),
            log: None,
            trace: None,
        }
    }

    /// Report each accepted connection, and what the listener is doing, to this log.
    ///
    /// Set before `start_network`; a manager without one stays silent about its own traffic, which
    /// is what the library's own tests want.
    pub fn set_log(&mut self, log: Log) {
        self.log = Some(log);
    }

    /// Report every byte of every connection to this log.
    ///
    /// Off unless asked for: it is an instrument for an investigation, not a default. Set before
    /// `start_network`.
    pub fn set_trace(&mut self, log: Log) {
        self.trace = Some(log);
    }

    /// How many connections have been accepted since the listener started.
    pub fn accepted_count(&self) -> usize {
        self.accepted.load(Ordering::Relaxed)
    }

    /// A handle on that count, for a thread that reports while the manager is busy serving.
    ///
    /// The manager itself is not shared - it owns the threads - so a status writer takes this
    /// instead of the manager.
    pub fn accepted_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.accepted)
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        if let Some(log) = self.log.as_ref() {
            log.out(level, "network", text);
        }
    }

    /// Bind, start the threads, and accept.
    ///
    /// Reference: `SocketMgr::StartNetwork`, whose thread count must be greater than zero.
    pub fn start_network(
        &mut self,
        bind_ip: &str,
        port: u16,
        thread_count: usize,
    ) -> Result<u16, String> {
        if thread_count == 0 {
            return Err("the network thread count must be greater than 0".to_string());
        }

        let mut threads = Vec::with_capacity(thread_count);
        for index in 0..thread_count {
            let thread = NetworkThread::start(index, self.trace.clone())
                .map_err(|error| format!("cannot start network thread {index}: {error}"))?;
            threads.push(thread);
        }

        let handles: Vec<NetworkThreadHandle> =
            threads.iter().filter_map(NetworkThread::handle).collect();
        let handles = Arc::new(handles);

        // Reference: `GetSocketForAccept` -> `SelectThreadWithMinConnections`. The factory builds a
        // session per connection; the thread a connection lands on is the one serving fewest.
        let factory = Arc::clone(&self.factory);
        let accepted = Arc::clone(&self.accepted);
        let log = self.log.clone();
        let dispatch: Dispatch = {
            let handles = Arc::clone(&handles);
            Arc::new(move |stream: TcpStream| {
                // Counted and reported before the protocol sees anything: a client that connects
                // and says nothing is still a client that reached this port.
                let count = accepted.fetch_add(1, Ordering::Relaxed) + 1;
                let peer = stream
                    .peer_addr()
                    .map(|address| address.to_string())
                    .unwrap_or_else(|_| "an unreadable address".to_string());
                if let Some(log) = log.as_ref() {
                    log.out(
                        LogLevel::Info,
                        "network",
                        format!("accepted connection #{count} from {peer}"),
                    );
                }

                let index = select_thread_with_min_connections(&handles);
                let session: Box<dyn Session> = (factory)();
                if !handles[index].submit(stream, session) {
                    // The thread stopped between the choice and the hand-over; the connection is
                    // dropped, which closes it.
                }
            })
        };

        let acceptor = AsyncAcceptor::bind_and_run(bind_ip, port, dispatch)?;
        let bound = acceptor.port();

        self.acceptor = Some(acceptor);
        self.threads = threads;

        self.say(
            LogLevel::Info,
            format!("listening on {bind_ip}:{bound} with {thread_count} network thread(s)"),
        );

        Ok(bound)
    }

    /// The port the listener bound, when there is one.
    pub fn port(&self) -> Option<u16> {
        self.acceptor.as_ref().map(AsyncAcceptor::port)
    }

    /// How many connections every thread is serving together.
    pub fn connection_count(&self) -> usize {
        self.threads
            .iter()
            .map(NetworkThread::connection_count)
            .sum()
    }

    /// How many threads are running.
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Close the acceptor, then stop every thread and wait for it.
    ///
    /// Reference: `SocketMgr::StopNetwork`, which does exactly this order: the acceptor first, so
    /// no new connection arrives while the threads are going away.
    pub fn stop_network(&mut self) {
        if let Some(mut acceptor) = self.acceptor.take() {
            acceptor.close();
        }
        for thread in &mut self.threads {
            thread.stop();
        }
        self.threads.clear();
    }
}

impl Drop for SocketMgr {
    fn drop(&mut self) {
        if self.acceptor.is_some() || !self.threads.is_empty() {
            self.stop_network();
        }
    }
}

/// The index of the thread serving the fewest connections.
///
/// Reference: `SocketMgr::SelectThreadWithMinConnections`, which answers 0 for an empty manager and
/// the first thread on a tie.
pub fn select_thread_with_min_connections(handles: &[NetworkThreadHandle]) -> usize {
    let mut best = 0;
    for index in 1..handles.len() {
        if handles[index].connection_count() < handles[best].connection_count() {
            best = index;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::socket::{Session, SessionFuture, SessionIo};
    use std::time::{Duration, Instant};

    /// A session that echoes one line and closes.
    struct Echo;

    impl Session for Echo {
        fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
            Box::pin(async move {
                let mut buffer = [0u8; 64];
                if let Ok(read) = io.read(&mut buffer).await
                    && read > 0
                {
                    let _ = io.write_all(&buffer[..read]).await;
                }
                let _ = io.close().await;
            })
        }
    }

    fn echo_factory() -> SessionFactory {
        Arc::new(|| Box::new(Echo) as Box<dyn Session>)
    }

    #[test]
    fn a_client_is_answered_and_the_connection_is_counted() {
        let mut manager = SocketMgr::new(echo_factory());
        let port = manager.start_network("127.0.0.1", 0, 2).unwrap();
        assert_eq!(manager.thread_count(), 2);

        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        use std::io::{Read, Write};
        client.write_all(b"hello").unwrap();

        let mut answer = [0u8; 5];
        client.read_exact(&mut answer).unwrap();
        assert_eq!(&answer, b"hello");

        manager.stop_network();
        assert_eq!(manager.connection_count(), 0);
    }

    #[test]
    fn stopping_frees_the_port() {
        let mut manager = SocketMgr::new(echo_factory());
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        manager.stop_network();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bound_here = false;
        while Instant::now() < deadline {
            if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                bound_here = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(bound_here, "the port stayed bound after the manager stopped");
    }

    #[test]
    fn a_zero_thread_count_is_refused() {
        let mut manager = SocketMgr::new(echo_factory());
        assert!(manager.start_network("127.0.0.1", 0, 0).is_err());
    }

    #[test]
    fn a_connection_goes_to_the_thread_with_fewest() {
        let mut manager = SocketMgr::new(echo_factory());
        let port = manager.start_network("127.0.0.1", 0, 3).unwrap();

        // Six connections, three threads: the least-loaded choice spreads them evenly.
        let mut clients = Vec::new();
        for _ in 0..6 {
            clients.push(std::net::TcpStream::connect(("127.0.0.1", port)).unwrap());
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        while manager.connection_count() < 6 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }

        let counts: Vec<usize> = manager
            .threads
            .iter()
            .map(NetworkThread::connection_count)
            .collect();
        manager.stop_network();
        drop(clients);

        assert_eq!(counts.iter().sum::<usize>(), 6, "{counts:?}");
        assert!(
            counts.iter().all(|count| *count <= 2),
            "the least-loaded choice should not stack connections on one thread: {counts:?}"
        );
    }
}
