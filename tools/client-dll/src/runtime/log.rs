//! The DLL's log: `Kryta.<process id>.log` beside the client, started when the client starts.
//!
//! What the DLL does, it says (rule 9): the build it found itself in, the server it reached, every
//! offset it resolved or failed to, every hook it installed, the first few times a hook answers the
//! client, and any fault inside the client. A fault inside the client is read from this file.
//!
//! Several clients can run from one folder, so each writes a file of its own, named by its process
//! id - the same number the link names the client by. Logs of earlier runs are removed once they
//! are a day old.

use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// How long the log of an earlier run is kept.
const KEPT_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether a file name is one of this DLL's logs.
fn is_log(name: &str) -> bool {
    name.strip_prefix("Kryta.")
        .and_then(|rest| rest.strip_suffix(".log"))
        .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Start the log beside the client's executable. A log that cannot be opened is done without.
pub fn open(directory: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > KEPT_FOR);
            if old && entry.file_name().to_str().is_some_and(is_log) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    if let Ok(mut file) = FILE.lock() {
        let name = format!("Kryta.{}.log", std::process::id());
        *file = File::create(directory.join(name)).ok();
    }
}

/// Write one line.
pub fn line(text: impl AsRef<str>) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    if let Ok(mut file) = FILE.lock()
        && let Some(file) = file.as_mut()
    {
        let _ = writeln!(file, "{seconds} {}", text.as_ref());
        let _ = file.flush();
    }
}
