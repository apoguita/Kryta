//! Flavors: the server setups this machine holds, which one it runs, and making another
//! (`docs/decisions/0016-a-server-is-a-flavor.md`).
//!
//! A flavor is a folder under `<ConfigFolder>/flavors/`, and `<ConfigFolder>/flavor.json` names the
//! one the programs read when they start. This area lists the folders, writes that one name, and
//! makes a new flavor as a copy of an existing one. The programs read the name at start-up, so a
//! flavor chosen here is run from the next restart of the stack; the panel's other pages follow
//! the name at once.
//!
//! Nothing here removes or renames a flavor: a flavor's name is also a folder on every client
//! that connected to it, and removing a server's whole setup is done in the files, on purpose.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use common::configuration::{ACTIVE_FLAVOR_FILE, FLAVORS, Flavor, is_flavor_name};

use crate::api::{Call, Failure, Reply, Route, bad, broken, conflict, get, missing, post};
use crate::areas::settings::replaced;
use crate::state::Manage;

/// The flavors area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/flavors", list),
        post("/api/v1/flavors/activate", activate),
        post("/api/v1/flavors/create", create),
    ]
}

/// The two settings files a flavor must hold for the programs to start on it.
const PROGRAM_FILES: [&str; 2] = ["authserver.json", "worldserver.json"];
/// The content folders a flavor can hold (`config/flavors/README.md`).
const CONTENT_FOLDERS: [&str; 3] = ["World", "Server", "Client"];

/// A path as the page prints it.
fn shown(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// How many maps a folder holds: one `basic.json` each (`docs/decisions/0014-the-world-content-is-a-pack.md`).
fn maps_under(folder: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => maps_under(&entry.path()),
            Ok(kind) if kind.is_file() && entry.file_name() == "basic.json" => 1,
            _ => 0,
        })
        .sum()
}

/// The names of the flavor folders under a configuration folder, sorted.
fn names(config: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(config.join(FLAVORS))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| is_flavor_name(name))
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names
}

/// What a configuration folder holds: every flavor, and the one the machine runs.
fn described(config: &Path) -> Value {
    let active = Flavor::active(config);
    let running = active.as_ref().ok().map(|flavor| flavor.name.clone());
    let flavors: Vec<Value> = names(config)
        .into_iter()
        .map(|name| {
            let folder = config.join(FLAVORS).join(&name);
            let lacks: Vec<&str> = PROGRAM_FILES
                .into_iter()
                .filter(|file| !folder.join(file).is_file())
                .collect();
            let holds: Vec<&str> = CONTENT_FOLDERS
                .into_iter()
                .filter(|part| folder.join(part).is_dir())
                .collect();
            json!({
                "name": name,
                "active": running.as_deref() == Some(name.as_str()),
                "path": shown(&folder),
                "holds": holds,
                "lacks": lacks,
                "maps": maps_under(&folder.join("World").join("maps")),
            })
        })
        .collect();
    json!({
        "config_folder": shown(config),
        "active_file": shown(&config.join(ACTIVE_FLAVOR_FILE)),
        "active": running,
        "error": active.err(),
        "flavors": flavors,
    })
}

/// A flavor's folder, for a name that is one and has one.
fn folder_of(config: &Path, name: &str) -> Result<PathBuf, Failure> {
    if !is_flavor_name(name) {
        return Err(bad("A flavor's name is 1 to 64 letters, digits, '_' and '-'."));
    }
    let folder = config.join(FLAVORS).join(name);
    if !folder.is_dir() {
        return Err(missing(format!("There is no flavor called {name}.")));
    }
    Ok(folder)
}

/// Name `name` as the flavor the machine runs: one value of `flavor.json` is rewritten, and the
/// file is made when it is not there. The answer is the file written.
fn choose(config: &Path, name: &str) -> Result<String, Failure> {
    let folder = folder_of(config, name)?;
    if let Some(file) = PROGRAM_FILES.into_iter().find(|file| !folder.join(file).is_file()) {
        return Err(conflict(format!(
            "{name} has no {file}, and a program started on it would stop. Add the file first."
        )));
    }
    let file = config.join(ACTIVE_FLAVOR_FILE);
    let token = serde_json::to_string(name).map_err(|error| broken(error.to_string()))?;
    let fresh = || format!("{{\n  \"flavor\": {{\n    \"value\": {token}\n  }}\n}}\n");
    let text = match std::fs::read_to_string(&file) {
        // A file that does not name a flavor where one can be written is replaced whole.
        Ok(text) => replaced(&text, "flavor", &token).unwrap_or_else(fresh),
        Err(_) => fresh(),
    };
    // Read the result back before it replaces the file: it names exactly this flavor.
    let names_it = serde_json::from_str::<Value>(&text).ok().is_some_and(|root| {
        let stated = &root["flavor"];
        stated.get("value").unwrap_or(stated) == &Value::String(name.to_string())
    });
    if !names_it {
        return Err(broken("The change did not read back as that flavor; nothing was written."));
    }
    std::fs::write(&file, text).map_err(|error| broken(format!("{}: {error}", file.display())))?;
    Flavor::active(config).map_err(broken)?;
    Ok(shown(&file))
}

/// Copy a folder and everything under it into a folder that does not exist yet. The answer is
/// how many files were copied.
fn copy_folder(from: &Path, to: &Path) -> std::io::Result<usize> {
    std::fs::create_dir(to)?;
    let mut copied = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copied += copy_folder(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Make the flavor `name` as a copy of the flavor `from`. The answer is the new folder and how
/// many files it holds.
fn copy(config: &Path, name: &str, from: &str) -> Result<(String, usize), Failure> {
    if !is_flavor_name(name) {
        return Err(bad("A flavor's name is 1 to 64 letters, digits, '_' and '-'."));
    }
    let source = folder_of(config, from)?;
    // A name is a folder on the server and on every client, and Windows does not tell two names
    // apart by their case: one that differs from an existing flavor only in case is refused.
    if let Some(taken) = names(config).into_iter().find(|taken| taken.eq_ignore_ascii_case(name)) {
        return Err(conflict(format!("There is already a flavor called {taken}.")));
    }
    let target = config.join(FLAVORS).join(name);
    if target.exists() {
        return Err(conflict(format!("{} already exists.", shown(&target))));
    }
    match copy_folder(&source, &target) {
        Ok(copied) => Ok((shown(&target), copied)),
        Err(error) => {
            // The folder is this call's own and is not left half made.
            let _ = std::fs::remove_dir_all(&target);
            Err(broken(format!("{from} could not be copied to {}: {error}", shown(&target))))
        }
    }
}

/// `GET /api/v1/flavors`: every flavor, and the one this machine runs.
fn list(manage: &Manage, _: &Call) -> Reply {
    Ok(described(Path::new(&manage.config_folder)))
}

/// `POST /api/v1/flavors/activate`: name the flavor the programs read from their next start.
fn activate(manage: &Manage, call: &Call) -> Reply {
    let name = call.text("name");
    let file = choose(Path::new(&manage.config_folder), &name)?;
    manage.say(format!("flavors: {file} now names {name}"));
    Ok(json!({ "active": name, "file": file }))
}

/// `POST /api/v1/flavors/create`: make a flavor as a copy of another.
fn create(manage: &Manage, call: &Call) -> Reply {
    let (name, from) = (call.text("name"), call.text("from"));
    let (folder, files) = copy(Path::new(&manage.config_folder), &name, &from)?;
    manage.say(format!("flavors: {name} was made in {folder} as a copy of {from} ({files} files)"));
    Ok(json!({ "name": name, "from": from, "path": folder, "files": files }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configuration folder with one whole flavor, one that lacks a settings file, and the file
    /// that names the first.
    fn folder(test: &str) -> PathBuf {
        let config = std::env::temp_dir().join(format!("kryta-manage-flavors-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&config);
        let vanilla = config.join(FLAVORS).join("Vanilla");
        let map = vanilla.join("World").join("maps").join("Prophecies").join("148");
        std::fs::create_dir_all(&map).unwrap();
        std::fs::write(map.join("basic.json"), "{}").unwrap();
        std::fs::write(vanilla.join("authserver.json"), "{}").unwrap();
        std::fs::write(vanilla.join("worldserver.json"), "{ \"WorldPack\": \"World\" }").unwrap();
        let partial = config.join(FLAVORS).join("partial");
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("authserver.json"), "{}").unwrap();
        std::fs::create_dir_all(config.join(FLAVORS).join("not a name")).unwrap();
        std::fs::write(
            config.join(ACTIVE_FLAVOR_FILE),
            "{\r\n  \"note\": \"n\",\r\n  \"flavor\": {\r\n    \"value\": \"Vanilla\",\r\n    \"doc\": \"d\"\r\n  }\r\n}\r\n",
        )
        .unwrap();
        config
    }

    #[test]
    fn the_flavors_are_listed_with_the_one_the_machine_runs() {
        let config = folder("list");
        let answer = described(&config);
        let _ = std::fs::remove_dir_all(&config);

        assert_eq!(answer["active"], "Vanilla");
        assert!(answer["error"].is_null());
        let flavors = answer["flavors"].as_array().expect("a list");
        assert_eq!(flavors.len(), 2, "a folder whose name is not a flavor's is not one");
        assert_eq!(flavors[0]["name"], "partial");
        assert_eq!(flavors[0]["active"], false);
        assert_eq!(flavors[0]["lacks"], json!(["worldserver.json"]));
        assert_eq!(flavors[1]["name"], "Vanilla");
        assert_eq!(flavors[1]["active"], true);
        assert_eq!(flavors[1]["holds"], json!(["World"]));
        assert_eq!(flavors[1]["lacks"], json!([]));
        assert_eq!(flavors[1]["maps"], 1);
    }

    #[test]
    fn a_flavor_is_copied_and_chosen_and_the_file_keeps_its_layout() {
        let config = folder("copy");
        let (path, files) = copy(&config, "fantasy_modded", "Vanilla").expect("the copy is made");
        assert_eq!(files, 3);
        assert!(path.ends_with("flavors/fantasy_modded"));
        let copied = config.join(FLAVORS).join("fantasy_modded");
        assert!(copied.join("World/maps/Prophecies/148/basic.json").is_file());

        choose(&config, "fantasy_modded").expect("the flavor is chosen");
        let text = std::fs::read_to_string(config.join(ACTIVE_FLAVOR_FILE)).unwrap();
        assert_eq!(
            text,
            "{\r\n  \"note\": \"n\",\r\n  \"flavor\": {\r\n    \"value\": \"fantasy_modded\",\r\n    \"doc\": \"d\"\r\n  }\r\n}\r\n"
        );
        assert_eq!(Flavor::active(&config).unwrap().name, "fantasy_modded");
        assert_eq!(described(&config)["active"], "fantasy_modded");

        // A machine with no file that names a flavor is given one.
        std::fs::remove_file(config.join(ACTIVE_FLAVOR_FILE)).unwrap();
        assert!(described(&config)["error"].is_string());
        choose(&config, "Vanilla").expect("the file is made");
        assert_eq!(Flavor::active(&config).unwrap().name, "Vanilla");
        let _ = std::fs::remove_dir_all(&config);
    }

    #[test]
    fn what_cannot_be_a_flavor_is_refused_and_nothing_is_written() {
        let config = folder("refuse");
        let before = std::fs::read_to_string(config.join(ACTIVE_FLAVOR_FILE)).unwrap();
        let status = |result: Result<String, Failure>| result.expect_err("refused").status;
        assert_eq!(status(choose(&config, "..")), 400);
        assert_eq!(status(choose(&config, "missing")), 404);
        assert_eq!(status(choose(&config, "partial")), 409, "it lacks a settings file");
        assert_eq!(std::fs::read_to_string(config.join(ACTIVE_FLAVOR_FILE)).unwrap(), before);

        let status = |name: &str, from: &str| copy(&config, name, from).expect_err("refused").status;
        assert_eq!(status("two words", "Vanilla"), 400);
        assert_eq!(status("new", "missing"), 404);
        assert_eq!(status("new", "../Vanilla"), 400);
        assert_eq!(status("Vanilla", "Vanilla"), 409);
        assert_eq!(status("vanilla", "Vanilla"), 409, "a name that differs only in case");
        assert_eq!(names(&config), ["partial", "Vanilla"]);
        let _ = std::fs::remove_dir_all(&config);
    }
}
