//! The boundary, as a check: `data/sql` is schema, and content is the pack.
//!
//! `docs/port/04-configuration.md` states it and `docs/decisions/0014-the-world-content-is-a-pack.md`
//! records it: a map record, its entry points and its doorways are the world pack's files, read at
//! start-up; a database row is for what a server *writes*. So a schema file may create, alter or drop
//! a table, and may never seed one - if a fresh server needs rows fed to it before it serves, the
//! content is in the wrong place.
//!
//! This is the rule with teeth. It fails by naming the file and the statement, which is what an
//! unenforced rule never does.

use std::path::{Path, PathBuf};

/// Every statement that writes rows, as opposed to shaping the schema.
const SEEDING: [&str; 3] = ["INSERT INTO", "UPDATE ", "DELETE FROM"];

/// The files applied before this rule existed. They are history: an applied migration is immutable,
/// so they stay as they were and `0013-content-is-the-pack.sql` removes what they seeded, once.
const HISTORY: [&str; 7] = [
    "0002-maps.sql",
    "0006-map-148-file-id.sql",
    "0007-map-instance-value.sql",
    "0008-map-doorways.sql",
    "0009-map-spawn-angle.sql",
    "0011-map-catalogue.sql",
    "0012-map-content.sql",
];

/// A statement a schema file may still make: the updater's own version record, and removing content
/// rows that an earlier migration seeded (`0013-content-is-the-pack.sql`). Content is never written.
fn allowed(statement: &str) -> bool {
    if statement.starts_with("INSERT INTO \"schema_version\"")
        || statement.starts_with("INSERT INTO schema_version")
    {
        return true;
    }
    if let Some(rest) = statement.strip_prefix("DELETE FROM \"") {
        for table in [
            "map\"",
            "map_spawn\"",
            "map_arch\"",
            "map_arrival\"",
            "item_template\"",
        ] {
            if rest.starts_with(table) {
                return true;
            }
        }
    }
    false
}

#[test]
fn no_schema_file_seeds_content() {
    let mut seeding = Vec::new();
    for file in sql_files(Path::new("../data/sql")) {
        if HISTORY.contains(
            &file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
        ) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("a schema file reads");
        for (number, line) in text.lines().enumerate() {
            let statement = line.trim_start();
            // A comment that names the statement is documentation, not a seed.
            if statement.starts_with("--") || allowed(statement) {
                continue;
            }
            for pattern in SEEDING {
                if statement.starts_with(pattern) {
                    seeding.push(format!("{}:{}: {statement}", file.display(), number + 1));
                }
            }
        }
    }
    assert!(
        seeding.is_empty(),
        "these schema files seed rows, which is content in SQL - it belongs in the world pack \
         (docs/decisions/0014-the-world-content-is-a-pack.md):\n{}",
        seeding.join("\n")
    );
}

/// Every `.sql` file under `root`, recursing.
fn sql_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(sql_files(&path));
        } else if path.extension().and_then(|kind| kind.to_str()) == Some("sql") {
            files.push(path);
        }
    }
    files
}
