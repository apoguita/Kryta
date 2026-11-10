//! What a connection does: the file check, then the part of the exchange that is not scrambled.
//!
//! Reference for the shape: `src/server/apps/authserver/Server/AuthSocket.cpp`, whose
//! `HandleAuthChallenge` reads the client's version, reads its seed, and answers with the server's.
//! The **bytes** are this client's, from the evidence in [`super::handshake`] and
//! [`super::file_check`].
//!
//! What this does today, in order:
//!
//! 1. Read the first four bytes and decide what they are. Two things can arrive first: the file
//!    check, whose question the client compares byte for byte, or the version message's header.
//! 2. If it is the file check: read the rest of the question, answer it, and go back to waiting -
//!    the client may ask again, and the version message follows.
//! 3. Read the rest of the client's 16-byte version message.
//! 4. Read the client's 66-byte seed, check its `0x4200` header, and answer with the 22-byte server
//!    seed.
//!
//! Then it stops and closes. Everything after this point is scrambled with a key both sides derive
//! from the master, and that derivation is not written - see the note in `handshake.rs`. A client
//! that gets this far has completed the plaintext half of the handshake and would fail on the first
//! encrypted message; saying so plainly is better than pretending the conversation works.

use common::logging::{Log, LogLevel};
use shared::network::socket::{Session, SessionFuture, SessionIo};

use crate::authentication::ConnectionKind;
use crate::server::file_check::FileCheck;
use crate::server::handshake::{
    self, CLIENT_SEED_HEADER, CLIENT_SEED_LEN, MASTER_SECRET_LEN, SERVER_SEED_LEN,
};

/// One client's conversation.
pub struct AuthSession {
    log: Log,
    master: [u8; MASTER_SECRET_LEN],
    file_check: FileCheck,
}

impl AuthSession {
    /// A session that will answer the file check with these bytes and the exchange with this master.
    ///
    /// Both are passed in rather than read or drawn here so a test can supply what it knows.
    pub fn new(log: Log, master: [u8; MASTER_SECRET_LEN], file_check: FileCheck) -> Self {
        Self {
            log,
            master,
            file_check,
        }
    }

    /// A session with a master drawn from the operating system's entropy.
    pub fn with_random_master(log: Log, file_check: FileCheck) -> Self {
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
        Self::new(log, master, file_check)
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        self.log.out(level, "server.authserver", text);
    }

    /// The whole conversation, as far as it goes.
    async fn converse(&self, io: &mut SessionIo) {
        let peer = io
            .peer()
            .map(|address| address.to_string())
            .unwrap_or_else(|| "an unknown address".to_string());

        loop {
            let mut first_four = [0u8; 4];
            if let Err(error) = read_exactly(io, &mut first_four).await {
                self.say(
                    LogLevel::Warn,
                    format!("Connection from {peer} ended while waiting for a message: {error}"),
                );
                let _ = io.close().await;
                return;
            }

            // The file check arrives before everything else on this socket, and the client compares
            // the whole answer, so it is answered before anything is classified.
            if self.file_check.could_be_request(&first_four) {
                if !self.answer_file_check(io, &peer, first_four).await {
                    let _ = io.close().await;
                    return;
                }
                // The client may repeat the check, and its version message follows it.
                continue;
            }

            match ConnectionKind::classify(first_four) {
                Some(ConnectionKind::Auth) => {
                    self.say(
                        LogLevel::Info,
                        format!(
                            "Connection from {peer}: {} connection, first message {} bytes.",
                            ConnectionKind::Auth.as_str(),
                            ConnectionKind::Auth.first_message_size()
                        ),
                    );
                    self.finish_exchange(io, &peer).await;
                    let _ = io.close().await;
                    return;
                }
                Some(other) => {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Connection from {peer}: {} connection on the auth port. This port answers auth connections only.",
                            other.as_str()
                        ),
                    );
                    let _ = io.close().await;
                    return;
                }
                None => {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Connection from {peer} opened with {first_four:02X?}, which is neither the file check nor a connection this server recognises."
                        ),
                    );
                    let _ = io.close().await;
                    return;
                }
            }
        }
    }

    /// Read the rest of the file check's question and answer it.
    ///
    /// Answers whether the conversation can continue: a question that does not match closes the
    /// socket, because the client is comparing bytes and a wrong answer is worse than none.
    async fn answer_file_check(
        &self,
        io: &mut SessionIo,
        peer: &str,
        first_four: [u8; 4],
    ) -> bool {
        let mut question = first_four.to_vec();
        let remaining = self.file_check.request_len().saturating_sub(question.len());
        question.resize(self.file_check.request_len(), 0);

        if remaining > 0
            && let Err(error) = read_exactly(io, &mut question[4..4 + remaining]).await
        {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} ended inside the file check: {error}"),
            );
            return false;
        }

        if !self.file_check.is_request(&question) {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer} started like the file check but is not it: {}.",
                    shared::network::socket::hex(&question)
                ),
            );
            return false;
        }

        if let Err(error) = io.write_all(self.file_check.answer()).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} could not be answered: {error}"),
            );
            return false;
        }

        self.say(
            LogLevel::Info,
            format!(
                "Connection from {peer}: file check answered with {} bytes.",
                self.file_check.answer().len()
            ),
        );
        true
    }

    /// The version message and the seed exchange.
    async fn finish_exchange(&self, io: &mut SessionIo, peer: &str) {
        // The rest of the version message: the client's build and two fields the protocol carries
        // and never compares. The size comes from the measured table rather than a literal, so the
        // two cannot drift apart.
        let version_size = ConnectionKind::Auth.first_message_size();
        let mut version_rest = vec![0u8; version_size - 4];
        if let Err(error) = read_exactly(io, &mut version_rest).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} closed inside its version message: {error}"),
            );
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

        let mut seed = [0u8; CLIENT_SEED_LEN];
        if let Err(error) = read_exactly(io, &mut seed).await {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer} closed inside its seed: {error}"),
            );
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
            return;
        }

        let shared = match handshake::shared_secret(&seed[2..]) {
            Ok(shared) => shared,
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Connection from {peer}: the exchange cannot proceed: {error}"),
                );
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
            return;
        }

        self.say(
            LogLevel::Info,
            format!(
                "Connection from {peer}: the plaintext half of the exchange is answered. Everything after it is scrambled, and the key derivation is not written yet."
            ),
        );
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

    fn shipped_file_check() -> FileCheck {
        FileCheck::load("../data/file_check.conf").unwrap()
    }

    fn factory(log: Log, file_check: FileCheck) -> SessionFactory {
        Arc::new(move || {
            Box::new(AuthSession::new(
                log.clone(),
                [0x5Au8; MASTER_SECRET_LEN],
                file_check.clone(),
            )) as Box<dyn Session>
        })
    }

    /// The client's version message and seed, as bytes.
    fn preamble(public_first_byte: u8) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&0x000C_0400u32.to_le_bytes());
        message.extend_from_slice(&39112u32.to_le_bytes());
        message.extend_from_slice(&1u32.to_le_bytes());
        message.extend_from_slice(&0u32.to_le_bytes());

        let mut public = [0u8; PUBLIC_LEN];
        public[0] = public_first_byte;
        message.extend_from_slice(&CLIENT_SEED_HEADER.to_le_bytes());
        message.extend_from_slice(&public);
        message
    }

    #[test]
    fn the_file_check_is_answered_and_the_exchange_follows_on_the_same_socket() {
        let log = Log::new();
        let file_check = shipped_file_check();
        let mut manager = SocketMgr::new(factory(log.clone(), file_check.clone()));
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // 1. The question, exactly as the value file states it.
        client.write_all(file_check.request()).unwrap();

        // 2. The answer, exactly as the value file states it.
        let mut answer = vec![0u8; file_check.answer().len()];
        client.read_exact(&mut answer).unwrap();
        assert_eq!(answer, file_check.answer());

        // 3. The same socket then carries the version message and the seed.
        client.write_all(&preamble(2)).unwrap();
        let mut server_seed = [0u8; SERVER_SEED_LEN];
        client.read_exact(&mut server_seed).unwrap();
        assert_eq!(&server_seed[..2], &0x1601u16.to_le_bytes());

        let shared = shared_secret(&{
            let mut public = [0u8; PUBLIC_LEN];
            public[0] = 2;
            public
        })
        .unwrap();
        for index in 0..MASTER_SECRET_LEN {
            assert_eq!(server_seed[2 + index] ^ shared[index], 0x5A);
        }

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    fn a_question_that_only_looks_like_the_file_check_closes_the_socket() {
        let log = Log::new();
        let file_check = shipped_file_check();
        let mut manager = SocketMgr::new(factory(log.clone(), file_check));
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // The right first four bytes, then twenty-one more that are not the question.
        client.write_all(&[0x01, 0x00, 0x00, 0x00]).unwrap();
        client.write_all(&[0xAA; 21]).unwrap();

        let mut answer = [0u8; 1];
        let mut total = 0;
        loop {
            match client.read(&mut answer) {
                Ok(0) => break,
                Ok(_) => total += 1,
                Err(_) => break,
            }
        }
        assert_eq!(total, 0, "a wrong question must not be answered");

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    fn a_seed_with_the_wrong_header_is_not_answered() {
        let log = Log::new();
        let file_check = shipped_file_check();
        let mut manager = SocketMgr::new(factory(log.clone(), file_check));
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

        let mut answer = [0u8; 1];
        let mut total = 0;
        loop {
            match client.read(&mut answer) {
                Ok(0) => break,
                Ok(_) => total += 1,
                Err(_) => break,
            }
        }
        assert_eq!(total, 0);

        manager.stop_network();
        log.shutdown();
    }
}
