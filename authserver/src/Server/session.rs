//! What a connection does: the file check, the exchange, and then everything the client asks for.
//!
//! Reference for the shape: `src/server/apps/authserver/Server/AuthSocket.cpp`, whose
//! `HandleAuthChallenge` reads the client's version, reads its seed, and answers with the server's,
//! and whose handler table then answers one message at a time. The **bytes** are this client's, from
//! the evidence in [`super::handshake`], [`super::stream`] and [`super::login_reply`].
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
//! 5. Send the session info and a heartbeat, then unscramble everything in both directions and
//!    answer, one message at a time, what the client asks: its login, its EULA, its ask-response,
//!    its player status, and its request for a map.
//!
//! The last of those is where the reference's auth service ends: the client asks for a map
//! (`0x8029`) and is answered with an address (`0x0009`). What it does with that address is not this
//! program's business - and is the open question `docs/port/02-wire.md` records, because the only
//! measurement of it says the client dials the address it was launched with instead.

use common::logging::{Log, LogLevel};
use shared::cryptography::AuthCrypt;
use shared::network::socket::{Session, SessionFuture, SessionIo};

use crate::authentication::{ConnectionKind, PORTAL_ACCOUNT_LOGIN};
use crate::server::file_check::FileCheck;
use crate::server::handshake::{
    self, CLIENT_SEED_HEADER, CLIENT_SEED_LEN, MASTER_SECRET_LEN, SERVER_SEED_LEN,
};
use crate::server::stream::{self, Take};

/// The client's EULA acceptance (`0x8026`): three bytes, one of them the version it accepted.
const ACCEPT_EULA: u16 = 0x8026;
/// The client's ask-server-response (`0x8035`): it wants an ordinary request response and nothing
/// else.
const ASK_SERVER_RESPONSE: u16 = 0x8035;
/// The client's set-player-status (`0x800E`): one dword, 1 for online, 0 for offline.
const SET_PLAYER_STATUS: u16 = 0x800E;
/// The client's request-game-instance (`0x8029`): the map it wants to load.
const REQUEST_GAME_INSTANCE: u16 = 0x8029;
/// The client's district-count query (`0x800B`): its transaction, game type, territory and mission.
const QUERY_INSTANCE_COUNT: u16 = 0x800B;
/// The client's disconnect (`0x800D`): it is leaving, and the session ends with it.
const DISCONNECT: u16 = 0x800D;
/// The client choosing a character (`0x800A`): a request id and the character's name as a counted
/// UTF-16 string. **This is the message a character click sends**, and a server that does not answer it
/// leaves the list unresponsive - the client waits on a request response that never comes.
///
/// Reference: `AuthSrv_HandleChangePlayCharacter`, which finds the character by name on the account,
/// records it on the session and answers the request. `ASURA/crates/asura-auth/src/lib.rs` carries the
/// same handler; the header and the layout are the `AUTH_CMSG_CHANGE_PLAY_CHARACTER` row of
/// `shared/src/Packets/Tables.rs`.
const CHANGE_PLAY_CHARACTER: u16 = 0x800A;
/// The settings uploads (`0x8009`, `0x8020`, `0x8021`): **they must be acknowledged**. An upload with no
/// answer is a hang, and the reference's own history has seven of them (`docs/evidence/client-protocol.md`
/// section 6).
const UPDATE_CHARACTER_SETTINGS: u16 = 0x8009;
const SETTING_UPDATE_CONTENT: u16 = 0x8020;
const SETTING_UPDATE_SIZE: u16 = 0x8021;
/// The client's access key (`0x801C`): the reference decodes it, logs it and stores nothing, then
/// answers the request with status 0.
const ADD_ACCESS_KEY: u16 = 0x801C;

/// `AUTH_CMSG_HEARTBEAT` (`0x8000`): the client's own keep-alive, answered with the server's.
const HEARTBEAT: u16 = 0x8000;
/// `AUTH_CMSG_SEND_COMPUTER_INFO` (`0x8001`): the machine's name and user, which the reference stores
/// and answers with nothing.
const COMPUTER_INFO: u16 = 0x8001;
/// `AUTH_CMSG_SEND_COMPUTER_HASH` (`0x8002`): the message the client waits on before the login.
const COMPUTER_HASH: u16 = 0x8002;
/// `AUTH_CMSG_DELETE_CHARACTER` (`0x8007`): one of this account's characters, by name.
const DELETE_CHARACTER: u16 = 0x8007;

/// The most bytes a stored settings blob may hold: what one login's `AUTH_SMSG_ACCOUNT_SETTINGS` can
/// carry (`ASURA/crates/asura-auth/src/lib.rs:83-88`, the message's declared `Array8` of 2048). An upload that
/// announces more is acknowledged and not stored.
const ACCOUNT_SETTINGS_CAPACITY: usize = 2048;

/// The largest single `0x8020` chunk the client sends
/// (`ASURA/crates/asura-protocol/src/client/messages.rs:572`).
const SETTINGS_CHUNK_CAPACITY: usize = 512;

/// ASURA line numbers in this subject are ASURA at `071735e` (the feature is ASURA `77b0ff8`, newer than
/// the `ca9f532` AGENTS.md pins); the measurement is `docs/evidence/ui-settings-persistence.md`.
///
/// A settings upload in flight: the size message (`0x8021`) opened it, and the content messages
/// (`0x8020`) that carry its request id fill it (`ASURA/crates/asura-auth/src/lib.rs:125-135`).
#[derive(Debug)]
struct SettingsUpload {
    /// The request id the size message announced; only content carrying it belongs to this upload.
    request: u32,
    /// The byte count the size message announced.
    total: usize,
    /// The bytes received so far.
    bytes: Vec<u8>,
}

/// Where a client that asks for a map is sent: a world's address and the id it is known by.
///
/// Reference: what the reference keeps in its realm list and hands to
/// `AuthSrv_SendGameServerInfo` - the world's `server_id` becomes the message's `map_token`. A
/// program with no world to name answers a travel with a refusal instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameServer {
    /// The world's id, which the client carries back as `map_token` on its game connection.
    pub token: u32,
    /// The address the client is told to dial.
    pub address: std::net::SocketAddr,
}

/// One client's conversation.
pub struct AuthSession {
    log: Log,
    master: [u8; MASTER_SECRET_LEN],
    file_check: FileCheck,
    /// The committed 591-byte block a login is answered with when the account has no stored upload.
    account_settings: Vec<u8>,
    /// The account the portal identified, once a login has resolved one. A character choice
    /// (0x800A) is answered from it, and it is the join a travel request uses.
    account: Option<[u8; 16]>,
    /// The front door: the worlds a travel request is answered from, and where a gameplay connection
    /// that arrives on this port is copied to (`docs/decisions/0004`).
    front_door: Option<std::sync::Arc<crate::server::FrontDoor>>,
    /// The character database, when the program opened one: the login's answer lists its account's
    /// characters from it (docs/decisions/0007-character-schema.md).
    characters: Option<std::sync::Arc<crate::server::DatabaseCharacters>>,
    /// The travel table in the login database, when the program opened one: a `0x8029` answer
    /// records the hand-off the world then claims (`docs/port/06-world-plan.md`, "The three seams").
    travels: Option<std::sync::Arc<game::maps::DatabaseTravels>>,
    /// The account table, when the program opened one: the EULA version a client accepts is
    /// recorded there, which is what stops it asking again.
    accounts: Option<std::sync::Arc<game::accounts::DatabaseAccounts>>,
    /// The character the client has chosen (`0x800A`), as the map its row names. A travel request
    /// (`0x8029`) is placed by it: a client naming the creation instance as a place is put where the
    /// character actually stands (`game::maps::placement`).
    character_map: Option<u16>,
    /// The character this connection chose, by its own id, so a delete knows whether it just took this
    /// connection's character away (`ASURA/crates/asura-auth/src/lib.rs:436-455`).
    character_chosen: Option<[u8; 16]>,
    /// The account settings upload in flight, if the client is in the middle of one.
    settings_upload: Option<SettingsUpload>,
    /// The salt this connection's session info carried, so the computer hash is answered with the salt
    /// the client's cipher is already derived from.
    salt: Option<u32>,
    /// The map id that means "the character-creation instance" - a setting (`CreationMap`).
    creation_map: u16,
    /// Where a character with no map anywhere enters - a setting (`FallbackOutpost`).
    fallback_outpost: u16,
}

impl AuthSession {
    /// A session that will answer the file check with these bytes and the exchange with this master.
    ///
    /// All of it is passed in rather than read or drawn here so a test can supply what it knows.
    pub fn new(
        log: Log,
        master: [u8; MASTER_SECRET_LEN],
        file_check: FileCheck,
        account_settings: Vec<u8>,
        front_door: Option<std::sync::Arc<crate::server::FrontDoor>>,
        characters: Option<std::sync::Arc<crate::server::DatabaseCharacters>>,
    ) -> Self {
        Self {
            log,
            master,
            file_check,
            account_settings,
            account: None,
            front_door,
            characters,
            travels: None,
            accounts: None,
            character_map: None,
            character_chosen: None,
            settings_upload: None,
            salt: None,
            creation_map: 0,
            fallback_outpost: 0,
        }
    }

    /// Give the session the two numbers a travel is placed by: which map id is the creation instance,
    /// and where a character with no map anywhere enters.
    ///
    /// Both are settings (`docs/port/04-configuration.md`); a session without them keeps 0 for each,
    /// which leaves every travel exactly as the client named it.
    pub fn with_placement(mut self, creation_map: u16, fallback_outpost: u16) -> Self {
        self.creation_map = creation_map;
        self.fallback_outpost = fallback_outpost;
        self
    }

    /// Give the session an account table to record the EULA version in.
    pub fn with_accounts(
        mut self,
        accounts: std::sync::Arc<game::accounts::DatabaseAccounts>,
    ) -> Self {
        self.accounts = Some(accounts);
        self
    }

    /// Give the session a travel table to record a hand-off in.
    ///
    /// A session without one answers `0x8029` the same way and records nothing: the world then reads
    /// the map from the version message, which is what a program with no login database can serve.
    pub fn with_travels(mut self, travels: std::sync::Arc<game::maps::DatabaseTravels>) -> Self {
        self.travels = Some(travels);
        self
    }

    /// A session with a master drawn from the operating system's entropy.
    pub fn with_random_master(
        log: Log,
        file_check: FileCheck,
        account_settings: Vec<u8>,
        front_door: Option<std::sync::Arc<crate::server::FrontDoor>>,
        characters: Option<std::sync::Arc<crate::server::DatabaseCharacters>>,
    ) -> Self {
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
        Self::new(
            log,
            master,
            file_check,
            account_settings,
            front_door,
            characters,
        )
    }

    /// The settings blob a login is answered with: the account's stored upload when it has a usable
    /// one, otherwise the committed block every new account starts from
    /// (`ASURA/crates/asura-auth/src/lib.rs:736-757` and `846-876`).
    fn settings_for_login(&self, peer: &str, user_id: &[u8; 16]) -> Vec<u8> {
        let Some(accounts) = self.accounts.as_ref() else {
            return self.account_settings.clone();
        };
        match accounts.settings_for_user(&game::characters::uuid_text(user_id)) {
            Ok(Some(bytes)) if !bytes.is_empty() && bytes.len() <= ACCOUNT_SETTINGS_CAPACITY => {
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: the account's own settings, {} bytes.",
                        bytes.len()
                    ),
                );
                bytes
            }
            Ok(Some(bytes)) => {
                self.say(
                    LogLevel::Warn,
                    format!(
                        "Connection from {peer}: the stored settings are {} bytes, outside 1..={ACCOUNT_SETTINGS_CAPACITY}; using the committed block.",
                        bytes.len()
                    ),
                );
                self.account_settings.clone()
            }
            Ok(None) => self.account_settings.clone(),
            // A read that fails is not worth losing a login over: the committed block still logs in.
            Err(error) => {
                self.say(
                    LogLevel::Error,
                    format!(
                        "Connection from {peer}: the stored settings could not be read ({error}); using the committed block."
                    ),
                );
                self.account_settings.clone()
            }
        }
    }

    /// Keep a completed settings upload on the account this connection logged in as.
    fn store_settings(&self, peer: &str, bytes: &[u8]) {
        let (Some(accounts), Some(account)) = (self.accounts.as_ref(), self.account) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: a {}-byte settings upload arrived with no account table or no login on this connection; not stored.",
                    bytes.len()
                ),
            );
            return;
        };
        let user_id = game::characters::uuid_text(&account);
        match accounts.set_settings_for_user(&user_id, bytes) {
            Ok(true) => self.say(
                LogLevel::Info,
                format!(
                    "Connection from {peer}: stored {} bytes of UI settings for account {user_id}.",
                    bytes.len()
                ),
            ),
            Ok(false) => self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: a settings upload for {user_id} matched no account row."
                ),
            ),
            Err(error) => self.say(
                LogLevel::Error,
                format!("Connection from {peer}: the settings could not be stored: {error}"),
            ),
        }
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        self.log.out(level, "server.authserver", text);
    }

    /// The whole conversation, as far as it goes.
    async fn converse(&mut self, io: &mut SessionIo) {
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
                    // A gameplay connection arrives on this port because this client dials the
                    // address it was launched with, not the one `0x0009` named - measured, see
                    // `docs/port/02-wire.md`. The front door copies it to the world it names.
                    if other == ConnectionKind::Game {
                        match self.front_door.as_ref() {
                            Some(door) => door.route(io, first_four).await,
                            None => {
                                self.say(
                                    LogLevel::Warn,
                                    format!(
                                        "Connection from {peer}: a gameplay connection arrived, and this program has no front door (no world is configured), so it is closed."
                                    ),
                                );
                                let _ = io.close().await;
                            }
                        }
                        return;
                    }

                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Connection from {peer}: a {} connection arrived on the auth port (its first four bytes are {}). The control channel is not ported yet; the reference serves it from a game server, not from here.",
                            other.as_str(),
                            shared::network::socket::hex(&first_four)
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
    async fn answer_file_check(&self, io: &mut SessionIo, peer: &str, first_four: [u8; 4]) -> bool {
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
    async fn finish_exchange(&mut self, io: &mut SessionIo, peer: &str) {
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
        // cipher's position carries across reads: a running stream, not one per message. One cipher
        // per direction, which is what `AuthCrypt` is.
        let key = handshake::derive_key(&self.master);
        let mut crypt = AuthCrypt::new(&key);

        // The reference answers the seed with two messages before the client asks for anything: the
        // session info and a heartbeat. A client that does not get them waits and then gives up -
        // a run against a server that answered only the seed sat for forty seconds and reset.
        let mut salt = [0u8; 4];
        if getrandom::getrandom(&mut salt).is_err() {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer}: no entropy for the session salt; using zero."),
            );
        }
        let salt = u32::from_le_bytes(salt);
        self.salt = Some(salt);
        let mut greeting = crate::server::login_reply::greeting(salt);
        crypt.encrypt(&mut greeting);
        match io.write_all(&greeting).await {
            Ok(()) => self.say(
                LogLevel::Info,
                format!(
                    "Connection from {peer}: greeted with the session info and a heartbeat, {} bytes.",
                    greeting.len()
                ),
            ),
            Err(error) => self.say(
                LogLevel::Warn,
                format!("Connection from {peer} could not be greeted: {error}"),
            ),
        }
        let mut buffer = [0u8; 4096];
        let mut pending: Vec<u8> = Vec::new();
        let mut messages = 0usize;

        loop {
            // Everything already unscrambled and not yet answered, before any more is read: one
            // read can hold several messages, and the table is what says where each one ends.
            loop {
                match stream::take(&pending) {
                    Take::Incomplete => break,
                    Take::Unknown { header } => {
                        self.say(
                            LogLevel::Warn,
                            format!(
                                "Connection from {peer}: a message with header 0x{header:04X} has no size this server knows, so the stream cannot be split any further. The bytes so far are {}.",
                                shared::network::socket::hex(&pending)
                            ),
                        );
                        return;
                    }
                    Take::Message(message) => {
                        let header = message.header;
                        let frame_len = message.frame.len();
                        let body = message.body().to_vec();
                        pending.drain(..frame_len);
                        messages += 1;

                        // The bytes as the client sent them once unscrambled. The raw trace cannot
                        // help here: it shows the ciphertext, and the key is per connection, so
                        // without this line nothing in the log says what a client asked for.
                        {
                            let shown = body.len().min(64);
                            self.log.out(
                                LogLevel::Debug,
                                "auth.trace",
                                format!(
                                    "auth.trace {peer} message {messages} 0x{header:04X} body: {}{}",
                                    shared::network::socket::hex(&body[..shown]),
                                    if body.len() > shown { " ..." } else { "" }
                                ),
                            );
                        }

                        if !self
                            .answer(io, peer, header, body.as_slice(), messages, &mut crypt)
                            .await
                        {
                            return;
                        }
                    }
                }
            }

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
                        format!(
                            "Connection from {peer} could not be read after the exchange: {error}"
                        ),
                    );
                    return;
                }
            };

            let mut plain = buffer[..count].to_vec();
            crypt.decrypt(&mut plain);
            pending.extend_from_slice(&plain);
        }
    }

    /// Answer one message. Answers whether the connection stays open.
    async fn answer(
        &mut self,
        io: &mut SessionIo,
        peer: &str,
        header: u16,
        body: &[u8],
        number: usize,
        crypt: &mut AuthCrypt,
    ) -> bool {
        match header {
            PORTAL_ACCOUNT_LOGIN => self.answer_login(io, peer, body, crypt).await,

            ACCEPT_EULA => {
                // Reference: the EULA handler records the version and sends nothing at all
                // (`AuthSrv.c:1270-1275`); the client proceeds on its own. The column is what stops
                // it asking again on the next login.
                let version = body.first().copied().unwrap_or(0);
                match (self.accounts.as_ref(), self.account) {
                    (Some(accounts), Some(account)) => {
                        let user_id = game::characters::uuid_text(&account);
                        match accounts.set_eula_for_user(&user_id, i64::from(version)) {
                            Ok(true) => self.say(
                                LogLevel::Info,
                                format!(
                                    "Connection from {peer}: the EULA version {version} is recorded for account {user_id}; the reference answers this with nothing."
                                ),
                            ),
                            Ok(false) => self.say(
                                LogLevel::Warn,
                                format!(
                                    "Connection from {peer}: the EULA version {version} is accepted, and no account row is keyed {user_id}."
                                ),
                            ),
                            Err(error) => self.say(
                                LogLevel::Error,
                                format!(
                                    "Connection from {peer}: the EULA version could not be recorded: {error}"
                                ),
                            ),
                        }
                    }
                    _ => self.say(
                        LogLevel::Info,
                        format!(
                            "Connection from {peer}: the EULA was accepted, version {version}, but no account is known on this connection yet, so it is not recorded; the reference answers this with nothing."
                        ),
                    ),
                }
                true
            }

            ASK_SERVER_RESPONSE => {
                // The reference answers with an ordinary request response, status 0:
                // `03 00 02 00 00 00 00 00 00 00` in the trace.
                let request = request_id(body);
                let answer = crate::server::login_reply::request_response(
                    request,
                    crate::server::login_reply::GM_ERROR_NONE,
                );
                self.send(io, peer, crypt, &[answer], "ask server response")
                    .await
            }

            SET_PLAYER_STATUS => {
                // Reference: nothing. The dword is 1 for "online", 0 for "offline", and the
                // reference only logs it.
                let status = request_id(body);
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: player status {}, answered with nothing as the reference does.",
                        if status == 1 { "online" } else { "offline" }
                    ),
                );
                true
            }

            REQUEST_GAME_INSTANCE => self.answer_game_instance(io, peer, body, crypt).await,

            // The client's district list asked for its counts (`0x800B`, E1's `QueryInstanceCount`:
            // the transaction, the game type, the territory and the mission). The counts are the
            // world's state and this program does not hold it, so the answer is the rule every world
            // starts from: one district, in language 0 (R16), and none for the languages it does not
            // run. The transaction is echoed so the client matches it to its request.
            QUERY_INSTANCE_COUNT => {
                let trans = body
                    .get(0..4)
                    .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    .unwrap_or(0);
                let territory = body
                    .get(8..12)
                    .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    .unwrap_or(0);
                let mut counts = [(0u8, 0u8); 11];
                for (index, pair) in counts.iter_mut().enumerate() {
                    *pair = (index as u8, u8::from(index == 0));
                }
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: the district list asked (transaction {trans}, territory {territory}); answered with one district in language 0 (R16)."
                    ),
                );
                let answer = crate::server::login_reply::instance_counts(trans, &counts);
                self.send(io, peer, crypt, &[answer], "the district counts")
                    .await
            }

            // A character was chosen. The reference finds it by name on the account and answers the
            // request; nothing is sent to the world from here, because the travel request is what
            // carries the choice (`0x8029`).
            CHANGE_PLAY_CHARACTER => {
                let request = request_id(body);
                let name = counted_name(body.get(4..).unwrap_or_default());
                let chosen = match (self.characters.as_ref(), self.account) {
                    (Some(characters), Some(account)) => characters
                        .for_account(&game::characters::uuid_text(&account))
                        .into_iter()
                        .find(|character| character.name == name),
                    _ => None,
                };
                let found = match (self.characters.as_ref(), self.account) {
                    (Some(_), Some(_)) => Some(chosen.is_some()),
                    _ => None,
                };
                // The character the client is about to enter, and the map its row names: a travel
                // request that follows is placed by it.
                if let Some(character) = &chosen {
                    self.character_map = Some(character.settings.last_outpost);
                    self.character_chosen = Some(character.uuid);
                }

                let status = if found.unwrap_or(false) {
                    crate::server::login_reply::GM_ERROR_NONE
                } else {
                    // `GM_ERROR_NETWORK_ERROR`, which is what the reference answers a name it does not
                    // hold; a client that gets it stays on the list instead of hanging.
                    crate::server::login_reply::GM_ERROR_NETWORK_ERROR
                };
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: the client chose the character '{name}', and this server answers {}.",
                        if status == 0 { "yes" } else { "that no character of that name is on the account" }
                    ),
                );
                let answer = crate::server::login_reply::request_response(request, status);
                self.send(io, peer, crypt, &[answer], "the character choice")
                    .await
            }

            // The character-select settings upload (`0x8009`): acknowledged and nothing more
            // (`ASURA/crates/asura-auth/src/lib.rs:491-501`). An unacknowledged settings upload is a hang (`docs/evidence/client-protocol.md`
            // section 6).
            UPDATE_CHARACTER_SETTINGS => {
                let request = request_id(body);
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: message 0x{header:04X} ({} bytes) acknowledged with status 0.",
                        body.len() + 2
                    ),
                );
                let answer = crate::server::login_reply::request_response(
                    request,
                    crate::server::login_reply::GM_ERROR_NONE,
                );
                self.send(io, peer, crypt, &[answer], "a settings upload")
                    .await
            }

            // The account settings upload, first half (`0x8021`): `req_id: u32, size: u32` announces the
            // whole blob. The blob is the client's UI state - window positions, visibility and the
            // other interface options - and it is kept verbatim so the next login hands it back
            // (`ASURA/crates/asura-auth/src/lib.rs:382-424`).
            SETTING_UPDATE_SIZE => {
                let request = request_id(body);
                match decode_settings_size(body) {
                    Some((announced, total)) if total > 0 && total <= ACCOUNT_SETTINGS_CAPACITY => {
                        self.say(
                            LogLevel::Info,
                            format!(
                                "Connection from {peer}: settings upload {announced} announces {total} bytes."
                            ),
                        );
                        self.settings_upload = Some(SettingsUpload {
                            request: announced,
                            total,
                            bytes: Vec::with_capacity(total),
                        });
                    }
                    Some((announced, total)) => {
                        self.say(
                            LogLevel::Warn,
                            format!(
                                "Connection from {peer}: settings upload {announced} announces {total} bytes, outside 1..={ACCOUNT_SETTINGS_CAPACITY}; not stored."
                            ),
                        );
                        self.settings_upload = None;
                    }
                    None => {
                        self.say(
                            LogLevel::Warn,
                            format!(
                                "Connection from {peer}: a settings size of {} bytes is too short; not stored.",
                                body.len() + 2
                            ),
                        );
                        self.settings_upload = None;
                    }
                }
                let answer = crate::server::login_reply::request_response(
                    request,
                    crate::server::login_reply::GM_ERROR_NONE,
                );
                self.send(io, peer, crypt, &[answer], "a settings size")
                    .await
            }

            // The account settings upload, second half (`0x8020`): `req_id: u32, count: u16` and that
            // many bytes. When the announced total has arrived the blob replaces the account's stored
            // one, and the next login is answered with it (`ASURA/crates/asura-auth/src/lib.rs:426-489`).
            SETTING_UPDATE_CONTENT => {
                let request = request_id(body);
                let mut completed = None;
                match decode_settings_content(body) {
                    Some((chunk_request, chunk)) => match self.settings_upload.as_mut() {
                        Some(upload) if upload.request == chunk_request => {
                            upload.bytes.extend_from_slice(chunk);
                            if upload.bytes.len() >= upload.total {
                                completed = self.settings_upload.take().map(|upload| upload.bytes);
                            }
                        }
                        Some(upload) => {
                            let open = upload.request;
                            self.say(
                                LogLevel::Warn,
                                format!(
                                    "Connection from {peer}: a settings chunk for request {chunk_request} while request {open} is open; ignored."
                                ),
                            );
                        }
                        None => self.say(
                            LogLevel::Warn,
                            format!(
                                "Connection from {peer}: a settings chunk for request {chunk_request} without a size announcement; ignored."
                            ),
                        ),
                    },
                    None => self.say(
                        LogLevel::Warn,
                        format!(
                            "Connection from {peer}: a settings chunk of {} bytes is malformed; ignored.",
                            body.len() + 2
                        ),
                    ),
                }
                if let Some(bytes) = completed {
                    self.store_settings(peer, &bytes);
                }
                let answer = crate::server::login_reply::request_response(
                    request,
                    crate::server::login_reply::GM_ERROR_NONE,
                );
                self.send(io, peer, crypt, &[answer], "a settings chunk")
                    .await
            }

            // `AUTH_CMSG_ADD_ACCESS_KEY`: the reference decodes the key to a string, logs it and stores
            // **nothing**; the working implementation counts the units instead, because a key in a log is
            // a secret in a log, and answers the request with status 0
            // (`ASURA/crates/asura-auth/src/lib.rs:384-406`). Parity means exactly that.
            ADD_ACCESS_KEY => {
                let request = request_id(body);
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: an access key of {} unit(s), answered with status 0 and kept nowhere, as the reference keeps it.",
                        access_key_units(body)
                    ),
                );
                let answer = crate::server::login_reply::request_response(
                    request,
                    crate::server::login_reply::GM_ERROR_NONE,
                );
                self.send(io, peer, crypt, &[answer], "an access key").await
            }

            DISCONNECT => {
                // Reference: the disconnect handler ends the session. Seen in this tree's own log
                // when a client's gameplay connection was refused: it said it was offline, then sent
                // `0D 80 00 00 00 00`, and a table without this header stopped splitting the stream.
                self.say(
                    LogLevel::Info,
                    format!("Connection from {peer}: the client is disconnecting."),
                );
                false
            }

            // `AUTH_CMSG_HEARTBEAT`: answered with the server's own heartbeat, tick 16 - the
            // reference's constant (`ASURA/crates/asura-auth/src/lib.rs:307-312`).
            HEARTBEAT => {
                let answer = crate::server::login_reply::heartbeat(16);
                self.send(io, peer, crypt, &[answer], "a client heartbeat")
                    .await
            }

            // `AUTH_CMSG_SEND_COMPUTER_HASH`: the message the client waits on before it logs in, and
            // the working implementation answers it with the session info
            // (`ASURA/crates/asura-auth/src/lib.rs:256-275`). This tree sends that pair unasked the
            // moment the seed is answered, because the reference's own trace shows it there
            // (`login_reply::greeting`), so the answer repeats the salt **already on the wire**: a
            // second salt would be one the client's cipher is not derived from.
            COMPUTER_HASH => match self.salt {
                Some(salt) => {
                    self.say(
                        LogLevel::Info,
                        format!(
                            "Connection from {peer}: the computer hash is answered with the session info it was already sent (salt 0x{salt:08X})."
                        ),
                    );
                    let answer = crate::server::login_reply::session_info(salt, 0);
                    self.send(io, peer, crypt, &[answer], "the session info")
                        .await
                }
                None => true,
            },

            // `AUTH_CMSG_SEND_COMPUTER_INFO`: the reported machine and user, which the reference
            // stores on the connection and answers with nothing
            // (`ASURA/crates/asura-auth/src/lib.rs:250-254`).
            COMPUTER_INFO => {
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: the computer info ({} bytes) is recorded and answered with nothing, as the reference does.",
                        body.len() + 2
                    ),
                );
                true
            }

            // `0x8023` carries nothing this server answers, as before.
            0x8023 => true,

            // `AUTH_CMSG_DELETE_CHARACTER`: the name must be one of **this account's** characters, then
            // the row goes and the request is answered with status 0; a name it does not know is 23 and
            // a delete that fails is 2 (`ASURA/crates/asura-auth/src/lib.rs:409-474`, and the
            // reference's `AuthSrv_HandleDeleteCharacter`). The character list is **not** re-sent.
            DELETE_CHARACTER => {
                let request = request_id(body);
                let name = counted_name(body.get(4..).unwrap_or_default());
                let status = match (self.characters.as_ref(), self.account) {
                    (Some(characters), Some(account)) => {
                        let account_text = game::characters::uuid_text(&account);
                        let found = characters
                            .for_account(&account_text)
                            .into_iter()
                            .find(|character| character.name == name);
                        match found {
                            None => {
                                self.say(
                                    LogLevel::Info,
                                    format!(
                                        "Connection from {peer}: no character named '{name}' on this account, so there is nothing to delete."
                                    ),
                                );
                                crate::server::login_reply::GM_ERROR_INVALID_CHARNAME
                            }
                            Some(character) => {
                                let uuid = game::characters::uuid_text(&character.uuid);
                                match characters.remove(&uuid) {
                                    Ok(true) => {
                                        self.say(
                                            LogLevel::Info,
                                            format!(
                                                "Connection from {peer}: the character '{name}' was deleted."
                                            ),
                                        );
                                        // This connection must stop naming it: the next travel
                                        // carries the choice into the world, and the world refuses a
                                        // character that is gone. The account's first remaining
                                        // character takes its place, which is what the login would
                                        // choose (`asura-auth/src/lib.rs:436-455`).
                                        if self.character_chosen == Some(character.uuid) {
                                            let remaining = characters.for_account(&account_text);
                                            self.character_chosen =
                                                remaining.first().map(|row| row.uuid);
                                            self.character_map = remaining
                                                .first()
                                                .map(|row| row.settings.last_outpost);
                                            self.say(
                                                LogLevel::Info,
                                                format!(
                                                    "Connection from {peer}: the deleted character was this connection's, which now names {}.",
                                                    remaining
                                                        .first()
                                                        .map(|row| row.name.clone())
                                                        .unwrap_or_else(|| "no character".to_string())
                                                ),
                                            );
                                        }
                                        crate::server::login_reply::GM_ERROR_NONE
                                    }
                                    Ok(false) => crate::server::login_reply::GM_ERROR_NETWORK_ERROR,
                                    Err(error) => {
                                        self.say(
                                            LogLevel::Warn,
                                            format!(
                                                "Connection from {peer}: the character '{name}' could not be deleted: {error}"
                                            ),
                                        );
                                        crate::server::login_reply::GM_ERROR_NETWORK_ERROR
                                    }
                                }
                            }
                        }
                    }
                    _ => crate::server::login_reply::GM_ERROR_NETWORK_ERROR,
                };
                let answer = crate::server::login_reply::request_response(request, status);
                self.send(io, peer, crypt, &[answer], "the character delete")
                    .await
            }

            other => {
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: message {number} is 0x{other:04X} with {} bytes, which this server does not answer.",
                        body.len() + 2
                    ),
                );
                true
            }
        }
    }

    /// Scramble and send whole frames, and say so.
    async fn send(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        frames: &[Vec<u8>],
        what: &str,
    ) -> bool {
        let mut answer = Vec::new();
        for frame in frames {
            answer.extend_from_slice(frame);
        }
        crypt.encrypt(&mut answer);

        match io.write_all(&answer).await {
            Ok(()) => {
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: {what} answered with {} message(s), {} bytes.",
                        frames.len(),
                        answer.len()
                    ),
                );
                true
            }
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Connection from {peer} could not be answered: {error}"),
                );
                false
            }
        }
    }

    /// The login: read its fields, and answer the five messages a client needs before it shows a
    /// character.
    async fn answer_login(
        &mut self,
        io: &mut SessionIo,
        peer: &str,
        body: &[u8],
        crypt: &mut AuthCrypt,
    ) -> bool {
        let Some(login) = decode_portal_login(body) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: the login message is {} bytes, which is too few for its fields.",
                    body.len() + 2
                ),
            );
            return true;
        };

        self.say(
            LogLevel::Info,
            format!(
                "Connection from {peer}: login request {}, user {} session {} character '{}' type '{}'.",
                login.request,
                uuid_le_text(&login.user_id),
                uuid_le_text(&login.session_id),
                login.character,
                login.account_type
            ),
        );

        // The answer: the messages a client needs before it shows a character, in the order and at
        // the sizes the records give. The characters are the account's own rows, read from the
        // character database; an account with none is owed an empty list rather than a refusal.
        let characters: Vec<crate::server::login_reply::ReplyCharacter> = match &self.characters {
            Some(source) => source
                .for_account(&game::characters::uuid_text(&login.user_id))
                .into_iter()
                .map(|character| crate::server::login_reply::ReplyCharacter {
                    id: character.uuid,
                    name: character.name,
                    settings: character.settings.to_blob(),
                })
                .collect(),
            None => Vec::new(),
        };
        if !characters.is_empty() {
            self.say(
                LogLevel::Info,
                format!(
                    "Connection from {peer}: {} character(s) listed: {}.",
                    characters.len(),
                    characters
                        .iter()
                        .map(|character| character.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }

        // The character this connection names, recorded so a travel that follows carries it: the one the
        // login message asked for when the account still has it, and the account's first one otherwise.
        //
        // `ASURA/crates/asura-auth/src/lib.rs:637-670` is the working implementation's own rule, and it
        // exists because of a measured fault: a session can name a character that no longer exists -
        // deleting one leaves its id behind, and every travel then names a ghost, which the world refuses
        // ("the character named by the transfer is gone"). The reference falls back to the first
        // character the same way when a session records none, because refusing to pick leaves the client
        // at character select with a travel it cannot complete.
        self.character_chosen = None;
        self.character_map = None;
        if let Some(rows) = self
            .characters
            .as_ref()
            .map(|source| source.for_account(&game::characters::uuid_text(&login.user_id)))
        {
            let named = rows
                .iter()
                .find(|character| character.name.eq_ignore_ascii_case(&login.character))
                .or_else(|| rows.first());
            if let Some(character) = named {
                self.character_chosen = Some(character.uuid);
                self.character_map = Some(character.settings.last_outpost);
                self.say(
                    LogLevel::Info,
                    format!(
                        "Connection from {peer}: this connection names the character '{}', which stands in map {}.",
                        character.name, character.settings.last_outpost
                    ),
                );
            }
        }

        // The EULA version this account accepted, which the account info carries back so the client
        // does not ask again (`AuthSrv_SendAccountInfo`; `ASURA/crates/asura-auth/src/lib.rs:610-629`
        // reads it from the account row and sends it, and the reference's own hardcoded 24 is why its
        // clients are asked every login). A read that fails is 0: one extra page rather than a login.
        let eula_accepted = self
            .accounts
            .as_ref()
            .and_then(|accounts| {
                accounts
                    .eula_for_user(&game::characters::uuid_text(&login.user_id))
                    .ok()
                    .flatten()
            })
            .map(|version| u8::try_from(version.max(0)).unwrap_or(0))
            .unwrap_or(0);

        // The account's own uploaded UI settings, when it has some, are what the login is answered
        // with; otherwise the committed block. Without this the client's saved layout was
        // acknowledged, thrown away, and every session started from the same one
        // (`data/sql/base/db_auth/account_settings.sql`).
        let account_settings = self.settings_for_login(peer, &login.user_id);

        let frames = crate::server::login_reply::reply_frames(
            login.request,
            login.user_id,
            &characters,
            eula_accepted,
            &account_settings,
        );
        self.account = Some(login.user_id);
        self.send(io, peer, crypt, &frames, "the login").await
    }

    /// `AUTH_CMSG_REQUEST_GAME_INSTANCE` (`0x8029`): the client asks for a map, and this program
    /// answers with a world or with a refusal.
    ///
    /// Reference: `AuthSrv_HandleRequestGameInstance`, which picks a live game server and answers
    /// `0x0009` (an address and two tokens) followed by a request response. A request this program
    /// cannot place is answered with the response alone, status 2 - never with an address nothing is
    /// listening on, because that turns a describable error into a connect timeout.
    async fn answer_game_instance(
        &self,
        io: &mut SessionIo,
        peer: &str,
        body: &[u8],
        crypt: &mut AuthCrypt,
    ) -> bool {
        let Some(request) = decode_game_instance(body) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: a request for a game instance came with {} bytes, too few for its six dwords.",
                    body.len() + 2
                ),
            );
            return true;
        };

        self.say(
            LogLevel::Info,
            format!(
                "Connection from {peer}: request {} for map {} (type {}, region {}, district {}, language {}) - the client wants a world.",
                request.request,
                request.map_id,
                request.map_type,
                request.region,
                request.district,
                request.language
            ),
        );

        // Where the character actually enters. A client has no business knowing which outpost its
        // character stands in, and this client names the creation map when a character is clicked
        // (`0x8029` with map 0 and type 0), so the row decides - the rule and its cases are
        // `game::maps::placement`.
        let (destination, substituted) = game::maps::placement::place(
            game::maps::placement::Requested {
                map_id: request.map_id as u16,
                map_type: request.map_type,
            },
            game::maps::placement::Character {
                // This tree keeps one map on a character, which is both where it stands and the
                // outpost it last entered: a character that has never been anywhere names neither.
                map_id: self.character_map.unwrap_or(self.creation_map),
                last_outpost: self.character_map.unwrap_or(self.creation_map),
            },
            // The catalogue's type for the map being asked for, which this program does not hold: its
            // configuration opens the login and character databases only, and the world's map rows are
            // the world's. `None` is rule 3's own first branch - the client's own map, for the caller
            // to refuse - so the rule is whole and the datum is the decision named in
            // `docs/port/09-parity-audit.md` §7a.
            None,
            self.creation_map,
            self.fallback_outpost,
            game::maps::placement::MAP_KINDS,
        );
        match substituted {
            Some(game::maps::placement::Substituted::CreationNamedAsPlace(why)) => self.say(
                LogLevel::Info,
                format!(
                    "Connection from {peer}: the client named the creation instance as a place; map {destination} is where it is sent instead, from {why}."
                ),
            ),
            Some(game::maps::placement::Substituted::NotAnOutpost) => self.say(
                LogLevel::Info,
                format!(
                    "Connection from {peer}: map {} is not an outpost this character may enter from the login; it is sent to its last outpost {destination} instead.",
                    request.map_id
                ),
            ),
            Some(game::maps::placement::Substituted::NothingToPlace) => self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: neither the character's row nor its last outpost names a map, so it is sent to the fallback outpost {destination} (FallbackOutpost)."
                ),
            ),
            None => {}
        }
        let destination = u32::from(destination);

        let world = self.front_door.as_ref().and_then(|door| door.pick());
        let Some(server) = world else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: no world is configured to serve map {destination}, so the request is refused with status {} and no address - what the reference does when it has no game server, and better than naming a port nothing listens on.",
                    crate::server::login_reply::GM_ERROR_NETWORK_ERROR
                ),
            );
            let refusal = crate::server::login_reply::request_response(
                request.request,
                crate::server::login_reply::GM_ERROR_NETWORK_ERROR,
            );
            return self.send(io, peer, crypt, &[refusal], "the refusal").await;
        };

        // A number drawn per travel, so the world can tell one client's connection from another's.
        // It is the pair - this token and the world's - that the world compares, and nothing else.
        let mut token_bytes = [0u8; 4];
        if getrandom::getrandom(&mut token_bytes).is_err() {
            self.say(
                LogLevel::Warn,
                format!("Connection from {peer}: no entropy for the player token; using zero."),
            );
        }
        let player_token = u32::from_le_bytes(token_bytes);
        let host = crate::server::login_reply::host_blob(server.address);

        let info = crate::server::login_reply::game_server_info(
            request.request,
            server.token,
            destination,
            &host,
            player_token,
        );
        let response = crate::server::login_reply::request_response(
            request.request,
            crate::server::login_reply::GM_ERROR_NONE,
        );

        // The hand-off: the world learns the map, the district and the language from this row when
        // the gameplay connection presents the same world, account and token
        // (`docs/port/06-world-plan.md`, "The three seams"). It is state, so it is a row.
        match (&self.travels, self.account) {
            (Some(travels), Some(account)) => {
                let travel = game::maps::Travel {
                    account,
                    world: server.token,
                    map_id: destination,
                    district: request.district,
                    language: request.language,
                    player_token,
                    // The login never states a landing: only a doorway crossing knows where the
                    // receiving map puts the character (travel.rs:446-468).
                    landing: None,
                };
                match travels.record(&travel) {
                    Ok(()) => self.say(
                        LogLevel::Info,
                        format!(
                            "Connection from {peer}: the travel to map {destination} (district {}, language {}) of account {} is recorded for world {}; the world claims it when the connection presents token 0x{player_token:08X}.",
                            request.district,
                            request.language,
                            uuid_le_text(&account),
                            server.token
                        ),
                    ),
                    Err(error) => self.say(
                        LogLevel::Error,
                        format!(
                            "Connection from {peer}: the travel to map {destination} was not recorded ({error}); the world will read the map from the version message instead."
                        ),
                    ),
                }
            }
            (Some(_), None) => self.say(
                LogLevel::Warn,
                format!(
                    "Connection from {peer}: a map was asked for before any login resolved an account, so no travel record is written."
                ),
            ),
            (None, _) => {}
        }

        self.say(
            LogLevel::Info,
            format!(
                "Connection from {peer}: told to dial {} as world {} (map token), carrying player token 0x{player_token:08X}. This client will dial the address it was launched with instead, and the front door will copy that connection here.",
                server.address, server.token
            ),
        );

        self.send(io, peer, crypt, &[info, response], "the game server info")
            .await
    }
}

/// The first dword of a body: the request id a reply must echo, or the value `0x800E` carries.
/// The counted UTF-16 name a message carries, from its `u16` count onward.
///
/// A counted string on this wire is a `u16` count of UTF-16 units and twice that many bytes, so it is
/// walked rather than fixed: the count is what says how long the name is.
/// How many units an access key's counted string holds.
///
/// The reference decodes the same field into a string and logs it (`AuthSrv_HandleAddAccessKey`); this
/// tree only counts it, because it stores nothing and a key in a log is a secret in a log
/// (`ASURA/crates/asura-auth/src/lib.rs:541-548`).
fn access_key_units(body: &[u8]) -> usize {
    if body.len() < 6 {
        return 0;
    }
    u16::from_le_bytes([body[4], body[5]]) as usize
}

fn counted_name(bytes: &[u8]) -> String {
    let Some(count) = bytes.get(0..2) else {
        return String::new();
    };
    let units = u16::from_le_bytes([count[0], count[1]]) as usize;
    let units: Vec<u16> = (0..units)
        .filter_map(|index| {
            let at = 2 + index * 2;
            let pair = bytes.get(at..at + 2)?;
            Some(u16::from_le_bytes([pair[0], pair[1]]))
        })
        .collect();
    String::from_utf16_lossy(&units)
}

fn request_id(body: &[u8]) -> u32 {
    if body.len() < 4 {
        return 0;
    }
    u32::from_le_bytes([body[0], body[1], body[2], body[3]])
}

/// `AUTH_CMSG_SETTING_UPDATE_SIZE` (`0x8021`): `req_id: u32, size: u32`
/// (`ASURA/crates/asura-protocol/src/client/messages.rs:579-582`).
fn decode_settings_size(body: &[u8]) -> Option<(u32, usize)> {
    let request = u32::from_le_bytes(body.get(0..4)?.try_into().ok()?);
    let size = u32::from_le_bytes(body.get(4..8)?.try_into().ok()?);
    Some((request, size as usize))
}

/// `AUTH_CMSG_SETTING_UPDATE_CONTENT` (`0x8020`): `req_id: u32, count: u16`, then exactly `count`
/// bytes. Measured by ASURA: `20 80 | 05 00 00 00 | 00 02 <512 bytes>` then `... | 42 00 <66 bytes>`
/// (`ASURA/crates/asura-protocol/src/client/messages.rs:584-601`).
fn decode_settings_content(body: &[u8]) -> Option<(u32, &[u8])> {
    let request = u32::from_le_bytes(body.get(0..4)?.try_into().ok()?);
    let count = u16::from_le_bytes(body.get(4..6)?.try_into().ok()?) as usize;
    if count > SETTINGS_CHUNK_CAPACITY {
        return None;
    }
    Some((request, body.get(6..6 + count)?))
}

/// What a `0x8029` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameInstanceRequest {
    /// Echoed in the address and in the response.
    pub request: u32,
    /// 0 for an outpost, 1 for an explorable area.
    pub map_type: u32,
    /// The map the client wants.
    pub map_id: u32,
    /// A capacity hint, never a permission.
    pub region: u32,
    /// 0 means "any".
    pub district: u32,
    /// The client's language id.
    pub language: u32,
}

/// Read a `0x8029` body: six dwords, 24 bytes.
pub fn decode_game_instance(body: &[u8]) -> Option<GameInstanceRequest> {
    if body.len() < 24 {
        return None;
    }
    let dword =
        |at: usize| u32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
    Some(GameInstanceRequest {
        request: dword(0),
        map_type: dword(4),
        map_id: dword(8),
        region: dword(12),
        district: dword(16),
        language: dword(20),
    })
}

impl Session for AuthSession {
    fn run(mut self: Box<Self>, mut io: SessionIo) -> SessionFuture {
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
    use super::{decode_settings_content, decode_settings_size};

    #[test]
    fn a_settings_upload_decodes_as_asura_measured_it() {
        // 0x8021: request 5 announces 578 bytes.
        let size = [5u8, 0, 0, 0, 0x42, 0x02, 0, 0];
        assert_eq!(decode_settings_size(&size), Some((5, 578)));
        assert_eq!(decode_settings_size(&size[..6]), None);

        // 0x8020: request 5, a 66-byte chunk.
        let mut content = vec![5u8, 0, 0, 0, 0x42, 0x00];
        content.extend(std::iter::repeat(0xAB).take(66));
        let (request, chunk) = decode_settings_content(&content).unwrap();
        assert_eq!((request, chunk.len()), (5, 66));

        // A count past the body, or past one chunk's capacity, is malformed.
        assert_eq!(decode_settings_content(&content[..20]), None);
        let too_big = [5u8, 0, 0, 0, 0x01, 0x02];
        assert_eq!(decode_settings_content(&too_big), None);
    }

    use super::*;
    use crate::server::handshake::{PUBLIC_LEN, shared_secret};
    use shared::network::socket::SessionFactory;
    use shared::network::socket_mgr::SocketMgr;
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::time::Duration;

    fn shipped_file_check() -> FileCheck {
        FileCheck::shipped().unwrap()
    }

    /// A front door onto one world, which is what a program with one running server has.
    fn one_world(id: u32, port: u16) -> Arc<crate::server::FrontDoor> {
        let mut worlds = shared::worlds::WorldList::new();
        worlds.insert(shared::worlds::Realm {
            id,
            name: format!("World {id}"),
            address: "127.0.0.1".parse().unwrap(),
            address_text: "127.0.0.1".to_string(),
            local_address: "127.0.0.1".parse().unwrap(),
            local_subnet_mask: "255.255.255.0".to_string(),
            port,
            icon: 0,
            flags: 0,
            timezone: 0,
            allowed_security_level: 0,
            population: 0.0,
            build: 39167,
        });
        Arc::new(crate::server::FrontDoor::new(
            Arc::new(std::sync::Mutex::new(worlds)),
            Log::new(),
        ))
    }

    fn factory(log: Log, file_check: FileCheck) -> SessionFactory {
        Arc::new(move || {
            Box::new(AuthSession::new(
                log.clone(),
                [0x5Au8; MASTER_SECRET_LEN],
                file_check.clone(),
                crate::server::account_settings::block(),
                None,
                None,
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
        use crate::server::handshake::{KEY_LEN, derive_key};
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
    /// The whole login answer, read the way the client reads it: one cipher, offset zero.
    #[test]
    fn the_login_is_answered_with_the_five_messages_the_client_reads() {
        use crate::authentication::PORTAL_ACCOUNT_LOGIN;
        use common::cryptography::Arc4;

        let log = Log::new();
        let master = [0x77u8; MASTER_SECRET_LEN];
        let settings = crate::server::account_settings::block();

        let factory: SessionFactory = {
            let log = log.clone();
            let file_check = shipped_file_check();
            let settings = settings.clone();
            Arc::new(move || {
                Box::new(AuthSession::new(
                    log.clone(),
                    master,
                    file_check.clone(),
                    settings.clone(),
                    None,
                    None,
                )) as Box<dyn Session>
            })
        };

        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // The preamble, then the seed.
        client.write_all(&preamble(2)).unwrap();
        let mut server_seed = [0u8; SERVER_SEED_LEN];
        client.read_exact(&mut server_seed).unwrap();
        assert_eq!(&server_seed[..2], &0x1601u16.to_le_bytes());

        // Then the greeting the reference sends before the client asks anything: session info and a
        // heartbeat, sixteen bytes, scrambled with the outbound stream's first keystream bytes.
        let key = crate::server::handshake::derive_key(&master);
        let mut greeting = [0u8; 16];
        client.read_exact(&mut greeting).unwrap();
        let mut reading = Arc4::new(&key);
        reading.apply(&mut greeting);
        assert_eq!(
            &greeting[..2],
            &0x0001u16.to_le_bytes(),
            "session info first"
        );
        assert_eq!(
            &greeting[10..12],
            &0x0000u16.to_le_bytes(),
            "then a heartbeat"
        );
        assert_eq!(&greeting[12..16], &16u32.to_le_bytes(), "the tick is 16");

        // The login, packed as the client packs it and scrambled with the key both sides derive.
        let mut body = Vec::new();
        body.extend_from_slice(&PORTAL_ACCOUNT_LOGIN.to_le_bytes());
        body.extend_from_slice(&0x2Au32.to_le_bytes());
        body.extend_from_slice(&[0x11u8; 16]);
        body.extend_from_slice(&[0x22u8; 16]);
        for text in ["Asura", "gw1"] {
            let units: Vec<u16> = text.encode_utf16().collect();
            body.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for unit in units {
                body.extend_from_slice(&unit.to_le_bytes());
            }
        }

        let mut sending = Arc4::new(&key);
        sending.apply(&mut body);
        client.write_all(&body).unwrap();

        // The answer: 700 bytes of five frames - four, with no characters - read back through a
        // cipher at offset zero, which is where the server's outbound stream started.
        let mut answer = vec![0u8; 700];
        client.read_exact(&mut answer).unwrap();

        // One keystream across the whole outbound direction: the greeting was its first sixteen bytes,
        // so the answer continues from where they left off.
        reading.apply(&mut answer);

        let mut at = 0usize;
        let mut frames = Vec::new();
        while at + 2 <= answer.len() {
            let header = u16::from_le_bytes([answer[at], answer[at + 1]]);
            let size = match header {
                0x0016 => 599,
                0x0014 => 10,
                0x0011 => 81,
                0x0003 => 10,
                other => panic!("unexpected header 0x{other:04X} at {at}"),
            };
            let request = u32::from_le_bytes([
                answer[at + 2],
                answer[at + 3],
                answer[at + 4],
                answer[at + 5],
            ]);
            assert_eq!(request, 0x2A, "the request id is echoed");
            frames.push(header);
            at += size;
        }

        assert_eq!(at, 700, "the five messages account for every byte");
        assert_eq!(frames, vec![0x0016, 0x0014, 0x0011, 0x0003]);

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    #[ignore = "needs the login database"]
    fn a_map_request_after_a_login_records_the_travel() {
        // The hand-off: a login answers `0x8029` with an address and draws a player token; the row
        // it writes is what the world claims (`docs/port/06-world-plan.md`, "The three seams").
        use crate::authentication::PORTAL_ACCOUNT_LOGIN;
        use common::cryptography::Arc4;
        use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
        use database::updater::{UpdateState, collect_files};

        let log = Log::new();
        let master = [0x6Au8; MASTER_SECRET_LEN];

        let _serial = database::updater::apply::test_lock("auth");
        let config = common::configuration::Config::load_json(
            "../authserver/authserver.json",
            false,
            common::configuration::Policy::default(),
        )
        .expect("the auth program's own configuration");
        let settings = read_settings(&config, DatabaseKind::Login).expect("the login settings");
        let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)
            .expect("the login pool opens");
        let schema = collect_files(
            std::path::Path::new("../data/sql/base/db_auth"),
            UpdateState::Released,
        )
        .expect("the login schema");
        pool.apply_updates(&schema, &log)
            .expect("the schema applies");
        let login = std::sync::Arc::new(std::sync::Mutex::new(pool));
        let travels = std::sync::Arc::new(game::maps::DatabaseTravels::new(
            std::sync::Arc::clone(&login),
            log.clone(),
        ));
        // The account the login resolves: a row with the user id the login message presents, so the
        // EULA it accepts afterward has somewhere to be recorded.
        let accounts = std::sync::Arc::new(game::accounts::DatabaseAccounts::new(
            std::sync::Arc::clone(&login),
            log.clone(),
        ));
        let eula_user = "eula-test@example.com";
        accounts
            .create(
                eula_user,
                &game::characters::uuid_text(&[0x11u8; 16]),
                "token",
            )
            .expect("the account is written");
        accounts
            .set_eula(eula_user, 0)
            .expect("the account starts unaccepted");

        let factory: SessionFactory = {
            let log = log.clone();
            let travels = std::sync::Arc::clone(&travels);
            let accounts = std::sync::Arc::clone(&accounts);
            let front_door = Some(one_world(7, 6113));
            Arc::new(move || {
                Box::new(
                    AuthSession::new(
                        log.clone(),
                        master,
                        shipped_file_check(),
                        crate::server::account_settings::block(),
                        front_door.clone(),
                        None,
                    )
                    .with_travels(std::sync::Arc::clone(&travels))
                    .with_accounts(std::sync::Arc::clone(&accounts)),
                ) as Box<dyn Session>
            })
        };
        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        // The preamble, the seed, and the greeting.
        client.write_all(&preamble(2)).unwrap();
        let mut server_seed = [0u8; SERVER_SEED_LEN];
        client.read_exact(&mut server_seed).unwrap();
        let key = crate::server::handshake::derive_key(&master);
        let mut reading = Arc4::new(&key);
        let mut greeting = [0u8; 16];
        client.read_exact(&mut greeting).unwrap();
        reading.apply(&mut greeting);

        // The login, which resolves the account the travel row is keyed by.
        let mut body = Vec::new();
        body.extend_from_slice(&PORTAL_ACCOUNT_LOGIN.to_le_bytes());
        body.extend_from_slice(&0x2Au32.to_le_bytes());
        body.extend_from_slice(&[0x11u8; 16]);
        body.extend_from_slice(&[0x22u8; 16]);
        for text in ["Asura", "gw1"] {
            let units: Vec<u16> = text.encode_utf16().collect();
            body.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for unit in units {
                body.extend_from_slice(&unit.to_le_bytes());
            }
        }
        let mut sending = Arc4::new(&key);
        sending.apply(&mut body);
        client.write_all(&body).unwrap();

        // Its answer is the five frames the client reads.
        let mut answer = vec![0u8; 700];
        client.read_exact(&mut answer).unwrap();
        reading.apply(&mut answer);

        // The EULA the client accepts after its login: the version is recorded on the account row,
        // and nothing is sent back (the reference's handler says nothing).
        let mut segment = vec![0x26, 0x80, 0x1A];
        sending.apply(&mut segment);
        client.write_all(&segment).unwrap();
        let mut eula = None;
        for _ in 0..40 {
            eula = {
                let mut pool = login.lock().expect("the login pool");
                let rows = pool
                    .query_with(
                        "SELECT \"eula\"::bigint FROM \"account\" WHERE \"username\" = $1",
                        &[&eula_user],
                    )
                    .expect("the account row reads");
                rows.first().map(|row| row.get::<usize, i64>(0))
            };
            if eula == Some(26) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(eula, Some(26), "the EULA version is recorded");

        // The request for a map, with a district and a language of its own.
        let mut segment = vec![
            0x29, 0x80, // 0x8029
            0x03, 0x00, 0x00, 0x00, // request 3
            0x01, 0x00, 0x00, 0x00, // map type 1
            0x92, 0x00, 0x00, 0x00, // map 146
            0x00, 0x00, 0x00, 0x00, // region 0
            0x02, 0x00, 0x00, 0x00, // district 2
            0x01, 0x00, 0x00, 0x00, // language 1
        ];
        sending.apply(&mut segment);
        client.write_all(&segment).unwrap();

        // The answer: the address, then the response behind it.
        let mut answer = [0u8; 52];
        client.read_exact(&mut answer).unwrap();
        reading.apply(&mut answer);
        assert_eq!(&answer[..2], &[0x09, 0x00], "the game server info first");

        // And the row the world will claim: world 7, map 146, district 2, language 1, this account.
        let account = game::characters::uuid_text(&[0x11u8; 16]);
        let rows = {
            let mut pool = login.lock().expect("the login pool");
            pool.query_with(
                "SELECT \"player_token\" FROM \"travel\" WHERE \"world\" = $1 AND \"map_id\" = \
                 $2 AND \"district\" = $3 AND \"language\" = $4 AND \"account\" = $5",
                &[&7i64, &146i64, &2i64, &1i64, &account],
            )
            .expect("the travel table reads")
        };
        assert_eq!(rows.len(), 1, "one travel was recorded");
        let token: i64 = rows[0].get(0);
        assert!(
            token >= 0 && token <= u32::MAX as i64,
            "the token the login drew"
        );

        // Claiming it consumes it: exactly once, by the pair it presents.
        assert!(
            travels
                .claim(7, &account, token as u32)
                .expect("the travel table reads")
                .is_some(),
            "the world finds the record"
        );
        assert!(
            travels
                .claim(7, &account, token as u32)
                .expect("the travel table reads")
                .is_none(),
            "and it is consumed"
        );

        manager.stop_network();
        accounts.delete(eula_user).expect("the test's account goes");
        log.shutdown();
    }

    /// A client that has finished the plaintext half: the seed answered, the greeting read.
    fn connected(
        log: &Log,
        master: [u8; MASTER_SECRET_LEN],
        front_door: Option<std::sync::Arc<crate::server::FrontDoor>>,
    ) -> (
        SocketMgr,
        std::net::TcpStream,
        [u8; crate::server::handshake::KEY_LEN],
    ) {
        let factory: SessionFactory = {
            let log = log.clone();
            let file_check = shipped_file_check();
            let settings = crate::server::account_settings::block();
            Arc::new(move || {
                Box::new(AuthSession::new(
                    log.clone(),
                    master,
                    file_check.clone(),
                    settings.clone(),
                    front_door.clone(),
                    None,
                )) as Box<dyn Session>
            })
        };

        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        client.write_all(&preamble(2)).unwrap();
        let mut server_seed = [0u8; SERVER_SEED_LEN];
        client.read_exact(&mut server_seed).unwrap();
        assert_eq!(&server_seed[..2], &0x1601u16.to_le_bytes());

        let mut greeting = [0u8; 16];
        client.read_exact(&mut greeting).unwrap();

        (
            manager,
            client,
            crate::server::handshake::derive_key(&master),
        )
    }

    /// The reference's own post-login stream, in the segments a real client writes it in.
    ///
    /// The client sends the EULA and its ask-response together in one write, and its player status
    /// and its request for a map together in another - nine and thirty-two bytes. A reader that
    /// treats one write as one message sees neither of the second messages, which is exactly what
    /// happened before `stream.rs` existed.
    #[test]
    fn the_eula_and_the_travel_request_are_answered_message_by_message() {
        use common::cryptography::Arc4;

        let log = Log::new();
        let master = [0x5Cu8; MASTER_SECRET_LEN];
        let (mut manager, mut client, key) = connected(&log, master, Some(one_world(7, 6113)));

        // One outbound keystream for the whole conversation. The greeting's sixteen bytes came off
        // it first, so the cipher is advanced past them before the answer is read.
        let mut reading = Arc4::new(&key);
        let mut greeting = [0u8; 16];
        reading.apply(&mut greeting);

        // 1. The EULA and the ask-response, in one write.
        let mut sending = Arc4::new(&key);
        let mut segment = vec![0x26, 0x80, 0x1A];
        segment.extend_from_slice(&[0x35, 0x80, 0x02, 0x00, 0x00, 0x00]);
        sending.apply(&mut segment);
        client.write_all(&segment).unwrap();

        // The EULA is answered with nothing; the ask-response with one request response, request 2.
        let mut answer = [0u8; 10];
        client.read_exact(&mut answer).unwrap();
        reading.apply(&mut answer);
        assert_eq!(&answer[..2], &[0x03, 0x00], "a request response");
        assert_eq!(&answer[2..6], &2u32.to_le_bytes(), "echoing request 2");
        assert_eq!(&answer[6..10], &[0, 0, 0, 0], "status 0");

        // 2. The player status and the request for a map, in another write.
        let mut segment = vec![0x0E, 0x80, 0x01, 0x00, 0x00, 0x00];
        segment.extend_from_slice(&[
            0x29, 0x80, // 0x8029
            0x03, 0x00, 0x00, 0x00, // request 3
            0x01, 0x00, 0x00, 0x00, // map type 1
            0x92, 0x00, 0x00, 0x00, // map 146
            0x00, 0x00, 0x00, 0x00, // region 0
            0x02, 0x00, 0x00, 0x00, // district 2
            0x00, 0x00, 0x00, 0x00, // language 0
        ]);
        sending.apply(&mut segment);
        client.write_all(&segment).unwrap();

        // The answer: the address, then the response behind it - 42 and 10.
        let mut answer = [0u8; 52];
        client.read_exact(&mut answer).unwrap();
        reading.apply(&mut answer);

        assert_eq!(&answer[..2], &[0x09, 0x00], "the game server info first");
        assert_eq!(&answer[2..6], &3u32.to_le_bytes(), "echoing request 3");
        assert_eq!(&answer[6..10], &7u32.to_le_bytes(), "the world's token");
        assert_eq!(
            &answer[10..14],
            &146u32.to_le_bytes(),
            "the map it asked for"
        );
        assert_eq!(
            &answer[14..24],
            &[0x02, 0x00, 0x17, 0xE1, 127, 0, 0, 1, 0, 0],
            "family 2, port 6113 big-endian, 127.0.0.1"
        );

        assert_eq!(&answer[42..44], &[0x03, 0x00], "then the request response");
        assert_eq!(&answer[44..48], &3u32.to_le_bytes(), "echoing request 3");

        // 3. The district list's count query, then the counts the world begins with: one district,
        // in language 0 (R16), and none for the languages it does not run.
        let mut segment = vec![
            0x0B, 0x80, // 0x800B, QueryInstanceCount
            0x05, 0x00, 0x00, 0x00, // transaction 5
            0x01, 0x00, 0x00, 0x00, // game type 1
            0x02, 0x00, 0x00, 0x00, // territory 2
            0x00, 0x00, 0x00, 0x00, // mission 0
        ];
        sending.apply(&mut segment);
        client.write_all(&segment).unwrap();
        let mut counts = vec![0u8; 29];
        client.read_exact(&mut counts).unwrap();
        reading.apply(&mut counts);
        assert_eq!(&counts[..2], &[0x1A, 0x00], "the count report");
        assert_eq!(
            &counts[2..6],
            &5u32.to_le_bytes(),
            "echoing the transaction"
        );
        assert_eq!(counts[6], 11, "eleven pairs");
        assert_eq!(
            &counts[7..9],
            &[0, 1],
            "language 0 has the one district (R16)"
        );
        for pair in counts[9..].chunks(2) {
            assert_eq!(pair[1], 0, "the other languages have none");
        }
        assert_eq!(&answer[48..52], &[0, 0, 0, 0], "status 0");

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    fn a_travel_request_with_no_world_is_refused_rather_than_given_an_address() {
        use common::cryptography::Arc4;

        let log = Log::new();
        let master = [0x5Du8; MASTER_SECRET_LEN];
        let (mut manager, mut client, key) = connected(&log, master, None);

        let mut reading = Arc4::new(&key);
        let mut greeting = [0u8; 16];
        reading.apply(&mut greeting);

        let mut sending = Arc4::new(&key);
        let mut segment = vec![0x29, 0x80, 0x04, 0x00, 0x00, 0x00];
        segment.extend_from_slice(&[0u8; 20]);
        sending.apply(&mut segment);
        client.write_all(&segment).unwrap();

        // One message only: the response, status 2, and no address. The reference's own rule for a
        // game instance nobody is serving.
        let mut answer = [0u8; 10];
        client.read_exact(&mut answer).unwrap();
        reading.apply(&mut answer);
        assert_eq!(&answer[..2], &[0x03, 0x00]);
        assert_eq!(&answer[2..6], &4u32.to_le_bytes());
        assert_eq!(&answer[6..10], &2u32.to_le_bytes(), "status 2");

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    fn a_game_connection_on_the_auth_port_is_closed_without_a_word_when_there_is_no_front_door() {
        // With no world configured there is nowhere to send it, and a client waiting on a port
        // that cannot answer is worse served by a closed socket than by silence.
        let log = Log::new();
        let (mut manager, mut client, _key) = connected(&log, [0x5Eu8; MASTER_SECRET_LEN], None);

        let mut game = 0x000C_0500u32.to_le_bytes().to_vec();
        game.extend_from_slice(&[0u8; 60]);
        client.write_all(&game).unwrap();

        let mut answer = [0u8; 1];
        let mut total = 0usize;
        loop {
            match client.read(&mut answer) {
                Ok(0) => break,
                Ok(_) => total += 1,
                Err(_) => break,
            }
        }
        assert_eq!(
            total, 0,
            "a gameplay connection is not answered on the auth port"
        );

        manager.stop_network();
        log.shutdown();
    }

    #[test]
    fn a_gameplay_connection_arriving_here_reaches_the_world_it_names() {
        // The stage's proof, without the client: a client's *second* connection to the auth port,
        // carrying a version message that names world 7, arrives at that world byte for byte, and
        // the world's answer comes back to the client. This is what the front door exists for
        // (`docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`).
        use game::server::protocol::GameVersion;
        use std::sync::Mutex;

        // The world: it reads the version message, answers, and reports what it was sent.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let world_address = listener.local_addr().unwrap();
        let seen: std::sync::Arc<Mutex<Vec<u8>>> = std::sync::Arc::new(Mutex::new(Vec::new()));
        let world = {
            let seen = std::sync::Arc::clone(&seen);
            std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut message = vec![0u8; game::server::protocol::GAME_CMSG_VERSION_LEN];
                stream.read_exact(&mut message).unwrap();
                *seen.lock().unwrap() = message;
                stream.write_all(b"the world answers").unwrap();
                let mut sink = [0u8; 64];
                while let Ok(read) = stream.read(&mut sink) {
                    if read == 0 {
                        break;
                    }
                }
            })
        };

        let log = Log::new();
        let (mut manager, _client, _key) = connected(
            &log,
            [0x5Fu8; MASTER_SECRET_LEN],
            Some(one_world(7, world_address.port())),
        );
        let auth_port = manager.port().unwrap();

        // The client's gameplay connection, to the port it was launched against.
        let mut gameplay = std::net::TcpStream::connect(("127.0.0.1", auth_port)).unwrap();
        gameplay
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 7,
            map_id: 0,
            player_token: 0xA2E3_DB8C,
            account: [0xAA; 16],
            character: [0xBB; 16],
            h0038: 0,
            h003c: 0,
        };
        let message = version.encode();
        gameplay.write_all(&message).unwrap();

        // The world's answer arrives on the gameplay connection, not on the login one.
        let mut answer = [0u8; 17];
        gameplay.read_exact(&mut answer).unwrap();
        assert_eq!(&answer, b"the world answers");

        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &message[..],
            "the world received the version message whole and unmodified"
        );

        drop(gameplay);
        manager.stop_network();
        let _ = world.join();
        log.shutdown();
    }
}
