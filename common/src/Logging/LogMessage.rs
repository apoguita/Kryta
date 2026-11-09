//! One log message and its time.
//!
//! Reference: `src/common/Logging/LogMessage.{h,cpp}`.
//!
//! One dependency, and it is the reference's own `<ctime>`: a message's timestamp is the local
//! clock, formatted the way the reference formats it. `chrono` is the equivalent call, and no
//! behaviour depends on it beyond the text that reaches a file.

use chrono::Local;

use super::log_common::LogLevel;

/// The format both the log text and a backup file's name use.
///
/// Reference: `LogMessage::getTimeStr`, which is `Acore::Time::TimeToTimestampStr(time, "%Y-%m-%d %X")`.
pub const TIME_FORMAT: &str = "%Y-%m-%d %X";

/// One message on its way to the appenders.
///
/// Reference: `struct LogMessage`. The reference also carries a `prefix` field, filled by each
/// appender before it writes; here the prefix is built and passed by the appender instead, so the
/// message itself stays immutable once it is queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMessage {
    /// How loud the message is.
    pub level: LogLevel,
    /// The logger's name, which is the `type` the reference prints in brackets.
    pub kind: String,
    /// The text, formatted already.
    pub text: String,
    /// The name used by a file appender's `%s` placeholder.
    pub param1: Option<String>,
    /// When it was made.
    time: String,
}

impl LogMessage {
    /// A message made now.
    pub fn new(level: LogLevel, kind: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            level,
            kind: kind.into(),
            text: text.into(),
            param1: None,
            time: timestamp(),
        }
    }

    /// A message made now, with the name a dynamic file appender substitutes.
    ///
    /// Reference: the second `LogMessage` constructor, which carries `_param1`.
    pub fn with_param1(
        level: LogLevel,
        kind: impl Into<String>,
        text: impl Into<String>,
        param1: impl Into<String>,
    ) -> Self {
        Self {
            level,
            kind: kind.into(),
            text: text.into(),
            param1: Some(param1.into()),
            time: timestamp(),
        }
    }

    /// The message's own time.
    pub fn time(&self) -> &str {
        &self.time
    }

    /// The size of the text an appender writes, in bytes.
    ///
    /// Reference: `LogMessage::Size`, which is the prefix plus the text.
    pub fn size(&self, prefix: &str) -> usize {
        prefix.len() + self.text.len()
    }
}

/// The local clock, spelled the reference's way.
///
/// Reference: `LogMessage::getTimeStr()`.
pub fn timestamp() -> String {
    Local::now().format(TIME_FORMAT).to_string()
}

/// The local clock in the form a backup file's suffix uses.
///
/// Reference: `AppenderFile::OpenFile`, which appends `LogMessage::getTimeStr(GetEpochTime())` and
/// then replaces every `:` with `-`, because a colon is not a filename character on Windows.
pub fn backup_suffix() -> String {
    timestamp().replace(':', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_its_level_kind_and_text() {
        let message = LogMessage::new(LogLevel::Info, "server.authserver", "hello");
        assert_eq!(message.level, LogLevel::Info);
        assert_eq!(message.kind, "server.authserver");
        assert_eq!(message.text, "hello");
    }

    #[test]
    fn the_timestamp_is_nineteen_characters_of_the_reference_shape() {
        let now = timestamp();
        assert_eq!(now.len(), 19, "{now}");
        assert_eq!(&now[4..5], "-");
        assert_eq!(&now[13..14], ":");
    }

    #[test]
    fn a_backup_suffix_holds_no_colon() {
        assert!(!backup_suffix().contains(':'));
    }

    #[test]
    fn the_size_is_the_prefix_and_the_text() {
        let message = LogMessage::new(LogLevel::Info, "root", "abc");
        assert_eq!(message.size("12 "), 6);
    }
}
