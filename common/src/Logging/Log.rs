//! The log registry: what is configured, where it writes, and the thread that does it.
//!
//! Reference: `src/common/Logging/Log.{h,cpp}`.
//!
//! The configuration is the reference's, in its own keys:
//!
//! ```text
//! LogsDir = ""
//! Appender.Console = 1,5,0,"1 9 3 6 5 8"      type, level, flags, optional...
//! Appender.Auth    = 2,5,0,Auth.log,w          type 2 is a file, then name and mode
//! Logger.root      = 4,Console Auth            level, then appender names
//! ```
//!
//! A logger's name is a dotted path and lookup walks up it: `server.authserver` uses the logger
//! `server.authserver` if it is configured, then `server`, then `root`. That is why a shipped file
//! configuring nothing but `Logger.root` still logs everything.
//!
//! The text is handed to a worker thread through a queue, as the reference does, so a caller never
//! waits on a file write. [`Log::shutdown`] drains it: it drops the sender and joins the thread,
//! so everything queued is written before the program exits.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use chrono::Local;

use super::appender_console::AppenderConsole;
use super::appender_file::AppenderFile;
use super::log_common::{AppenderFlags, AppenderType, LogLevel, LOGGER_ROOT};
use super::logger::{Logger, SharedAppender};
use super::log_message::LogMessage;
use crate::configuration::{Config, ConfigError};

/// The format a file-name timestamp uses.
///
/// Reference: `Log::GetLogsTimestamp`, which the `USE_TIMESTAMP` flag puts into a file name.
pub const LOGS_TIMESTAMP_FORMAT: &str = "%Y-%m-%d_%H-%M-%S";

/// Where a configured log writes.
#[derive(Debug, Clone)]
struct LogPaths {
    dir: PathBuf,
    timestamp: String,
}

struct LogInner {
    paths: Mutex<LogPaths>,
    appenders: Mutex<BTreeMap<String, SharedAppender>>,
    loggers: Mutex<BTreeMap<String, Logger>>,
    sender: Mutex<Option<Sender<LogMessage>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

/// The configured log, shareable between threads.
#[derive(Clone)]
pub struct Log {
    inner: Arc<LogInner>,
}

impl Default for Log {
    fn default() -> Self {
        Self::new()
    }
}

impl Log {
    /// A log with nothing configured and no worker running.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(LogInner {
                paths: Mutex::new(LogPaths {
                    dir: PathBuf::new(),
                    timestamp: Local::now().format(LOGS_TIMESTAMP_FORMAT).to_string(),
                }),
                appenders: Mutex::new(BTreeMap::new()),
                loggers: Mutex::new(BTreeMap::new()),
                sender: Mutex::new(None),
                worker: Mutex::new(None),
            }),
        }
    }

    /// Read the configuration, build the appenders and loggers, and start the worker.
    ///
    /// Reference: `Log::Initialize`. The appenders are created before the loggers, because a
    /// logger's line names them. A line the reader cannot use is reported in the answer and
    /// skipped, as the reference prints and carries on; a constructor that refuses its arguments
    /// is an error here, as the reference throws.
    pub fn initialize(&self, config: &Config) -> Result<Vec<String>, String> {
        let logs_dir = config
            .value_string("LogsDir", "")
            .map_err(|error: ConfigError| error.to_string())?;

        {
            let mut paths = self.inner.paths.lock().map_err(|_| "log paths poisoned")?;
            paths.dir = PathBuf::from(logs_dir.0);
            paths.timestamp = Local::now().format(LOGS_TIMESTAMP_FORMAT).to_string();
        }

        let mut notes = Vec::new();
        self.create_appenders(config, &mut notes)?;
        self.create_loggers(config, &mut notes)?;
        self.start_worker()?;

        Ok(notes)
    }

    fn create_appenders(&self, config: &Config, notes: &mut Vec<String>) -> Result<(), String> {
        let paths = self
            .inner
            .paths
            .lock()
            .map_err(|_| "log paths poisoned")?
            .clone();

        let mut next_id: u8 = 0;
        for key in config.keys().map(str::to_string).collect::<Vec<_>>() {
            let Some(name) = key.strip_prefix("Appender.") else {
                continue;
            };
            let Some(options) = config.raw(&key) else {
                continue;
            };

            next_id = next_id.saturating_add(1);

            let tokens: Vec<String> = options.split(',').map(|part| part.trim().to_string()).collect();
            if tokens.len() < 3 {
                notes.push(format!(
                    "Log::CreateAppenderFromConfig: Wrong configuration for appender {name}. Config line: {options}"
                ));
                continue;
            }

            let appender_type = tokens[0]
                .parse::<u8>()
                .ok()
                .and_then(AppenderType::from_u8)
                .ok_or_else(|| {
                    format!(
                        "Log::CreateAppenderFromConfig: Wrong configuration for appender {name}. Config line: {options}"
                    )
                })?;
            let level = tokens[1]
                .parse::<u8>()
                .ok()
                .and_then(LogLevel::from_u8)
                .ok_or_else(|| {
                    format!(
                        "Log::CreateAppenderFromConfig: Wrong configuration for appender {name}. Config line: {options}"
                    )
                })?;
            let flags = AppenderFlags::from_bits(tokens[2].parse::<u8>().unwrap_or(0));
            let optional = &tokens[3..];

            let appender: SharedAppender = match appender_type {
                AppenderType::Console => {
                    let built = AppenderConsole::new(next_id, name, level, flags, optional)?;
                    Arc::new(Mutex::new(Box::new(built)))
                }
                AppenderType::File => {
                    let built = AppenderFile::new(
                        next_id,
                        name,
                        level,
                        flags,
                        optional,
                        &paths.dir,
                        &paths.timestamp,
                    )?;
                    Arc::new(Mutex::new(Box::new(built)))
                }
                AppenderType::Db => {
                    notes.push(format!(
                        "Log::CreateAppenderFromConfig: appender {name} is a database appender, which lives in database/Logging and is not ported yet"
                    ));
                    continue;
                }
                AppenderType::None => {
                    notes.push(format!(
                        "Log::CreateAppenderFromConfig: Wrong configuration for appender {name}. Config line: {options}"
                    ));
                    continue;
                }
            };

            self.inner
                .appenders
                .lock()
                .map_err(|_| "appenders poisoned")?
                .insert(name.to_string(), appender);
        }

        Ok(())
    }

    fn create_loggers(&self, config: &Config, notes: &mut Vec<String>) -> Result<(), String> {
        let appenders = self.inner.appenders.lock().map_err(|_| "appenders poisoned")?;
        let mut loggers = self.inner.loggers.lock().map_err(|_| "loggers poisoned")?;

        for key in config.keys().map(str::to_string).collect::<Vec<_>>() {
            let Some(name) = key.strip_prefix("Logger.") else {
                continue;
            };
            let Some(options) = config.raw(&key) else {
                continue;
            };

            let Some((level_text, list)) = options.split_once(',') else {
                notes.push(format!(
                    "Log::CreateLoggerFromConfig: Wrong configuration for logger {name}. Config line: {options}"
                ));
                continue;
            };

            let level = level_text
                .trim()
                .parse::<u8>()
                .ok()
                .and_then(LogLevel::from_u8)
                .unwrap_or(LogLevel::Disabled);

            let mut logger = Logger::new(name, level);
            for appender_name in list.split_whitespace() {
                match appenders.get(appender_name) {
                    Some(appender) => logger.add_appender(appender_name, appender.clone()),
                    None => notes.push(format!(
                        "Log::CreateLoggerFromConfig: logger {name} names appender '{appender_name}', which is not configured"
                    )),
                }
            }

            loggers.insert(name.to_string(), logger);
        }

        Ok(())
    }

    fn start_worker(&self) -> Result<(), String> {
        let (sender, receiver) = channel::<LogMessage>();

        {
            let mut slot = self.inner.sender.lock().map_err(|_| "sender poisoned")?;
            if slot.is_some() {
                return Err("the log worker is already running".to_string());
            }
            *slot = Some(sender);
        }

        let inner = Arc::clone(&self.inner);
        let worker = std::thread::Builder::new()
            .name("log".to_string())
            .spawn(move || run_worker(&inner, receiver))
            .map_err(|error| format!("cannot start the log worker: {error}"))?;

        *self.inner.worker.lock().map_err(|_| "worker poisoned")? = Some(worker);
        Ok(())
    }

    /// The directory files are written under.
    pub fn logs_dir(&self) -> PathBuf {
        self.inner
            .paths
            .lock()
            .map(|paths| paths.dir.clone())
            .unwrap_or_default()
    }

    /// The timestamp a `USE_TIMESTAMP` file name carries.
    pub fn logs_timestamp(&self) -> String {
        self.inner
            .paths
            .lock()
            .map(|paths| paths.timestamp.clone())
            .unwrap_or_default()
    }

    /// The logger a kind resolves to, walking up its dotted name.
    ///
    /// Reference: `Log::GetLoggerByType`: the exact name, then the name without its last segment,
    /// then `root`, then nothing.
    pub fn logger(&self, kind: &str) -> Option<Logger> {
        let loggers = self.inner.loggers.lock().ok()?;

        let mut candidate = kind.to_string();
        loop {
            if let Some(logger) = loggers.get(&candidate) {
                return Some(logger.clone());
            }
            if candidate == LOGGER_ROOT {
                return None;
            }
            candidate = match candidate.rfind('.') {
                Some(dot) => candidate[..dot].to_string(),
                None => LOGGER_ROOT.to_string(),
            };
        }
    }

    /// Whether a message at this level would reach any appender.
    ///
    /// Reference: `Log::ShouldLog`, which the `LOG_*` macros call so the text is not formatted for
    /// a message nobody will write.
    pub fn enabled(&self, kind: &str, level: LogLevel) -> bool {
        self.logger(kind)
            .map(|logger| logger.level() != LogLevel::Disabled && logger.level() >= level)
            .unwrap_or(false)
    }

    /// Queue a message.
    ///
    /// Reference: `Log::write` for a message, which hands it to the worker rather than writing it
    /// on the caller's thread.
    pub fn out(&self, level: LogLevel, kind: &str, text: impl Into<String>) {
        let message = LogMessage::new(level, kind, text);
        self.enqueue(message);
    }

    /// Queue a message with the name a dynamic file appender substitutes.
    pub fn out_with_param(
        &self,
        level: LogLevel,
        kind: &str,
        text: impl Into<String>,
        param1: impl Into<String>,
    ) {
        let message = LogMessage::with_param1(level, kind, text, param1);
        self.enqueue(message);
    }

    fn enqueue(&self, message: LogMessage) {
        if let Ok(sender) = self.inner.sender.lock()
            && let Some(sender) = sender.as_ref()
        {
            let _ = sender.send(message);
        }
    }

    /// Change a logger's level, when one is configured.
    pub fn set_log_level(&self, kind: &str, level: LogLevel) -> bool {
        let Ok(mut loggers) = self.inner.loggers.lock() else {
            return false;
        };
        match loggers.get_mut(kind) {
            Some(logger) => {
                logger.set_level(level);
                true
            }
            None => false,
        }
    }

    /// Stop the worker and wait for what is queued to be written.
    ///
    /// Reference: the reference stops the worker in `Log::Close`; the ordering is the same - the
    /// queue is closed first, then the thread is joined.
    pub fn shutdown(&self) {
        if let Ok(mut sender) = self.inner.sender.lock() {
            sender.take();
        }
        if let Ok(mut worker) = self.inner.worker.lock()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }
}

/// Drain the queue until the sender is gone.
fn run_worker(inner: &Arc<LogInner>, receiver: Receiver<LogMessage>) {
    for message in receiver {
        let Ok(loggers) = inner.loggers.lock() else {
            continue;
        };

        let mut candidate = message.kind.clone();
        loop {
            if let Some(logger) = loggers.get(&candidate) {
                logger.write(&message);
                break;
            }
            if candidate == LOGGER_ROOT {
                break;
            }
            candidate = match candidate.rfind('.') {
                Some(dot) => candidate[..dot].to_string(),
                None => LOGGER_ROOT.to_string(),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config_in(dir: &Path, body: &str) -> Config {
        let source = format!("LogsDir = \"{}\"\n{body}", dir.display());
        Config::parse(&source, PathBuf::from("test.conf"), Default::default())
    }

    #[test]
    fn a_line_reaches_a_configured_file_through_the_worker() {
        let dir = temp_dir("worker");
        let config = config_in(
            &dir,
            "Appender.Auth = 2,5,0,Auth.log,w\nLogger.root = 4,Auth\n",
        );

        let log = Log::new();
        let notes = log.initialize(&config).unwrap();
        assert!(notes.is_empty(), "{notes:?}");

        log.out(LogLevel::Info, "server.authserver", "hello");
        log.shutdown();

        let written = std::fs::read_to_string(dir.join("Auth.log")).unwrap();
        assert_eq!(written, "hello\n");
    }

    #[test]
    fn a_kind_falls_back_to_its_parent_and_then_to_root() {
        let dir = temp_dir("fallback");
        let config = config_in(&dir, "Appender.Auth = 2,5,0,Auth.log,w\nLogger.root = 4,Auth\n");

        let log = Log::new();
        log.initialize(&config).unwrap();

        assert!(log.enabled("server.authserver", LogLevel::Info));
        assert!(!log.enabled("server.authserver", LogLevel::Debug));

        log.out(LogLevel::Info, "server.authserver", "through root");
        log.shutdown();

        let written = std::fs::read_to_string(dir.join("Auth.log")).unwrap();
        assert_eq!(written, "through root\n");
    }

    #[test]
    fn a_more_specific_logger_wins_over_root() {
        let dir = temp_dir("specific");
        let config = config_in(
            &dir,
            "Appender.Auth = 2,5,0,Auth.log,w\nAppender.Speech = 2,5,0,Speech.log,w\nLogger.root = 4,Auth\nLogger.server = 6,Speech\n",
        );

        let log = Log::new();
        log.initialize(&config).unwrap();
        log.out(LogLevel::Debug, "server.authserver", "detailed");
        log.shutdown();

        assert_eq!(
            std::fs::read_to_string(dir.join("Speech.log")).unwrap(),
            "detailed\n"
        );
        // The other appender's file exists - a file appender creates its file when it is built -
        // and it received nothing.
        assert_eq!(
            std::fs::read_to_string(dir.join("Auth.log")).unwrap_or_default(),
            ""
        );
    }

    #[test]
    fn a_logger_naming_a_missing_appender_is_reported_and_kept() {
        let dir = temp_dir("missing-appender");
        let config = config_in(&dir, "Logger.root = 4,Nowhere\n");

        let log = Log::new();
        let notes = log.initialize(&config).unwrap();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("names appender 'Nowhere'"), "{notes:?}");

        // The logger exists and holds no appender, so nothing is written and nothing panics.
        log.out(LogLevel::Info, "root", "nowhere");
        log.shutdown();
    }

    #[test]
    fn a_wrong_appender_line_is_reported() {
        let dir = temp_dir("wrong-line");
        let config = config_in(&dir, "Appender.Broken = not,enough\nLogger.root = 4,Broken\n");

        let log = Log::new();
        let notes = log.initialize(&config).unwrap();
        assert!(notes.iter().any(|note| note.contains("Wrong configuration for appender Broken")));
        log.shutdown();
    }

    #[test]
    fn a_disabled_level_lets_nothing_through() {
        let dir = temp_dir("disabled");
        let config = config_in(&dir, "Appender.Auth = 2,5,0,Auth.log,w\nLogger.root = 0,Auth\n");

        let log = Log::new();
        log.initialize(&config).unwrap();
        assert!(!log.enabled("root", LogLevel::Fatal));
        log.out(LogLevel::Fatal, "root", "silent");
        log.shutdown();

        assert!(std::fs::read_to_string(dir.join("Auth.log"))
            .map(|text| text.is_empty())
            .unwrap_or(true));
    }
}
