//! Overview: the numbers an operator checks first, and what needs attention.
//!
//! The counts and the checks are read from the other areas' own functions, so the overview never
//! keeps a second opinion. The services and the running world are separate processes: the page
//! asks for them through their own areas, beside this answer, so a slow owner never delays it.

use serde_json::{Value, json};

use crate::api::{Call, Reply, Route, broken, get};
use crate::state::Manage;

/// The overview area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![get("/api/v1/overview", overview)]
}

/// One line of the "needs attention" list.
fn item(level: &str, text: String, detail: String, href: &str) -> Value {
    json!({ "level": level, "text": text, "detail": detail, "href": href })
}

/// What the recordings folder needs: files whose header says they are not sanitised, and files
/// the index does not list. `rows` is the library as `recordings::scan` answers it.
pub fn recordings_attention(rows: &[Value]) -> Vec<Value> {
    let total = rows.len();
    let raw = rows.iter().filter(|row| row["sanitised"] == false).count();
    let unlisted = rows.iter().filter(|row| row["indexed"] == false).count();
    let mut items = Vec::new();
    if raw > 0 {
        items.push(item(
            "warn",
            format!("{raw} recording(s) are not sanitised"),
            "their headers say they hold names and ids".to_string(),
            "#/recordings",
        ));
    }
    if unlisted > 0 {
        items.push(item(
            "warn",
            format!("{unlisted} recording(s) are not in the index"),
            format!("{} of {total} are listed", total - unlisted),
            "#/recordings",
        ));
    }
    if items.is_empty() {
        items.push(item(
            "ok",
            "Recordings are sanitised and indexed".to_string(),
            format!("{total} file(s)"),
            "#/recordings",
        ));
    }
    items
}

/// What the schema needs: files changed after they were applied, and files not applied yet.
/// `states` is every file's state, across the databases.
pub fn schema_attention(states: &[&str]) -> Value {
    let count = |state: &str| states.iter().filter(|found| **found == state).count();
    let (changed, new, gone) = (count("changed"), count("new"), count("recorded, not on disk"));
    if changed + new + gone == 0 {
        return item(
            "ok",
            "Schema is current".to_string(),
            format!("{} file(s) applied, none changed", states.len()),
            "#/schema",
        );
    }
    item(
        if changed > 0 { "bad" } else { "warn" },
        format!("Schema: {changed} changed, {new} new, {gone} recorded but not on disk"),
        "a changed file is refused by the updater until it is resolved".to_string(),
        "#/schema",
    )
}

/// What the characters need: any whose item rows have no type. `unsound` is their names.
pub fn characters_attention(checked: usize, unsound: &[String]) -> Value {
    if unsound.is_empty() {
        return item(
            "ok",
            "Character data is sound".to_string(),
            format!("{checked} character(s) checked"),
            "#/players",
        );
    }
    item(
        "bad",
        format!("{} character(s) have item rows with no type", unsound.len()),
        unsound.join(", "),
        "#/players",
    )
}

/// The characters whose rows are unsound, and how many were checked.
fn unsound_characters(manage: &Manage) -> Option<(usize, Vec<String>)> {
    let characters = manage.characters().ok()?;
    let all = characters.list();
    let unsound = all
        .iter()
        .filter(|character| {
            let uuid = game::characters::uuid_text(&character.uuid);
            super::players::soundness_of(characters, &uuid).is_some_and(|check| !check.sound())
        })
        .map(|character| character.name.clone())
        .collect();
    Some((all.len(), unsound))
}

/// `GET /api/v1/overview`: the counts and the attention list.
fn overview(manage: &Manage, _: &Call) -> Reply {
    let accounts = manage
        .accounts()
        .list()
        .map_err(|error| broken(format!("The account table said: {error}")))?;
    let characters = manage
        .characters()
        .ok()
        .and_then(|characters| characters.count().ok());
    let maps = manage
        .world_pack()
        .ok()
        .and_then(|pack| pack.load_maps().ok())
        .map(|(maps, _)| maps.len());

    let mut attention = recordings_attention(&super::recordings::scan());
    let schema = super::schema::databases(manage);
    let states: Vec<&str> = schema
        .iter()
        .flat_map(|(_, files)| files.iter().map(|file| file.state))
        .collect();
    attention.push(schema_attention(&states));
    if let Some((checked, unsound)) = unsound_characters(manage) {
        attention.push(characters_attention(checked, &unsound));
    }

    Ok(json!({
        "counts": {
            "accounts": accounts.len(),
            "active_accounts": accounts.iter().filter(|account| account.active).count(),
            "characters": characters,
            "maps": maps,
        },
        "attention": attention,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsanitised_and_unlisted_recordings_are_called_out() {
        let rows = [
            json!({"sanitised": false, "indexed": false}),
            json!({"sanitised": true, "indexed": true}),
            json!({"sanitised": null, "indexed": false}),
        ];
        let items = recordings_attention(&rows);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["text"], "1 recording(s) are not sanitised");
        assert_eq!(items[1]["text"], "2 recording(s) are not in the index");
        assert_eq!(items[1]["detail"], "1 of 3 are listed");

        let clean = recordings_attention(&[json!({"sanitised": true, "indexed": true})]);
        assert_eq!(clean.len(), 1);
        assert_eq!(clean[0]["level"], "ok");
    }

    #[test]
    fn a_changed_schema_file_is_worse_than_a_new_one() {
        assert_eq!(schema_attention(&["applied", "applied"])["level"], "ok");
        assert_eq!(schema_attention(&["applied", "new"])["level"], "warn");
        let changed = schema_attention(&["changed", "new", "applied"]);
        assert_eq!(changed["level"], "bad");
        assert_eq!(changed["text"], "Schema: 1 changed, 1 new, 0 recorded but not on disk");
    }

    #[test]
    fn unsound_characters_are_named() {
        assert_eq!(characters_attention(4, &[])["level"], "ok");
        let bad = characters_attention(4, &["Player One".to_string()]);
        assert_eq!(bad["level"], "bad");
        assert_eq!(bad["detail"], "Player One");
    }
}
