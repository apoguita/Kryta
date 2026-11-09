//! The log: levels, appenders, loggers and the queue that feeds them.
//!
//! Reference: `src/common/Logging/` at 30a36ec4d. The files are the reference's own, one module
//! each, because the arrangement inside them is the part being ported:
//!
//! | here | reference |
//! |---|---|
//! | [`log_common`] | `LogCommon.h` - the levels, the appender types and the appender flags |
//! | [`log_message`] | `LogMessage.{h,cpp}` - one message and its timestamp |
//! | [`appender`] | `Appender.{h,cpp}` - the base that filters and builds a prefix |
//! | [`appender_console`] | `AppenderConsole.{h,cpp}` |
//! | [`appender_file`] | `AppenderFile.{h,cpp}` |
//! | [`logger`] | `Logger.{h,cpp}` - a named logger holding appenders |
//! | [`log`] | `Log.{h,cpp}` - the registry, the configuration and the worker thread |
//!
//! `AppenderDB` lives in `database/Logging/`, as it does in the reference.

#[path = "LogCommon.rs"]
pub mod log_common;
#[path = "LogMessage.rs"]
pub mod log_message;
#[path = "Appender.rs"]
pub mod appender;
#[path = "AppenderConsole.rs"]
pub mod appender_console;
#[path = "AppenderFile.rs"]
pub mod appender_file;
#[path = "Logger.rs"]
pub mod logger;
#[path = "Log.rs"]
pub mod log;

pub use appender::Appender;
pub use log::Log;
pub use log_common::{AppenderFlags, AppenderType, LogLevel, LOGGER_ROOT};
pub use log_message::LogMessage;
pub use logger::Logger;
