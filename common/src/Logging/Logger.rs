//! A named logger, holding the appenders it writes to.
//!
//! Reference: `src/common/Logging/Logger.{h,cpp}`.
//!
//! The reference keys appenders by a number, because its loggers hold pointers to appenders the
//! log owns. Here the appenders are shared references keyed by name, which is what a logger's
//! configuration line actually lists - `Logger.root=4,Console Auth` names two appenders by name -
//! and it removes the id from the interface as well as the lifetime problem.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::appender::Appender;
use super::log_common::LogLevel;
use super::log_message::LogMessage;

/// One appender a logger writes to, shared with every other logger that names it.
pub type SharedAppender = Arc<Mutex<Box<dyn Appender>>>;

/// A named channel with a level and a set of appenders.
#[derive(Clone)]
pub struct Logger {
    name: String,
    level: LogLevel,
    appenders: BTreeMap<String, SharedAppender>,
}

impl Logger {
    /// A logger with a name and a level, holding no appender yet.
    pub fn new(name: impl Into<String>, level: LogLevel) -> Self {
        Self {
            name: name.into(),
            level,
            appenders: BTreeMap::new(),
        }
    }

    /// The name a message carries as its kind, and the name a configuration line uses.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The level this logger listens at.
    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// Change the level.
    pub fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    /// Attach an appender by name.
    pub fn add_appender(&mut self, name: impl Into<String>, appender: SharedAppender) {
        self.appenders.insert(name.into(), appender);
    }

    /// Detach an appender by name.
    pub fn del_appender(&mut self, name: &str) {
        self.appenders.remove(name);
    }

    /// The appenders this logger writes to, by name.
    pub fn appenders(&self) -> impl Iterator<Item = &SharedAppender> {
        self.appenders.values()
    }

    /// Hand a message to every appender, if the level and the text allow it.
    ///
    /// Reference: `Logger::write`: a disabled logger passes nothing, a logger configured above the
    /// message's severity passes nothing, and an empty message is dropped.
    pub fn write(&self, message: &LogMessage) {
        if self.level == LogLevel::Disabled
            || self.level < message.level
            || message.text.is_empty()
        {
            return;
        }

        for appender in self.appenders.values() {
            if let Ok(mut appender) = appender.lock() {
                appender.write(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::appender::AppenderBase;
    use crate::logging::log_common::{AppenderFlags, AppenderType};

    struct Collector {
        base: AppenderBase,
        lines: Vec<String>,
    }

    impl Appender for Collector {
        fn base(&self) -> &AppenderBase {
            &self.base
        }
        fn base_mut(&mut self) -> &mut AppenderBase {
            &mut self.base
        }
        fn appender_type(&self) -> AppenderType {
            AppenderType::None
        }
        fn write_text(&mut self, message: &LogMessage, prefix: &str) {
            self.lines.push(format!("{prefix}{}", message.text));
        }
    }

    fn shared(level: LogLevel) -> SharedAppender {
        Arc::new(Mutex::new(Box::new(Collector {
            base: AppenderBase::new(1, "collector", level, AppenderFlags::NONE),
            lines: Vec::new(),
        }) as Box<dyn Appender>))
    }

    #[test]
    fn a_logger_at_info_drops_debug() {
        let appender = shared(LogLevel::Trace);
        let mut logger = Logger::new("root", LogLevel::Info);
        logger.add_appender("collector", appender.clone());

        logger.write(&LogMessage::new(LogLevel::Info, "root", "yes"));
        logger.write(&LogMessage::new(LogLevel::Debug, "root", "no"));

        let locked = appender.lock().unwrap();
        let collector = locked
            .base()
            .name()
            .to_string();
        assert_eq!(collector, "collector");
    }

    #[test]
    fn a_disabled_logger_passes_nothing() {
        let appender = shared(LogLevel::Trace);
        let mut logger = Logger::new("root", LogLevel::Disabled);
        logger.add_appender("collector", appender.clone());
        logger.write(&LogMessage::new(LogLevel::Fatal, "root", "nothing"));
    }

    #[test]
    fn an_empty_message_is_dropped() {
        let appender = shared(LogLevel::Trace);
        let mut logger = Logger::new("root", LogLevel::Trace);
        logger.add_appender("collector", appender);
        logger.write(&LogMessage::new(LogLevel::Info, "root", ""));
    }

    #[test]
    fn an_appender_can_be_removed_by_name() {
        let appender = shared(LogLevel::Trace);
        let mut logger = Logger::new("root", LogLevel::Trace);
        logger.add_appender("collector", appender);
        assert_eq!(logger.appenders().count(), 1);
        logger.del_appender("collector");
        assert_eq!(logger.appenders().count(), 0);
    }
}
