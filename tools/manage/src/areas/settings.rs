//! Settings: each program's own JSON file, read here and never written.
//!
//! The values are the file's, not the program's effective ones: a program reads its file at
//! start-up and applies the environment over it. The passwords in a connection string are masked
//! by the settings reader itself (`Config::describe`), so what this area answers is safe to paste.

use serde_json::{Value, json};

use common::configuration::{Config, Policy};

use crate::api::{Call, Reply, Route, get};
use crate::state::Manage;

/// The settings area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![get("/api/v1/settings", list)]
}

/// One program's file as rows of key and masked value, sorted by key.
pub fn rows(path: &str) -> Result<Vec<(String, String)>, String> {
    let config =
        Config::load_json(path, false, Policy::default()).map_err(|error| error.to_string())?;
    let mut rows = config.describe();
    rows.sort();
    Ok(rows)
}

/// One program's settings as the API states them.
fn program(id: &str, label: &str, setting: &str, path: Option<&str>) -> Value {
    let Some(path) = path else {
        return json!({
            "id": id, "label": label, "path": Value::Null, "rows": [],
            "error": format!("No settings file is named in manage.json ({setting})."),
        });
    };
    match rows(path) {
        Ok(rows) => json!({
            "id": id, "label": label, "path": path,
            "rows": rows.iter().map(|(key, value)| json!({ "key": key, "value": value })).collect::<Vec<Value>>(),
        }),
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let shown = program("world", "World", "WorldConfigFile", None);
        assert!(shown["error"].as_str().is_some_and(|text| text.contains("WorldConfigFile")));
    }
}
