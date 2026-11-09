//! The vocabulary a log is made of.
//!
//! Reference: `src/common/Logging/LogCommon.h`. The numbers are the ones the configuration files
//! carry, so they are stated once and never invented: `Logger.root=4,Console Auth` says level 4,
//! and level 4 has to mean Info.

/// How loud a message is, and how loud a logger or appender listens.
///
/// Reference: `enum LogLevel : uint8`. A lower number is more severe, and both the logger filter
/// and the appender filter read it that way: a component configured at `Info` accepts a message
/// whose level is a smaller number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Nothing passes. Reference: `LOG_LEVEL_DISABLED = 0`.
    Disabled = 0,
    /// Reference: `LOG_LEVEL_FATAL = 1`.
    Fatal = 1,
    /// Reference: `LOG_LEVEL_ERROR = 2`.
    Error = 2,
    /// Reference: `LOG_LEVEL_WARN = 3`.
    Warn = 3,
    /// Reference: `LOG_LEVEL_INFO = 4`.
    Info = 4,
    /// Reference: `LOG_LEVEL_DEBUG = 5`.
    Debug = 5,
    /// Reference: `LOG_LEVEL_TRACE = 6`.
    Trace = 6,
}

impl LogLevel {
    /// The number the configuration files use.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Read a configuration number, or nothing when it is not a level.
    ///
    /// Reference: the `Acore::StringTo<LogLevel>` conversion used by `Log::CreateLoggerFromConfig`.
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Disabled,
            1 => Self::Fatal,
            2 => Self::Error,
            3 => Self::Warn,
            4 => Self::Info,
            5 => Self::Debug,
            6 => Self::Trace,
            _ => return None,
        })
    }

    /// The word an appender prefixes.
    ///
    /// Reference: `Appender::getLogLevelString`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "DISABLED",
            Self::Fatal => "FATAL",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }
}

/// Where log text goes.
///
/// Reference: `enum AppenderType : uint8`. The numbers are the `Appender.<name>` configuration
/// values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppenderType {
    /// Reference: `APPENDER_NONE`.
    None = 0,
    /// Reference: `APPENDER_CONSOLE`.
    Console = 1,
    /// Reference: `APPENDER_FILE`.
    File = 2,
    /// Reference: `APPENDER_DB`. The appender itself lives in `database/Logging/`.
    Db = 3,
}

impl AppenderType {
    /// Read a configuration number, or nothing when it is not an appender type.
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::None,
            1 => Self::Console,
            2 => Self::File,
            3 => Self::Db,
            _ => return None,
        })
    }
}

/// What an appender puts in front of the text.
///
/// Reference: `enum AppenderFlags : uint8`, a bitmask. The numbers are the third field of an
/// `Appender.<name>` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppenderFlags(u8);

impl AppenderFlags {
    /// Reference: `APPENDER_FLAGS_NONE`.
    pub const NONE: Self = Self(0x00);
    /// Reference: `APPENDER_FLAGS_PREFIX_TIMESTAMP`. Prefix the message with its time.
    pub const PREFIX_TIMESTAMP: Self = Self(0x01);
    /// Reference: `APPENDER_FLAGS_PREFIX_LOGLEVEL`. Prefix the message with its level word.
    pub const PREFIX_LOGLEVEL: Self = Self(0x02);
    /// Reference: `APPENDER_FLAGS_PREFIX_LOGFILTERTYPE`. Prefix the message with `[<logger>]`.
    pub const PREFIX_LOGFILTERTYPE: Self = Self(0x04);
    /// Reference: `APPENDER_FLAGS_USE_TIMESTAMP`. Put the log's timestamp in a file name.
    pub const USE_TIMESTAMP: Self = Self(0x08);
    /// Reference: `APPENDER_FLAGS_MAKE_FILE_BACKUP`. Rename an existing file before overwriting.
    pub const MAKE_FILE_BACKUP: Self = Self(0x10);

    /// Build the set from a configuration number.
    pub fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// The number the configuration carries.
    pub fn bits(self) -> u8 {
        self.0
    }

    /// Whether every flag in `other` is set here.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for AppenderFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// The logger everything inherits from.
///
/// Reference: `LOGGER_ROOT`, the name a logger falls back to when nothing more specific is
/// configured.
pub const LOGGER_ROOT: &str = "root";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lower_number_is_more_severe() {
        assert!(LogLevel::Fatal < LogLevel::Info);
        assert!(LogLevel::Trace > LogLevel::Warn);
    }

    #[test]
    fn the_configuration_numbers_round_trip() {
        for value in 0..=6u8 {
            let level = LogLevel::from_u8(value).unwrap();
            assert_eq!(level.as_u8(), value);
        }
        assert_eq!(LogLevel::from_u8(7), None);
    }

    #[test]
    fn the_level_words_are_the_reference_strings() {
        assert_eq!(LogLevel::Warn.as_str(), "WARN");
        assert_eq!(LogLevel::Disabled.as_str(), "DISABLED");
    }

    #[test]
    fn flags_are_a_bitmask() {
        let flags = AppenderFlags::from_bits(0x03);
        assert!(flags.contains(AppenderFlags::PREFIX_TIMESTAMP));
        assert!(flags.contains(AppenderFlags::PREFIX_LOGLEVEL));
        assert!(!flags.contains(AppenderFlags::PREFIX_LOGFILTERTYPE));
    }
}
