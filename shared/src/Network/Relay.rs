//! Copying one client's connection to another machine: what a front door does.
//!
//! Reference: the reference's auth service hands a gameplay connection to a game server over its
//! control channel (`CtrlMsgId_TransferConnection`), and the game server then speaks to the client
//! directly. A socket handle cannot cross a process boundary here, so the crossing is a copy: the
//! front door keeps both connections and carries bytes between them
//! (`docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`).
//!
//! What it is not: a decoder. The bytes are forwarded exactly as they arrived - including the ones
//! already read - and the world that receives them decodes them itself, which is the authoritative
//! decode. A front door that started interpreting the game protocol would be the game library in
//! the wrong program.
//!
//! The copy is `tokio`'s [`copy_bidirectional`], so the two directions run in one future with no
//! lock and no extra thread: a client's gameplay connection costs one task, as it did before.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

use super::socket::SessionIo;

/// How many bytes the copy carried, in each direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Relayed {
    /// Bytes the client sent, which the world received.
    pub to_world: u64,
    /// Bytes the world sent, which the client received.
    pub from_world: u64,
}

/// Why a connection could not be relayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
    /// The world did not answer inside the timeout.
    Unreachable(String),
    /// Both connections were made and the copy failed part-way.
    Broken(String),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(reason) => write!(f, "the world did not answer: {reason}"),
            Self::Broken(reason) => write!(f, "the copy ended in an error: {reason}"),
        }
    }
}

impl SessionIo {
    /// Connect to `target`, hand it `head`, and carry bytes between the two until both are done.
    ///
    /// `head` is what was already read from the client before the decision to relay was taken: it is
    /// part of the client's stream, not separate from it, so dropping it would leave the world
    /// waiting for a message's first half. It is forwarded whole, and nothing in it is interpreted
    /// here.
    pub async fn relay_to(
        &mut self,
        target: SocketAddr,
        head: &[u8],
        timeout: Duration,
    ) -> Result<Relayed, RelayError> {
        let connect = tokio::time::timeout(timeout, TcpStream::connect(target));
        let mut world = match connect.await {
            Ok(Ok(world)) => world,
            Ok(Err(error)) => return Err(RelayError::Unreachable(format!("{target}: {error}"))),
            Err(_) => {
                return Err(RelayError::Unreachable(format!(
                    "{target} within {} ms",
                    timeout.as_millis()
                )));
            }
        };

        if !head.is_empty() {
            use tokio::io::AsyncWriteExt;
            world
                .write_all(head)
                .await
                .map_err(|error| RelayError::Broken(format!("writing the head: {error}")))?;
        }

        let (to_world, from_world) = copy_bidirectional(self.stream_mut(), &mut world)
            .await
            .map_err(|error| RelayError::Broken(error.to_string()))?;

        Ok(Relayed {
            to_world: to_world + head.len() as u64,
            from_world,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::socket::{Session, SessionFactory, SessionFuture};
    use crate::network::socket_mgr::SocketMgr;
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// What the front door of the test saw, written when its copy finishes.
    type Report = Arc<Mutex<Option<Result<Relayed, RelayError>>>>;

    /// A world that reads one message, answers `ok:` and the message, then reads until the client is
    /// done - a peer that stays open, so the copy ends on the client's close rather than on a reset.
    fn fake_world() -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // The head and the rest arrive in however many pieces the relay writes them, so the
            // world reads until it has the whole message before answering.
            let mut message = Vec::new();
            let mut buffer = [0u8; 64];
            while message.len() < 5 {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => message.extend_from_slice(&buffer[..read]),
                    Err(_) => break,
                }
            }
            let mut answer = b"ok:".to_vec();
            answer.extend_from_slice(&message);
            stream.write_all(&answer).unwrap();
            // Stay open until the other end closes, so nothing is reset mid-copy.
            while let Ok(read) = stream.read(&mut buffer) {
                if read == 0 {
                    break;
                }
            }
        });
        (address, handle)
    }

    /// A front door that relays to one address, forwarding the four bytes it read first.
    struct Door {
        target: std::net::SocketAddr,
        report: Report,
    }

    impl Session for Door {
        fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
            Box::pin(async move {
                let mut head = [0u8; 4];
                let _ = io.read(&mut head).await;
                let outcome = io
                    .relay_to(self.target, &head, Duration::from_secs(3))
                    .await;
                if let Ok(mut slot) = self.report.lock() {
                    *slot = Some(outcome);
                }
            })
        }
    }

    fn wait_for(report: &Report) -> Result<Relayed, RelayError> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(slot) = report.lock()
                && let Some(outcome) = slot.clone()
            {
                return outcome;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the front door never finished");
    }

    #[test]
    fn a_connection_is_copied_to_the_world_and_the_answer_comes_back() {
        let (address, world) = fake_world();
        let report: Report = Arc::new(Mutex::new(None));

        let factory: SessionFactory = {
            let report = Arc::clone(&report);
            Arc::new(move || {
                Box::new(Door {
                    target: address,
                    report: Arc::clone(&report),
                }) as Box<dyn Session>
            })
        };
        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.write_all(b"hello").unwrap();

        let mut answer = [0u8; 8];
        client.read_exact(&mut answer).unwrap();
        assert_eq!(
            &answer, b"ok:hello",
            "the head and the rest arrive, and the answer returns"
        );

        drop(client);
        let relayed = wait_for(&report).expect("the copy ends cleanly");
        assert_eq!(
            relayed.to_world, 5,
            "every byte the client sent reached the world"
        );
        assert_eq!(
            relayed.from_world, 8,
            "every byte the world sent reached the client"
        );

        manager.stop_network();
        let _ = world.join();
    }

    #[test]
    fn a_world_that_is_not_there_is_reported_rather_than_hanging() {
        let report: Report = Arc::new(Mutex::new(None));

        struct Unreachable(Report);
        impl Session for Unreachable {
            fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
                Box::pin(async move {
                    // A port nobody listens on: the system refuses at once.
                    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                    let address = closed.local_addr().unwrap();
                    drop(closed);

                    let outcome = io
                        .relay_to(address, b"head", Duration::from_millis(500))
                        .await;
                    if let Ok(mut slot) = self.0.lock() {
                        *slot = Some(outcome);
                    }
                })
            }
        }

        let factory: SessionFactory = {
            let report = Arc::clone(&report);
            Arc::new(move || Box::new(Unreachable(Arc::clone(&report))) as Box<dyn Session>)
        };
        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();

        let error = wait_for(&report).unwrap_err();
        assert!(matches!(error, RelayError::Unreachable(_)), "{error}");

        manager.stop_network();
    }
}
