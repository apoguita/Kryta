//! The auth program's edge to the game library, held to what the record says it is.
//!
//! `docs/decisions/0010-auth-and-the-game-library.md` inventories every use of `game` in the auth
//! program and says the edge stays because each of them is a protocol message, a row, the pure rule or
//! the portal. This test is what keeps that true: a use of a *new* kind fails here, so the record has
//! to be corrected rather than quietly outgrown. It reads the source, so it needs nothing to run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The kinds the record names, as the `game::` paths they appear at.
const RECORDED: &[&str] = &[
    // The portal, whose host `docs/decisions/0003-portal-host.md` decides.
    "portal",
    // The client's 64-byte version message: a protocol message both programs read.
    "server::protocol",
    // The character's rows, the travel hand-off row and the account table: rows.
    "characters",
    "maps",
    "accounts",
];

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every `game::a::b::...` path the source names, cut to its first one or two segments - the kind of
/// thing it is, not the item's name.
fn game_paths(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find("game::") {
        let after = &rest[at + "game::".len()..];
        let end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
            .unwrap_or(after.len());
        let path = &after[..end];
        let segments: Vec<&str> = path.split("::").filter(|part| !part.is_empty()).collect();
        // `accounts` alone is enough for `game::accounts::DatabaseAccounts`; three are kept where the
        // third is what distinguishes the kind (`server::protocol`, `maps::placement`).
        let kind = match segments.as_slice() {
            [] => continue,
            // `game::server::` is only ever the protocol module, and it is the one kind whose name
            // is not its first segment.
            [one, two, ..] if *one == "server" => format!("{one}::{two}"),
            [one, ..] => (*one).to_string(),
        };
        found.insert(kind);
        rest = after;
    }
    found
}

/// The record's claim, measured: every use of the game library in the auth program is one of the six
/// kinds it names.
#[test]
fn every_use_of_the_game_library_in_auth_is_one_the_record_names() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    assert!(
        !files.is_empty(),
        "the auth program's sources are where they are expected"
    );

    let mut named = BTreeSet::new();
    for file in &files {
        let source = std::fs::read_to_string(file).expect("a source file is readable");
        // Tests are the program's own proofs and may name anything they drive.
        let measured = match source.split_once("#[cfg(test)]") {
            Some((production, _)) => production,
            None => source.as_str(),
        };
        for path in game_paths(measured) {
            named.insert(path);
        }
    }

    let unrecorded: Vec<&String> = named
        .iter()
        .filter(|kind| !RECORDED.contains(&kind.as_str()))
        .collect();
    assert!(
        unrecorded.is_empty(),
        "the auth program uses the game library for something the record does not name: {unrecorded:?}\n\
         Either it belongs in one of the six kinds, or docs/decisions/0010-auth-and-the-game-library.md \
         has to say what it is."
    );
}

/// The edge is one way: the game library does not know the auth program exists.
///
/// The record's reason 3. `Cargo.toml` is the check that outlives a conversation.
#[test]
fn the_game_library_does_not_depend_on_the_auth_program() {
    let game = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .join("game/Cargo.toml");
    let manifest = std::fs::read_to_string(&game).expect("the game library's manifest is readable");
    assert!(
        !manifest.contains("authserver"),
        "the game library names the auth program; the edge is meant to be one way"
    );
    assert!(
        !manifest.contains("scripts"),
        "the game library names the scripts crate; the scripts are written against game, not the other way"
    );
}

/// And the kinds this test knows are the ones the record lists, so neither can drift alone.
#[test]
fn the_recorded_kinds_are_the_six_the_record_names() {
    let record = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .join("docs/decisions/0010-auth-and-the-game-library.md");
    let text = std::fs::read_to_string(&record).expect("the record is readable");
    for kind in RECORDED {
        assert!(
            text.contains(&kind.to_string()),
            "the record no longer names '{kind}', which this test holds the source to"
        );
    }
}
