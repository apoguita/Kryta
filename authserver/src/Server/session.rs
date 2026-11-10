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

use common::cryptography::Arc4;

use crate::authentication::{ConnectionKind, PORTAL_ACCOUNT_LOGIN};
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
                "Connection from {peer}: the plaintext half of the exchange is answered. Everything after it is scrambled."
            ),
        );

        // Everything from here is scrambled with a key both sides derive from the master, and the
        // cipher's position carries across reads: a running stream, not one per message.
        let key = handshake::derive_key(&self.master);
        let mut decrypt = Arc4::new(&key);
        let mut buffer = [0u8; 1024];
        let mut messages = 0usize;

        loop {
            let count = match io.read(&mut buffer).await {
                Ok(0) => {
                    self.say(
                        LogLevel::Info,
                        format!(
                            "Connection from {peer} closed after {messages} encrypted message(s)."
                        ),
                    );
                    return;
                }
                Ok(count) => count,
                Err(error) => {
                    self.say(
                        LogLevel::Warn,
                        format!("Connection from {peer} could not be read after the exchange: {error}"),
                    );
                    return;
                }
            };

            messages += 1;
            let mut plain = buffer[..count].to_vec();
            decrypt.apply(&mut plain);

            let id = if plain.len() >= 2 {
                u16::from_le_bytes([plain[0], plain[1]])
            } else {
                0
            };

            // The ids worth naming, because each one is a different answer to "how far did the
            // client get": its 64-byte preamble arrives first, then the login message.
            let meaning = match id {
                0x8001 => " - the client's preamble, which this server does not answer",
                PORTAL_ACCOUNT_LOGIN => {
                    " - the login message: the key derivation matches what this client expects"
                }
                _ => "",
            };
            if id == PORTAL_ACCOUNT_LOGIN {
                match decode_portal_login(&plain[2..]) {
                    Some(login) => self.say(
                        LogLevel::Info,
                        format!(
                            "Connection from {peer}: login request {}, user {} session {} character '{}' type '{}'.",
                            login.request,
                            uuid_le_text(&login.user_id),
                            uuid_le_text(&login.session_id),
                            login.character,
                            login.account_type
                        ),
                    ),
                    None => self.say(
                        LogLevel::Warn,
                        format!(
                            "Connection from {peer}: the login message is {} bytes, which is too few for its fields.",
                            plain.len()
                        ),
                    ),
                }
            }

            self.say(
                LogLevel::Info,
                format!(
                    "Connection from {peer}: encrypted message {messages} is 0x{id:04X} with {count} bytes{meaning}."
                ),
            );
        }
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

/// What a `0x8038` login message says, once the connection is unscrambled.
///
/// Layout, from the recorded shape of `AuthSrv_PortalAccountLogin`: the header, a `u32` request id the
/// reply must echo, the portal's `user_id` and `session_id` as 16-byte little-endian UUIDs, a counted
/// UTF-16 character name, and a counted UTF-16 account type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalLogin {
    /// Echoed in every reply to this request.
    pub request: u32,
    /// The portal user id, in the wire's little-endian UUID form.
    pub user_id: [u8; 16],
    /// The session id the portal answered with.
    pub session_id: [u8; 16],
    /// The character the client last played. Empty on a first login.
    pub character: String,
    /// A client-side word about the account; carried so a trace matches.
    pub account_type: String,
}

/// Read a `0x8038` message's body.
///
/// Answers `None` when the bytes are too few for the fields that are required, which is the shape of a
/// message that has not fully arrived. The counted strings are read as the client packs them: a `u16`
/// count and that many UTF-16 units, no padding.
pub fn decode_portal_login(body: &[u8]) -> Option<PortalLogin> {
    if body.len() < 4 + 16 + 16 {
        return None;
    }

    let request = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
    let mut user_id = [0u8; 16];
    user_id.copy_from_slice(&body[4..20]);
    let mut session_id = [0u8; 16];
    session_id.copy_from_slice(&body[20..36]);

    let mut at = 36;
    let counted = |at: &mut usize| -> String {
        if *at + 2 > body.len() {
            return String::new();
        }
        let count = u16::from_le_bytes([body[*at], body[*at + 1]]) as usize;
        *at += 2;
        let end = (*at + count * 2).min(body.len());
        let units: Vec<u16> = (0..(end - *at) / 2)
            .map(|index| {
                let offset = *at + index * 2;
                u16::from_le_bytes([body[offset], body[offset + 1]])
            })
            .collect();
        *at = end;
        String::from_utf16_lossy(&units)
    };

    let character = counted(&mut at);
    let account_type = counted(&mut at);

    Some(PortalLogin {
        request,
        user_id,
        session_id,
        character,
        account_type,
    })
}

/// A 16-byte UUID in the wire's order, as text: the RFC 4122 little-endian variant.
pub fn uuid_le_text(bytes: &[u8; 16]) -> String {
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_le_bytes([bytes[4], bytes[5]]),
        u16::from_le_bytes([bytes[6], bytes[7]]),
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
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
    #[test]
    fn a_running_cipher_reads_the_clients_messages_in_order() {
        use crate::authentication::PORTAL_ACCOUNT_LOGIN;
        use crate::server::handshake::{derive_key, KEY_LEN};
        use common::cryptography::Arc4;

        let master = [0x33u8; MASTER_SECRET_LEN];
        let key: [u8; KEY_LEN] = derive_key(&master);

        // What the client sends: its preamble, then the login message, both through one running
        // cipher - which is what the session reads with.
        let mut preamble = Vec::new();
        preamble.extend_from_slice(&0x8001u16.to_le_bytes());
        preamble.extend_from_slice(&[0u8; 62]);

        let mut login = Vec::new();
        login.extend_from_slice(&PORTAL_ACCOUNT_LOGIN.to_le_bytes());
        login.extend_from_slice(&[0u8; 20]);

        let mut sending = Arc4::new(&key);
        let mut scrambled_preamble = preamble.clone();
        let mut scrambled_login = login.clone();
        sending.apply(&mut scrambled_preamble);
        sending.apply(&mut scrambled_login);

        // The same reading: one cipher, two messages, in order.
        let mut receiving = Arc4::new(&key);
        let mut first = scrambled_preamble.clone();
        receiving.apply(&mut first);
        let mut second = scrambled_login.clone();
        receiving.apply(&mut second);

        assert_eq!(u16::from_le_bytes([first[0], first[1]]), 0x8001);
        assert_eq!(
            u16::from_le_bytes([second[0], second[1]]),
            PORTAL_ACCOUNT_LOGIN
        );

        // A different key reads neither as a message id.
        let other: [u8; KEY_LEN] = derive_key(&[0x44u8; MASTER_SECRET_LEN]);
        let mut wrong = Arc4::new(&other);
        let mut text = scrambled_preamble.clone();
        wrong.apply(&mut text);
        assert_ne!(u16::from_le_bytes([text[0], text[1]]), 0x8001);
    }
    #[test]
    fn a_login_message_is_read_into_its_fields() {
        use crate::authentication::PORTAL_ACCOUNT_LOGIN;

        let mut body = Vec::new();
        body.extend_from_slice(&PORTAL_ACCOUNT_LOGIN.to_le_bytes());
        body.extend_from_slice(&7u32.to_le_bytes()); // the request id to echo

        let mut user = [0u8; 16];
        user[0] = 0x11;
        user[15] = 0xAA;
        body.extend_from_slice(&user);

        let mut session = [0u8; 16];
        session[0] = 0x22;
        body.extend_from_slice(&session);

        // The character name and the account type, as the client packs them: a u16 count and units.
        for text in ["Asura One", "gw1"] {
            let units: Vec<u16> = text.encode_utf16().collect();
            body.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for unit in units {
                body.extend_from_slice(&unit.to_le_bytes());
            }
        }

        let login = decode_portal_login(&body[2..]).expect("the body is long enough");
        assert_eq!(login.request, 7);
        assert_eq!(login.character, "Asura One");
        assert_eq!(login.account_type, "gw1");
        assert_eq!(uuid_le_text(&login.user_id).len(), 36);
        assert!(uuid_le_text(&login.user_id).starts_with("00000011"));
    }

    #[test]
    fn a_body_that_is_too_short_is_not_a_login() {
        assert!(decode_portal_login(&[0u8; 20]).is_none());
    }
}
