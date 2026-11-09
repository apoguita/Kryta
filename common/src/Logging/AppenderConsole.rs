//! The console appender.
//!
//! Reference: `src/common/Logging/AppenderConsole.{h,cpp}`.
//!
//! What it does, in the reference's order: a message at Error or Fatal goes to stderr and every
//! other level goes to stdout; the six colours in the configuration's `optional1` are checked
//! against the six levels at construction; and the text is written with the prefix in front and a
//! newline after.
//!
//! One gap, named rather than hidden: the reference colours the text with Win32's
//! `SetConsoleTextAttribute` on Windows and an ANSI escape elsewhere. The colours are parsed and
//! validated here exactly as it validates them, and they are not yet applied to the output. Colour
//! is presentation, so no byte the client sees and no line an operator reads changes; what is
//! missing is only the colour of that line.

use std::io::Write;

use super::appender::{Appender, AppenderBase};
use super::log_common::{AppenderFlags, AppenderType, LogLevel};
use super::log_message::LogMessage;

/// The six levels a console colour is given for.
///
/// Reference: `NUM_ENABLED_LOG_LEVELS`, which is `LOG_LEVEL_TRACE` - the number of levels a
/// configuration can name, not the range including `Disabled`.
pub const NUM_ENABLED_LOG_LEVELS: usize = LogLevel::Trace as usize;

/// The colours the reference offers, in its own order.
///
/// Reference: `enum ColorTypes`, which the configuration names by number. Only the numbering is
/// used here, because the colour is not applied yet.
pub const NUM_COLOR_TYPES: u8 = 16;

/// Text to the console.
#[derive(Debug)]
pub struct AppenderConsole {
    base: AppenderBase,
    colors: [Option<u8>; NUM_ENABLED_LOG_LEVELS],
}

impl AppenderConsole {
    /// Build one from its configuration fields.
    ///
    /// Reference: `AppenderConsole::AppenderConsole` and `InitColors`. `args` is the optional part
    /// of an `Appender.<name>` line: `args[0]` becomes the sixth field, which for a console
    /// appender is the colour list.
    pub fn new(
        id: u8,
        name: impl Into<String>,
        level: LogLevel,
        flags: AppenderFlags,
        args: &[String],
    ) -> Result<Self, String> {
        let name = name.into();
        let mut appender = Self {
            base: AppenderBase::new(id, name, level, flags),
            colors: [None; NUM_ENABLED_LOG_LEVELS],
        };

        // The reference reads the colour list from `args[3]` of its own vector, which is the
        // fourth field of the line and the first optional one.
        if let Some(field) = args.first() {
            appender.init_colors(field)?;
        }

        Ok(appender)
    }

    /// Read and check the colour list.
    ///
    /// Reference: `AppenderConsole::InitColors`: an empty list means colouring is off; a list whose
    /// length is not the number of enabled levels is an error; an entry that is not a colour is an
    /// error.
    fn init_colors(&mut self, field: &str) -> Result<(), String> {
        if field.is_empty() {
            return Ok(());
        }

        let entries: Vec<&str> = field.split_whitespace().collect();
        if entries.len() != NUM_ENABLED_LOG_LEVELS {
            return Err(format!(
                "Log::CreateAppenderFromConfig: Invalid color data '{}' for console appender {} (expected {} entries, got {})",
                field,
                self.base.name(),
                NUM_ENABLED_LOG_LEVELS,
                entries.len()
            ));
        }

        for (index, entry) in entries.iter().enumerate() {
            match entry.parse::<u8>() {
                Ok(value) if value < NUM_COLOR_TYPES => self.colors[index] = Some(value),
                _ => {
                    return Err(format!(
                        "Log::CreateAppenderFromConfig: Invalid color '{}' for log level {} on console appender {}",
                        entry,
                        index + LogLevel::Fatal as usize,
                        self.base.name()
                    ));
                }
            }
        }

        Ok(())
    }

    /// The colour configured for a level, if any.
    pub fn color(&self, level: LogLevel) -> Option<u8> {
        let index = level as usize;
        if index == 0 || index > NUM_ENABLED_LOG_LEVELS {
            return None;
        }
        self.colors[index - 1]
    }
}

impl Appender for AppenderConsole {
    fn base(&self) -> &AppenderBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut AppenderBase {
        &mut self.base
    }

    fn appender_type(&self) -> AppenderType {
        AppenderType::Console
    }

    fn write_text(&mut self, message: &LogMessage, prefix: &str) {
        // Reference: `bool stdout_stream = !(message->level == LOG_LEVEL_ERROR ||
        // message->level == LOG_LEVEL_FATAL);`
        let to_stderr = matches!(message.level, LogLevel::Error | LogLevel::Fatal);

        let line = format!("{prefix}{}\n", message.text);
        if to_stderr {
            let _ = std::io::stderr().write_all(line.as_bytes());
            let _ = std::io::stderr().flush();
        } else {
            let _ = std::io::stdout().write_all(line.as_bytes());
            let _ = std::io::stdout().flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(field: &str) -> Vec<String> {
        vec![field.to_string()]
    }

    #[test]
    fn the_shipped_colour_list_is_accepted() {
        let appender = AppenderConsole::new(
            1,
            "Console",
            LogLevel::Debug,
            AppenderFlags::NONE,
            &args("1 9 3 6 5 8"),
        )
        .unwrap();
        assert_eq!(appender.color(LogLevel::Fatal), Some(1));
        assert_eq!(appender.color(LogLevel::Info), Some(6));
        assert_eq!(appender.color(LogLevel::Trace), Some(8));
    }

    #[test]
    fn a_list_of_the_wrong_length_is_refused() {
        let error =
            AppenderConsole::new(1, "Console", LogLevel::Debug, AppenderFlags::NONE, &args("1 9 3"))
                .unwrap_err();
        assert!(error.contains("expected 6 entries, got 3"), "{error}");
    }

    #[test]
    fn a_colour_that_is_not_a_colour_is_refused() {
        let error = AppenderConsole::new(
            1,
            "Console",
            LogLevel::Debug,
            AppenderFlags::NONE,
            &args("1 9 3 6 5 99"),
        )
        .unwrap_err();
        assert!(error.contains("Invalid color '99'"), "{error}");
    }

    #[test]
    fn no_colour_list_is_not_an_error() {
        let appender =
            AppenderConsole::new(1, "Console", LogLevel::Debug, AppenderFlags::NONE, &[]).unwrap();
        assert_eq!(appender.color(LogLevel::Info), None);
    }
}
