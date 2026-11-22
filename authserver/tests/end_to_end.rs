//! The end-to-end run: the client's login, and then the world it is sent to, against the two
//! programs.
//!
//! The session tests in this crate play the conversations against socket managers a test builds.
//! This test plays them against the binaries: it spawns `authserver` from this package and
//! `worldserver` from the sibling package, waits for each READY line, performs the client's own
//! sequence - the file check, the auth version message, the key exchange, the portal login, the map
//! request - and then, with the address and tokens the login answered, connects to the world and
//! reads the load block it claims the travel with. The seam is the hand-off row the login writes and
//! the world claims (R9, `docs/port/06-world-plan.md`, "The three seams").
//!
//! It is `#[ignore]` because it needs the databases the programs' configurations name, and because
//! it binds the world program's own port: run it with the two servers stopped.
//!
//! Run with `cargo test -p authserver --test end_to_end -- --ignored --nocapture`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use common::configuration::{Config, Policy};
use common::logging::Log;
use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
use game::server::packets::instance_packets;
use game::server::protocol::GameVersion;
use shared::cryptography::handshake::{
    self, CLIENT_SEED_HEADER, MASTER_SECRET_LEN, PUBLIC_LEN, SERVER_SEED_LEN,
};

/// The file check's question, as a capture prints it (`docs/evidence/client-protocol.md` section 7).
const FILE_CHECK_QUESTION: [u8; 21] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0xf1, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The file check's answer, as a capture prints it: the header, the length and the ids.
const FILE_CHECK_ANSWER_HEADER: [u8; 4] = [0xf1, 0x02, 0x20, 0x00];

/// The auth port is told to bind 0, so the operating system picks one and the program prints it.
const PORT_ENV: &str = "AC_REALM_SERVER_PORT";
/// The portal would bind 6601, which another program may hold; the login conversation needs it not.
const PORTAL_ENV: &str = "AC_PORTAL_ENABLE";
/// The status interval, so the program's own liveness reaches this test's log quickly.
const STATUS_ENV: &str = "AC_STATUS_INTERVAL";

/// Wait for one line of the program's output that contains `needle`, and answer it.
fn wait_for(reader: &Receiver<String>, needle: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match reader.recv_timeout(remaining) {
            Ok(line) => {
                if line.contains(needle) {
                    return line;
                }
            }
            Err(_) => panic!("the program never printed a line containing '{needle}'"),
        }
    }
}

/// Start the auth program on its own port, and answer the port it bound.
fn start_auth() -> (Child, Receiver<String>, u16) {
    let exe = env!("CARGO_BIN_EXE_authserver");
    let mut child = Command::new(exe)
        .arg("-c")
        .arg("authserver/authserver.json")
        .current_dir("..")
        .env(PORT_ENV, "0")
        .env(PORTAL_ENV, "0")
        .env(STATUS_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the auth program starts");
    let stdout = child.stdout.take().expect("the program's stdout is piped");
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });
    let ready = wait_for(&rx, "READY: listening on", Duration::from_secs(60));
    // "READY: listening on 0.0.0.0:41234; status reported every 5 s." - the address is the fourth
    // word, and the port is its last colon-separated part.
    let address = ready
        .split_whitespace()
        .nth(3)
        .expect("the READY line names the address it bound");
    let port: u16 = address
        .trim_end_matches(';')
        .rsplit(':')
        .next()
        .expect("the address has a port")
        .parse()
        .expect("the port is a number");
    (child, rx, port)
}

/// Read exactly `size` bytes, or fail saying how many arrived.
fn read_exact(stream: &mut TcpStream, size: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; size];
    stream
        .read_exact(&mut buffer)
        .unwrap_or_else(|error| panic!("expected {size} byte(s), failed: {error}"));
    buffer
}

/// Read until the connection has been quiet for a moment, answering everything decrypted.
fn read_quiet(stream: &mut TcpStream, reading: &mut common::cryptography::Arc4) -> Vec<u8> {
    let mut all = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return all,
            Ok(n) => {
                let mut plain = chunk[..n].to_vec();
                reading.apply(&mut plain);
                all.extend_from_slice(&plain);
            }
            Err(_) => return all,
        }
    }
}

/// The key exchange both connections start with: the client's public value, the server's seed, and
/// the two ciphers both sides derive from the master the seed carries.
fn exchange(stream: &mut TcpStream) -> (common::cryptography::Arc4, common::cryptography::Arc4) {
    let mut public = [0u8; PUBLIC_LEN];
    public[0] = 2;
    let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
    seed.extend_from_slice(&public);
    stream.write_all(&seed).expect("the seed message");
    let answer = read_exact(stream, SERVER_SEED_LEN);
    assert_eq!(
        &answer[..2],
        &handshake::SERVER_SEED_HEADER.to_le_bytes(),
        "the server's seed message"
    );
    let shared = handshake::shared_secret(&public).expect("the exchange's own arithmetic");
    let mut master = [0u8; MASTER_SECRET_LEN];
    for index in 0..MASTER_SECRET_LEN {
        master[index] = answer[2 + index] ^ shared[index];
    }
    let key = handshake::derive_key(&master);
    (
        common::cryptography::Arc4::new(&key),
        common::cryptography::Arc4::new(&key),
    )
}

/// Start the world program from the sibling package (cargo builds it if it is not built), and
/// answer the port it bound. The world is where the login's answer says it is: the realmlist's own
/// address, so the world program's port is its configuration's and this test binds it.
fn start_world() -> (Child, Receiver<String>, u16) {
    let mut child = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "worldserver",
            "--",
            "-c",
            "worldserver/worldserver.json",
        ])
        .current_dir("..")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cargo starts the world program");
    let (tx, rx) = channel();
    let stdout = child.stdout.take().expect("the program's stdout is piped");
    let stderr = child.stderr.take().expect("cargo's own output is piped");
    let drain = |stream: Box<dyn Read + Send>, tx: std::sync::mpsc::Sender<String>| {
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else {
                    return;
                };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
    };
    drain(Box::new(stdout), tx.clone());
    drain(Box::new(stderr), tx);
    let ready = wait_for(&rx, "READY: listening on", Duration::from_secs(120));
    // "READY: listening on 127.0.0.1:6113 as world 2; ..." - the address is the fourth word.
    let address = ready
        .split_whitespace()
        .nth(3)
        .expect("the READY line names the address it bound");
    let port: u16 = address
        .rsplit(':')
        .next()
        .expect("the address has a port")
        .trim_end_matches(';')
        .parse()
        .expect("the port is a number");
    (child, rx, port)
}

/// Remove the copies of map 148 a test's world opened, so the world database is as it was found.
fn remove_instances(map_id: u32) {
    let log = Log::new();
    let config = Config::load_json("../worldserver/worldserver.json", false, Policy::default())
        .expect("the world program's own configuration");
    let settings = read_settings(&config, DatabaseKind::World).expect("the world settings");
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::World, &settings, &log)
        .expect("the world pool opens");
    let map = i64::from(map_id);
    let _ = pool.query_with(
        "DELETE FROM \"instance\" WHERE \"map_id\"::bigint = $1",
        &[&map],
    );
}

/// The account the login presents: the owner's own row, read from the login database.
fn account_of(user: &str) -> [u8; 16] {
    let log = Log::new();
    let config = Config::load_json("authserver.json", false, Policy::default())
        .expect("the auth program's own configuration");
    let settings = read_settings(&config, DatabaseKind::Login).expect("the login settings");
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)
        .expect("the login pool opens");
    let rows = pool
        .query_with(
            "SELECT \"user_id\" FROM \"account\" WHERE \"username\" = $1",
            &[&user],
        )
        .expect("the account is readable");
    assert!(!rows.is_empty(), "the account {user} exists");
    let text: String = rows[0].get(0);
    game::characters::uuid_bytes(&text).expect("the account's uuid")
}

/// The travel rows the account has, as (map, district, language).
fn travel_rows(account: &[u8; 16]) -> Vec<(i64, i64, i64)> {
    let log = Log::new();
    let config = Config::load_json("authserver.json", false, Policy::default())
        .expect("the auth program's own configuration");
    let settings = read_settings(&config, DatabaseKind::Login).expect("the login settings");
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)
        .expect("the login pool opens");
    let account_text = game::characters::uuid_text(account);
    let rows = pool
        .query_with(
            "SELECT \"map_id\"::bigint, \"district\"::bigint, \"language\"::bigint \
             FROM \"travel\" WHERE \"account\" = $1 ORDER BY \"id\" DESC",
            &[&account_text],
        )
        .expect("the travel table is readable");
    rows.iter()
        .map(|row| {
            (
                row.get::<usize, i64>(0),
                row.get::<usize, i64>(1),
                row.get::<usize, i64>(2),
            )
        })
        .collect()
}

/// Remove every travel row the account has: the test's own hand-off.
fn remove_travel(account: &[u8; 16]) {
    let log = Log::new();
    let config = Config::load_json("authserver.json", false, Policy::default())
        .expect("the auth program's own configuration");
    let settings = read_settings(&config, DatabaseKind::Login).expect("the login settings");
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log)
        .expect("the login pool opens");
    let account_text = game::characters::uuid_text(account);
    let _ = pool.query_with(
        "DELETE FROM \"travel\" WHERE \"account\" = $1",
        &[&account_text],
    );
}

/// A character of this test's own, written for the account and removed afterwards: the login's list
/// is composed from the character database's rows, and a row the test owns proves it without
/// depending on what a client created by hand.
fn write_character(account: &[u8; 16], name: &str) -> String {
    let log = Log::new();
    let config = Config::load_json("authserver.json", false, Policy::default())
        .expect("the auth program's own configuration");
    let settings = read_settings(&config, DatabaseKind::Character).expect("the character settings");
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
        .expect("the character pool opens");
    let uuid = "00000000-0000-0000-0000-0000000000E2";
    let account_text = game::characters::uuid_text(account);
    pool.query_with("DELETE FROM \"characters\" WHERE \"uuid\" = $1", &[&uuid])
        .expect("the row is removable");
    pool.query_with(
        "INSERT INTO \"characters\" (\"uuid\", \"account_uuid\", \"name\", \"appearance\", \"primary_profession\", \"unlocked_professions\", \"campaign\", \"level\") \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[
            &uuid,
            &account_text,
            &name,
            &0i64,
            &1i64,
            &2i64,
            &0i64,
            &1i64,
        ],
    )
    .expect("the row is written");
    uuid.to_string()
}

/// Remove the row `write_character` wrote, with everything the world then wrote for it.
fn remove_character(uuid: &str) {
    let log = Log::new();
    let config = Config::load_json("authserver.json", false, Policy::default())
        .expect("the auth program's own configuration");
    let settings = read_settings(&config, DatabaseKind::Character).expect("the character settings");
    let mut pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
        .expect("the character pool opens");
    for statement in [
        "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
        "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
        "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
    ] {
        let _ = pool.query_with(statement, &[&uuid]);
    }
}

/// The 62-byte settings block the login carries for one character: the row's own, read back through
/// the same repository the login uses.
fn settings_blob_of(uuid: &str) -> Vec<u8> {
    let log = Log::new();
    let config = Config::load_json("authserver.json", false, Policy::default())
        .expect("the auth program's own configuration");
    let settings = read_settings(&config, DatabaseKind::Character).expect("the character settings");
    let pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
        .expect("the character pool opens");
    let characters = game::characters::DatabaseCharacters::new(
        std::sync::Arc::new(std::sync::Mutex::new(pool)),
        log,
    );
    characters
        .list()
        .into_iter()
        .find(|character| game::characters::uuid_text(&character.uuid) == uuid)
        .map(|character| character.settings.to_blob())
        .expect("the character's row is there")
}

/// The world program's own `WorldServerPort`: the hand-off's address is compared with the world
/// program's configuration, not with a number written into the test.
fn world_server_port() -> u16 {
    let config = Config::load_json("../worldserver/worldserver.json", false, Policy::default())
        .expect("the world program's own configuration");
    config
        .value::<u16>("WorldServerPort", 6113)
        .expect("the key is a port")
        .0
}

#[test]
#[ignore = "needs the databases the programs' configurations name, and the world's port free"]
fn an_end_to_end_login_and_load_against_the_live_programs() {
    let (mut child, lines, port) = start_auth();
    println!("the auth program is listening on port {port}");

    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("the program accepts the connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the socket reads with a timeout");

    // 1. The file check, before anything else on this socket: the client compares the answer.
    stream
        .write_all(&FILE_CHECK_QUESTION)
        .expect("the file check question");
    let answer = read_exact(&mut stream, 32);
    assert_eq!(
        &answer[..4],
        &FILE_CHECK_ANSWER_HEADER,
        "the file check answers its own header and length: {}",
        shared::network::socket::hex(&answer)
    );

    // 2. The auth version message: the bare little-endian header, the version, and twelve bytes.
    let mut version = vec![0u8; 16];
    version[0..4].copy_from_slice(&0x000C_0400u32.to_le_bytes());
    version[4..8].copy_from_slice(&39167u32.to_le_bytes());
    stream
        .write_all(&version)
        .expect("the auth version message");

    // 3. The key exchange: the client's public value, the server's seed, and the master secret
    // recovered from it - the same arithmetic the two programs run.
    let (mut reading, mut sending) = exchange(&mut stream);

    // The greeting: the session info and the heartbeat, sixteen bytes, the first scrambled bytes.
    let mut greeting = read_exact(&mut stream, 16);
    reading.apply(&mut greeting);
    assert_eq!(
        &greeting[..2],
        &[0x01, 0x00],
        "the session info: {}",
        shared::network::socket::hex(&greeting)
    );

    // 3b. The client's own keep-alive (`AUTH_CMSG_HEARTBEAT`, `0x8000`: the header and a dword) is
    // answered with the server's heartbeat, tick 16 - the reference's constant
    // (`ASURA/crates/asura-auth/src/lib.rs:307-312`).
    let mut beat = vec![0u8; 6];
    beat[0..2].copy_from_slice(&0x8000u16.to_le_bytes());
    sending.apply(&mut beat);
    stream.write_all(&beat).expect("the client heartbeat");
    let mut answered_beat = read_exact(&mut stream, 6);
    reading.apply(&mut answered_beat);
    assert_eq!(
        &answered_beat[..2],
        &[0x00, 0x00],
        "the server's heartbeat: {}",
        shared::network::socket::hex(&answered_beat)
    );
    assert_eq!(
        &answered_beat[2..6],
        &16u32.to_le_bytes(),
        "the reference's tick is 16"
    );

    // 3c. The computer info (`0x8001`: two counted strings) is recorded and answered with nothing.
    let mut info = vec![0u8; 6];
    info[0..2].copy_from_slice(&0x8001u16.to_le_bytes());
    sending.apply(&mut info);
    stream.write_all(&info).expect("the computer info");

    // 3d. The computer hash the client waits on (`0x8002`: the header, a dword and sixteen bytes) is
    // answered with the session info again - and with the **same salt** the greeting carried, because
    // that salt is what the client's cipher is derived from
    // (`ASURA/crates/asura-auth/src/lib.rs:256-275`).
    let salt = u32::from_le_bytes([greeting[2], greeting[3], greeting[4], greeting[5]]);
    let mut hash = vec![0u8; 22];
    hash[0..2].copy_from_slice(&0x8002u16.to_le_bytes());
    sending.apply(&mut hash);
    stream.write_all(&hash).expect("the computer hash");
    let mut answered_hash = read_exact(&mut stream, 10);
    reading.apply(&mut answered_hash);
    assert_eq!(
        &answered_hash[..2],
        &[0x01, 0x00],
        "the session info: {}",
        shared::network::socket::hex(&answered_hash)
    );
    assert_eq!(
        &answered_hash[2..6],
        &salt.to_le_bytes(),
        "the salt already on the wire, not a second one"
    );

    // 4. The portal login: the account's own id, a session id, and the two counted strings. The
    // wire is 54 bytes - the measured login message - not the 150 that the game-side table declares
    // as a maximum: the two counted strings carry what they carry, and the server's walk follows
    // them.
    let account = account_of("user1@example.com");
    let character_name = "E2E Login One";
    let created = write_character(&account, character_name);
    let mut login = vec![0u8; 54];
    login[0..2].copy_from_slice(&0x8038u16.to_le_bytes());
    login[2..6].copy_from_slice(&7u32.to_le_bytes());
    login[6..22].copy_from_slice(&account);
    login[22..38].copy_from_slice(&[0u8; 16]);
    // The two counted strings: four units and two units, as the trace's own login carried.
    let name: Vec<u16> = "user".encode_utf16().collect();
    login[38..40].copy_from_slice(&(name.len() as u16).to_le_bytes());
    for (index, unit) in name.iter().enumerate() {
        let at = 40 + index * 2;
        login[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    let at = 40 + name.len() * 2;
    let account_type: Vec<u16> = "ok".encode_utf16().collect();
    login[at..at + 2].copy_from_slice(&(account_type.len() as u16).to_le_bytes());
    for (index, unit) in account_type.iter().enumerate() {
        let offset = at + 2 + index * 2;
        login[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(login.len(), 54, "the login message is 54 bytes");
    let mut scrambled = login;
    sending.apply(&mut scrambled);
    stream.write_all(&scrambled).expect("the login message");

    // 5. The answer: the character list, which the login composes from the character database.
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("the socket reads with a timeout");
    let answered = read_quiet(&mut stream, &mut reading);
    while let Ok(line) = lines.try_recv() {
        if line.contains("Connection from") || line.contains("login") || line.contains("listed") {
            println!("auth> {line}");
        }
    }
    assert!(
        answered.len() >= 2,
        "the login answer is there at all: {}",
        shared::network::socket::hex(&answered)
    );
    assert_eq!(
        &answered[..2],
        &[0x07, 0x00],
        "the first message is the character list: {}",
        shared::network::socket::hex(&answered[..32.min(answered.len())])
    );
    // The character's name is in the list, as a counted UTF-16 string: the count precedes it.
    let name_units: Vec<u8> = character_name
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert!(
        answered
            .windows(name_units.len())
            .any(|window| window == name_units.as_slice()),
        "the answer names the account's character '{character_name}': {}",
        shared::network::socket::hex(&answered)
    );
    // And the 62-byte settings block is the row's own: the frame carries it right after the name,
    // counted. The row is read back through the same repository the login uses.
    let expected_settings = settings_blob_of(&created);
    let settings_at = 30 + name_units.len();
    assert_eq!(
        &answered[settings_at - 2..settings_at],
        &(game::characters::CHARACTER_SETTINGS_LEN as u16).to_le_bytes(),
        "the settings block's count"
    );
    assert_eq!(
        &answered[settings_at..settings_at + game::characters::CHARACTER_SETTINGS_LEN],
        expected_settings.as_slice(),
        "the settings block the login carries is the row's own"
    );

    // 5b. The district list the character screen's world map asks for (R16): a transaction, a game
    // type, a territory and a mission; answered with the one district a world opens to begin with.
    let mut query = vec![0u8; 18];
    query[0..2].copy_from_slice(&0x800Bu16.to_le_bytes());
    query[2..6].copy_from_slice(&42u32.to_le_bytes());
    query[10..14].copy_from_slice(&1u32.to_le_bytes());
    let mut scrambled = query;
    sending.apply(&mut scrambled);
    stream
        .write_all(&scrambled)
        .expect("the district list query");
    let answered = read_quiet(&mut stream, &mut reading);
    assert_eq!(
        answered.len(),
        29,
        "the district list is the reference's own shape: {}",
        shared::network::socket::hex(&answered)
    );
    assert_eq!(&answered[..2], &[0x1A, 0x00], "the counts message");
    assert_eq!(
        &answered[2..6],
        &42u32.to_le_bytes(),
        "the transaction is echoed"
    );
    assert_eq!(answered[6], 11, "eleven language pairs");
    assert_eq!(&answered[7..9], &[0, 1], "language 0 has one district");

    // 5c. The character click (`0x800A`): the client picks a character on the list, and the login
    // answers the request with status 0 when the name is on the account.
    let mut choose = vec![0u8; 8 + character_name.len() * 2];
    choose[0..2].copy_from_slice(&0x800Au16.to_le_bytes());
    choose[2..6].copy_from_slice(&9u32.to_le_bytes());
    choose[6..8].copy_from_slice(&(character_name.encode_utf16().count() as u16).to_le_bytes());
    for (index, unit) in character_name.encode_utf16().enumerate() {
        let at = 8 + index * 2;
        choose[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    let mut scrambled = choose;
    sending.apply(&mut scrambled);
    stream.write_all(&scrambled).expect("the character click");
    let answered = read_quiet(&mut stream, &mut reading);
    assert_eq!(
        answered.len(),
        10,
        "the request response: {}",
        shared::network::socket::hex(&answered)
    );
    assert_eq!(&answered[..2], &[0x03, 0x00], "the request response");
    assert_eq!(
        &answered[2..6],
        &9u32.to_le_bytes(),
        "the request is echoed"
    );
    assert_eq!(
        &answered[6..10],
        &0u32.to_le_bytes(),
        "the name is on the account"
    );

    // 5d. What this client really sends next: `0x8029` naming the **creation map** as a place to
    // stand - request 6, map 0, type 0, district 0, language -1, measured 2026-11-12 (`Auth.log`). The
    // character the click chose stands in map 148, and that is where the login has to send it: a
    // client told the creation instance again waits on a loading screen that never ends
    // (`game/src/Maps/Placement.rs`).
    remove_travel(&account);
    let mut place = vec![0u8; 26];
    place[0..2].copy_from_slice(&0x8029u16.to_le_bytes());
    place[2..6].copy_from_slice(&6u32.to_le_bytes());
    place[22..26].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut scrambled = place;
    sending.apply(&mut scrambled);
    stream
        .write_all(&scrambled)
        .expect("the map request after a character click");
    let answered = read_quiet(&mut stream, &mut reading);
    assert!(
        answered.len() >= 52,
        "the answer is the info and the response: {}",
        shared::network::socket::hex(&answered)
    );
    // The info message: header, the request, the map token, then the map id.
    assert_eq!(&answered[..2], &[0x09, 0x00], "the game server info");
    assert_eq!(
        &answered[2..6],
        &6u32.to_le_bytes(),
        "the request is echoed"
    );
    assert_eq!(
        &answered[10..14],
        &148u32.to_le_bytes(),
        "the client is sent to the character's own map, not the creation instance: {}",
        shared::network::socket::hex(&answered[..answered.len().min(44)])
    );
    // And the hand-off the world claims names that map too.
    let travels = travel_rows(&account);
    assert_eq!(
        travels.first().copied(),
        Some((148, 0, u32::MAX as i64)),
        "the travel the login recorded: {travels:?}"
    );

    // 6. The map request: the client names the map it wants, and the login answers with the world it
    // was told to dial and records the hand-off the world claims (R9). The wire is the traced six
    // dwords: request, map type, map, region, district, language.
    remove_travel(&account);
    let mut request = vec![0u8; 26];
    request[0..2].copy_from_slice(&0x8029u16.to_le_bytes());
    request[2..6].copy_from_slice(&3u32.to_le_bytes());
    request[6..10].copy_from_slice(&1u32.to_le_bytes());
    request[10..14].copy_from_slice(&148u32.to_le_bytes());
    request[18..22].copy_from_slice(&1u32.to_le_bytes());
    let mut scrambled = request;
    sending.apply(&mut scrambled);
    stream.write_all(&scrambled).expect("the map request");
    let answered = read_quiet(&mut stream, &mut reading);
    assert!(
        answered.len() >= 52,
        "the map answer is 42 bytes and the response 10: {}",
        shared::network::socket::hex(&answered)
    );
    assert_eq!(
        &answered[..2],
        &[0x09, 0x00],
        "the game server info: {}",
        shared::network::socket::hex(&answered[..answered.len().min(24)])
    );
    assert_eq!(
        &answered[42..44],
        &[0x03, 0x00],
        "the request response follows the info"
    );
    // The address it names is the world's own: the family, the port big-endian, and the address -
    // the bytes the client dials. The port is the world program's `WorldServerPort`.
    let world_port = world_server_port();
    let host = [
        0x02,
        0x00,
        (world_port >> 8) as u8,
        (world_port & 0xFF) as u8,
        0x7F,
        0x00,
        0x00,
        0x01,
    ];
    assert!(
        answered
            .windows(host.len())
            .any(|window| window == host.as_slice()),
        "the info names 127.0.0.1:{world_port}: {}",
        shared::network::socket::hex(&answered[..answered.len().min(44)])
    );
    // The login recorded the hand-off for the world it names: the map it was asked for, and the
    // district and language the request carried.
    let travels = travel_rows(&account);
    assert_eq!(
        travels.first().copied(),
        Some((148, 1, 0)),
        "the travel the login recorded: {travels:?}"
    );

    // 7. The world: the answer named the address and the tokens, and the connection that presents
    // them claims the travel the login recorded (R9). The block it answers is map 148's, in the
    // district and language the request carried - the seam between the two programs, end to end.
    let map_id = u32::from_le_bytes([answered[10], answered[11], answered[12], answered[13]]);
    assert_eq!(map_id, 148, "the answer names the map the request named");
    let player_token = u32::from_le_bytes([answered[38], answered[39], answered[40], answered[41]]);
    let (mut world, world_lines, world_port) = start_world();
    assert_eq!(
        world_port,
        world_server_port(),
        "the world is where the login said it is"
    );

    let created_id = game::characters::uuid_bytes(&created).expect("the character's uuid");
    let version = GameVersion {
        version: 39167,
        h0008: 1,
        map_token: 2,
        map_id: 0,
        player_token,
        account,
        character: created_id,
        h0038: 0,
        h003c: 0,
    };
    let mut world_stream =
        TcpStream::connect(("127.0.0.1", world_port)).expect("the world accepts the connection");
    world_stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the socket reads with a timeout");
    world_stream
        .write_all(&version.encode())
        .expect("the world's version message");
    let (mut world_reading, _world_sending) = exchange(&mut world_stream);

    // The block: head, player-data start, the name, the instance's record. Its record names map
    // 148 - the travel's, not the version message's own - which is the hand-off working.
    let block_size = instance_packets::instance_load_head().len()
        + instance_packets::instance_player_data_start().len()
        + instance_packets::instance_load_player_name(character_name).len()
        + instance_packets::instance_load_info(1, 148, 0, 1, 0, 0).len();
    let mut block = read_exact(&mut world_stream, block_size);
    world_reading.apply(&mut block);
    assert_eq!(
        &block[..2],
        &[0x7C, 0x01],
        "the world's load block: {}",
        shared::network::socket::hex(&block)
    );
    let name_at = instance_packets::instance_load_head().len()
        + instance_packets::instance_player_data_start().len();
    let name_size = instance_packets::instance_load_player_name(character_name).len();
    assert_eq!(
        &block[name_at + name_size..],
        instance_packets::instance_load_info(1, 148, 1, 1, 0, 0).as_slice(),
        "the travel decided the map: 148, the explorable byte 1 the reference hardcodes, district 1"
    );
    // The hand-off row is consumed by the claim: a second claim finds nothing.
    assert!(
        travel_rows(&account).is_empty(),
        "the world claimed the travel the login recorded"
    );
    let _ = world_stream.shutdown(std::net::Shutdown::Both);
    let _ = world.kill();
    let _ = world.wait();
    while let Ok(line) = world_lines.try_recv() {
        if line.contains("travel record is claimed") || line.contains("is loading") {
            println!("world> {line}");
        }
    }
    remove_instances(148);

    // 8. A character is deleted by name (`AUTH_CMSG_DELETE_CHARACTER`, `0x8007`: the header, a request
    // dword and the counted name). The row must be one of this account's, and the answer is a request
    // response with status 0 - the reference's own answer
    // (`ASURA/crates/asura-auth/src/lib.rs:409-474`).
    let units: Vec<u16> = character_name.encode_utf16().collect();
    let mut delete = vec![0u8; 6 + 2 + units.len() * 2];
    delete[0..2].copy_from_slice(&0x8007u16.to_le_bytes());
    delete[2..6].copy_from_slice(&0x42u32.to_le_bytes());
    delete[6..8].copy_from_slice(&(units.len() as u16).to_le_bytes());
    for (index, unit) in units.iter().enumerate() {
        let at = 8 + index * 2;
        delete[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    sending.apply(&mut delete);
    stream.write_all(&delete).expect("the character delete");
    let mut answered_delete = read_exact(&mut stream, 10);
    reading.apply(&mut answered_delete);
    assert_eq!(&answered_delete[..2], &[0x03, 0x00], "a request response");
    assert_eq!(
        &answered_delete[2..6],
        &0x42u32.to_le_bytes(),
        "the request it answers"
    );
    assert_eq!(
        u32::from_le_bytes([
            answered_delete[6],
            answered_delete[7],
            answered_delete[8],
            answered_delete[9]
        ]),
        0,
        "status 0: the character is gone"
    );
    {
        // The row is gone from the database, read back through the same repository the login uses.
        let log = Log::new();
        let config = Config::load_json("authserver.json", false, Policy::default())
            .expect("the auth program's own configuration");
        let settings =
            read_settings(&config, DatabaseKind::Character).expect("the character settings");
        let pool = DatabaseWorkerPool::open(DatabaseKind::Character, &settings, &log)
            .expect("the character pool opens");
        let characters = game::characters::DatabaseCharacters::new(
            std::sync::Arc::new(std::sync::Mutex::new(pool)),
            log,
        );
        let remaining = characters.for_account(&game::characters::uuid_text(&account));
        assert!(
            !remaining.iter().any(|row| row.name == character_name),
            "the row the delete removed is gone from the database: {remaining:?}"
        );
    }

    // A name this account does not hold is not deleted: the reference answers 23
    // (`GM_ERROR_INVALID_CHARNAME`, `ASURA/crates/asura-auth/src/lib.rs:426-429`).
    let unknown: Vec<u16> = "No Such Character".encode_utf16().collect();
    let mut refused = vec![0u8; 6 + 2 + unknown.len() * 2];
    refused[0..2].copy_from_slice(&0x8007u16.to_le_bytes());
    refused[2..6].copy_from_slice(&0x43u32.to_le_bytes());
    refused[6..8].copy_from_slice(&(unknown.len() as u16).to_le_bytes());
    for (index, unit) in unknown.iter().enumerate() {
        let at = 8 + index * 2;
        refused[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    sending.apply(&mut refused);
    stream.write_all(&refused).expect("the refused delete");
    let mut answered_refused = read_exact(&mut stream, 10);
    reading.apply(&mut answered_refused);
    assert_eq!(&answered_refused[..2], &[0x03, 0x00], "a request response");
    assert_eq!(
        &answered_refused[2..6],
        &0x43u32.to_le_bytes(),
        "the request it answers"
    );
    assert_eq!(
        u32::from_le_bytes([
            answered_refused[6],
            answered_refused[7],
            answered_refused[8],
            answered_refused[9]
        ]),
        23,
        "status 23: no such character on this account"
    );

    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = child.kill();
    let _ = child.wait();
    remove_character(&created);
    remove_travel(&account);
    while let Ok(line) = lines.try_recv() {
        if line.contains("Connection from") || line.contains("login") || line.contains("listed") {
            println!("auth> {line}");
        }
    }
}
