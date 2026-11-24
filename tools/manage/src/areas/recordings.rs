//! Recordings: what a capture needs, the library of captures, and the index over them.
//!
//! The panel cannot run the client capture: `tools/client/sniff-capture.py` needs an elevated
//! shell, and this service is not elevated (`docs/port/23-control-panel.md`, "Recordings"). So this
//! area checks what can be checked without elevation, lets the page build the command for the
//! operator to run by hand, and manages the files under `recordings/` (`recordings/README.md`).
//!
//! It writes one file: `recordings/index.jsonl`, one appended line per indexed recording. It never
//! edits a recording.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::api::{Call, Failure, Reply, Route, bad, broken, conflict, get, missing, post};
use crate::state::Manage;

/// The folder the recordings live in, relative to the working directory (the repository root).
const RECORDINGS: &str = "recordings";
/// The index, inside that folder.
const INDEX: &str = "index.jsonl";
/// The capture script the command builder names.
const CAPTURE_SCRIPT: &str = "tools/client/sniff-capture.py";
/// How much of a file is read to find its header record: a header is one line of a few hundred
/// bytes, and a file with no line break in this much is not a capture this area can describe.
const HEADER_LIMIT: u64 = 64 * 1024;

/// The recordings area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/recordings", library),
        get("/api/v1/recordings/preflight", preflight),
        get("/api/v1/recordings/describe", describe),
        post("/api/v1/recordings/index", add_to_index),
    ]
}

// ---- the path guard --------------------------------------------------------------------------

/// Whether a file name is a recording: the two extensions the folder holds.
fn is_recording(name: &str) -> bool {
    name.ends_with(".jsonl") || name.ends_with(".log")
}

/// Turn a path the page sent into a file under `recordings/`, or refuse it.
///
/// The path is relative to `recordings/` with forward slashes, as the index writes it. Anything
/// that could leave the folder is refused: an absolute path, a drive, a backslash, an empty, `.` or
/// `..` segment. Only a recording's extension is accepted, so the index itself is not a recording.
pub fn guard_path(relative: &str) -> Result<PathBuf, String> {
    let refused = || format!("Not a recording under {RECORDINGS}/: {relative}");
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.contains(['\\', ':', '\0'])
        || !is_recording(relative)
        || relative == INDEX
    {
        return Err(refused());
    }
    let mut path = PathBuf::from(RECORDINGS);
    for segment in relative.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(refused());
        }
        path.push(segment);
    }
    Ok(path)
}

// ---- the folder scheme -----------------------------------------------------------------------

/// What a recording's place in the folder says about it (`recordings/README.md`, "The layout").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheme {
    /// The first folder: `asura`, `gwps`, `client`.
    pub source: String,
    /// The second folder: `login`, `world`, `session`, `feature`, `raw`.
    pub kind: String,
    /// The folder under `feature/`, when the kind is a feature: `movement`, `travel`, ...
    pub feature: Option<String>,
    /// The file's name without its extension and without a trailing date.
    pub subject: String,
}

/// Read the scheme from a path relative to `recordings/`.
pub fn scheme(relative: &str) -> Scheme {
    let segments: Vec<&str> = relative.split('/').collect();
    let folders = &segments[..segments.len().saturating_sub(1)];
    let file = segments.last().copied().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let kind = folders.get(1).copied().unwrap_or_default();
    Scheme {
        source: folders.first().copied().unwrap_or_default().to_string(),
        kind: kind.to_string(),
        feature: (kind == "feature")
            .then(|| folders.get(2).map(|name| name.to_string()))
            .flatten(),
        subject: strip_date(stem).to_string(),
    }
}

/// A name without the `-YYYYMMDD` or `-YYYYMMDD-HHMMSS` the naming rule appends to disambiguate.
pub fn strip_date(stem: &str) -> &str {
    let digits = |text: &str, length: usize| {
        text.len() == length && text.bytes().all(|byte| byte.is_ascii_digit())
    };
    let Some((rest, last)) = stem.rsplit_once('-') else {
        return stem;
    };
    if digits(last, 8) {
        return rest;
    }
    match rest.rsplit_once('-') {
        Some((subject, date)) if digits(last, 6) && digits(date, 8) => subject,
        _ => stem,
    }
}

// ---- reading files ---------------------------------------------------------------------------

/// The header record of a capture: its first line, when that line is a JSON object.
fn read_header(path: &Path) -> Option<Value> {
    let file = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file.take(HEADER_LIMIT))
        .read_line(&mut line)
        .ok()?;
    serde_json::from_str::<Value>(line.trim())
        .ok()
        .filter(Value::is_object)
}

/// Every recording under `dir`, as paths relative to `recordings/` with forward slashes.
fn walk(dir: &Path, prefix: &str, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        if path.is_dir() {
            walk(&path, &relative, found);
        } else if is_recording(&name) && relative != INDEX {
            found.push(relative);
        }
    }
}

/// The paths `index.jsonl` lists.
fn indexed_paths() -> Vec<String> {
    std::fs::read_to_string(Path::new(RECORDINGS).join(INDEX))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|entry| entry.get("path").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// One recording as the library lists it: its place, its size and what its header says. The file
/// is not read past its first line.
fn row(relative: &str, indexed: &[String]) -> Value {
    let path = Path::new(RECORDINGS).join(relative);
    let scheme = scheme(relative);
    let header = read_header(&path);
    let field = |name: &str| header.as_ref().and_then(|header| header.get(name)).cloned();
    json!({
        "path": relative,
        "name": relative.rsplit('/').next(),
        "source": scheme.source,
        "kind": scheme.kind,
        "feature": scheme.feature,
        "subject": scheme.subject,
        "bytes": std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0),
        "has_header": header.is_some(),
        "tool": field("tool"),
        "layer": field("layer"),
        "started_at": field("started_at"),
        "sanitised": field("sanitised"),
        "indexed": indexed.iter().any(|listed| listed == relative),
    })
}

/// Every recording, as the library and the overview read it.
pub fn scan() -> Vec<Value> {
    let mut paths = Vec::new();
    walk(Path::new(RECORDINGS), "", &mut paths);
    paths.sort();
    let indexed = indexed_paths();
    paths.iter().map(|path| row(path, &indexed)).collect()
}

/// `GET /api/v1/recordings`: the library.
fn library(_: &Manage, _: &Call) -> Reply {
    let rows = scan();
    let bytes: u64 = rows.iter().filter_map(|row| row["bytes"].as_u64()).sum();
    Ok(json!({ "folder": RECORDINGS, "recordings": rows, "bytes": bytes }))
}

/// The file a request names in `path`, when it exists.
fn named_file(relative: &str) -> Result<PathBuf, Failure> {
    let path = guard_path(relative).map_err(bad)?;
    if !path.is_file() {
        return Err(missing(format!("No recording is at {RECORDINGS}/{relative}.")));
    }
    Ok(path)
}

/// `GET /api/v1/recordings/describe?path=...`: one file's header record, its summary record (the
/// last line, when it is one) and how many message records lie between them.
fn describe(_: &Manage, call: &Call) -> Reply {
    let relative = call.query("path").unwrap_or_default();
    let path = named_file(&relative)?;
    let text = std::fs::read(&path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| broken(format!("{}: {error}", path.display())))?;
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    let record = |line: Option<&&str>, kind: &str| {
        line.and_then(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|value| value.get("record").and_then(Value::as_str) == Some(kind))
    };
    let header = record(lines.first(), "capture");
    let summary = record(lines.last(), "summary");
    let messages = lines.len() - usize::from(header.is_some()) - usize::from(summary.is_some());
    Ok(json!({
        "path": relative,
        "bytes": text.len(),
        "lines": lines.len(),
        "messages": messages,
        "header": header,
        "summary": summary,
        "scheme": scheme_json(&scheme(&relative)),
        "indexed": indexed_paths().contains(&relative),
    }))
}

fn scheme_json(scheme: &Scheme) -> Value {
    json!({
        "source": scheme.source, "kind": scheme.kind,
        "feature": scheme.feature, "subject": scheme.subject,
    })
}

// ---- the index -------------------------------------------------------------------------------

/// A count of seconds since 1970 as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn iso_from_epoch(seconds: u64) -> String {
    // Howard Hinnant's `civil_from_days`, for a day count that is never negative here.
    let days = seconds / 86_400;
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    let second_of_day = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60,
        second_of_day % 60
    )
}

/// When a recording was made, in ISO-8601: its header's `started_at` when that is a date, the same
/// when it is a count of seconds (`asura-world` wrote `1793251739.946451100Z`), else the file's
/// own modification time.
fn recorded(header: Option<&Value>, modified_epoch: Option<u64>) -> Option<String> {
    let started = header
        .and_then(|header| header.get("started_at"))
        .and_then(Value::as_str);
    if let Some(started) = started {
        if started.contains('T') {
            return Some(started.to_string());
        }
        let whole = started.split(['.', 'Z']).next().unwrap_or_default();
        if let Ok(seconds) = whole.parse::<u64>() {
            return Some(iso_from_epoch(seconds));
        }
    }
    modified_epoch.map(iso_from_epoch)
}

/// The line `index.jsonl` gets for one recording, with the fields in the order
/// `recordings/README.md` gives them: `source`, `kind`, `subject`, `path`, `recorded`, `tool`, and
/// a `note` when the operator wrote one.
pub fn index_entry(
    relative: &str,
    header: Option<&Value>,
    modified_epoch: Option<u64>,
    note: &str,
) -> String {
    let scheme = scheme(relative);
    let tool = header
        .and_then(|header| header.get("tool"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let quoted = |text: &str| Value::String(text.to_string()).to_string();
    let mut fields = vec![
        format!("\"source\":{}", quoted(&scheme.source)),
        format!("\"kind\":{}", quoted(&scheme.kind)),
        format!("\"subject\":{}", quoted(&scheme.subject)),
        format!("\"path\":{}", quoted(relative)),
    ];
    if let Some(recorded) = recorded(header, modified_epoch) {
        fields.push(format!("\"recorded\":{}", quoted(&recorded)));
    }
    fields.push(format!("\"tool\":{}", quoted(tool)));
    if !note.trim().is_empty() {
        fields.push(format!("\"note\":{}", quoted(note.trim())));
    }
    format!("{{{}}}", fields.join(","))
}

/// `POST /api/v1/recordings/index`: append one entry for a file the index does not list yet.
fn add_to_index(manage: &Manage, call: &Call) -> Reply {
    let relative = call.text("path");
    let path = named_file(&relative)?;
    if indexed_paths().contains(&relative) {
        return Err(conflict(format!("{relative} is already in the index.")));
    }
    let modified = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|age| age.as_secs());
    let entry = index_entry(&relative, read_header(&path).as_ref(), modified, &call.text("note"));

    let index_path = Path::new(RECORDINGS).join(INDEX);
    let existing = std::fs::read(&index_path).unwrap_or_default();
    let separator = if existing.is_empty() || existing.ends_with(b"\n") { "" } else { "\n" };
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .and_then(|mut file| file.write_all(format!("{separator}{entry}\n").as_bytes()))
        .map_err(|error| broken(format!("{}: {error}", index_path.display())))?;
    manage.say(format!("recordings: {relative} added to the index"));
    Ok(json!({ "path": relative, "entry": serde_json::from_str::<Value>(&entry).unwrap_or(Value::Null) }))
}

// ---- the preflight ---------------------------------------------------------------------------

/// The game client processes in a `tasklist /FO CSV /NH` listing: every image whose name starts
/// with `Gw`, with its pid. The capture script looks for the same names (`Gw*.exe`).
pub fn parse_tasklist(csv: &str) -> Vec<(String, u32)> {
    csv.lines()
        .filter_map(|line| {
            let mut fields = line.trim().trim_matches('"').split("\",\"");
            let name = fields.next()?;
            let pid = fields.next()?.parse::<u32>().ok()?;
            name.to_ascii_lowercase()
                .starts_with("gw")
                .then(|| (name.to_string(), pid))
        })
        .collect()
}

/// The running game clients, asked of the operating system.
fn running_clients() -> Result<Vec<(String, u32)>, String> {
    let output = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()
        .map_err(|error| format!("tasklist could not be run: {error}"))?;
    if !output.status.success() {
        return Err(format!("tasklist answered {}", output.status));
    }
    Ok(parse_tasklist(&String::from_utf8_lossy(&output.stdout)))
}

/// Where the capture script looks for a 32-bit Python, in its order: `KRYTA_PY32`, `ASURA_PY32`,
/// then the per-user 3.13 and 3.12 installs (`tools/client/sniff-capture.py:322-327`).
pub fn python_candidates(variable: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = ["KRYTA_PY32", "ASURA_PY32"]
        .iter()
        .filter_map(|name| variable(name))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect();
    if let Some(local) = variable("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        for version in ["Python313-32", "Python312-32"] {
            candidates.push(
                Path::new(&local)
                    .join("Programs")
                    .join("Python")
                    .join(version)
                    .join("python.exe"),
            );
        }
    }
    candidates
}

/// Where the Py4GW Stealth checkout is expected: `PY4GW_STEALTH`, else `Py4GW_Stealth` in the
/// user's profile.
pub fn stealth_folder(variable: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    variable("PY4GW_STEALTH")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            variable("USERPROFILE")
                .filter(|value| !value.is_empty())
                .map(|profile| Path::new(&profile).join("Py4GW_Stealth"))
        })
}

/// `GET /api/v1/recordings/preflight`: what a client capture needs that can be checked without
/// elevation. The elevated shell itself cannot be checked or opened from here.
fn preflight(_: &Manage, _: &Call) -> Reply {
    let variable = |name: &str| std::env::var(name).ok();
    let candidates = python_candidates(variable);
    let python = candidates.iter().find(|path| path.is_file());
    let stealth = stealth_folder(variable);
    let stealth_found = stealth.as_ref().is_some_and(|path| path.join("py4gw").is_dir());
    let (clients, clients_error) = match running_clients() {
        Ok(clients) => (clients, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    let shown = |path: &PathBuf| path.display().to_string();
    Ok(json!({
        "root": std::env::current_dir().ok().as_ref().map(shown),
        "script": CAPTURE_SCRIPT,
        "script_found": Path::new(CAPTURE_SCRIPT).is_file(),
        "python": {
            "found": python.is_some(),
            "path": python.map(shown),
            "looked": candidates.iter().map(shown).collect::<Vec<String>>(),
        },
        "stealth": {
            "found": stealth_found,
            "path": stealth.as_ref().map(shown),
        },
        "clients": clients.iter().map(|(name, pid)| json!({ "name": name, "pid": pid })).collect::<Vec<Value>>(),
        "clients_error": clients_error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_leaves_the_folder_is_refused() {
        let inside = guard_path("client/raw/title-login-17072.jsonl").expect("a recording");
        assert_eq!(inside, Path::new("recordings").join("client").join("raw").join("title-login-17072.jsonl"));
        assert!(guard_path("gwps/world/load-stream.log").is_ok());

        for escape in [
            "",
            "../Cargo.toml",
            "../secret.jsonl",
            "client/../../secret.jsonl",
            "client/raw/../../../worldserver/control.jsonl",
            "/etc/passwd.log",
            "\\\\server\\share\\a.jsonl",
            "client\\..\\..\\a.jsonl",
            "C:/Windows/a.jsonl",
            "C:\\Windows\\a.jsonl",
            "client//a.jsonl",
            "./a.jsonl",
            "client/./a.jsonl",
            "client/raw/",
            "client/raw/notes.md",
            "index.jsonl",
            "a.jsonl\0.md",
        ] {
            assert!(guard_path(escape).is_err(), "{escape:?} must be refused");
        }
    }

    #[test]
    fn the_folder_scheme_names_source_kind_and_subject() {
        let world = scheme("asura/world/live-client-session-20261027-220258.jsonl");
        assert_eq!(
            (world.source.as_str(), world.kind.as_str(), world.subject.as_str()),
            ("asura", "world", "live-client-session")
        );
        assert_eq!(world.feature, None);

        let feature = scheme("client/feature/movement/movement-sniff.log");
        assert_eq!(feature.kind, "feature");
        assert_eq!(feature.feature.as_deref(), Some("movement"));
        assert_eq!(feature.subject, "movement-sniff");

        let raw = scheme("client/raw/title-login-17072.jsonl");
        assert_eq!((raw.kind.as_str(), raw.subject.as_str()), ("raw", "title-login-17072"));

        let loose = scheme("orphan.jsonl");
        assert_eq!((loose.source.as_str(), loose.kind.as_str()), ("", ""));
    }

    #[test]
    fn only_a_trailing_date_is_taken_off_a_name() {
        assert_eq!(strip_date("portal-battery-20261031-013856"), "portal-battery");
        assert_eq!(strip_date("northlands-live-20261106"), "northlands-live");
        assert_eq!(strip_date("title-login-17072"), "title-login-17072");
        assert_eq!(strip_date("fresh-character-20261121-151849-756"), "fresh-character-20261121-151849-756");
        assert_eq!(strip_date("creation-1"), "creation-1");
        assert_eq!(strip_date("plain"), "plain");
    }

    #[test]
    fn the_tasklist_listing_gives_the_game_clients() {
        let csv = "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n\
                   \"Gw.exe\",\"17072\",\"Console\",\"1\",\"412,104 K\"\r\n\
                   \"msedge.exe\",\"900\",\"Console\",\"1\",\"1,000 K\"\r\n\
                   \"Gw.custom.exe\",\"23980\",\"Console\",\"1\",\"390,220 K\"\r\n\
                   \"gwsl.exe\",\"notanumber\",\"Console\",\"1\",\"1 K\"\r\n\
                   INFO: garbage line\r\n";
        assert_eq!(
            parse_tasklist(csv),
            vec![("Gw.exe".to_string(), 17072), ("Gw.custom.exe".to_string(), 23980)]
        );
        assert!(parse_tasklist("").is_empty());
    }

    #[test]
    fn an_index_entry_has_the_readmes_fields_in_its_order() {
        let header = json!({
            "record": "capture",
            "tool": "tools/client/sniff-capture.py",
            "started_at": "2026-11-21T18:14:27.609045+00:00",
        });
        let entry = index_entry("client/raw/title-login-17072.jsonl", Some(&header), None, " title rows ");
        assert_eq!(
            entry,
            "{\"source\":\"client\",\"kind\":\"raw\",\"subject\":\"title-login-17072\",\
             \"path\":\"client/raw/title-login-17072.jsonl\",\
             \"recorded\":\"2026-11-21T18:14:27.609045+00:00\",\
             \"tool\":\"tools/client/sniff-capture.py\",\"note\":\"title rows\"}"
        );
        assert!(serde_json::from_str::<Value>(&entry).is_ok());
    }

    #[test]
    fn an_index_entry_without_a_header_uses_the_files_time_and_says_the_tool_is_unknown() {
        let entry = index_entry("gwps/world/load-stream.log", None, Some(1_793_251_739), "");
        let value: Value = serde_json::from_str(&entry).expect("one JSON line");
        assert_eq!(value["source"], "gwps");
        assert_eq!(value["subject"], "load-stream");
        assert_eq!(value["tool"], "unknown");
        assert_eq!(value["recorded"], "2026-10-29T05:28:59Z");
        assert_eq!(value.get("note"), None);
        assert!(!entry.contains('\n'));

        // A note with a quote stays one valid line.
        let entry = index_entry("gwps/world/a.log", None, None, "said \"hi\"\nthen left");
        let value: Value = serde_json::from_str(&entry).expect("still one JSON line");
        assert_eq!(value["note"], "said \"hi\"\nthen left");
        assert_eq!(value.get("recorded"), None);
    }

    #[test]
    fn a_count_of_seconds_is_written_as_a_date() {
        assert_eq!(iso_from_epoch(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_epoch(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_from_epoch(1_793_160_752), "2026-10-28T04:12:32Z");
        // asura-world wrote its start as seconds with a fraction.
        let header = json!({"started_at": "1793160752.435951200Z"});
        assert_eq!(recorded(Some(&header), None).as_deref(), Some("2026-10-28T04:12:32Z"));
    }

    #[test]
    fn the_preflight_looks_where_the_capture_script_looks() {
        let variable = |name: &str| match name {
            "KRYTA_PY32" => Some("D:\\py32\\python.exe".to_string()),
            "LOCALAPPDATA" => Some("C:\\Users\\op\\AppData\\Local".to_string()),
            "USERPROFILE" => Some("C:\\Users\\op".to_string()),
            _ => None,
        };
        let candidates = python_candidates(variable);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0], PathBuf::from("D:\\py32\\python.exe"));
        assert!(candidates[1].to_string_lossy().contains("Python313-32"));
        assert!(candidates[2].to_string_lossy().contains("Python312-32"));
        assert_eq!(
            stealth_folder(variable),
            Some(Path::new("C:\\Users\\op").join("Py4GW_Stealth"))
        );
        let named = |name: &str| (name == "PY4GW_STEALTH").then(|| "E:\\stealth".to_string());
        assert_eq!(stealth_folder(named), Some(PathBuf::from("E:\\stealth")));
        assert!(python_candidates(|_| None).is_empty());
    }
}
