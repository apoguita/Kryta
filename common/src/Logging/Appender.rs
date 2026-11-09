//! The appender base: what every sink has, and the filter and prefix it applies.
//!
//! Reference: `src/common/Logging/Appender.{h,cpp}`.

use super::log_common::{AppenderFlags, AppenderType, LogLevel};
use super::log_message::LogMessage;

/// The four things every appender carries.
///
/// Reference: `class Appender`'s own fields, separated out here so an appender holds one of these
/// instead of four fields and four accessors.
#[derive(Debug, Clone)]
pub struct AppenderBase {
    id: u8,
    name: String,
    level: LogLevel,
    flags: AppenderFlags,
}

impl AppenderBase {
    /// A base with its identifier, its name, its level and its flags.
    pub fn new(id: u8, name: impl Into<String>, level: LogLevel, flags: AppenderFlags) -> Self {
        Self {
            id,
            name: name.into(),
            level,
            flags,
        }
    }

    /// The appender's number, which a logger's list names.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// The appender's name, the part after `Appender.` in the configuration.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The level this appender listens at.
    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// Change the level.
    pub fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    /// What this appender prefixes.
    pub fn flags(&self) -> AppenderFlags {
        self.flags
    }

    /// Build the text in front of a message.
    ///
    /// Reference: `Appender::write`, which appends the timestamp, then the level word, then
    /// `[<logger>]`, each only when its flag is set, and then hands the message on.
    pub fn prefix(&self, message: &LogMessage) -> String {
        let mut prefix = String::new();

        if self.flags.contains(AppenderFlags::PREFIX_TIMESTAMP) {
            prefix.push_str(message.time());
            prefix.push(' ');
        }

        if self.flags.contains(AppenderFlags::PREFIX_LOGLEVEL) {
            prefix.push_str(message.level.as_str());
            prefix.push(' ');
        }

        if self.flags.contains(AppenderFlags::PREFIX_LOGFILTERTYPE) {
            prefix.push('[');
            prefix.push_str(&message.kind);
            prefix.push_str("] ");
        }

        prefix
    }
}

/// A sink for log text.
///
/// Reference: `class Appender`. An implementor supplies its base, its type, and somewhere to put
/// the text; the filtering and the prefix are the base's job and are not repeated per appender.
pub trait Appender: Send {
    /// The four things every appender carries.
    fn base(&self) -> &AppenderBase;

    /// The four things every appender carries, to be changed.
    fn base_mut(&mut self) -> &mut AppenderBase;

    /// Where this appender puts text.
    fn appender_type(&self) -> AppenderType;

    /// The realm this appender writes for, when a caller wants to set it.
    ///
    /// Reference: `Appender::setRealmId`, which does nothing in the base class.
    fn set_realm_id(&mut self, _realm_id: u32) {}

    /// Put one message here, if this appender's level lets it through.
    ///
    /// Reference: `Appender::write`: a disabled appender takes nothing, and an appender configured
    /// above the message's severity takes nothing either.
    fn write(&mut self, message: &LogMessage) {
        let level = self.base().level();
        if level == LogLevel::Disabled || level < message.level {
            return;
        }

        let prefix = self.base().prefix(message);
        self.write_text(message, &prefix);
    }

    /// Put the text somewhere, with the prefix already built.
    ///
    /// Reference: the private `_write` each appender implements.
    fn write_text(&mut self, message: &LogMessage, prefix: &str);
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn collector(level: LogLevel, flags: u8) -> Collector {
        Collector {
            base: AppenderBase::new(1, "test", level, AppenderFlags::from_bits(flags)),
            lines: Vec::new(),
        }
    }

    #[test]
    fn a_disabled_appender_takes_nothing() {
        let mut appender = collector(LogLevel::Disabled, 0);
        appender.write(&LogMessage::new(LogLevel::Fatal, "root", "x"));
        assert!(appender.lines.is_empty());
    }

    #[test]
    fn an_appender_listens_down_to_its_level() {
        let mut appender = collector(LogLevel::Info, 0);
        appender.write(&LogMessage::new(LogLevel::Warn, "root", "warned"));
        appender.write(&LogMessage::new(LogLevel::Info, "root", "informed"));
        appender.write(&LogMessage::new(LogLevel::Debug, "root", "debugged"));
        assert_eq!(appender.lines, vec!["warned".to_string(), "informed".to_string()]);
    }

    #[test]
    fn the_prefix_follows_the_flags_in_the_reference_order() {
        let flags = AppenderFlags::PREFIX_TIMESTAMP
            | AppenderFlags::PREFIX_LOGLEVEL
            | AppenderFlags::PREFIX_LOGFILTERTYPE;
        let appender = collector(LogLevel::Trace, AppenderFlags::NONE.bits());
        let mut with_flags = appender;
        with_flags.base.flags = flags;

        let message = LogMessage::new(LogLevel::Error, "server.loading", "boom");
        let prefix = with_flags.base.prefix(&message);

        assert!(prefix.ends_with("ERROR [server.loading] "), "{prefix}");
        assert!(prefix.starts_with(&message.time().to_string()), "{prefix}");
    }
}
