//! The listener: bind, accept, hand each connection on, and close.
//!
//! Reference: `src/server/shared/Network/AsyncAcceptor.h`. The reference re-arms one asynchronous
//! accept in the program's io context and stops re-arming when it is closed. Here the listener is
//! non-blocking and one thread accepts in a loop, which keeps the two behaviours that matter:
//! every accepted connection is handed to the dispatch the manager set, and closing the acceptor
//! ends the loop instead of leaving it waiting.

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// How long the accept loop waits before looking again when nothing is pending.
///
/// The reference's own network threads wake on a 1 ms timer, so the same order of magnitude is in
/// keeping with it.
const ACCEPT_POLL: Duration = Duration::from_millis(1);

/// Where a newly accepted connection goes.
pub type Dispatch = Arc<dyn Fn(TcpStream) + Send + Sync>;

/// A bound listener and the thread accepting on it.
pub struct AsyncAcceptor {
    port: u16,
    closed: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl AsyncAcceptor {
    /// Bind an address and a port, and start accepting.
    ///
    /// Reference: `Bind` followed by `AsyncAcceptWithCallback`. Port 0 asks the system for one,
    /// which is what a test wants. A bind that fails answers the reference's own sentence, with
    /// the error's reason after it.
    pub fn bind_and_run(bind_ip: &str, port: u16, dispatch: Dispatch) -> Result<Self, String> {
        let address = format!("{bind_ip}:{port}");
        let resolved = address
            .to_socket_addrs()
            .map_err(|error| format!("could not resolve {address}: {error}"))?
            .next()
            .ok_or_else(|| format!("could not resolve {address}"))?;

        let listener = TcpListener::bind(resolved)
            .map_err(|error| format!("Could not bind to {bind_ip}:{port} {error}"))?;

        let port = listener
            .local_addr()
            .map_err(|error| format!("cannot read the bound address: {error}"))?
            .port();

        listener
            .set_nonblocking(true)
            .map_err(|error| format!("cannot make the listener non-blocking: {error}"))?;

        let closed = Arc::new(AtomicBool::new(false));
        let worker = spawn_accept_loop(listener, Arc::clone(&closed), dispatch);

        Ok(Self {
            port,
            closed,
            worker: Some(worker),
        })
    }

    /// The port actually bound, which differs from 0 when the system chose it.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop accepting and wait for the loop to end.
    ///
    /// Reference: `Close`, which sets a flag once and closes the acceptor. The join is what proves
    /// the loop ended rather than being left behind.
    pub fn close(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AsyncAcceptor {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.close();
        }
    }
}

fn spawn_accept_loop(
    listener: TcpListener,
    closed: Arc<AtomicBool>,
    dispatch: Dispatch,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("network-accept".to_string())
        .spawn(move || {
            while !closed.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => dispatch(stream),
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(ACCEPT_POLL);
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("the accept thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    #[test]
    fn a_connection_is_handed_to_the_dispatch() {
        let (sender, receiver) = channel();
        let dispatch: Dispatch = Arc::new(move |_stream| {
            let _ = sender.send(());
        });

        let mut acceptor = AsyncAcceptor::bind_and_run("127.0.0.1", 0, dispatch).unwrap();
        let port = acceptor.port();
        assert_ne!(port, 0);

        let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let arrived = receiver.recv_timeout(Duration::from_secs(5));
        acceptor.close();

        assert!(arrived.is_ok(), "the acceptor never handed the connection on");
    }

    #[test]
    fn closing_stops_the_loop_and_the_port_can_be_bound_again() {
        let dispatch: Dispatch = Arc::new(|_stream| {});
        let mut acceptor = AsyncAcceptor::bind_and_run("127.0.0.1", 0, dispatch).unwrap();
        let port = acceptor.port();
        acceptor.close();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut rebound = None;
        while Instant::now() < deadline {
            let attempt: Dispatch = Arc::new(|_stream| {});
            match AsyncAcceptor::bind_and_run("127.0.0.1", port, attempt) {
                Ok(acceptor) => {
                    rebound = Some(acceptor);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(25)),
            }
        }

        assert!(rebound.is_some(), "the closed port stayed bound");
        if let Some(mut acceptor) = rebound {
            acceptor.close();
        }
    }
}
