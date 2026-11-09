//! Small shared helpers.
//!
//! Reference: `src/common/Utilities/Util.{h,cpp}`, which is a large file of string, numeric and
//! process helpers. It is ported by subject rather than in one go: what is here is what a landed
//! stage needs, and the rest arrives with the stage that calls it. Not here yet: `Trim`,
//! `ToString`, `StringTo`, the utf8 helpers and the time helpers.
//!
//! Here now: the process id and the PID file, which a program writes at start-up so a service
//! manager or an operator can find it.

use std::path::Path;

/// This process's id.
///
/// Reference: `GetPID`, which is `GetCurrentProcessId` on Windows and `getpid` elsewhere.
pub fn get_pid() -> u32 {
    std::process::id()
}

/// Write this process's id to a file and answer it, or answer nothing when the file cannot be
/// written.
///
/// Reference: `CreatePIDFile`. The file holds the number and nothing else - no newline - and is
/// truncated if it exists. A failure is the caller's to report: the reference's auth program logs
/// `Cannot create PID file ... (possible error: permission)` and exits.
pub fn create_pid_file(filename: impl AsRef<Path>) -> Option<u32> {
    let pid = get_pid();
    std::fs::write(filename, pid.to_string()).ok()?;
    Some(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-util-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_file_holds_the_pid_and_nothing_else() {
        let dir = temp_dir("pid");
        let path = dir.join("authserver.pid");

        let pid = create_pid_file(&path).unwrap();
        assert_eq!(pid, get_pid());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), pid.to_string());
    }

    #[test]
    fn an_existing_file_is_truncated() {
        let dir = temp_dir("truncate");
        let path = dir.join("authserver.pid");
        std::fs::write(&path, "9999999 trailing").unwrap();

        let pid = create_pid_file(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), pid.to_string());
    }

    #[test]
    fn a_directory_that_does_not_exist_answers_nothing() {
        let dir = temp_dir("missing");
        assert_eq!(create_pid_file(dir.join("nope").join("x.pid")), None);
    }
}
