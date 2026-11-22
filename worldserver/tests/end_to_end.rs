//! The end-to-end run: the client's conversation against the world **program**.
//!
//! The session tests in `game` play the conversation against a socket manager a test builds. This
//! test plays it against the binary `worldserver` itself: it spawns the program from this package,
//! waits for its own READY line, and drives a creation screen and a load of map 148 exactly as the
//! manual test says a client does - the version message, the key exchange, the creation confirm, the
//! asks the load is built from, the travel request and the ticks. What is proven is the program: its
//! configuration reading, its listener, its three pools and its sessions, together - and the item
//! stream it answers with is compared byte for byte with the stream the character's own rows draw.
//!
//! It is `#[ignore]` because it needs the databases the program's configuration names, and it is a
//! **live** test: it writes a character row and an instance row, then removes both.
//!
//! Run with `cargo test -p worldserver --test end_to_end -- --ignored --nocapture`, with the two
//! servers of the same databases not running - the live world's sweep would otherwise dispose the
//! instance row this test checks. The world's console output must carry its debug lines too (the
//! `Console` appender at level 5 and the `server` logger at 5): the test reads a load's size from the
//! program's own "is loading" line, the same line an operator reads in `Server.log`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::configuration::{Config, Policy};
use common::logging::Log;
use database::database::{DatabaseKind, DatabaseWorkerPool, read_settings};
use game::server::packets::instance_packets;
use game::server::protocol::GameVersion;
use shared::cryptography::handshake::{
    self, CLIENT_SEED_HEADER, MASTER_SECRET_LEN, PUBLIC_LEN, SERVER_SEED_LEN,
};

/// The port the program is told to bind: the environment variable wins over the file, and `0` means
/// the operating system picks one - which the program then prints in its READY line.
const PORT_ENV: &str = "AC_WORLD_SERVER_PORT";
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

/// Start the world program with its own port, and answer the port it bound.
fn start_world() -> (Child, Receiver<String>, u16) {
    let exe = env!("CARGO_BIN_EXE_worldserver");
    let mut child = Command::new(exe)
        .arg("-c")
        .arg("worldserver/worldserver.json")
        .current_dir("..")
        .env(PORT_ENV, "0")
        .env(STATUS_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the world program starts");
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
    // "READY: listening on 127.0.0.1:41234 as world 2; ..." - the address is the fourth word.
    let address = ready
        .split_whitespace()
        .nth(3)
        .expect("the READY line names the address it bound");
    let port: u16 = address
        .rsplit(':')
        .next()
        .expect("the address has a port")
        .parse()
        .expect("the port is a number");
    (child, rx, port)
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

/// Read exactly `size` bytes, or fail saying how many arrived.
fn read_exact(stream: &mut TcpStream, size: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; size];
    stream
        .read_exact(&mut buffer)
        .unwrap_or_else(|error| panic!("expected {size} byte(s), failed: {error}"));
    buffer
}

/// Read complete frames until the header `terminal` has been seen `count` times, answering every
/// complete frame read, decrypted. The walker is the server table's own: it knows where each message
/// ends, so a counted field is followed to its own length rather than to a guessed number.
fn read_frames(
    stream: &mut TcpStream,
    reading: &mut common::cryptography::Arc4,
    terminal: u16,
    count: usize,
) -> Vec<(u16, Vec<u8>)> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut frames: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut seen = 0;
    let mut chunk = [0u8; 4096];
    loop {
        while !buffer.is_empty() {
            match game::server::protocol::decode_game_message(&buffer, false) {
                Ok((frame, used)) => {
                    let bytes = buffer[..used].to_vec();
                    let header = frame.header();
                    buffer.drain(..used);
                    if header == terminal {
                        seen += 1;
                    }
                    frames.push((header, bytes));
                    if header == terminal && seen == count {
                        return frames;
                    }
                }
                Err(_) => break,
            }
        }
        let read = match stream.read(&mut chunk) {
            Ok(read) => read,
            Err(error) => {
                let walked: Vec<u16> = frames.iter().map(|(header, _)| *header).collect();
                panic!(
                    "the answer stalls with {error}; walked {walked:04X?}; {} bytes left: {}",
                    buffer.len(),
                    shared::network::socket::hex(&buffer[..buffer.len().min(64)])
                );
            }
        };
        assert_ne!(
            read, 0,
            "the program closed the connection before {terminal:#06X}"
        );
        let mut plain = chunk[..read].to_vec();
        reading.apply(&mut plain);
        buffer.extend_from_slice(&plain);
    }
}

/// The version message and the key exchange, answering the cipher each direction reads with.
fn open_connection(
    port: u16,
    version: GameVersion,
) -> (
    TcpStream,
    common::cryptography::Arc4,
    common::cryptography::Arc4,
) {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("the program accepts the connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the socket reads with a timeout");
    stream
        .write_all(&version.encode())
        .expect("the version message is written");
    let mut public = [0u8; PUBLIC_LEN];
    public[0] = 2;
    let mut seed = CLIENT_SEED_HEADER.to_le_bytes().to_vec();
    seed.extend_from_slice(&public);
    stream
        .write_all(&seed)
        .expect("the seed message is written");
    let answer = read_exact(&mut stream, SERVER_SEED_LEN);
    assert_eq!(
        &answer[..2],
        &handshake::SERVER_SEED_HEADER.to_le_bytes(),
        "the server's seed message"
    );
    // The master is what the server's twenty bytes turn into once the same XOR is applied on this
    // side: master = answer[..] XOR shared[..], and the key is the client's digest of the master.
    let shared = handshake::shared_secret(&public).expect("the exchange's own arithmetic");
    let mut master = [0u8; MASTER_SECRET_LEN];
    for index in 0..MASTER_SECRET_LEN {
        master[index] = answer[2 + index] ^ shared[index];
    }
    let key = handshake::derive_key(&master);
    let reading = common::cryptography::Arc4::new(&key);
    let sending = common::cryptography::Arc4::new(&key);
    (stream, reading, sending)
}

/// What the creation conversation is answered: the block, then the confirm, which writes the row.
fn create_character(port: u16, account: [u8; 16], name: &str) -> ([u8; 16], String) {
    let version = GameVersion {
        version: 39167,
        h0008: 1,
        map_token: 2,
        map_id: 0,
        player_token: 0x1234,
        account,
        character: [0; 16],
        h0038: 0,
        h003c: 0,
    };
    let (mut stream, mut reading, mut sending) = open_connection(port, version);

    // The creation block: the load head and the creation start, in that order.
    let mut block = read_exact(&mut stream, instance_packets::creation_load_block().len());
    reading.apply(&mut block);
    assert_eq!(
        &block[..2],
        &[0x7C, 0x01],
        "the creation instance's load head: {}",
        shared::network::socket::hex(&block)
    );
    assert_eq!(&block[6..8], &[0x89, 0x01], "the creation start");

    // The creation screen's own asks, in the order a client sends them: its player's data first -
    // the item stream and the bags the record list draws - and then the armoury, its PvP half.
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("the socket reads with a timeout");
    let mut ask = vec![0x89, 0x80];
    sending.apply(&mut ask);
    stream.write_all(&ask).expect("the player-data ask");
    let player = read_quiet(&mut stream, &mut reading);
    assert!(
        player.starts_with(&[0x86, 0x01]),
        "the player-data stream opens: {}",
        shared::network::socket::hex(&player[..player.len().min(24)])
    );
    assert!(
        player.windows(2).any(|window| window == [0x61, 0x01]),
        "the creation set's items are in it: {}",
        shared::network::socket::hex(&player[..player.len().min(24)])
    );
    let mut ask = vec![0x8A, 0x80];
    sending.apply(&mut ask);
    stream.write_all(&ask).expect("the armoury ask");
    let armory = read_quiet(&mut stream, &mut reading);
    assert!(
        armory.starts_with(&[0x1D, 0x00]),
        "the armoury opens with the skills half: {}",
        shared::network::socket::hex(&armory)
    );
    for header in [[0x18u8, 0x00], [0x1B, 0x00]] {
        assert!(
            armory.windows(2).any(|window| window == header),
            "the armoury carries 0x{header:02X?}: {}",
            shared::network::socket::hex(&armory)
        );
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the socket reads with a timeout");

    // The confirm, in the client's own size: sixteen bytes plus twice the name
    // (`docs/game-rules.md`, E73) - header, count, name, the packed appearance word, eight more.
    let units: Vec<u16> = name.encode_utf16().collect();
    assert!(units.len() <= 20, "the name fits the client's field");
    let appearance_at = 4 + units.len() * 2;
    let mut confirm = vec![0u8; 16 + units.len() * 2];
    confirm[0..2].copy_from_slice(&0x808Bu16.to_le_bytes());
    confirm[2..4].copy_from_slice(&(units.len() as u16).to_le_bytes());
    for (index, unit) in units.iter().enumerate() {
        let at = 4 + index * 2;
        confirm[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    confirm[appearance_at..appearance_at + 4].copy_from_slice(&0x00AB_CDEFu32.to_le_bytes());
    let mut scrambled = confirm;
    sending.apply(&mut scrambled);
    stream
        .write_all(&scrambled)
        .expect("the confirm is written");

    // `0x0188`: the character's id, its name, and the settings block.
    let size = 2 + 16 + 2 + units.len() * 2 + 2 + 2 + game::characters::CHARACTER_SETTINGS_LEN;
    let mut answered = read_exact(&mut stream, size);
    reading.apply(&mut answered);
    assert_eq!(
        &answered[..2],
        &0x0188u16.to_le_bytes(),
        "the creation succeeded: {}",
        shared::network::socket::hex(&answered)
    );
    let mut id = [0u8; 16];
    id.copy_from_slice(&answered[2..18]);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    (id, game::characters::uuid_text(&id))
}

/// The three pools the program's configuration names: characters, world and login.
type Pools = (
    Arc<Mutex<DatabaseWorkerPool>>,
    Arc<Mutex<DatabaseWorkerPool>>,
    Arc<Mutex<DatabaseWorkerPool>>,
);

/// Open the pools the program's configuration names: characters, world and login.
fn open_pools() -> Pools {
    let log = Log::new();
    let config = Config::load_json("worldserver.json", false, Policy::default())
        .expect("the world program's own configuration");
    let mut opened = Vec::new();
    for kind in [
        DatabaseKind::Character,
        DatabaseKind::World,
        DatabaseKind::Login,
    ] {
        let settings = read_settings(&config, kind).expect("the pool's settings");
        let mut pool = DatabaseWorkerPool::open(kind, &settings, &log)
            .unwrap_or_else(|error| panic!("the {kind:?} pool opens: {error}"));
        pool.query_with("SELECT 1", &[])
            .unwrap_or_else(|error| panic!("the {kind:?} pool answers: {error}"));
        opened.push(Arc::new(Mutex::new(pool)));
    }
    let login = opened.pop().expect("three pools");
    let world = opened.pop().expect("three pools");
    let characters = opened.pop().expect("three pools");
    (characters, world, login)
}

#[test]
#[ignore = "needs the databases the world program's configuration names"]
fn an_end_to_end_creation_and_load_against_the_live_program() {
    let name = "E2E Kryta One";
    let (mut child, _lines, port) = start_world();
    println!("the world program is listening on port {port}");

    let (characters, world_pool, login) = open_pools();
    let created = Arc::new(Mutex::new(None::<String>));
    let created_for_cleanup = Arc::clone(&created);
    let copy = Arc::new(Mutex::new(None::<i64>));
    let copy_for_cleanup = Arc::clone(&copy);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // The account the character is written under: the login database's own column. The world
        // does not verify it beyond writing it into the row, and the login's list would answer it.
        let account_text: String = {
            let mut pool = login.lock().unwrap();
            let rows = pool
                .query_with(
                    "SELECT \"user_id\" FROM \"account\" WHERE \"username\" = 'user1@example.com'",
                    &[],
                )
                .expect("the account is readable");
            assert!(
                !rows.is_empty(),
                "the owner's account exists: user1@example.com"
            );
            rows[0].get(0)
        };
        let account = game::characters::uuid_bytes(&account_text).expect("the account's uuid");
        println!("the character is written under account {account_text}");

        // A row this test left behind by an interrupted earlier run would make the confirm refuse
        // the name, so the name is cleared first: the same three tables the cleanup removes from.
        {
            let mut pool = characters.lock().unwrap();
            let rows = pool
                .query_with(
                    "SELECT \"uuid\" FROM \"characters\" WHERE \"name\" = $1",
                    &[&name],
                )
                .expect("the character table is readable");
            for row in &rows {
                let uuid: String = row.get(0);
                for statement in [
                    "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
                    "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
                    "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
                ] {
                    pool.query_with(statement, &[&uuid])
                        .expect("the stale row is removable");
                }
            }
        }

        // The creation screen, against the program.
        let (id, uuid_text) = create_character(port, account, name);
        *created.lock().unwrap() = Some(uuid_text.clone());
        println!("the creation wrote {uuid_text}");
        assert_ne!(id, [0u8; 16], "the answer carries the character's id");

        // The row is there, with its starting items and its starting unlock - the write is the
        // program's own, through the same repository the load draws from.
        {
            let repository =
                game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new());
            let held = repository
                .items_of(&uuid_text)
                .expect("the items are readable");
            println!("the confirm wrote {} item row(s)", held.len());
            assert!(!held.is_empty(), "the confirm wrote the starting items");
            let unlocked = repository
                .unlocked_maps(&uuid_text)
                .expect("the unlocks are readable");
            assert_eq!(unlocked, vec![148], "the starting unlock is map 148 (R12)");
            // And the row names where the character stands, which is what the login's list reads
            // back: a character with no map is one the client can only be sent into the creation
            // instance for.
            let start_map: i64 = {
                let mut pool = characters.lock().unwrap();
                let rows = pool
                    .query_with(
                        "SELECT \"last_outpost\"::bigint FROM \"characters\" WHERE \"uuid\" = $1",
                        &[&uuid_text],
                    )
                    .expect("the character row is readable");
                rows[0].get(0)
            };
            assert_eq!(
                start_map, 148,
                "it starts in the outpost it was created for"
            );
        }

        // The load of map 148, in the measured order, against the program. The version message
        // names the map itself: this connection presents no travel record, so the map is its own.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: 148,
            player_token: 0x2345,
            account,
            character: id,
            h0038: 0,
            h003c: 0,
        };
        let (mut stream, mut reading, mut sending) = open_connection(port, version);

        // The block: head, player-data start, the name, the instance's record, the exploration grid
        // and the instance table - right after the record, which is where a live server sends them
        // (`ASURA/crates/asura-world/src/map.rs:22-58`).
        let grid = game::maps::cartography::init_frames(game::maps::cartography::grid_for(148));
        let hosted = game::maps::DatabaseMaps::new(Arc::clone(&world_pool), Log::new())
            .load()
            .expect("the maps are readable")
            .0;
        let (table, notes) = game::maps::instance_table::table_for(&hosted);
        assert!(
            notes.is_empty(),
            "no map id fails the wire's width: {notes:?}"
        );
        let table_frames = table.frames();
        assert!(
            table.pairs.contains(&(148, 0xBB14_F539)),
            "map 148's own value is in the table: {:?}",
            table.pairs
        );
        let block_size = instance_packets::instance_load_head().len()
            + instance_packets::instance_player_data_start().len()
            + instance_packets::instance_load_player_name(name).len()
            + instance_packets::instance_load_info(1, 148, 0, 1, 0, 0).len()
            + grid.iter().map(Vec::len).sum::<usize>()
            + table_frames.iter().map(Vec::len).sum::<usize>();
        let mut block = read_exact(&mut stream, block_size);
        reading.apply(&mut block);
        assert_eq!(
            &block[..2],
            &[0x7C, 0x01],
            "the outpost's load head, not the creation instance's: {}",
            shared::network::socket::hex(&block)
        );
        let name_size = instance_packets::instance_load_player_name(name).len();
        let name_at = instance_packets::instance_load_head().len()
            + instance_packets::instance_player_data_start().len();
        assert_eq!(
            &block[name_at..name_at + name_size],
            instance_packets::instance_load_player_name(name).as_slice(),
            "the character's own name, from its row"
        );
        assert_eq!(
            &block[name_at + name_size..name_at + name_size + 16],
            instance_packets::instance_load_info(1, 148, 1, 1, 0, 0).as_slice(),
            "the instance's record: map 148, the explorable byte 1 the reference hardcodes, district 1"
        );
        // And the grid: the announcement, then one slice of the compressed bands - every band empty,
        // which is what a character that has explored nothing has.
        let grid_at = name_at + name_size + 16;
        assert_eq!(
            &block[grid_at..grid_at + 2],
            &[0x8B, 0x00],
            "the grid's dims"
        );
        assert_eq!(
            &block[grid_at..grid_at + 14],
            grid[0].as_slice(),
            "64 x 128 cells and sixteen compressed bytes"
        );
        assert_eq!(&block[grid_at + 14..grid_at + 16], &[0x8A, 0x00], "a slice");
        // And the instance table: the destinations the client can resolve, in the batches a load uses.
        let grid_size = grid.iter().map(Vec::len).sum::<usize>();
        let table_at = grid_at + grid_size;
        assert_eq!(
            &block[grid_at + 14..table_at],
            grid[1].as_slice(),
            "the slice is the blob in a counted, fixed array"
        );
        assert_eq!(
            &block[table_at..],
            table_frames.concat().as_slice(),
            "the instance table, in the batch a load sends"
        );

        // The ping pair, unasked.
        let mut ping = read_exact(&mut stream, 8);
        reading.apply(&mut ping);
        assert_eq!(&ping[..2], &[0x0C, 0x00], "the ping request");
        assert_eq!(&ping[2..4], &[0x0D, 0x00], "its reply");

        // What the items answer must be, drawn from the character's own rows: the creation setting's
        // bags, the templates the world database holds, and the items the confirm wrote. This is the
        // block the client sees, so it is compared byte for byte, not only by its headers.
        let tables = game::creation::Tables::load(
            "../config/World/Vanilla/creation/inventory.json",
            "../config/World/Vanilla/creation/equipment.json",
            Some(std::path::Path::new(
                "../config/World/Vanilla/creation/worn_slots.json",
            )),
            &Log::new(),
        )
        .expect("the creation data");
        let templates = game::data_stores::DatabaseItems::new(Arc::clone(&world_pool), Log::new())
            .list()
            .expect("the item templates are readable");
        let held = game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new())
            .items_of(&uuid_text)
            .expect("the character's items are readable");
        assert!(!held.is_empty(), "the load had items to draw");
        let inventory = game::creation::inventory_from_held(
            game::creation::bags(&tables),
            &held,
            &templates,
            &Log::new(),
        );
        let mut expected_items = game::creation::load_item_stream(&inventory, 0);
        expected_items.push(game::server::packets::misc_packets::hard_mode_unlocked(1));
        expected_items.push(game::server::packets::character_packets::character_update_info(""));
        expected_items.push(game::server::packets::misc_packets::map_update_current(
            148, 0,
        ));
        expected_items.push(instance_packets::ready_for_map_spawn(0));
        expected_items.push(instance_packets::instance_manifest_phase(0));
        expected_items.push(instance_packets::instance_manifest_phase(1));
        expected_items.push(instance_packets::instance_manifest_done(2, 877, 0));
        expected_items.push(instance_packets::instance_manifest_phase(0));
        expected_items.push(instance_packets::instance_manifest_phase(1));
        expected_items.push(instance_packets::instance_manifest_done(0, 148, 0));

        // The items ask: the character's items, the map's record and the manifest. The answer ends
        // at the manifest's second end message.
        let mut ask = vec![0x91, 0x80, 0x00, 0x00];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the items ask");
        let items = read_frames(&mut stream, &mut reading, 0x0198, 2);
        let headers: Vec<u16> = items.iter().map(|(header, _)| *header).collect();
        for header in [0x0144u16, 0x0070, 0x0030, 0x0099, 0x01AC, 0x0199, 0x0198] {
            assert!(
                headers.contains(&header),
                "the items answer carries 0x{header:04X}: {headers:04X?}"
            );
        }
        assert_eq!(
            *headers.last().unwrap(),
            0x0198,
            "the manifest ends the answer"
        );
        // Byte for byte: the same stream the creation set would draw, from the rows that are this
        // character's own. A change to the item stream or to the rows breaks here.
        let mut whole = Vec::new();
        for frame in &expected_items {
            whole.extend_from_slice(frame);
        }
        let mut answered = Vec::new();
        for (_, frame) in &items {
            answered.extend_from_slice(frame);
        }
        assert_eq!(
            answered.len(),
            whole.len(),
            "the items answer is the same size as the set draws"
        );
        assert_eq!(answered, whole, "the items answer is the set's own bytes");

        // The spawn ask: where it stands, and the first tick.
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the spawn ask");
        // The entry point, one frame: that is `spawn_block` whole (`ASURA/crates/asura-world/src/map.rs:13-20`),
        // and the simulation's ticks come from the ticker, not from this answer.
        let mut spawn = read_exact(&mut stream, 26);
        reading.apply(&mut spawn);
        assert_eq!(&spawn[..2], &[0x96, 0x01], "the entry position");
        // The one number in this block the client cannot be wrong about: an entry in its own `Gw.dat`.
        // 113021 (0x0001B97D) is what a real load of map 148 carried - `7D B9 01 00` at the fourth byte
        // of the `0x0196` frame in `recordings/client/feature/maps/pre-searing-load-3.jsonl` seq 137.
        // 112509 (0x0001B77D) is not a map file in this client, and a load naming it ended with "not a
        // valid riff type", "Creating default map" and an assertion in `Map.cpp(1762)`.
        assert_eq!(
            &spawn[2..6],
            &113_021u32.to_le_bytes(),
            "map 148's file id, as the recording carried it"
        );
        let x = f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]);
        let y = f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]);
        // Map 148's own list, as its pack states it (`Outposts/148/spawns.json`): two points, and an
        // entry picks one at random and lands inside World.SpawnScatter (250) of it (R13).
        let near = [(9497.82f32, 4835.33f32), (9686.74, 7979.27)]
            .iter()
            .any(|(px, py)| (x - px).abs() <= 250.0 && (y - py).abs() <= 250.0);
        assert!(
            near,
            "the point is scattered within the world's spawn scatter of one of the map's own spawns ({x}, {y})"
        );

        // The players ask: the PvP halves, the character's body and the end of the load. The ask's
        // own wire is nineteen bytes: a sixteen-byte blob and one byte (docs/port/02-wire.md row 14).
        let mut ask = vec![0u8; 19];
        ask[0] = 0x90;
        ask[1] = 0x80;
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the players ask");
        let players = read_frames(&mut stream, &mut reading, 0x018E, 1);
        assert_eq!(&players[0].1[..2], &[0x1D, 0x00], "no unlocked PvP skills");
        // The heroes half is the one a load sends: eight dwords, every bit set.
        assert_eq!(
            players[1].0, 0x0018,
            "the unlocked-hero bitmap: {players:?}"
        );
        assert_eq!(
            players[1].1.len(),
            36,
            "the bitmap is a counted field and its eight dwords"
        );
        assert!(
            players[1].1[4..].iter().all(|byte| *byte == 0xFF),
            "the unlocked-hero bitmap: eight dwords of ones"
        );
        assert_eq!(
            players.last().unwrap().0,
            0x018E,
            "the load's last message: {players:?}"
        );
        // The party, in the reference's own order: created, the player added, the member stream ended,
        // and only then which party the character is in. A client given `0x01B3` alone asserts
        // `m_partyClient` (`PyCliParty.cpp(650)`, measured 2026-11-12).
        let headers: Vec<u16> = players.iter().map(|(header, _)| *header).collect();
        let party_at = headers
            .iter()
            .position(|header| *header == 0x01D3)
            .unwrap_or_else(|| {
                panic!("the party is created in the players answer: {headers:04X?}")
            });
        assert_eq!(
            &headers[party_at..party_at + 4],
            &[0x01D3, 0x01CC, 0x01D4, 0x01B3],
            "the party comes before the character's own party word: {headers:04X?}"
        );

        // From here the world ticks the connection: one `0x001E` arrives on the interval.
        let mut tick = read_exact(&mut stream, 6);
        reading.apply(&mut tick);
        assert_eq!(&tick[..2], &[0x1E, 0x00], "the simulation tick");

        // 8. A click in the outpost (`GAME_CMSG_MOVE_TO_COORD`, `0x803E`: the point and its plane).
        // The world answers from map 148's own path graph, which it imports out of `Gw.dat` at
        // start-up (`ASURA/crates/asura-world/src/agent.rs:802-856`): the finder's first waypoint in
        // `0x0029`, and nothing at all when it refuses. Skipped on a machine with no archive, where
        // every walk is `NoGraph` by design.
        let archive = Config::load_json("worldserver.json", false, Policy::default())
            .expect("the world program's own configuration")
            .value_string("GwDat", "Gw.dat")
            .expect("GwDat is a string")
            .0;
        if std::path::Path::new(&archive).exists() {
            let mut walk = vec![0u8; 14];
            walk[0..2].copy_from_slice(&0x803Eu16.to_le_bytes());
            // Twenty units from where the character was put rather than across the map: the walk is
            // inside the ground the client already stands on, so the answer is the reference's own
            // path and not a route no outpost holds.
            walk[2..6].copy_from_slice(&(x + 20.0).to_le_bytes());
            walk[6..10].copy_from_slice(&(y + 20.0).to_le_bytes());
            // The plane is the u32 the client sends; 0 is the floor, which is where an entry stands.
            sending.apply(&mut walk);
            stream.write_all(&walk).expect("the walk request");
            let answer = read_frames(&mut stream, &mut reading, 0x0029, 1);
            let headers: Vec<u16> = answer.iter().map(|(header, _)| *header).collect();
            let last = &answer.last().unwrap().1;
            assert_eq!(
                &last[..2],
                &[0x29, 0x00],
                "the walk is answered with the finder's first waypoint: {headers:04X?}"
            );
            // `0x0029` carries the point it walks to, and it is the finder's, at or beside the click:
            // `29 00 | agent | x | y | plane | current_plane`.
            let wx = f32::from_le_bytes([last[6], last[7], last[8], last[9]]);
            let wy = f32::from_le_bytes([last[10], last[11], last[12], last[13]]);
            assert!(
                (wx - (x + 20.0)).abs() <= 300.0 && (wy - (y + 20.0)).abs() <= 300.0,
                "the first waypoint ({wx}, {wy}) is the path's, not a point across the map"
            );
        } else {
            eprintln!("skipped the walk step: no client archive at {archive}");
        }

        // The item operations the manual test's steps 10 to 12 do, against the program: a drag, an
        // equip and a destroy. Each one is a row in the character database and an answer to the
        // client; the items are the ones the confirm wrote, read back through the same repository.
        let tables = game::creation::Tables::load(
            "../config/World/Vanilla/creation/inventory.json",
            "../config/World/Vanilla/creation/equipment.json",
            Some(std::path::Path::new(
                "../config/World/Vanilla/creation/worn_slots.json",
            )),
            &Log::new(),
        )
        .expect("the creation data");
        let templates = game::data_stores::DatabaseItems::new(Arc::clone(&world_pool), Log::new())
            .list()
            .expect("the item templates are readable");
        let held = game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new())
            .items_of(&uuid_text)
            .expect("the character's items are readable");
        assert!(!held.is_empty(), "the load had items to operate on");

        // 9. A drag: the row moves when the placement is one the character may have, and the client is
        // told the same placement back (`ASURA/crates/asura-world/src/item.rs:255-312`). The target is a
        // free slot of the backpack - a move to where the item already is answers nothing at all, which
        // is ASURA's own `MoveOutcome::Unchanged`.
        let dragged = &held[0];
        let backpack = game::creation::bags(&tables)
            .iter()
            .find(|bag| bag.model == game::characters::BACKPACK_MODEL)
            .copied()
            .expect("the content has a backpack");
        let free = (0..backpack.slots)
            .find(|slot| {
                !held
                    .iter()
                    .any(|row| row.bag_id == backpack.id && row.slot == *slot)
            })
            .expect("the backpack has a free slot");
        let mut ask = vec![0u8; 9];
        ask[0..2].copy_from_slice(&0x8072u16.to_le_bytes());
        ask[2..6].copy_from_slice(&dragged.item_id.to_le_bytes());
        ask[6..8].copy_from_slice(&backpack.id.to_le_bytes());
        ask[8] = free;
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the drag");
        let mut placed = read_exact(&mut stream, 11);
        reading.apply(&mut placed);
        assert_eq!(
            &placed[..2],
            &[0x3E, 0x01],
            "the placement: {}",
            shared::network::socket::hex(&placed)
        );
        assert_eq!(&placed[4..8], &dragged.item_id.to_le_bytes(), "the item");
        assert_eq!(&placed[8..10], &backpack.id.to_le_bytes(), "the bag");
        assert_eq!(placed[10], free, "the slot");
        {
            let after =
                game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new())
                    .items_of(&uuid_text)
                    .expect("the character's items are readable");
            let row = after
                .iter()
                .find(|row| row.item_id == dragged.item_id)
                .expect("the dragged item is still the character's");
            assert_eq!(
                (row.bag_id, row.slot),
                (backpack.id, free),
                "the row is the state"
            );
        }

        // A move into an occupied slot is refused with where the item really is: the client has already
        // moved it in its own model, so the answer puts it back where it sits.
        let occupied = held.iter().find(|row| {
            row.bag_id == backpack.id && row.slot == 0 && row.item_id != dragged.item_id
        });
        if let Some(blocker) = occupied {
            let mut ask = vec![0u8; 9];
            ask[0..2].copy_from_slice(&0x8072u16.to_le_bytes());
            ask[2..6].copy_from_slice(&dragged.item_id.to_le_bytes());
            ask[6..8].copy_from_slice(&backpack.id.to_le_bytes());
            ask[8] = 0;
            sending.apply(&mut ask);
            stream.write_all(&ask).expect("the refused drag");
            let mut back = read_exact(&mut stream, 11);
            reading.apply(&mut back);
            assert_eq!(&back[..2], &[0x3E, 0x01], "the refusal is a placement");
            assert_eq!(&back[4..8], &dragged.item_id.to_le_bytes(), "the item");
            assert_eq!(
                &back[8..10],
                &backpack.id.to_le_bytes(),
                "where it really is"
            );
            assert_eq!(back[10], free, "its own slot, not the occupied one");
            assert_eq!(blocker.slot, 0, "the blocker still sits in slot 0");
        }

        // 10. An equip: the item goes to the slot its kind is worn in, in the equipped bag, and whatever
        // was worn there comes off to the first free carrying slot
        // (`ASURA/crates/asura-world/src/item.rs:314-436`). An item already in its own worn slot is
        // `MoveOutcome::Unchanged` and answers nothing, so the one picked is somewhere else.
        let template_of = |item_id: u32| {
            let row = held.iter().find(|row| row.item_id == item_id);
            row.and_then(|row| {
                templates
                    .iter()
                    .find(|template| template.id == row.template_id)
                    .cloned()
            })
        };
        let equipped_bag = game::creation::equipped_bag(&tables).expect("the equipped bag");
        let worn_table = tables.worn();
        let wearable = held.iter().find_map(|row| {
            let template = template_of(row.item_id)?;
            let slot = game::creation::worn_slot(worn_table, template.item_type)?;
            if row.bag_id == equipped_bag.id && row.slot == slot {
                return None;
            }
            Some((row.item_id, template.item_type))
        });
        if let Some((item_id, item_type)) = wearable {
            let worn_slot =
                game::creation::worn_slot(worn_table, item_type).expect("the kind is worn");
            // Whatever is worn there now is taken off first, and the client is told that placement
            // before the item's own: the eviction comes first, exactly as ASURA sends it.
            let occupant = held
                .iter()
                .find(|row| row.bag_id == equipped_bag.id && row.slot == worn_slot)
                .map(|row| row.item_id);
            let mut ask = vec![0u8; 6];
            ask[0..2].copy_from_slice(&0x8030u16.to_le_bytes());
            ask[2..6].copy_from_slice(&item_id.to_le_bytes());
            sending.apply(&mut ask);
            stream.write_all(&ask).expect("the equip");
            if let Some(occupant) = occupant
                && occupant != item_id
            {
                let mut off = read_exact(&mut stream, 11);
                reading.apply(&mut off);
                assert_eq!(&off[..2], &[0x3E, 0x01], "the eviction's placement");
                assert_eq!(&off[4..8], &occupant.to_le_bytes(), "the item taken off");
            }
            let mut placed = read_exact(&mut stream, 11);
            reading.apply(&mut placed);
            assert_eq!(&placed[..2], &[0x3E, 0x01], "the placement");
            assert_eq!(&placed[4..8], &item_id.to_le_bytes(), "the item");
            assert_eq!(
                placed[8..10],
                equipped_bag.id.to_le_bytes(),
                "the equipped bag"
            );
            assert_eq!(placed[10], worn_slot, "the slot its kind is worn in");
            // And what the agent is drawn wearing is sent again, because that is the one thing the
            // client cannot work out for itself.
            let mut drawn = read_exact(&mut stream, 2 + 4 + 9 * 4);
            reading.apply(&mut drawn);
            assert_eq!(&drawn[..2], &[0x6E, 0x00], "the agent's worn items");
            assert_eq!(&drawn[2..6], &1u32.to_le_bytes(), "the agent");
            assert_eq!(
                &drawn[6 + worn_slot as usize * 4..10 + worn_slot as usize * 4],
                &item_id.to_le_bytes(),
                "its slot names the item just worn"
            );
        }

        // 11. A drop (`GAME_CMSG_DROP_ITEM`, `0x802C`: the item and a count byte): the stack is the
        // state and **nothing goes on the wire**, which is what the working implementation does and
        // what its own notes say both candidate messages cost a live client
        // (`ASURA/crates/asura-world/src/item.rs:187-246`).
        let dropped = held
            .iter()
            .find(|row| row.item_id != dragged.item_id && row.quantity > 1)
            .map(|row| (row.item_id, row.quantity))
            .expect("the set holds a stack of more than one");
        let mut ask = vec![0u8; 7];
        ask[0..2].copy_from_slice(&0x802Cu16.to_le_bytes());
        ask[2..6].copy_from_slice(&dropped.0.to_le_bytes());
        ask[6] = 1;
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the drop");
        // No answer: the next frame on the connection is the simulation tick, not an item message.
        let mut next = read_exact(&mut stream, 6);
        reading.apply(&mut next);
        assert_eq!(&next[..2], &[0x1E, 0x00], "a drop is answered with nothing");
        {
            let held =
                game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new())
                    .items_of(&uuid_text)
                    .expect("the character's items are readable");
            let after = held
                .iter()
                .find(|row| row.item_id == dropped.0)
                .expect("the stack is still there, one lighter");
            assert_eq!(
                after.quantity,
                dropped.1 - 1,
                "one was taken off the stack and nothing else changed"
            );
        }

        // A ping on the loaded connection (`GAME_CMSG_PING_REQUEST`, `0x800D`, two bytes) is answered
        // with the ping reply - `0x000D` and the value the world runs with - and not with an echo
        // (`ASURA/crates/asura-world/src/connection.rs:579-587`).
        let mut ping = vec![0x0Du8, 0x80];
        sending.apply(&mut ping);
        stream.write_all(&ping).expect("the ping");
        // A tick may already have been on its way, so the answer is read until it is the reply itself.
        let mut answered;
        loop {
            answered = read_exact(&mut stream, 6);
            reading.apply(&mut answered);
            if answered[..2] == [0x0D, 0x00] {
                break;
            }
            assert_eq!(
                &answered[..2],
                &[0x1E, 0x00],
                "only a tick comes before the reply"
            );
        }
        assert_eq!(
            &answered[2..6],
            &20u32.to_le_bytes(),
            "the value the world runs with (World.PingValue)"
        );

        // 12. The client speaks (`GAME_CMSG_SEND_CHAT_MESSAGE`, `0x8064`: `{agent u32, text
        // String16}`, whose first unit is the channel). A `!` line is answered with the text
        // (`0x005D`) and who spoke it (`0x0061`), **sent ungated** as the reference sends a player's
        // own line (`ASURA/crates/asura-world/src/chat.rs:227-247`).
        let speak = |text: &str| {
            let mut ask = vec![0x64, 0x80];
            ask.extend_from_slice(&1u32.to_le_bytes()); // the agent the client sends with it
            let units: Vec<u16> = text.encode_utf16().collect();
            ask.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for unit in &units {
                ask.extend_from_slice(&unit.to_le_bytes());
            }
            ask
        };
        let mut ask = speak("!hello");
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("a spoken line");
        // The measured pair (`docs/decisions/0012-chat-text-is-sent.md`, and captured from this tree and
        // read with `tools/capture/read.py`): the body is `5d 00` + the unit count + `0108 0107` + the
        // text + `0001`, twenty bytes, and the tag is five - `61 00` + the player's u16 + the channel.
        // The count is eight because it counts the two wrapper units, the five of "hello" and the
        // terminator, which is what the owner's client rendered.
        let mut said = read_exact(&mut stream, 20 + 5);
        reading.apply(&mut said);
        assert_eq!(&said[..2], &[0x5D, 0x00], "the text of the line");
        assert_eq!(
            u16::from_le_bytes([said[2], said[3]]),
            8,
            "eight units: two wrapper units, the five of \"hello\", and the terminator (0012)"
        );
        assert_eq!(
            &said[4..6],
            &0x0108u16.to_le_bytes(),
            "the All-chat body wrapper"
        );
        assert_eq!(&said[6..8], &0x0107u16.to_le_bytes(), "the literal mark");
        assert_eq!(
            &said[8..18],
            b"h\0e\0l\0l\0o\0",
            "the line itself, as UTF-16"
        );
        assert_eq!(&said[18..20], &0x0001u16.to_le_bytes(), "the terminator");
        assert_eq!(&said[20..22], &[0x61, 0x00], "who spoke it");
        assert_eq!(u16::from_le_bytes([said[22], said[23]]), 1, "the speaker");
        assert_eq!(said[24], 3, "Channel_All, the channel `!` speaks on");

        // The commands the reference answers in its log and nowhere else, and a channel it drops:
        // `/maps` prints the alias table, `/tp ascalon` is the command's own "You are already here."
        // (this connection is in map 148), and `@` is one of the three channels the reference ignores.
        // Every one of them leaves the wire to the simulation tick
        // (`ASURA/crates/asura-world/src/chat.rs:288-352`).
        for silent in ["/maps", "/tp ascalon", "@/ignored", "/nonsense"] {
            let mut ask = speak(silent);
            sending.apply(&mut ask);
            stream.write_all(&ask).expect("a command");
            let mut tick = read_exact(&mut stream, 6);
            reading.apply(&mut tick);
            assert_eq!(
                &tick[..2],
                &[0x1E, 0x00],
                "{silent} is answered in the log and not on the wire"
            );
        }

        // `/stuck` broadcasts where the agent stands: `0x002C`, **not** `AGENT_STOP_MOVING`
        // (`ASURA/crates/asura-world/src/chat.rs:260-287`).
        let mut ask = speak("/stuck");
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the stuck command");
        let mut stood = read_exact(&mut stream, 16);
        reading.apply(&mut stood);
        assert_eq!(&stood[..2], &[0x2C, 0x00], "the agent's position");
        assert_eq!(
            &stood[2..6],
            &1u32.to_le_bytes(),
            "the agent that stood still"
        );

        // The instance is a row in the world database while the connection is inside it. Its id is
        // kept, so the cleanup removes this test's copy and nothing else.
        {
            let mut pool = world_pool.lock().unwrap();
            let rows = pool
                .query_with(
                    "SELECT \"id\"::bigint, \"map_id\"::bigint, \"district\"::bigint \
                     FROM \"instance\" WHERE \"map_id\" = 148 ORDER BY \"district\"",
                    &[],
                )
                .expect("the instance table is readable");
            assert!(!rows.is_empty(), "a copy of map 148 is open");
            assert_eq!(rows[0].get::<usize, i64>(1), 148);
            assert_eq!(
                rows[0].get::<usize, i64>(2),
                1,
                "district 1, the first with room"
            );
            *copy.lock().unwrap() = Some(rows[0].get::<usize, i64>(0));
        }

        // The client's own travel request (`GAME_CMSG_PARTY_TRAVEL`, `0x80B1`, nine bytes: the header,
        // the instance, and three bytes it does not interpret). It is answered with the eight frames
        // `ASURA/crates/asura-world/src/travel.rs:480-518` sends, in that order: the travel effect, the
        // walk's stop, the redirect, the map update in its twelve-byte form, and the travelling body
        // withdrawn - its party size, its party, the player and the agent.
        //
        // Read as one block rather than by the frame walker, because `0x0099`'s table row is its
        // five-byte form and this answer carries the twelve-byte one; the sizes are the table's own.
        let mut travel = vec![0u8; 9];
        travel[0..2].copy_from_slice(&0x80B1u16.to_le_bytes());
        travel[2..4].copy_from_slice(&148u16.to_le_bytes());
        travel[4] = 0;
        travel[5..7].copy_from_slice(&1u16.to_le_bytes());
        travel[7] = 0;
        travel[8] = 0;
        sending.apply(&mut travel);
        stream.write_all(&travel).expect("the travel request");
        let sizes = [10usize, 6, 46, 12, 12, 12, 8, 8];
        let mut redirect = read_exact(&mut stream, sizes.iter().sum());
        reading.apply(&mut redirect);
        let mut headers = Vec::new();
        let mut at = 0;
        for size in sizes {
            headers.push(u16::from_le_bytes([redirect[at], redirect[at + 1]]));
            at += size;
        }
        assert_eq!(
            headers,
            vec![
                0x00F1, 0x0028, 0x01A6, 0x0099, 0x00B0, 0x00B1, 0x005A, 0x0021
            ],
            "the redirect's eight frames, in the working implementation's order"
        );
        // The twelve-byte map update names the map the client is being sent to: the effect, the stop
        // and the redirect come first (10 + 6 + 46 bytes), then its header, the upper half of its
        // four-byte game header, and the map.
        assert_eq!(
            &redirect[66..70],
            &148u32.to_le_bytes(),
            "the redirect's map update names map 148"
        );

        // Ending the socket releases the copy: its row is emptied or disposed by the world.
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let mut released = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            {
                let mut pool = world_pool.lock().unwrap();
                let rows = pool
                    .query_with(
                        "SELECT \"empty_since\" FROM \"instance\" WHERE \"map_id\" = 148",
                        &[],
                    )
                    .expect("the instance table is readable");
                // The column is `timestamptz`: a null is an empty copy that is still held.
                let empty: Option<std::time::SystemTime> = rows.first().and_then(|row| row.get(0));
                if rows.is_empty() || empty.is_none() {
                    released = true;
                }
            }
            if released {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(
            released,
            "the copy is empty or gone after the connection ends"
        );
    }));

    // The program is stopped: the test's own rows are what it leaves.
    let _ = child.kill();
    let _ = child.wait();
    while let Ok(line) = _lines.try_recv() {
        if line.contains("Gameplay connection")
            || line.contains("panicked")
            || line.contains("WARN")
        {
            println!("world> {line}");
        }
    }

    // Cleanup: the character this test wrote, and the copy the world recorded.
    if let Some(uuid_text) = created_for_cleanup.lock().unwrap().clone() {
        let mut pool = characters.lock().unwrap();
        for statement in [
            "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
        ] {
            let _ = pool.query_with(statement, &[&uuid_text]);
        }
    }
    if let Some(id) = *copy_for_cleanup.lock().unwrap() {
        let mut pool = world_pool.lock().unwrap();
        let _ = pool.query_with("DELETE FROM \"instance\" WHERE \"id\" = $1", &[&id]);
    }

    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// A map that arrived with the catalogue import loads through the same protocol map 148 does.
///
/// The catalogue is content - 265 rows, 145 of them hosted - and 148 is the one map the schema seeded by
/// hand. This test takes a second one, **163 The Barradin Estate** (file id 380713, two spawn points of
/// its own), and loads it the way a client does: a character created through the creation screen, R12's
/// unlock written for the destination as a scripted travel would, a travel record for the login to have
/// written, and the connection that presents it. What it pins is that the row's own file id reaches
/// `0x0196` and that the point the world scatters is one of that map's own (R13).
#[test]
#[ignore = "needs the databases the world program's configuration names"]
fn a_second_outpost_loads_with_its_own_file_id_and_a_point_of_its_own_list() {
    const MAP: u32 = 163;
    let name = "E2E Kryta Two";
    let token: u32 = 0x4242;
    let (mut child, lines, port) = start_world();
    println!("the world program is listening on port {port}");
    let (characters, world_pool, login) = open_pools();

    let account = [0x99u8; 16];
    let account_text = game::characters::uuid_text(&account);
    let (character, _written) = create_character(port, account, name);
    let uuid_text = game::characters::uuid_text(&character);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // What the world's own row says about the destination: the archive file and the points.
        let (file_id, spawn_points): (i64, Vec<(f64, f64)>) = {
            let mut pool = world_pool.lock().unwrap();
            let row = pool
                .query_with(
                    "SELECT \"file_id\"::bigint FROM \"map\" WHERE \"id\" = $1::bigint::int",
                    &[&(MAP as i64)],
                )
                .expect("the map row reads")
                .into_iter()
                .next()
                .expect("map 163 is in the catalogue");
            let file_id: i64 = row.get(0);
            let points = pool
                .query_with(
                    "SELECT \"x\"::text::float8, \"y\"::text::float8 FROM \"map_spawn\" \
                     WHERE \"map_id\" = $1::bigint::int ORDER BY \"idx\"",
                    &[&(MAP as i64)],
                )
                .expect("the spawn rows read")
                .iter()
                .map(|row| (row.get::<usize, f64>(0), row.get::<usize, f64>(1)))
                .collect();
            (file_id, points)
        };
        assert!(
            spawn_points.len() >= 2,
            "map {MAP} states its own points: {spawn_points:?}"
        );

        // R12: a character starts with one unlock. The unlock a scripted travel grants at arrival is
        // written here, because a second outpost is not reachable any other way yet.
        let repository =
            game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new());
        assert!(
            repository
                .unlock_map(&uuid_text, MAP)
                .expect("the unlock writes"),
            "the unlock row is written"
        );

        // The hand-off the login writes when it answers a map request, and the connection that
        // presents it: map 163, district 1, language 0.
        game::maps::DatabaseTravels::new(Arc::clone(&login), Log::new())
            .record(&game::maps::Travel {
                account,
                world: 2,
                map_id: MAP,
                district: 1,
                language: 0,
                player_token: token,
                landing: None,
            })
            .expect("the travel is recorded");

        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: MAP,
            player_token: token,
            account,
            character,
            h0038: 0,
            h003c: 0,
        };
        let (mut stream, mut reading, mut sending) = open_connection(port, version);

        // The load's head, read by size rather than by the table's walker: the `0x01A0` row states a
        // `NestedStruct` of sixteen-byte units (`Tables.rs`, the row is ASURA's own) while the frame
        // carries `u8` count and six-byte pairs, so a walker cannot split it. The pairs come from the
        // same builder the world used.
        let table_batches = {
            let mut pool = world_pool.lock().unwrap();
            let row = pool
                .query_with(
                    "SELECT count(*)::bigint FROM \"map\" WHERE \"instance_value\" IS NOT NULL",
                    &[],
                )
                .expect("the catalogue counts")
                .into_iter()
                .next()
                .expect("one row");
            let pairs: i64 = row.get(0);
            (pairs as usize).div_ceil(15)
        };
        let table_bytes = {
            let mut pool = world_pool.lock().unwrap();
            let rows = pool
                .query_with(
                    "SELECT \"id\"::bigint, \"instance_value\"::bigint FROM \"map\" \
                     WHERE \"instance_value\" IS NOT NULL ORDER BY \"id\"",
                    &[],
                )
                .expect("the pairs read");
            let pairs: Vec<(u16, u32)> = rows
                .iter()
                .filter_map(|row| {
                    let id: i64 = row.get(0);
                    let value: i64 = row.get(1);
                    u16::try_from(id).ok().map(|id| (id, value as u32))
                })
                .collect();
            pairs
                .chunks(15)
                .map(instance_packets::instance_table)
                .map(|frame| frame.len())
                .sum::<usize>()
        };
        // The block the program says it sent is the block that arrives: it logs the size before it
        // writes, so reading that many bytes is exact - and it is the program's own statement, not
        // this test's arithmetic.
        let announced = wait_for(&lines, "is loading - ", Duration::from_secs(30));
        let block_bytes: usize = announced
            .split(" message(s), ")
            .nth(1)
            .and_then(|rest| rest.split(" bytes").next())
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or_else(|| panic!("the load's size is in its log line: {announced}"));
        println!(
            "the load of map {MAP}: {block_bytes} byte(s), {table_batches} table batch(es), {} of them the table",
            table_bytes
        );
        let mut block = read_exact(&mut stream, block_bytes);
        reading.apply(&mut block);
        let name_size = instance_packets::instance_load_player_name(name).len();
        let record_at = instance_packets::instance_load_head().len()
            + instance_packets::instance_player_data_start().len()
            + name_size;
        assert_eq!(
            &block[..2],
            &[0x7C, 0x01],
            "the outpost's load head, not the creation instance's"
        );
        assert_eq!(
            &block[record_at..record_at + 2],
            &[0x9A, 0x01],
            "the instance's record follows the character's name"
        );
        assert_eq!(
            u16::from_le_bytes([block[record_at + 6], block[record_at + 7]]),
            MAP as u16,
            "the instance's record names the map the travel did"
        );
        assert_eq!(
            block[record_at + 8],
            1,
            "and the explorable byte the reference hardcodes"
        );
        // The instance table closes the block: `u8` count and six bytes a pair, fifteen to a batch.
        let table_start = block_bytes - table_bytes;
        assert_eq!(
            &block[table_start..table_start + 2],
            &[0xA0, 0x01],
            "the instance table follows the grid"
        );
        assert_eq!(block[table_start + 2], 15, "a batch holds fifteen pairs");
        // The pairs are in map-id order, so the first is the lowest record that states a value.
        assert_eq!(
            u16::from_le_bytes([block[table_start + 3], block[table_start + 4]]),
            4,
            "the first pair is the lowest map id with an instance value"
        );

        // The ping pair the measured load follows the head block with.
        let mut ping = read_exact(&mut stream, 8);
        reading.apply(&mut ping);
        assert_eq!(&ping[..2], &[0x0C, 0x00], "the ping request");
        assert_eq!(
            &ping[2..4],
            &[0x0D, 0x00],
            "and the reply the measured load pairs it with"
        );
        assert_eq!(
            u32::from_le_bytes([ping[4], ping[5], ping[6], ping[7]]),
            20,
            "the reply carries the world ping value (World.PingValue)"
        );

        // The entry point: this map's own archive file, and a point of its own list (R13).
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the spawn ask");
        let mut spawn = read_exact(&mut stream, 26);
        reading.apply(&mut spawn);
        assert_eq!(&spawn[..2], &[0x96, 0x01], "the entry position");
        assert_eq!(
            i64::from(u32::from_le_bytes([spawn[2], spawn[3], spawn[4], spawn[5]])),
            file_id,
            "map {MAP}'s own archive file, from its catalogue row"
        );
        let x = f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]) as f64;
        let y = f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]) as f64;
        let near = spawn_points
            .iter()
            .any(|(px, py)| (x - px).abs() <= 250.0 && (y - py).abs() <= 250.0);
        assert!(
            near,
            "the point ({x}, {y}) is one of map {MAP}'s own, scattered (R13): {spawn_points:?}"
        );
        println!("map {MAP}: {name} stands at ({x}, {y})");
    }));

    // Cleanup: the character, its unlocks, and the travel row if the claim never happened.
    {
        let mut pool = characters.lock().unwrap();
        for statement in [
            "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
        ] {
            let _ = pool.query_with(statement, &[&uuid_text]);
        }
    }
    if let Ok(mut pool) = login.lock() {
        let _ = pool.query_with(
            "DELETE FROM \"travel\" WHERE \"account\" = $1",
            &[&account_text],
        );
    }
    let _ = child.kill();
    let _ = child.wait();
    // What the program said while this ran: the lines after its READY, which is where a refusal is.
    while let Ok(line) = lines.try_recv() {
        println!("world> {line}");
    }

    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
/// A reconnect to the map the character left resumes where it left (R14).
///
/// `ASURA/crates/asura-world/src/agent.rs:531-545` (`Standing::resume`) and
/// `ASURA/crates/asura-store/src/state/character.rs:226-294`: an **explorable** stands where the
/// character was saved when it left from that map - only an outpost rolls a spawn point (R13) - and the
/// position is written when the connection ends. This test takes **43 Silverwood** (file id 40812, two
/// spawn points of its own), reports a position, drops the connection, and comes back: the entry point
/// is the point it reported, not the map's own first spawn.
#[test]
#[ignore = "needs the databases the world program's configuration names"]
fn a_reconnect_to_an_explorable_resumes_where_the_character_left() {
    const MAP: u32 = 43;
    let name = "E2E Kryta Three";
    let (mut child, lines, port) = start_world();
    println!("the world program is listening on port {port}");
    let (characters, world_pool, login) = open_pools();

    let account = [0x55u8; 16];
    let (character, _written) = create_character(port, account, name);
    let uuid_text = game::characters::uuid_text(&character);
    let repository = game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new());

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // R12: the destination has to be unlocked, as a scripted travel would.
        assert!(
            repository
                .unlock_map(&uuid_text, MAP)
                .expect("the unlock writes")
        );

        // The map's own first spawn point: what the entry rule answers with no saved position.
        let own_point: (f64, f64) = {
            let mut pool = world_pool.lock().unwrap();
            let row = pool
                .query_with(
                    "SELECT \"x\"::text::float8, \"y\"::text::float8 FROM \"map_spawn\" \
                     WHERE \"map_id\" = $1::bigint::int ORDER BY \"idx\" LIMIT 1",
                    &[&(MAP as i64)],
                )
                .expect("the spawn rows read")
                .into_iter()
                .next()
                .expect("map 43 states a spawn point");
            (row.get::<usize, f64>(0), row.get::<usize, f64>(1))
        };

        // The first connection: the travel record, then the load, then the entry point.
        game::maps::DatabaseTravels::new(Arc::clone(&login), Log::new())
            .record(&game::maps::Travel {
                account,
                world: 2,
                map_id: MAP,
                district: 1,
                language: 0,
                player_token: 0x5151,
                landing: None,
            })
            .expect("the first travel is recorded");
        let (mut stream, mut reading, mut sending) = open_connection(
            port,
            GameVersion {
                version: 39167,
                h0008: 1,
                map_token: 2,
                map_id: MAP,
                player_token: 0x5151,
                account,
                character,
                h0038: 0,
                h003c: 0,
            },
        );
        let announced = wait_for(&lines, "is loading - ", Duration::from_secs(30));
        let block_bytes: usize = announced
            .split(" message(s), ")
            .nth(1)
            .and_then(|rest| rest.split(" bytes").next())
            .and_then(|count| count.trim().parse().ok())
            .expect("the load's size is in its log line");
        // Every byte the cipher reads must be decrypted, or the stream loses its place - the block is
        // discarded, but it still advances the stream.
        let mut block = read_exact(&mut stream, block_bytes);
        reading.apply(&mut block);
        let mut ping = read_exact(&mut stream, 8);
        reading.apply(&mut ping);
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the spawn ask");
        let mut spawn = read_exact(&mut stream, 26);
        reading.apply(&mut spawn);
        let entry = (
            f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]) as f64,
            f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]) as f64,
        );
        assert!(
            (entry.0 - own_point.0).abs() <= 0.5 && (entry.1 - own_point.1).abs() <= 0.5,
            "an explorable with no saved position stands on its own first point: {entry:?} against {own_point:?}"
        );

        // Where the client says it is: a correction the server takes, 120 units away and on the same
        // plane (`GAME_CMSG_LAST_POS_BEFORE_MOVE_CANCELED`, `0x8047`, ported from `agent.rs:1117`).
        let reported = (entry.0 as f32 + 120.0, entry.1 as f32 + 120.0);
        let mut position = vec![0u8; 14];
        position[0..2].copy_from_slice(&0x8047u16.to_le_bytes());
        position[2..6].copy_from_slice(&reported.0.to_le_bytes());
        position[6..10].copy_from_slice(&reported.1.to_le_bytes());
        sending.apply(&mut position);
        stream.write_all(&position).expect("the position report");
        let answered = read_frames(&mut stream, &mut reading, 0x0029, 1);
        assert_eq!(
            answered.last().map(|(header, _)| *header),
            Some(0x0029),
            "a taken correction is answered with a destination at that point"
        );

        // The connection ends: the world saves where the character stood (R14).
        drop(stream);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut saved = None;
        while std::time::Instant::now() < deadline {
            match repository.position(&uuid_text).expect("the position reads") {
                Some(position) => {
                    saved = Some(position);
                    break;
                }
                None => std::thread::sleep(Duration::from_millis(200)),
            }
        }
        let saved = saved.unwrap_or_else(|| panic!("the leaving connection saved a position"));
        println!("saved: {saved:?}");
        assert_eq!(
            saved.map_id, MAP,
            "the position belongs to the map it was taken in"
        );
        assert!(
            (f64::from(saved.x) - f64::from(reported.0)).abs() <= 1.0
                && (f64::from(saved.y) - f64::from(reported.1)).abs() <= 1.0,
            "the saved point is the one the client reported: {saved:?} against {reported:?}"
        );

        // The second connection: the same map, a new travel record, and the entry point is the saved
        // position rather than the map's own list.
        game::maps::DatabaseTravels::new(Arc::clone(&login), Log::new())
            .record(&game::maps::Travel {
                account,
                world: 2,
                map_id: MAP,
                district: 1,
                language: 0,
                player_token: 0x5252,
                landing: None,
            })
            .expect("the second travel is recorded");
        let (mut stream, mut reading, mut sending) = open_connection(
            port,
            GameVersion {
                version: 39167,
                h0008: 1,
                map_token: 2,
                map_id: MAP,
                player_token: 0x5252,
                account,
                character,
                h0038: 0,
                h003c: 0,
            },
        );
        let announced = wait_for(&lines, "is loading - ", Duration::from_secs(30));
        let block_bytes: usize = announced
            .split(" message(s), ")
            .nth(1)
            .and_then(|rest| rest.split(" bytes").next())
            .and_then(|count| count.trim().parse().ok())
            .expect("the load's size is in its log line");
        let mut block = read_exact(&mut stream, block_bytes);
        reading.apply(&mut block);
        let mut ping = read_exact(&mut stream, 8);
        reading.apply(&mut ping);
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the spawn ask");
        let mut spawn = read_exact(&mut stream, 26);
        reading.apply(&mut spawn);
        let resumed = (
            f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]) as f64,
            f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]) as f64,
        );
        assert!(
            (resumed.0 - f64::from(reported.0)).abs() <= 1.0
                && (resumed.1 - f64::from(reported.1)).abs() <= 1.0,
            "the reconnect resumed where the character left: {resumed:?} against {reported:?}"
        );
        assert!(
            (resumed.0 - own_point.0).abs() > 1.0 || (resumed.1 - own_point.1).abs() > 1.0,
            "and not on the map's own first spawn point: {own_point:?}"
        );
        println!(
            "resumed at ({}, {}), the point the client reported",
            resumed.0, resumed.1
        );
    }));

    {
        let mut pool = characters.lock().unwrap();
        for statement in [
            "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
        ] {
            let _ = pool.query_with(statement, &[&uuid_text]);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    while let Ok(line) = lines.try_recv() {
        println!("world> {line}");
    }
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
/// A step through Ascalon City's arch starts the crossing, and the connection that follows stands on
/// the arrival the receiving map states.
///
/// This is the journey the owner plays with the client: walk at the walled arch out of Ascalon City
/// (pre-Searing) and the server transfers you to Lakeside County (146). The wall is map 148's own row
/// (`map_arch`, `ASURA/config/World/Vanilla/maps/Prophecies/Ascalon (pre-Searing)/Outposts/148/portals.json`),
/// the landing is map 146's arrival from 148 (`map_arrival`, R21), and the two client reports are the
/// ones `ASURA/crates/asura-world/src/portals.rs:422-428` uses for the same wall.
///
/// What used to happen: the arriving connection was **refused** because the character had not unlocked
/// 146, and the client showed `007`. The rule is `ASURA/crates/asura-world/src/player.rs:442-471` - the
/// walk sets the bit and the bit never gates the walk - so the arrival now unlocks the map it reaches.
#[test]
#[ignore = "needs the databases the world program's configuration names"]
fn a_step_through_ascalon_citys_arch_crosses_to_lakeside_county() {
    const FROM: u32 = 148;
    const TO: u32 = 146;
    /// Map 146's arrival for a character coming from 148: the receiving map owns it, and it is the
    /// pack's own row (`ASURA/config/World/Vanilla/maps/Prophecies/Ascalon (pre-Searing)/Explorables/146/portals.json`,
    /// imported by `data/sql/base/db_world/0012-map-content.sql`). Map 148's arrival `(7629.7, 5843.2)`
    /// is the other direction - where a character arriving *in* 148 from 146 stands.
    const ARRIVAL: (f32, f32) = (6602.0, 4485.0);
    let name = "E2E Kryta Five";
    let (mut child, lines, port) = start_world();
    println!("the world program is listening on port {port}");
    let (characters, _world_pool, _login) = open_pools();

    // The name this test uses may be left behind by an earlier run, and a taken name is a refusal:
    // the row goes first, this test's own character and nobody else's.
    {
        let mut pool = characters.lock().unwrap();
        let rows = pool
            .query_with(
                "SELECT \"uuid\" FROM \"characters\" WHERE \"name\" = $1",
                &[&name],
            )
            .expect("the stale row is readable");
        for row in rows {
            let uuid: String = row.get(0);
            for statement in [
                "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
                "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
                "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
            ] {
                let _ = pool.query_with(statement, &[&uuid]);
            }
        }
    }
    let account = [0x77u8; 16];
    let (character, _written) = create_character(port, account, name);
    let uuid_text = game::characters::uuid_text(&character);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // The load of map 148, named by the version message alone: the outpost, its grid and its table.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: FROM,
            player_token: 0x6161,
            account,
            character,
            h0038: 0,
            h003c: 0,
        };
        let (mut stream, mut reading, mut sending) = open_connection(port, version);

        let announced = wait_for(&lines, "is loading - ", Duration::from_secs(30));
        let block_bytes: usize = announced
            .split(" message(s), ")
            .nth(1)
            .and_then(|rest| rest.split(" bytes").next())
            .and_then(|count| count.trim().parse().ok())
            .expect("the load's size is in its log line");
        let mut block = read_exact(&mut stream, block_bytes);
        reading.apply(&mut block);
        let mut ping = read_exact(&mut stream, 8);
        reading.apply(&mut ping);

        // The items, the entry point and the players: the load a client walks before it can move.
        let mut ask = vec![0x91, 0x80, 0x00, 0x00];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the items ask");
        let items = read_frames(&mut stream, &mut reading, 0x0198, 2);
        assert_eq!(
            items.last().map(|(header, _)| *header),
            Some(0x0198),
            "the manifest ends the items answer"
        );
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the spawn ask");
        let mut spawn = read_exact(&mut stream, 26);
        reading.apply(&mut spawn);
        assert_eq!(&spawn[..2], &[0x96, 0x01], "the entry point");
        let entry = (
            f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]),
            f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]),
        );
        println!("standing at {entry:?} on map {FROM}");
        let mut ask = vec![0x90, 0x80];
        ask.extend_from_slice(&[0u8; 17]);
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the players ask");
        let players = read_frames(&mut stream, &mut reading, 0x018E, 1);
        assert_eq!(
            players.last().map(|(header, _)| *header),
            Some(0x018E),
            "the load is over"
        );

        // A position report is 26 bytes: the header, the point, the plane, the offset and the movement
        // type - and a type outside 1..=8 is dropped, exactly as `ASURA`'s own handler drops it
        // (`agent.rs:923-929`).
        let report = |x: f32, y: f32| {
            let mut frame = vec![0u8; 26];
            frame[0..2].copy_from_slice(&0x803Du16.to_le_bytes());
            frame[2..6].copy_from_slice(&x.to_le_bytes());
            frame[6..10].copy_from_slice(&y.to_le_bytes());
            frame[10..14].copy_from_slice(&0u32.to_le_bytes()); // plane 0
            frame[14..18].copy_from_slice(&0f32.to_le_bytes()); // no offset
            frame[18..22].copy_from_slice(&0f32.to_le_bytes());
            frame[22..26].copy_from_slice(&1u32.to_le_bytes()); // move type 1
            frame
        };

        // The clock first, because that is what a doorway waits on: over 600 ms of silence the map ticks
        // on its own interval - `World.SimulationTickMs`, ASURA's 100 - and each tick carries the time
        // that really elapsed, not a counter and not a catch-up burst
        // (`ASURA/crates/asura-world/src/instance.rs:394-403`, `:290-308`).
        {
            use std::io::Read as _;
            let deadline = std::time::Instant::now() + Duration::from_millis(600);
            let mut buffer: Vec<u8> = Vec::new();
            let mut ticks: Vec<u32> = Vec::new();
            let mut chunk = [0u8; 4096];
            while std::time::Instant::now() < deadline {
                if let Ok(read) = stream.read(&mut chunk)
                    && read > 0
                {
                    let mut plain = chunk[..read].to_vec();
                    reading.apply(&mut plain);
                    buffer.extend_from_slice(&plain);
                    while !buffer.is_empty() {
                        match game::server::protocol::decode_game_message(&buffer, false) {
                            Ok((frame, used)) => {
                                let header = frame.header();
                                let mut bytes = buffer[..used].to_vec();
                                buffer.drain(..used);
                                if header == 0x001E && bytes.len() >= 6 {
                                    ticks.push(u32::from_le_bytes([
                                        bytes[2], bytes[3], bytes[4], bytes[5],
                                    ]));
                                }
                                bytes.clear();
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            println!("ticks in 600 ms: {ticks:?}");
            assert!(
                (3..=8).contains(&ticks.len()),
                "the map ticks on its own clock about every 100 ms: {ticks:?}"
            );
            for elapsed in &ticks {
                assert!(
                    *elapsed >= 50 && *elapsed <= 250,
                    "a tick carries the time that really elapsed, never a catch-up jump: {ticks:?}"
                );
            }
        }

        // The doorway test uses the segment between two finite client reports, so the first report is
        // the point the character is already standing on: a repeated point is not a step
        // (`ASURA/crates/asura-world/src/player.rs:60-74`).
        for (x, y) in [entry, (7277.4, 5417.9), (7184.3, 5307.3)] {
            let mut frame = report(x, y);
            sending.apply(&mut frame);
            stream.write_all(&frame).expect("a position report");
            std::thread::sleep(Duration::from_millis(120));
        }

        // The crossing answers with the movement frames and then the travel's own: the effect, the
        // stop, the redirect, the map update, the party, and the body withdrawn from the map it left
        // (`ASURA/crates/asura-world/src/travel.rs:479-519`). The redirect's token is what the client
        // reconnects with, so it is read out of the frame: host[24] then map_token, region, token.
        // Whatever the program says while the crossing is decided, printed as it arrives.
        let crossed = {
            use std::io::Read as _;
            use std::time::Instant;
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut buffer: Vec<u8> = Vec::new();
            let mut frames: Vec<(u16, Vec<u8>)> = Vec::new();
            let mut whole: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 4096];
            while Instant::now() < deadline {
                while let Ok(line) = lines.try_recv() {
                    println!("world> {line}");
                }
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut plain = chunk[..n].to_vec();
                        reading.apply(&mut plain);
                        whole.extend_from_slice(&plain);
                        buffer.extend_from_slice(&plain);
                        while !buffer.is_empty() {
                            match game::server::protocol::decode_game_message(&buffer, false) {
                                Ok((frame, used)) => {
                                    let bytes = buffer[..used].to_vec();
                                    let header = frame.header();
                                    buffer.drain(..used);
                                    frames.push((header, bytes));
                                }
                                Err(_) => break,
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
            (frames, whole)
        };
        let (crossed, plain) = crossed;
        let headers: Vec<u16> = crossed.iter().map(|(header, _)| *header).collect();
        println!("the crossing answered: {headers:04X?}");
        while let Ok(line) = lines.try_recv() {
            println!("world> {line}");
        }
        // The redirect is read by hand, the way GWPS's own build frames it: `0x01A6` is 46 bytes (a
        // two-byte header, host[24] and five dwords) and the `0x0099` after it is twelve
        // (`ASURA/crates/asura-world/tests/load.rs:1082-1110`, `read_redirect`). The table's own walker
        // cannot split them, which is what `0099`'s five-byte login row is about.

        let at = plain
            .windows(2)
            .position(|window| window == [0xA6, 0x01])
            .expect("the redirect is in the crossing's answer");
        let transfer = &plain[at..at + 46];
        let update = &plain[at + 46..at + 58];
        assert_eq!(
            &update[0..2],
            &[0x99, 0x00],
            "the map update follows the redirect"
        );
        assert_eq!(update.len(), 12, "the redirect form is twelve bytes");
        // The four frames after the pair are in the stream whether or not the table's walker can split
        // them: the party size, the party, the player withdrawn and its agent removed
        // (`ASURA/crates/asura-world/src/travel.rs:498-518`).
        for header in [0xB0u8, 0xB1, 0x5A, 0x21] {
            assert!(
                plain.windows(2).any(|window| window == [header, 0x00]),
                "0x{header:02X}00 is in the crossing's answer"
            );
        }
        let dword = |at: usize, bytes: &[u8]| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let player_token = dword(34, transfer);
        assert_ne!(player_token, 0, "a token to reconnect with");
        assert_eq!(dword(38, transfer), 0, "is_explorable: ASURA's default 0");
        assert_eq!(
            dword(42, transfer),
            TO,
            "the redirect names the map the arch leads to"
        );
        assert_eq!(dword(4, update), TO, "and the map update agrees");
        println!("the redirect carries map {TO} and token 0x{player_token:08X}");

        // The connection the client opens next, exactly as the client does it: the same world, the
        // token the redirect carried.
        let version = GameVersion {
            version: 39167,
            h0008: 1,
            map_token: 2,
            map_id: TO,
            player_token,
            account,
            character,
            h0038: 0,
            h003c: 0,
        };
        let (mut stream, mut reading, mut sending) = open_connection(port, version);
        let announced = wait_for(&lines, "is loading - ", Duration::from_secs(30));
        println!("the arrival: {announced}");
        assert!(
            announced.contains("Lakeside County (146)"),
            "the arriving connection loads the map the arch leads to: {announced}"
        );
        let block_bytes: usize = announced
            .split(" message(s), ")
            .nth(1)
            .and_then(|rest| rest.split(" bytes").next())
            .and_then(|count| count.trim().parse().ok())
            .expect("the load's size is in its log line");
        let mut block = read_exact(&mut stream, block_bytes);
        reading.apply(&mut block);
        let mut ping = read_exact(&mut stream, 8);
        reading.apply(&mut ping);
        let mut ask = vec![0x88, 0x80];
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the spawn ask");
        let mut spawn = read_exact(&mut stream, 26);
        reading.apply(&mut spawn);
        assert_eq!(&spawn[..2], &[0x96, 0x01], "the arrival's entry point");
        let landed = (
            f32::from_le_bytes([spawn[6], spawn[7], spawn[8], spawn[9]]),
            f32::from_le_bytes([spawn[10], spawn[11], spawn[12], spawn[13]]),
        );
        assert!(
            (landed.0 - ARRIVAL.0).abs() <= 1.0 && (landed.1 - ARRIVAL.1).abs() <= 1.0,
            "the character stands on the arrival map 146 states for someone coming from 148: {landed:?} against {ARRIVAL:?}"
        );
        println!("landed at {landed:?} on map {TO}");

        // The arrival's own load finishes: the players ask is what writes the reached instance.
        let mut ask = vec![0x90, 0x80];
        ask.extend_from_slice(&[0u8; 17]);
        sending.apply(&mut ask);
        stream.write_all(&ask).expect("the players ask");
        let players = read_frames(&mut stream, &mut reading, 0x018E, 1);
        assert_eq!(
            players.last().map(|(header, _)| *header),
            Some(0x018E),
            "the arrival's load is over"
        );

        // And the arrival unlocked the map it reached (R12, `player.rs:442-462`).
        let repository =
            game::characters::DatabaseCharacters::new(Arc::clone(&characters), Log::new());
        let unlocked = repository
            .unlocked_maps(&uuid_text)
            .expect("the unlocks are readable");
        assert!(
            unlocked.contains(&TO),
            "the map the character walked into is unlocked: {unlocked:?}"
        );
    }));

    {
        let mut pool = characters.lock().unwrap();
        for statement in [
            "DELETE FROM \"item_instance\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"character_unlock\" WHERE \"character_uuid\" = $1",
            "DELETE FROM \"characters\" WHERE \"uuid\" = $1",
        ] {
            let _ = pool.query_with(statement, &[&uuid_text]);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    while let Ok(line) = lines.try_recv() {
        println!("world> {line}");
    }
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
