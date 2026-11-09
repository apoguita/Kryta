//! The file appender.
//!
//! Reference: `src/common/Logging/AppenderFile.{h,cpp}`.
//!
//! Its configuration is `Appender.<name> = 2,<level>,<flags>,<file>,<mode>,<size>`:
//!
//! - `<file>` is required, and missing it is an error.
//! - `<mode>` is `a` (append, the default) or `w` (overwrite).
//! - `<size>` caps the file; past it, the file is overwritten and the old one is kept as a backup.
//! - a `%s` in the name makes a dynamic name, substituted from the message's first parameter, and
//!   the file is reopened for every message.
//! - `USE_TIMESTAMP` puts the log's timestamp into the name; `MAKE_FILE_BACKUP` renames an
//!   existing file before opening it for overwrite.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::appender::{Appender, AppenderBase};
use super::log_common::{AppenderFlags, AppenderType, LogLevel};
use super::log_message::{backup_suffix, LogMessage};

/// Text to a file under the log directory.
#[derive(Debug)]
pub struct AppenderFile {
    base: AppenderBase,
    log_dir: PathBuf,
    file_name: String,
    dynamic_name: bool,
    backup: bool,
    max_file_size: u32,
    file_size: u64,
    file: Option<File>,
}

impl AppenderFile {
    /// Build one from its configuration fields.
    ///
    /// Reference: `AppenderFile::AppenderFile`. `args` is the optional part of an
    /// `Appender.<name>` line, so `args[0]` is the file name, `args[1]` the mode and `args[2]` the
    /// size. `log_dir` and `logs_timestamp` are what the reference asks the log for.
    pub fn new(
        id: u8,
        name: impl Into<String>,
        level: LogLevel,
        flags: AppenderFlags,
        args: &[String],
        log_dir: &Path,
        logs_timestamp: &str,
    ) -> Result<Self, String> {
        let name = name.into();

        let Some(file_name) = args.first().filter(|field| !field.is_empty()) else {
            return Err(format!(
                "Log::CreateAppenderFromConfig: Missing file name for appender {}",
                name
            ));
        };

        let mode = args
            .get(1)
            .filter(|field| !field.is_empty())
            .cloned()
            .unwrap_or_else(|| "a".to_string());

        let mut file_name = file_name.clone();

        // Reference: the timestamp goes before the last dot, or after the whole name.
        if flags.contains(AppenderFlags::USE_TIMESTAMP) {
            match file_name.rfind('.') {
                Some(dot) => file_name.insert_str(dot, logs_timestamp),
                None => file_name.push_str(logs_timestamp),
            }
        }

        let max_file_size = match args.get(2).filter(|field| !field.is_empty()) {
            Some(size) => match size.parse::<u32>() {
                Ok(size) => size,
                Err(_) => {
                    return Err(format!(
                        "Log::CreateAppenderFromConfig: Invalid size '{}' for appender {}",
                        size, name
                    ));
                }
            },
            None => 0,
        };

        let dynamic_name = file_name.contains("%s");
        let backup = flags.contains(AppenderFlags::MAKE_FILE_BACKUP);

        let mut appender = Self {
            base: AppenderBase::new(id, name, level, flags),
            log_dir: log_dir.to_path_buf(),
            file_name,
            dynamic_name,
            backup,
            max_file_size,
            file_size: 0,
            file: None,
        };

        if !appender.dynamic_name {
            appender.file = appender.open_file(&appender.file_name.clone(), &mode, mode == "w" && backup);
        }

        Ok(appender)
    }

    /// The name the appender writes to, after any timestamp insertion.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Open a file under the log directory, making a backup first when asked.
    ///
    /// Reference: `AppenderFile::OpenFile`. A backup that cannot be made is ignored, which is the
    /// reference's own choice: losing the backup is better than losing the log.
    fn open_file(&mut self, filename: &str, mode: &str, backup: bool) -> Option<File> {
        let full_name = self.log_dir.join(filename);

        if backup {
            self.close_file();
            let mut backup_name = full_name.clone().into_os_string();
            backup_name.push(".");
            backup_name.push(backup_suffix());
            let _ = std::fs::rename(&full_name, PathBuf::from(backup_name));
        }

        let mut options = OpenOptions::new();
        options.create(true);
        if mode == "w" {
            options.write(true).truncate(true);
        } else {
            options.append(true);
        }

        match options.open(&full_name) {
            Ok(file) => {
                self.file_size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
                Some(file)
            }
            Err(_) => None,
        }
    }

    fn close_file(&mut self) {
        self.file = None;
    }

    /// Write one line to a path, opening it for the occasion.
    fn write_to(&mut self, filename: &str, mode: &str, backup: bool, line: &str, size: usize) {
        let Some(mut file) = self.open_file(filename, mode, backup) else {
            return;
        };
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
        self.file_size += size as u64;
    }
}

impl Appender for AppenderFile {
    fn base(&self) -> &AppenderBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut AppenderBase {
        &mut self.base
    }

    fn appender_type(&self) -> AppenderType {
        AppenderType::File
    }

    fn write_text(&mut self, message: &LogMessage, prefix: &str) {
        let line = format!("{prefix}{}\n", message.text);
        let size = message.size(prefix);
        let exceed_max_size =
            self.max_file_size > 0 && self.file_size + size as u64 > self.max_file_size as u64;

        if self.dynamic_name {
            let Some(param1) = message.param1.as_deref() else {
                return;
            };
            let name = self.file_name.replace("%s", param1);
            // Always append for a dynamic name: overwriting would delete what the previous message
            // in this same file wrote.
            self.write_to(&name, "a", self.backup || exceed_max_size, &line, size);
            return;
        }

        if exceed_max_size {
            self.file = self.open_file(&self.file_name.clone(), "w", true);
        }

        let Some(file) = self.file.as_mut() else {
            return;
        };
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
        self.file_size += size as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-log-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(fields: &[&str]) -> Vec<String> {
        fields.iter().map(|field| field.to_string()).collect()
    }

    #[test]
    fn a_file_name_is_required() {
        let dir = temp_dir("missing");
        let error = AppenderFile::new(1, "Auth", LogLevel::Debug, AppenderFlags::NONE, &[], &dir, "")
            .unwrap_err();
        assert!(error.contains("Missing file name for appender Auth"), "{error}");
    }

    #[test]
    fn a_size_that_is_not_a_number_is_refused() {
        let dir = temp_dir("size");
        let error = AppenderFile::new(
            1,
            "Auth",
            LogLevel::Debug,
            AppenderFlags::NONE,
            &args(&["Auth.log", "w", "big"]),
            &dir,
            "",
        )
        .unwrap_err();
        assert!(error.contains("Invalid size 'big'"), "{error}");
    }

    #[test]
    fn the_timestamp_goes_before_the_last_dot() {
        let dir = temp_dir("stamp");
        let appender = AppenderFile::new(
            1,
            "Auth",
            LogLevel::Debug,
            AppenderFlags::USE_TIMESTAMP,
            &args(&["Auth.log"]),
            &dir,
            "STAMP",
        )
        .unwrap();
        assert_eq!(appender.file_name(), "AuthSTAMP.log");
    }

    #[test]
    fn a_name_without_a_dot_takes_the_timestamp_at_the_end() {
        let dir = temp_dir("stamp2");
        let appender = AppenderFile::new(
            1,
            "Auth",
            LogLevel::Debug,
            AppenderFlags::USE_TIMESTAMP,
            &args(&["AuthLog"]),
            &dir,
            "STAMP",
        )
        .unwrap();
        assert_eq!(appender.file_name(), "AuthLogSTAMP");
    }

    #[test]
    fn a_line_reaches_the_file() {
        let dir = temp_dir("write");
        let mut appender = AppenderFile::new(
            1,
            "Auth",
            LogLevel::Debug,
            AppenderFlags::PREFIX_LOGLEVEL,
            &args(&["Auth.log"]),
            &dir,
            "",
        )
        .unwrap();

        appender.write(&LogMessage::new(LogLevel::Info, "root", "hello"));
        drop(appender);

        let written = std::fs::read_to_string(dir.join("Auth.log")).unwrap();
        assert_eq!(written, "INFO hello\n");
    }

    #[test]
    fn a_dynamic_name_is_substituted_and_appended() {
        let dir = temp_dir("dynamic");
        let mut appender = AppenderFile::new(
            1,
            "Per",
            LogLevel::Debug,
            AppenderFlags::NONE,
            &args(&["%s.log"]),
            &dir,
            "",
        )
        .unwrap();

        appender.write(&LogMessage::with_param1(LogLevel::Info, "root", "one", "alpha"));
        appender.write(&LogMessage::with_param1(LogLevel::Info, "root", "two", "beta"));
        assert_eq!(std::fs::read_to_string(dir.join("alpha.log")).unwrap(), "one\n");
        assert_eq!(std::fs::read_to_string(dir.join("beta.log")).unwrap(), "two\n");
    }
}
