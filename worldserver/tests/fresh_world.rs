//! The fresh-database run: the world program against a database nobody has touched.
//!
//! Every other live test uses the databases this machine already has. This one makes its own: it
//! connects to the server's maintenance database with the credentials the world program's file
//! states, creates a database, and starts the real `worldserver` pointed at it through the
//! configuration's own environment override. What the run proves is the whole start-up a fresh
//! machine would see: the schema applied from `data/sql/base/db_world/`, the map rows the migrations
//! seed, the item templates seeded from the creation record and read back, and the listener up.
//!
//! It is `#[ignore]` because it needs a database server and the right to create a database on it; a
//! role without `CREATEDB` makes it say so and stop rather than fail, because nothing about this tree
//! is wrong in that case. Run with
//! `cargo test -p worldserver --test fresh_world -- --ignored --nocapture`.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use common::configuration::{Config, Policy};

/// The database this test makes, and drops again.
const FRESH_DATABASE: &str = "kryta_world_fresh";

/// The world database's connection fields, as the world program's own file states them.
fn world_database_info() -> (String, u16, String, String) {
    let config = Config::load_json("worldserver.json", false, Policy::default())
        .expect("the world program's own configuration");
    let (info, _) = config
        .value_string("WorldDatabaseInfo", "")
        .expect("the key is a connection string");
    let fields: Vec<&str> = info.split(';').collect();
    assert!(
        fields.len() >= 4,
        "the connection string has host, port, user and password: {info}"
    );
    (
        fields[0].to_string(),
        fields[1].parse().expect("the port is a number"),
        fields[2].to_string(),
        fields[3].to_string(),
    )
}

/// The server's maintenance connection, on which databases are made and dropped.
fn maintenance() -> postgres::Client {
    let (host, port, user, password) = world_database_info();
    let params = format!("host={host} port={port} user={user} password={password} dbname=postgres");
    postgres::Client::connect(&params, postgres::NoTls).expect("the database server answers")
}

/// Wait until the program has printed a line containing `needle`, answering every line it printed.
fn wait_for(reader: &Receiver<String>, needle: &str, timeout: Duration) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    let mut lines = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match reader.recv_timeout(remaining) {
            Ok(line) => {
                lines.push(line.clone());
                if line.contains(needle) {
                    return lines;
                }
            }
            Err(_) => {
                let printed = lines.join("\n");
                panic!("the program never printed '{needle}'; it printed:\n{printed}");
            }
        }
    }
}

/// Start the world program against the fresh database, answer its stdout through a channel, and
/// answer every line it printed while starting up.
fn start_world_on(reader: &mut Option<Receiver<String>>) -> Child {
    let (host, port, user, password) = world_database_info();
    let info = format!("{host};{port};{user};{password};{FRESH_DATABASE}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_worldserver"))
        .args(["-c", "worldserver/worldserver.json"])
        .current_dir("..")
        .env("AC_WORLD_DATABASE_INFO", info)
        .env("AC_WORLD_SERVER_PORT", "0")
        .env("AC_STATUS_INTERVAL", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the world program starts");
    let stdout = child.stdout.take().expect("the program's stdout is piped");
    let (tx, rx) = channel();
    *reader = Some(rx);
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                return;
            };
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    child
}

#[test]
#[ignore = "needs a database server, and the right to create a database on it"]
fn a_fresh_world_database_is_applied_seeded_and_served() {
    // Nothing may be left over from an interrupted run, or the schema apply would find half of it.
    let mut admin = maintenance();
    let _ = admin.batch_execute(&format!("DROP DATABASE IF EXISTS \"{FRESH_DATABASE}\""));
    match admin.batch_execute(&format!("CREATE DATABASE \"{FRESH_DATABASE}\"")) {
        Ok(()) => {}
        Err(error)
            if format!("{error:?}").contains("permission denied to create database")
                || error
                    .as_db_error()
                    .map(|db| db.code() == &postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
                    .unwrap_or(false) =>
        {
            // A role that cannot make a database needs another machine or a grant; the test says so
            // rather than failing, because nothing about this tree is wrong.
            println!(
                "skipped: this database role may not create a database (the server said: {error:?})"
            );
            return;
        }
        Err(error) => panic!("the fresh database is created: {error:?}"),
    }

    let mut reader = None;
    let mut child = start_world_on(&mut reader);
    let reader = reader.expect("the program's stdout is piped");
    let lines = wait_for(&reader, "READY: listening on", Duration::from_secs(120));
    // The world's loop runs: it calls the world's update and writes the status line on its own
    // interval (`WorldUpdateLoop`, `Main.cpp:584-620`), which this run has at one second.
    let mut lines = lines;
    lines.extend(wait_for(&reader, "status: up", Duration::from_secs(30)));
    let _ = child.kill();
    let _ = child.wait();

    let startup = lines.join("\n");
    // The schema applied from the files, and the migrations' own seeds - the maps and the one
    // outpost with its spawn point.
    assert!(
        startup.contains("file(s) applied, 0 already current")
            || startup.contains("file(s) applied"),
        "the fresh schema is applied:\n{startup}"
    );
    assert!(
        startup.contains("2 map(s), 1 spawn point(s)"),
        "the migrations' maps are read:\n{startup}"
    );
    assert!(
        startup.contains("Ascalon City (pre-Searing)"),
        "the seeded outpost is the world's:\n{startup}"
    );
    // The item templates, seeded from the creation record and read back.
    assert!(
        startup.contains("row(s) from 188 definition(s)"),
        "the templates are seeded:\n{startup}"
    );
    assert!(
        startup.contains("Item templates read back:"),
        "and read back:\n{startup}"
    );
    assert!(
        startup.contains("listening on 127.0.0.1:"),
        "the listener is up:\n{startup}"
    );

    admin
        .batch_execute(&format!(
            "DROP DATABASE IF EXISTS \"{FRESH_DATABASE}\" WITH (FORCE)"
        ))
        .expect("the fresh database is dropped again");
}
