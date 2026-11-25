//! Settings: each program's own JSON file, shown here and changed here one value at a time.
//!
//! The values are the file's, not the program's effective ones: a program reads its file at
//! start-up and applies the environment over it, so a change made here takes effect when the
//! program is restarted. The passwords in a connection string are masked by the settings reader
//! itself (`Config::describe`), so what this area answers is safe to paste, and a masked setting is
//! never changed from here.
//!
//! A program's `.dist` file beside its settings file is the list of what the program reads. A
//! setting the `.dist` states and the file does not is shown with the `.dist`'s value, marked as
//! not in the file, and saving it writes it into the file.
//!
//! A change rewrites only the value's own characters, so the file keeps its order, its layout and
//! its `doc` lines. Before anything is written the changed text is read back, and the write is
//! refused unless exactly that one setting differs.

use serde_json::{Value, json};

use common::configuration::{Config, Policy};

use crate::api::{Call, Failure, Reply, Route, bad, broken, get, missing, post, unavailable};
use crate::state::Manage;

/// The names that describe a file rather than configure something
/// (`common/src/Configuration/Json.rs`, `METADATA`).
const METADATA: [&str; 4] = ["doc", "note", "immutable", "information"];

/// The settings area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/settings", list),
        post("/api/v1/settings/{program}/set", set),
    ]
}

/// One setting as a file states it.
#[derive(Debug, Clone, PartialEq)]
struct Leaf {
    /// The dotted key, as the settings reader spells it.
    key: String,
    /// The value itself.
    value: Value,
    /// The `doc` beside it, when the setting is written in the envelope form.
    doc: Option<String>,
    /// Whether the value is bytes (`hex`) rather than a `value`.
    hex: bool,
}

impl Leaf {
    /// What the page offers for it: a number, a flag, text, or nothing it can edit.
    fn kind(&self) -> &'static str {
        match &self.value {
            _ if self.hex => "bytes",
            Value::Number(_) => "number",
            Value::Bool(_) => "flag",
            Value::String(_) => "text",
            _ => "list",
        }
    }

    /// Whether this area may change it: one plain value that is not a secret.
    fn editable(&self) -> bool {
        matches!(self.kind(), "number" | "flag" | "text") && !self.key.ends_with("DatabaseInfo")
    }
}

/// Every setting of a parsed file, walked as the settings reader walks it
/// (`common/src/Configuration/Json.rs`, `flatten`).
fn leaves(node: &Value, at: &str, out: &mut Vec<Leaf>) {
    let Value::Object(section) = node else {
        if !node.is_null() && !at.is_empty() {
            out.push(Leaf { key: at.to_string(), value: node.clone(), doc: None, hex: false });
        }
        return;
    };
    if section.contains_key("value") || section.contains_key("hex") {
        let hex = section.contains_key("hex");
        let value = section.get("value").or_else(|| section.get("hex")).cloned();
        if let Some(value) = value
            && !at.is_empty()
        {
            let doc = section.get("doc").and_then(Value::as_str).map(str::to_string);
            out.push(Leaf { key: at.to_string(), value, doc, hex });
        }
        return;
    }
    for (key, child) in section {
        if METADATA.contains(&key.as_str()) {
            continue;
        }
        let path = if at.is_empty() { key.clone() } else { format!("{at}.{key}") };
        leaves(child, &path, out);
    }
}

/// Every setting of a file's text.
fn leaves_of(text: &str) -> Result<Vec<Leaf>, String> {
    let root: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    leaves(&root, "", &mut out);
    Ok(out)
}

/// The `.dist` file beside a settings file: what the program reads, with its defaults.
fn dist_path(path: &str) -> String {
    format!("{path}.dist")
}

/// One program's file as rows of key and masked value, sorted by key.
pub fn rows(path: &str) -> Result<Vec<(String, String)>, String> {
    let config =
        Config::load_json(path, false, Policy::default()).map_err(|error| error.to_string())?;
    let mut rows = config.describe();
    rows.sort();
    Ok(rows)
}

/// One program's settings as the page shows them: the file's own, then the ones only its `.dist`
/// states.
fn described(path: &str) -> Result<Vec<Value>, String> {
    let row = |key: &str, value: &str, leaf: Option<&Leaf>, in_file: bool| {
        json!({
            "key": key,
            "value": value,
            "doc": leaf.and_then(|leaf| leaf.doc.clone()),
            "kind": leaf.map_or("list", Leaf::kind),
            "editable": leaf.is_some_and(Leaf::editable),
            "in_file": in_file,
        })
    };
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let own = leaves_of(&text)?;
    let mut shown: Vec<Value> = rows(path)?
        .iter()
        .map(|(key, value)| row(key, value, own.iter().find(|leaf| &leaf.key == key), true))
        .collect();

    // The `.dist` is optional: a file with none beside it is shown as it is.
    let dist = dist_path(path);
    if let (Ok(dist_text), Ok(dist_rows)) = (std::fs::read_to_string(&dist), rows(&dist)) {
        let defaults = leaves_of(&dist_text)?;
        for (key, value) in &dist_rows {
            if own.iter().all(|leaf| &leaf.key != key) {
                shown.push(row(key, value, defaults.iter().find(|leaf| &leaf.key == key), false));
            }
        }
    }
    shown.sort_by(|left, right| left["key"].as_str().cmp(&right["key"].as_str()));
    Ok(shown)
}

/// One program's settings as the API states them.
fn program(id: &str, label: &str, setting: &str, path: Option<&str>) -> Value {
    let Some(path) = path else {
        return json!({
            "id": id, "label": label, "path": Value::Null, "rows": [],
            "error": format!("No settings file is named in manage.json ({setting})."),
        });
    };
    match described(path) {
        Ok(rows) => json!({ "id": id, "label": label, "path": path, "rows": rows }),
        Err(error) => json!({ "id": id, "label": label, "path": path, "rows": [], "error": error }),
    }
}

/// `GET /api/v1/settings`: the world's and the login's settings files.
fn list(manage: &Manage, _: &Call) -> Reply {
    Ok(json!({
        "programs": [
            program("world", "World", "WorldConfigFile", manage.world_config.as_deref()),
            program("login", "Login", "AuthConfigFile", manage.auth_config.as_deref()),
        ],
    }))
}

// ---- finding a value in the file's text ----------------------------------------------------------

/// The byte at a position, or 0 past the end.
fn byte(text: &[u8], at: usize) -> u8 {
    text.get(at).copied().unwrap_or(0)
}

/// The first position at or after `at` that is not white space.
fn skip_space(text: &[u8], mut at: usize) -> usize {
    while matches!(byte(text, at), b' ' | b'\t' | b'\r' | b'\n') {
        at += 1;
    }
    at
}

/// The position after the string that opens at `at`.
fn string_end(text: &[u8], at: usize) -> Option<usize> {
    let mut at = at + 1;
    loop {
        match byte(text, at) {
            0 => return None,
            b'\\' => at += 2,
            b'"' => return Some(at + 1),
            _ => at += 1,
        }
    }
}

/// The position after the value that starts at `at`.
fn value_end(text: &[u8], at: usize) -> Option<usize> {
    match byte(text, at) {
        b'"' => string_end(text, at),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut at = at;
            loop {
                match byte(text, at) {
                    0 => return None,
                    b'"' => {
                        at = string_end(text, at)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(at + 1);
                        }
                    }
                    _ => {}
                }
                at += 1;
            }
        }
        0 => None,
        _ => {
            let mut at = at;
            while !matches!(byte(text, at), 0 | b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n') {
                at += 1;
            }
            Some(at)
        }
    }
}

/// The members of the object that opens at `at`: each name, and where its value starts and ends.
fn members(text: &str, at: usize) -> Option<Vec<(&str, usize, usize)>> {
    let bytes = text.as_bytes();
    if byte(bytes, at) != b'{' {
        return None;
    }
    let mut found = Vec::new();
    let mut at = skip_space(bytes, at + 1);
    if byte(bytes, at) == b'}' {
        return Some(found);
    }
    loop {
        if byte(bytes, at) != b'"' {
            return None;
        }
        let name_end = string_end(bytes, at)?;
        let name = text.get(at + 1..name_end - 1)?;
        at = skip_space(bytes, name_end);
        if byte(bytes, at) != b':' {
            return None;
        }
        let start = skip_space(bytes, at + 1);
        let end = value_end(bytes, start)?;
        found.push((name, start, end));
        at = skip_space(bytes, end);
        match byte(bytes, at) {
            b',' => at = skip_space(bytes, at + 1),
            b'}' => return Some(found),
            _ => return None,
        }
    }
}

/// Where the value of the setting `key` is written, inside the object that opens at `at`.
///
/// A section's name may itself hold a dot (`Logger` holds `network.trace`), so a name is matched
/// against the front of what is left of the key instead of the key being split at its dots.
fn locate(text: &str, at: usize, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    for (name, start, end) in members(text, at)? {
        if METADATA.contains(&name) {
            continue;
        }
        let object = byte(bytes, start) == b'{';
        if name == key {
            if !object {
                return Some((start, end));
            }
            // The envelope form: the value is the member called `value`.
            return members(text, start)?
                .into_iter()
                .find(|(inner, _, _)| *inner == "value")
                .map(|(_, start, end)| (start, end));
        }
        if object
            && let Some(rest) = key.strip_prefix(name).and_then(|rest| rest.strip_prefix('.'))
            && let Some(found) = locate(text, start, rest)
        {
            return Some(found);
        }
    }
    None
}

// ---- changing a value ----------------------------------------------------------------------------

/// The JSON a new value is written as, read from what the page sent and held to the kind the
/// setting already has.
fn token(leaf: &Leaf, call: &Call) -> Result<String, Failure> {
    let typed = call.text("value");
    match &leaf.value {
        Value::Bool(_) => call
            .flag("value")
            .map(|flag| flag.to_string())
            .ok_or_else(|| bad(format!("{} is a flag: true or false.", leaf.key))),
        Value::Number(number) => {
            if let Ok(whole) = typed.parse::<i64>() {
                return Ok(whole.to_string());
            }
            match typed.parse::<f64>() {
                Ok(real) if number.is_f64() && real.is_finite() => Ok(real.to_string()),
                _ => Err(bad(format!("{} is a number, and \"{typed}\" is not one.", leaf.key))),
            }
        }
        Value::String(_) => {
            if !typed.is_ascii() || typed.chars().any(char::is_control) {
                return Err(bad("A settings file is plain ASCII; that value is not."));
            }
            serde_json::to_string(&typed).map_err(|error| broken(error.to_string()))
        }
        _ => Err(bad(format!("{} is not one plain value; change it in the file.", leaf.key))),
    }
}

/// A file's text with one setting's value replaced, or with the setting added from the `.dist`.
fn changed(text: &str, key: &str, token: &str, adding: Option<&Leaf>) -> Result<String, Failure> {
    let Some(default) = adding else {
        let (start, end) = locate(text, 0, key)
            .ok_or_else(|| broken(format!("{key} could not be found in the file's text.")))?;
        return Ok(format!("{}{token}{}", &text[..start], &text[end..]));
    };
    if key.contains('.') {
        return Err(bad(format!(
            "{key} is inside a section the file does not have; copy the section from the .dist file by hand."
        )));
    }
    let close = text
        .rfind('}')
        .ok_or_else(|| broken("The file does not end with a closing brace."))?;
    let body = text[..close].trim_end();
    let line = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let name = serde_json::to_string(key).map_err(|error| broken(error.to_string()))?;
    let doc = match &default.doc {
        Some(doc) => format!(
            ",{line}    \"doc\": {}",
            serde_json::to_string(doc).map_err(|error| broken(error.to_string()))?
        ),
        None => String::new(),
    };
    Ok(format!(
        "{body},{line}  {name}: {{{line}    \"value\": {token}{doc}{line}  }}{line}{}",
        &text[close..]
    ))
}

/// Change one setting in a file's text. The answer is the new text; nothing is written here.
fn apply(text: &str, dist: Option<&str>, key: &str, call: &Call) -> Result<String, Failure> {
    let before = leaves_of(text).map_err(broken)?;
    let own = before.iter().find(|leaf| leaf.key == key).cloned();
    let default = match (&own, dist) {
        (Some(_), _) => None,
        (None, Some(dist)) => leaves_of(dist)
            .map_err(broken)?
            .into_iter()
            .find(|leaf| leaf.key == key),
        (None, None) => None,
    };
    let leaf = own
        .as_ref()
        .or(default.as_ref())
        .ok_or_else(|| missing(format!("No setting is called {key}.")))?;
    if !leaf.editable() {
        return Err(bad(format!("{key} is not changed from the panel; change it in the file.")));
    }
    let token = token(leaf, call)?;
    let text_after = changed(text, key, &token, default.as_ref())?;

    // Read the result back: the one setting holds the new value, and every other one is as it was.
    let wanted: Value = serde_json::from_str(&token).map_err(|error| broken(error.to_string()))?;
    let after = leaves_of(&text_after)
        .map_err(|error| broken(format!("The change would break the file ({error}); nothing was written.")))?;
    let others = |all: &[Leaf]| all.iter().filter(|leaf| leaf.key != key).cloned().collect::<Vec<Leaf>>();
    let holds = after.iter().any(|leaf| leaf.key == key && leaf.value == wanted);
    if !holds || others(&before) != others(&after) || !text_after.is_ascii() && text.is_ascii() {
        return Err(broken("The change did not read back as that one setting; nothing was written."));
    }
    Ok(text_after)
}

/// `POST /api/v1/settings/{program}/set`: write one setting's value into the program's file.
fn set(manage: &Manage, call: &Call) -> Reply {
    let (path, setting) = match call.param(0) {
        "world" => (manage.world_config.as_deref(), "WorldConfigFile"),
        "login" => (manage.auth_config.as_deref(), "AuthConfigFile"),
        _ => return Err(missing("No program has that name.")),
    };
    let path = path.ok_or_else(|| {
        unavailable(format!("No settings file is named in manage.json ({setting})."))
    })?;
    let key = call.text("key");
    if key.is_empty() {
        return Err(bad("A change names its setting."));
    }
    let text = std::fs::read_to_string(path).map_err(|error| broken(format!("{path}: {error}")))?;
    let dist = std::fs::read_to_string(dist_path(path)).ok();
    let text_after = apply(&text, dist.as_deref(), &key, call)?;
    std::fs::write(path, &text_after).map_err(|error| broken(format!("{path}: {error}")))?;

    let value = call.text("value");
    manage.say(format!("settings: {key} in {path} is now {value}"));
    Ok(json!({ "key": key, "value": value, "path": path }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::network::http::Request;

    const FILE: &str = "{\r\n  \"note\": \"a file\",\r\n  \"Port\": {\r\n    \"value\": 6112,\r\n    \"doc\": \"the port, value 6112\"\r\n  },\r\n  \"Name\": \"old\",\r\n  \"Network\": {\r\n    \"Trace\": { \"value\": false, \"doc\": \"d\" }\r\n  },\r\n  \"Logger\": {\r\n    \"network.trace\": { \"value\": [6, \"Console\"], \"doc\": \"d\" },\r\n    \"Level\": 4\r\n  },\r\n  \"LoginDatabaseInfo\": { \"value\": \"h;1;u;secret;d\" }\r\n}\r\n";
    const DIST: &str = "{\n  \"Port\": { \"value\": 1 },\n  \"CharacterSlots\": {\n    \"value\": 6,\n    \"doc\": \"how many\"\n  },\n  \"Portal\": { \"Bind\": { \"value\": \"x\" } }\n}\n";

    fn change(text: &str, key: &str, value: Value) -> Result<String, Failure> {
        let request = Request::new("POST", "/api/v1/settings/login/set");
        let call = Call { request: &request, params: Vec::new(), body: json!({ "key": key, "value": value }) };
        apply(text, Some(DIST), key, &call)
    }

    #[test]
    fn a_change_rewrites_only_the_value() {
        let after = change(FILE, "Port", json!("7000")).expect("a number is written");
        assert_eq!(after, FILE.replacen("\"value\": 6112", "\"value\": 7000", 1));

        let after = change(FILE, "Name", json!("new \"one\"")).expect("text is written");
        assert_eq!(after, FILE.replace("\"old\"", "\"new \\\"one\\\"\""));

        let after = change(FILE, "Network.Trace", json!(true)).expect("a flag is written");
        assert_eq!(after, FILE.replace("\"value\": false", "\"value\": true"));

        // A section whose own key holds a dot, and a bare value inside it.
        let after = change(FILE, "Logger.Level", json!(5)).expect("a nested number is written");
        assert_eq!(after, FILE.replace("\"Level\": 4", "\"Level\": 5"));
    }

    #[test]
    fn a_setting_only_the_dist_states_is_added_with_its_doc() {
        let after = change(FILE, "CharacterSlots", json!(32)).expect("the setting is added");
        assert!(after.ends_with(
            "\"LoginDatabaseInfo\": { \"value\": \"h;1;u;secret;d\" },\r\n  \"CharacterSlots\": {\r\n    \"value\": 32,\r\n    \"doc\": \"how many\"\r\n  }\r\n}\r\n"
        ), "{after}");
        assert!(after.starts_with(&FILE[..FILE.len() - 5]));
        // And it is then changed where it stands.
        let again = change(&after, "CharacterSlots", json!(8)).expect("it is changed");
        assert_eq!(again, after.replace("\"value\": 32", "\"value\": 8"));
    }

    /// Every setting the two programs' own `.dist` files offer for editing is found where it is
    /// written: saving it with the value it has leaves the file byte for byte as it was.
    #[test]
    fn every_editable_setting_of_the_dist_files_is_found_in_its_text() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        for file in ["authserver/authserver.json.dist", "worldserver/worldserver.json.dist"] {
            let text = std::fs::read_to_string(format!("{root}{file}")).expect("the .dist reads");
            let editable: Vec<Leaf> = leaves_of(&text)
                .expect("the .dist parses")
                .into_iter()
                .filter(Leaf::editable)
                .collect();
            assert!(editable.len() > 5, "{file} offers settings");
            for leaf in editable {
                let request = Request::new("POST", "/api/v1/settings/login/set");
                let body = json!({ "key": leaf.key, "value": leaf.value });
                let call = Call { request: &request, params: Vec::new(), body };
                let after = apply(&text, None, &leaf.key, &call)
                    .unwrap_or_else(|failure| panic!("{file} {}: {}", leaf.key, failure.error));
                assert_eq!(after, text, "{file} {}", leaf.key);
            }
        }
    }

    #[test]
    fn what_is_not_one_plain_value_is_refused() {
        let status = |key: &str, value: Value| change(FILE, key, value).expect_err("refused").status;
        assert_eq!(status("Port", json!("many")), 400);
        assert_eq!(status("Port", json!("1.5")), 400);
        assert_eq!(status("Network.Trace", json!("perhaps")), 400);
        assert_eq!(status("Logger.network.trace", json!("6")), 400);
        assert_eq!(status("LoginDatabaseInfo", json!("h;1;u;other;d")), 400);
        assert_eq!(status("Nothing", json!("1")), 404);
        // A setting inside a section the file lacks is not built here.
        assert_eq!(status("Portal.Bind", json!("y")), 400);
    }

    #[test]
    fn a_password_in_a_settings_file_is_masked() {
        let path =
            std::env::temp_dir().join(format!("kryta-manage-settings-{}.json", std::process::id()));
        std::fs::write(
            &path,
            "{\"WorldServerPort\":{\"value\":6113,\"doc\":\"d\"},\
             \"CharacterDatabaseInfo\":{\"value\":\"127.0.0.1;5432;kryta;hunter2secret;kryta_characters\",\"doc\":\"d\"}}",
        )
        .expect("the file is written");
        let rows = rows(&path.to_string_lossy()).expect("the file reads");
        let shown = described(&path.to_string_lossy()).expect("the file is described");
        let _ = std::fs::remove_file(&path);

        let value = |key: &str| {
            rows.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        assert_eq!(value("WorldServerPort"), "6113");
        let database = value("CharacterDatabaseInfo");
        assert!(!database.contains("hunter2secret"), "the password is masked: {database}");
        assert!(database.contains("kryta_characters"));
        assert!(rows.windows(2).all(|pair| pair[0].0 <= pair[1].0), "sorted by key");

        // The page's rows: the secret is masked there too and is not offered for editing.
        assert!(!serde_json::to_string(&shown).expect("json").contains("hunter2secret"));
        assert_eq!(shown[0]["key"], "CharacterDatabaseInfo");
        assert_eq!(shown[0]["editable"], false);
        assert_eq!(shown[1]["editable"], true);
        assert_eq!(shown[1]["kind"], "number");
        assert_eq!(shown[1]["doc"], "d");

        let shown = program("world", "World", "WorldConfigFile", None);
        assert!(shown["error"].as_str().is_some_and(|text| text.contains("WorldConfigFile")));
    }
}
