//! Small shared helpers.
//!
//! Reference: `src/common/Utilities/Util.{h,cpp}`, which is a large file of string, numeric and
//! process helpers. It is ported by subject rather than in one go: what is here is what a landed
//! stage needs, and the rest arrives with the stage that calls it. Not here yet: `Trim`,
//! `ToString`, `StringTo`, the utf8 helpers and the time helpers.
//!
//! Here now: the process id and the PID file, which a program writes at start-up so a service
//! manager or an operator can find it; and the hexadecimal decoder, which is the one place in the
//! tree where `f1 02 20 00` becomes four bytes.

use std::path::Path;

/// This process's id.
///
/// Reference: `GetPID`, which is `GetCurrentProcessId` on Windows and `getpid` elsewhere.
pub fn get_pid() -> u32 {
    std::process::id()
}

/// Decode hexadecimal text: bytes separated by spaces, commas or nothing at all.
///
/// This is the project's only hexadecimal decoder, and it is here rather than in any one caller
/// because two kinds of caller need it: a configuration file that states a byte string, and code
/// that holds a protocol constant as text. A byte string is written the way a capture reads it -
/// `f1 02 20 00` - so an operator can compare it against a trace without converting anything.
pub fn hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for word in text.split(|character: char| character.is_whitespace() || character == ',') {
        let word = word.trim();
        let word = word.strip_prefix("0x").unwrap_or(word);
        if word.is_empty() {
            continue;
        }
        if word.len() % 2 != 0 {
            return Err(format!("'{word}' is an odd number of hexadecimal digits"));
        }
        for pair in word.as_bytes().chunks(2) {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| format!("'{word}' is not hexadecimal text"))?;
            let byte = u8::from_str_radix(pair, 16)
                .map_err(|_| format!("'{pair}' is not a hexadecimal byte"))?;
            bytes.push(byte);
        }
    }
    Ok(bytes)
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

    #[test]
    fn hex_text_is_read_the_way_a_capture_prints_it() {
        assert_eq!(
            hex_bytes("f1 02 20 00").unwrap(),
            vec![0xF1, 0x02, 0x20, 0x00]
        );
        assert_eq!(
            hex_bytes("F102 2000").unwrap(),
            vec![0xF1, 0x02, 0x20, 0x00]
        );
        assert_eq!(hex_bytes("0xf1,0x02").unwrap(), vec![0xF1, 0x02]);
        assert!(hex_bytes("").unwrap().is_empty());
        assert!(hex_bytes("f1 0").unwrap_err().contains("odd number"));
        assert!(
            hex_bytes("zz")
                .unwrap_err()
                .contains("not a hexadecimal byte")
        );
    }
}
