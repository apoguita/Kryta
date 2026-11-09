//! What a connection does: the part of the exchange that is not scrambled.
//!
//! Reference for the shape: `src/server/apps/authserver/Server/AuthSocket.cpp`, whose
//! `HandleAuthChallenge` reads the client's version, reads its seed, and answers with the server's.
//! The **bytes** are this client's, from the evidence in [`super::handshake`].
//!
//! What this does today, in order:
//!
//! 1. Read the four bytes that say what the connection is, and refuse anything but an auth one.
//! 2. Read the rest of the client's 16-byte version message.
//! 3. Read the client's 66-byte seed and check its `0x4200` header.
//! 4. Answer with the 22-byte server seed: `0x1601`, then the master secret XORed with the shared
//!    secret.
//!
//! Then it stops and closes. Everything after this point is scrambled with a key both sides derive
//! from the master, and that derivation is not written - see the note in `handshake.rs`. A client
//! that gets this far has completed the plaintext half of the handshake and would fail on the first
//! encrypted message; saying so plainly is better than pretending the conversation works.

use common::logging::{Log, LogLevel};
use shared::network::socket::{Session, SessionFuture, SessionIo};

use crate::authentication::ConnectionKind;
use crate::server::handshake::{
    self, CLIENT_SEED_HEADER, CLIENT_SEED_LEN, MASTER_SECRET_LEN, SERVER_SEED_LEN,
};

/// One client's conversation.
pub struct AuthSession {
    log: Log,
    master: [u8; MASTER_SECRET_LEN],
}

impl AuthSession {
    /// A session that will answer with this master secret.
    ///
    /// The master is passed in rather than drawn here so a test can supply one it knows and check
    /// what comes back.
    pub fn new(log: Log, master: [u8; MASTER_SECRET_LEN]) -> Self {
        Self { log, master }
    }

    /// A session with a master drawn from the operating system's entropy.
    pub fn with_random_master(log: Log) -> Self {
        let mut master = [0u8; MASTER_SECRET_LEN];
        if getrandom::getrandom(&mut master).is_err() {
            // Without entropy there is no secret to exchange. A zero master is what the protocol
            // cannot distinguish from a real one, so it is logged loudly rather than hidden.
            log.out(
                LogLevel::Error,
                "server.authserver",
                "no entropy available for the master secret; the exchange will use zeros",
            );
        }
        Self::new(log, master)
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        self.log.out(level, "server.authserver", text);
    }

    /// Read the four bytes and answer what kind of connection they name.
    async fn classify(&self, io: &mut SessionIo, peer: &str) -> Option<ConnectionKind> {
        let mut first_four = [0u8; 4];
        if let Err(error) = read_exactly(io, &mut first_four).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} closed before it said what it was: {error}"),
            );
            return None;
        }

        match ConnectionKind::classify(first_four) {
            Some(kind) => Some(kind),
            None => {
                self.say(
                    LogLevel::Warn,
                    format!(
                        "Connection from {peer} opened with {first_four:02X?}, which is not a connection this server recognises."
                    ),
                );
                None
            }
        }
    }

    /// The whole conversation, as far as it goes.
    async fn converse(&self, io: &mut SessionIo) {
        let peer = io
            .peer()
            .map(|address| address.to_string())
            .unwrap_or_else(|| "an unknown address".to_string());

        let Some(kind) = self.classify(io, &peer).await else {
            let _ = io.close().await;
            return;
        };

        if kind != ConnectionKind::Auth {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: {} connection on the auth port. This port answers auth connections only.",
                    kind.as_str()
                ),
            );
            let _ = io.close().await;
            return;
        }

        // The rest of the version message: the client's build and two fields the protocol carries
        // and never compares. They are read because the framer must consume them, and reported
        // because a build number is useful when something else fails. The size comes from the
        // measured table rather than from a literal, so the two cannot drift apart.
        let version_size = kind.first_message_size();
        let mut version_rest = vec![0u8; version_size - 4];
        if let Err(error) = read_exactly(io, &mut version_rest).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} closed inside its version message: {error}"),
            );
            let _ = io.close().await;
            return;
        }
        let version = u32::from_le_bytes([
            version_rest[0],
            version_rest[1],
            version_rest[2],
            version_rest[3],
        ]);
        self.say(
            LogLevel::Info,
            format!("Connection from {peer}: auth version {version}."),
        );

        // The client's seed.
        let mut seed = [0u8; CLIENT_SEED_LEN];
        if let Err(error) = read_exactly(io, &mut seed).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} closed inside its seed: {error}"),
            );
            let _ = io.close().await;
            return;
        }

        let header = u16::from_le_bytes([seed[0], seed[1]]);
        if header != CLIENT_SEED_HEADER {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer} sent a seed with header 0x{header:04X}; the exchange starts with 0x{CLIENT_SEED_HEADER:04X}."
                ),
            );
            let _ = io.close().await;
            return;
        }

        let shared = match handshake::shared_secret(&seed[2..]) {
            Ok(shared) => shared,
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Connection from {peer}: the exchange cannot proceed: {error}"),
                );
                let _ = io.close().await;
                return;
            }
        };

        let answer = handshake::server_seed(&self.master, &shared);
        debug_assert_eq!(answer.len(), SERVER_SEED_LEN);
        if let Err(error) = io.write_all(&answer).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} could not be answered: {error}"),
            );
            let _ = io.close().await;
            return;
        }

        self.say(
            LogLevel::Info,
            format!(
                "Connection from {peer}: the plaintext half of the exchange is answered. Everything after it is scrambled, and the key derivation is not written yet."
            ),
        );

        let _ = io.close().await;
    }
}

impl Session for AuthSession {
    fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
        Box::pin(async move {
            self.converse(&mut io).await;
        })
    }
}

/// Read exactly as many bytes as the slice holds.
pub async fn read_exactly(io: &mut SessionIo, out: &mut [u8]) -> Result<(), String> {
    let mut filled = 0;
    while filled < out.len() {
        match io.read(&mut out[filled..]).await {
            Ok(0) => return Err("the peer closed".to_string()),
            Ok(count) => filled += count,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::handshake::{shared_secret, PUBLIC_LEN};
    use shared::network::socket::SessionFactory;
    use shared::network::socket_mgr::SocketMgr;
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::time::Duration;

    /// Play the client's half against a real listener.
    ///
    /// The client's private value is not known to this server - that is the point of the exchange -
    /// so the test cannot check the shared secret the way a client would. What it can check is the
    /// shape: 16 bytes of version, 66 of seed, and 22 back whose header is the server's and whose
    /// body is the master XORed with the shared secret the server itself computed.
    #[test]
    fn the_plaintext_exchange_is_answered() {
        let log = Log::new();
        let master = [0x5Au8; MASTER_SECRET_LEN];

        let factory: SessionFactory = {
            let log = log.clone();
            Arc::new(move || Box::new(AuthSession::new(log.clone(), master)) as Box<dyn Session>)
        };

        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // 1. The four bytes that say what this is, then the rest of the 16-byte version message.
        let mut version = Vec::new();
        version.extend_from_slice(&0x000C_0400u32.to_le_bytes());
        version.extend_from_slice(&39112u32.to_le_bytes());
        version.extend_from_slice(&1u32.to_le_bytes());
        version.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(version.len(), 16);

        // 2. The seed: header, then 64 bytes of public value. One, so the arithmetic is defined.
        let mut public = [0u8; PUBLIC_LEN];
        public[0] = 2;
        let mut seed = Vec::new();
        seed.extend_from_slice(&CLIENT_SEED_HEADER.to_le_bytes());
        seed.extend_from_slice(&public);
        assert_eq!(seed.len(), CLIENT_SEED_LEN);

        version.extend_from_slice(&seed);
        client.write_all(&version).unwrap();

        // 3. The answer.
        let mut answer = [0u8; SERVER_SEED_LEN];
        client.read_exact(&mut answer).unwrap();

        assert_eq!(&answer[..2], &0x1601u16.to_le_bytes());

        let shared = shared_secret(&public).unwrap();
        for index in 0..MASTER_SECRET_LEN {
            assert_eq!(
                answer[2 + index] ^ shared[index],
                master[index],
                "the client recovers the master by the same XOR"
            );
        }

        manager.stop_network();
    }

    #[test]
    fn a_seed_with_the_wrong_header_is_not_answered() {
        let log = Log::new();
        let factory: SessionFactory = {
            let log = log.clone();
            Arc::new(move || {
                Box::new(AuthSession::new(log.clone(), [0u8; MASTER_SECRET_LEN])) as Box<dyn Session>
            })
        };

        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let mut message = Vec::new();
        message.extend_from_slice(&0x000C_0400u32.to_le_bytes());
        message.extend_from_slice(&[0u8; 12]);
        message.extend_from_slice(&0x1234u16.to_le_bytes()); // the wrong header
        message.extend_from_slice(&[0u8; PUBLIC_LEN]);
        client.write_all(&message).unwrap();

        // The server closes instead of answering, which the client reads as an end of stream.
        let mut answer = [0u8; 1];
        let mut total = 0;
        loop {
            match client.read(&mut answer) {
                Ok(0) => break,
                Ok(_) => total += 1,
                Err(_) => break,
            }
        }
        assert_eq!(total, 0, "nothing should come back for a seed with the wrong header");

        manager.stop_network();
    }
}
