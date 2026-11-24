//! Schema: each database's own record of the files it has applied, against the files on disk.
//!
//! A database records each schema file it has had in its `updates` table, read here by the
//! updater's own reader. `new` has not been applied; `changed` was edited after it was applied,
//! which the updater refuses to apply again; `recorded, not on disk` is a record for a file this
//! checkout does not carry.

use serde_json::{Value, json};

use crate::api::{Call, Reply, Route, get};
use crate::state::{DatabaseSchema, Manage};

/// The schema area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![get("/api/v1/schema", list)]
}

/// One schema file, as the database's record and the disk agree or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFile {
    /// The file's name, the one the updater records.
    pub name: String,
    /// `applied`, `changed`, `new`, or `recorded, not on disk`.
    pub state: &'static str,
}

/// Compare what a database recorded with what is on disk.
///
/// `applied` is the record: name and hash. `on_disk` is the files: name and the hash of the text
/// they hold now.
pub fn compare(applied: &[(String, String)], on_disk: &[(String, String)]) -> Vec<SchemaFile> {
    let mut rows: Vec<SchemaFile> = on_disk
        .iter()
        .map(|(name, hash)| {
            let state = match applied.iter().find(|(recorded, _)| recorded == name) {
                Some((_, recorded_hash)) if recorded_hash.eq_ignore_ascii_case(hash) => "applied",
                Some(_) => "changed",
                None => "new",
            };
            SchemaFile { name: name.clone(), state }
        })
        .collect();
    for (name, _) in applied {
        if !on_disk.iter().any(|(file, _)| file == name) {
            rows.push(SchemaFile { name: name.clone(), state: "recorded, not on disk" });
        }
    }
    rows
}

/// One database's files with their states.
fn files_of(database: &DatabaseSchema) -> Vec<SchemaFile> {
    let applied = match database.pool.lock() {
        Ok(pool) => database::updater::apply::read_applied(&pool).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let on_disk: Vec<(String, String)> =
        database::updater::collect_files(database.dir, database::updater::UpdateState::Released)
            .unwrap_or_default()
            .iter()
            .map(|file| {
                let text = std::fs::read_to_string(&file.path).unwrap_or_default();
                (file.name(), database::updater::apply::sha1_hex(&text))
            })
            .collect();
    compare(&applied, &on_disk)
}

/// Every database with its files.
pub fn databases(manage: &Manage) -> Vec<(&'static str, Vec<SchemaFile>)> {
    manage
        .schemas
        .iter()
        .map(|database| (database.label, files_of(database)))
        .collect()
}

/// `GET /api/v1/schema`.
fn list(manage: &Manage, _: &Call) -> Reply {
    let databases: Vec<Value> = databases(manage)
        .iter()
        .map(|(label, files)| json!({
            "label": label,
            "files": files.iter().map(|file| json!({ "name": file.name, "state": file.state })).collect::<Vec<Value>>(),
        }))
        .collect();
    Ok(json!({ "databases": databases }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, hash: &str) -> (String, String) {
        (name.to_string(), hash.to_string())
    }

    #[test]
    fn a_file_is_applied_changed_new_or_gone() {
        let applied = [pair("a.sql", "AA11"), pair("b.sql", "bb22"), pair("gone.sql", "cc33")];
        let on_disk = [pair("a.sql", "aa11"), pair("b.sql", "edited"), pair("c.sql", "dd44")];
        let rows = compare(&applied, &on_disk);
        let state = |name: &str| rows.iter().find(|row| row.name == name).map(|row| row.state);
        assert_eq!(state("a.sql"), Some("applied"), "hashes compare without case");
        assert_eq!(state("b.sql"), Some("changed"));
        assert_eq!(state("c.sql"), Some("new"));
        assert_eq!(state("gone.sql"), Some("recorded, not on disk"));
        assert_eq!(rows.len(), 4);
    }
}
