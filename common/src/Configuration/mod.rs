//! Config: the reader both programs load before anything else.
//!
//! Reference: `src/common/Configuration/Config.{h,cpp}` at 30a36ec4d, and
//! `src/common/Configuration/ConfigValueCache.h`.
//!
//! # The format
//!
//! The file is **JSON**, read by [`json::Document`], and `docs/decisions/0005-json-configuration.md`
//! is the record of why this tree deviates there: a configuration in this project is an operator's
//! file, edited and shared, and JSON is the format such a file can be shown, diffed and read by
//! another program. The reference's own format is `Key = value`, and its parser is still here -
//! [`Config::parse`] - because it is the reference's translation and its quirks are measured
//! behaviour, but no file this tree ships is in it.
//!
//! What did **not** change is the arrangement, and that is the point:
//!
//! - A key is a dotted path (`RealmServerPort`, `Logger.network.trace`), spelled the reference's way
//!   whatever the file format, so every caller and every document still says the same word.
//! - Lookup is [`Config::value`], the reference's `GetValueDefault<T>`: an environment variable
//!   wins over the file, a missing key is reported at its severity and the default answers, and a
//!   value that does not parse is reported and the default answers.
//! - A problem is a [`Complaint`] with the reference's severity policy, and a `Fatal` one stops the
//!   program instead of logging and carrying on.
//!
//! One deviation, and it is a Rust shape rather than a behaviour change: where the reference
//! calls `ABORT()` on a `Fatal` severity, this returns [`ConfigError::Fatal`] and the program
//! turns it into exit code 1. The message is the reference's.
//!
//! # The reference's own parser, and its quirks
//!
//! Kept because it is the reference's translation, and because the quirks are behaviour somebody may
//! port a file against:
//!
//! - A line is trimmed; empty lines are skipped.
//! - A line starting with `#` is a comment, and a line starting with `[` is a section header.
//!   **Both are skipped entirely** - a key is global and a section is decoration. That is the
//!   reference's behaviour, not an oversight here: `[worldserver]` appears at the top of both
//!   shipped conf files and never scopes a key.
//! - A line without `=` is reported and skipped, which is why a `;` line is an error in the
//!   reference and not a comment.
//! - The value is everything after the first `=`, trimmed, with **every** `"` removed - not only
//!   a surrounding pair.
//! - A key that appears twice in one file is reported, and the first one wins.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[path = "Json.rs"]
pub mod json;

/// What a configuration problem is worth.
///
/// Reference: `enum class ConfigSeverity`. `Skip` says nothing; `Warn` and `Error` print;
/// `Fatal` stops the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Say nothing.
    Skip,
    /// Print, and carry on.
    Warn,
    /// Print, and treat the load as failed.
    Error,
    /// Print, and stop: the server cannot start without this value.
    Fatal,
}

/// How strict each kind of problem is.
///
/// Reference: `struct ConfigPolicy`, whose defaults are these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// A value missing from the file and from the environment.
    pub missing_option: Severity,
    /// A value missing that the program cannot run without.
    pub critical_option: Severity,
    /// A value that does not parse as its type.
    pub value_error: Severity,
    /// The file itself, or a file with no keys in it.
    pub missing_file: Severity,
    /// A key the server does not know.
    pub unknown_option: Severity,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            missing_option: Severity::Warn,
            critical_option: Severity::Fatal,
            value_error: Severity::Error,
            missing_file: Severity::Error,
            unknown_option: Severity::Error,
        }
    }
}

/// One `key=severity` pair from the command line.
///
/// Reference: the `--config-policy` argument, parsed by `ConfigMgr::Configure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyOverride {
    /// The name of the severity to override, as the reference spells it.
    pub key: String,
    /// The severity to give it.
    pub severity: Severity,
}

impl PolicyOverride {
    /// Read a `key=severity` pair.
    ///
    /// The keys are the reference's own: `default`, `critical_option`, `missing_option`,
    /// `value_error`, `missing_file`, `unknown_option`.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let (key, severity) = text
            .split_once('=')
            .ok_or_else(|| ConfigError::BadPolicy(text.to_string()))?;

        let severity = match severity.trim() {
            "skip" => Severity::Skip,
            "warn" => Severity::Warn,
            "error" => Severity::Error,
            "fatal" => Severity::Fatal,
            other => return Err(ConfigError::BadPolicy(other.to_string())),
        };

        Ok(Self {
            key: key.trim().to_string(),
            severity,
        })
    }

    /// Apply this override to a policy, and answer whether it named a key the policy has.
    ///
    /// Reference: the same switch, where `default` sets every severity.
    pub fn apply(&self, policy: &mut Policy) -> bool {
        match self.key.as_str() {
            "default" => {
                policy.missing_option = self.severity;
                policy.critical_option = self.severity;
                policy.value_error = self.severity;
                policy.missing_file = self.severity;
                policy.unknown_option = self.severity;
                true
            }
            "missing_option" => {
                policy.missing_option = self.severity;
                true
            }
            "critical_option" => {
                policy.critical_option = self.severity;
                true
            }
            "value_error" => {
                policy.value_error = self.severity;
                true
            }
            "missing_file" => {
                policy.missing_file = self.severity;
                true
            }
            "unknown_option" => {
                policy.unknown_option = self.severity;
                true
            }
            _ => false,
        }
    }
}

/// A problem the reader found, with the reference's sentence ready to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Complaint {
    /// The severity the policy gives this kind of problem.
    pub severity: Severity,
    /// The reference's own sentence.
    pub message: String,
}

/// Why a configuration could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The file is missing or holds no keys, at a severity that does not let the server start.
    Unusable(Complaint),
    /// A value is missing at [`Severity::Fatal`]: the server cannot start without it.
    ///
    /// Reference: `ABORT()`. This returns instead, and the program exits 1 with the message.
    Fatal(String),
    /// A `--config-policy` value this reader does not understand.
    BadPolicy(String),
    /// The file is not a configuration at all: it does not parse as JSON, or its root is not an
    /// object of keys.
    Malformed(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unusable(complaint) => f.write_str(&complaint.message),
            Self::Fatal(message) => f.write_str(message),
            Self::BadPolicy(text) => write!(f, "unknown config policy '{text}'"),
            Self::Malformed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {}

/// The configuration a program reads.
#[derive(Debug, Clone)]
pub struct Config {
    filename: PathBuf,
    options: BTreeMap<String, String>,
    critical: BTreeSet<String>,
    policy: Policy,
    dry_run: bool,
    complaints: Vec<Complaint>,
}

impl Config {
    /// Read a file.
    ///
    /// Reference: `ConfigMgr::LoadAppConfigs` -> `LoadInitial` -> `ParseFile`.
    pub fn load(
        filename: impl AsRef<Path>,
        is_optional: bool,
        policy: Policy,
    ) -> Result<Self, ConfigError> {
        let filename = filename.as_ref().to_path_buf();

        let source = match std::fs::read_to_string(&filename) {
            Ok(source) => source,
            Err(_) => {
                let severity = if is_optional {
                    Severity::Skip
                } else {
                    policy.missing_file
                };
                let message = format!(
                    "> Config::LoadFile: Failed open {}file '{}'",
                    if is_optional { "optional " } else { "" },
                    filename.display()
                );
                // A skip is a successful no-op: the program carries on with no keys.
                if severity == Severity::Skip {
                    return Ok(Self::empty(filename, policy));
                }
                return Err(ConfigError::Unusable(Complaint { severity, message }));
            }
        };

        let config = Self::parse(&source, filename, policy);

        if config.options.is_empty() {
            let severity = if is_optional {
                Severity::Skip
            } else {
                policy.missing_file
            };
            let message = format!(
                "> Config::LoadFile: Empty file '{}'",
                config.filename.display()
            );
            if severity == Severity::Skip {
                return Ok(config);
            }
            return Err(ConfigError::Unusable(Complaint { severity, message }));
        }

        Ok(config)
    }

    /// Read a JSON file: the project's format.
    ///
    /// The same three outcomes the reference's loader has, in the same order: a missing file is the
    /// `missing_file` severity (or a successful empty load when the caller says the file is
    /// optional), a file with no keys in it is the same, and a file that does not parse is refused
    /// with what the parser said. What the document holds is then the reference's bag of keys, so
    /// every getter below behaves exactly as it does for the reference's own format.
    pub fn load_json(
        filename: impl AsRef<Path>,
        is_optional: bool,
        policy: Policy,
    ) -> Result<Self, ConfigError> {
        let filename = filename.as_ref().to_path_buf();

        let text = match std::fs::read_to_string(&filename) {
            Ok(text) => text,
            Err(_) => {
                let severity = if is_optional {
                    Severity::Skip
                } else {
                    policy.missing_file
                };
                let message = format!(
                    "> Config::LoadFile: Failed open {}file '{}'",
                    if is_optional { "optional " } else { "" },
                    filename.display()
                );
                // A skip is a successful no-op: the program carries on with no keys.
                if severity == Severity::Skip {
                    return Ok(Self::empty(filename, policy));
                }
                return Err(ConfigError::Unusable(Complaint { severity, message }));
            }
        };

        // A file that exists and does not parse is refused whatever `is_optional` says: a typo must
        // not become a server that quietly runs on defaults while its operator believes otherwise.
        let document = json::Document::parse(&text, &filename)?;

        let (options, complaints) = document.settings();
        let config = Self {
            filename: filename.clone(),
            options,
            critical: BTreeSet::new(),
            policy,
            dry_run: false,
            complaints,
        };

        if config.options.is_empty() {
            // A file that stated keys and had every one of them refused is not empty: saying "Empty
            // file" would send the operator looking for the wrong problem.
            let refused: Vec<&str> = config
                .complaints
                .iter()
                .filter(|complaint| complaint.severity >= Severity::Error)
                .map(|complaint| complaint.message.as_str())
                .collect();
            if !refused.is_empty() {
                return Err(ConfigError::Malformed(refused.join("\n")));
            }

            let severity = if is_optional {
                Severity::Skip
            } else {
                policy.missing_file
            };
            let message = format!(
                "> Config::LoadFile: Empty file '{}'",
                config.filename.display()
            );
            if severity == Severity::Skip {
                return Ok(config);
            }
            return Err(ConfigError::Unusable(Complaint { severity, message }));
        }

        Ok(config)
    }

    /// A file's text, parsed as the reference's own format.
    ///
    /// Kept for the reference's parser; no file this tree ships is written this way.
    pub fn parse(source: &str, filename: PathBuf, policy: Policy) -> Self {
        let mut options: BTreeMap<String, String> = BTreeMap::new();
        let mut complaints = Vec::new();

        for (index, raw) in source.lines().enumerate() {
            let line = raw.trim();

            if line.is_empty() {
                continue;
            }

            // Comments and section headers are both skipped, and by the same test.
            if line.starts_with('#') || line.starts_with('[') {
                continue;
            }

            let Some((entry, value)) = line.split_once('=') else {
                complaints.push(Complaint {
                    severity: Severity::Warn,
                    message: format!(
                        "> Config::LoadFile: Failure to read line number {} in file '{}'. Skip this line",
                        index + 1,
                        filename.display()
                    ),
                });
                continue;
            };

            let entry = entry.trim();
            // Every quote is removed, not only a surrounding pair.
            let value = value.trim().replace('"', "");

            if options.contains_key(entry) {
                complaints.push(Complaint {
                    severity: Severity::Warn,
                    message: format!(
                        "> Config::LoadFile: Duplicate key name '{}' in config file '{}'",
                        entry,
                        filename.display()
                    ),
                });
                continue;
            }

            options.insert(entry.to_string(), value);
        }

        Self {
            filename,
            options,
            critical: BTreeSet::new(),
            policy,
            dry_run: false,
            complaints,
        }
    }

    fn empty(filename: PathBuf, policy: Policy) -> Self {
        Self {
            filename,
            options: BTreeMap::new(),
            critical: BTreeSet::new(),
            policy,
            dry_run: false,
            complaints: Vec::new(),
        }
    }

    /// The file this configuration came from.
    pub fn filename(&self) -> &Path {
        &self.filename
    }

    /// The policy in force.
    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// What the reader found while loading, in file order.
    pub fn complaints(&self) -> &[Complaint] {
        &self.complaints
    }

    /// Every key the file holds, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.options.keys().map(String::as_str)
    }

    /// Mark a key as one the server cannot start without.
    ///
    /// Reference: `_criticalConfigOptions`.
    pub fn mark_critical(&mut self, name: &str) {
        self.critical.insert(name.to_string());
    }

    /// Every key this configuration holds, with the values an operator may see and the ones that
    /// must not leave the machine masked.
    ///
    /// A configuration that cannot be read back is a configuration nobody can debug: an operator who
    /// cannot see which file was read, or which keys it held, is left guessing at a running process.
    /// The masking is not decoration - a `*DatabaseInfo` value holds a password, and a log line is
    /// copied into issue reports.
    pub fn describe(&self) -> Vec<(String, String)> {
        self.options
            .iter()
            .map(|(key, value)| (key.clone(), masked(key, value)))
            .collect()
    }

    /// The raw text of a key, if the file has it.
    pub fn raw(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }

    /// The lookup every getter shares.
    ///
    /// Reference: the head of `GetValueDefault<T>`. The environment variable
    /// `AC_<UPPER SNAKE KEY>` wins, then the file, then the default - which is reported at
    /// [`Policy::critical_option`] for a key marked critical and [`Policy::missing_option`]
    /// otherwise, and answered.
    fn lookup(
        &self,
        name: &str,
        default_text: &str,
    ) -> Result<(String, Vec<Complaint>), ConfigError> {
        let mut complaints = Vec::new();
        let env_var = env_var_name(name);

        if let Ok(from_env) = std::env::var(&env_var) {
            complaints.push(Complaint {
                severity: Severity::Skip,
                message: format!(
                    "> Config: Found config value '{}' from environment variable '{}'.",
                    name, env_var
                ),
            });
            return Ok((from_env, complaints));
        }

        if let Some(value) = self.options.get(name) {
            return Ok((value.clone(), complaints));
        }

        let is_critical = self.critical.contains(name);
        let severity = if is_critical {
            self.policy.critical_option
        } else {
            self.policy.missing_option
        };

        let message = if is_critical {
            format!(
                "> Config:\n\nFATAL ERROR: Missing property {} in config file {}, add \"{} = {}\" to this file or define '{}' as an environment variable\n\nYour server cannot start without this option!",
                name,
                self.filename.display(),
                name,
                default_text,
                env_var
            )
        } else {
            format!(
                "> Config: Missing property {} in config file {}, add \"{} = {}\" to this file or define '{}' as an environment variable.",
                name,
                self.filename.display(),
                name,
                default_text,
                env_var
            )
        };

        if severity == Severity::Fatal {
            return Err(ConfigError::Fatal(message));
        }
        complaints.push(Complaint { severity, message });
        Ok((default_text.to_string(), complaints))
    }

    /// A value, with the reference's lookup order and its severity policy.
    ///
    /// Reference: `GetValueDefault<T>`. A value that does not parse is reported at
    /// [`Policy::value_error`], and the default answers.
    pub fn value<T>(&self, name: &str, default: T) -> Result<(T, Vec<Complaint>), ConfigError>
    where
        T: FromStr + std::fmt::Display + Copy,
    {
        let (text, mut complaints) = self.lookup(name, &default.to_string())?;

        match T::from_str(&text) {
            Ok(value) => Ok((value, complaints)),
            Err(_) => {
                let message = format!(
                    "> Config: Bad value defined for name '{}', going to use '{}' instead",
                    name, default
                );
                if self.policy.value_error == Severity::Fatal {
                    return Err(ConfigError::Fatal(message));
                }
                complaints.push(Complaint {
                    severity: self.policy.value_error,
                    message,
                });
                Ok((default, complaints))
            }
        }
    }

    /// A boolean, with the reference's own rule.
    ///
    /// Reference: the `GetValueDefault<bool>` specialization, which reads the value as text with
    /// the default spelled `"1"`/`"0"` and answers `value == "1"`. Anything else is false, and
    /// that is not an error in the reference.
    pub fn value_bool(
        &self,
        name: &str,
        default: bool,
    ) -> Result<(bool, Vec<Complaint>), ConfigError> {
        let (text, complaints) = self.lookup(name, if default { "1" } else { "0" })?;
        Ok((text == "1", complaints))
    }

    /// A string value.
    ///
    /// Reference: the `GetValueDefault<std::string>` specialization, which is the same lookup
    /// without the conversion step.
    pub fn value_string(
        &self,
        name: &str,
        default: &str,
    ) -> Result<(String, Vec<Complaint>), ConfigError> {
        self.lookup(name, default)
    }

    /// Dry run, set by `--dry-run`. Reference: `ConfigMgr::setDryRun`.
    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
    }

    /// Whether this run is a dry run.
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}

/// The environment variable that overrides a key.
///
/// Reference: `GetEnvVarName`, which is `"AC_"` followed by [`ini_key_to_env_var_key`].
pub fn env_var_name(key: &str) -> String {
    format!("AC_{}", ini_key_to_env_var_key(key))
}

/// Turn a configuration key into an environment variable's own spelling.
///
/// Reference: `IniKeyToEnvVarKey`, with its own worked examples:
/// `SomeConfig -> SOME_CONFIG`, `myNestedConfig.opt1 -> MY_NESTED_CONFIG_OPT_1`,
/// `LogDB.Opt.ClearTime -> LOG_DB_OPT_CLEAR_TIME`.
pub fn ini_key_to_env_var_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let mut result = String::with_capacity(key.len() * 2);

    for (index, &current) in chars.iter().enumerate() {
        if current == ' ' || current == '.' || current == '-' {
            result.push('_');
            continue;
        }

        if index < chars.len() - 1 {
            let next = chars[index + 1];

            // "aB" becomes "A_B"
            if !current.is_uppercase() && next.is_uppercase() {
                result.push(current.to_ascii_uppercase());
                result.push('_');
                continue;
            }

            // "a1" becomes "A_1"
            if !current.is_ascii_digit() && next.is_ascii_digit() {
                result.push(current.to_ascii_uppercase());
                result.push('_');
                continue;
            }
        }

        result.push(current.to_ascii_uppercase());
    }

    result
}

/// A value as it may appear in a log line.
///
/// `LoginDatabaseInfo` and its siblings are `host;port;user;password;database`, so the fourth field is
/// a password. Everything else is shown as it is; a value that is not a connection string is not a
/// secret this server knows how to recognise, and hiding it would make the dump useless.
pub fn masked(key: &str, value: &str) -> String {
    if !key.ends_with("DatabaseInfo") {
        return value.to_string();
    }

    let fields: Vec<&str> = value.split(';').collect();
    if fields.len() < 5 {
        return value.to_string();
    }

    let mut masked: Vec<String> = fields.iter().map(|field| field.to_string()).collect();
    masked[3] = "********".to_string();
    masked.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(source: &str) -> Config {
        Config::parse(source, PathBuf::from("test.conf"), Policy::default())
    }

    #[test]
    fn a_section_header_scopes_nothing_and_is_skipped() {
        let config = config("[worldserver]\nRealmServerPort = 3724\n");
        assert_eq!(config.raw("RealmServerPort"), Some("3724"));
    }

    #[test]
    fn a_hash_line_is_a_comment() {
        let config = config("# RealmServerPort = 1\nRealmServerPort = 3724\n");
        assert_eq!(config.raw("RealmServerPort"), Some("3724"));
    }

    #[test]
    fn a_line_without_an_equals_is_a_complaint_and_is_skipped() {
        let config = config("; this is not a comment to the reference\nBindIP = \"0.0.0.0\"\n");
        assert_eq!(config.complaints().len(), 1);
        assert!(config.complaints()[0].message.contains("line number 1"));
        assert_eq!(config.raw("BindIP"), Some("0.0.0.0"));
    }

    #[test]
    fn every_quote_is_removed_not_only_a_surrounding_pair() {
        let config = config("PidFile = \"a\"b\"c\"\n");
        assert_eq!(config.raw("PidFile"), Some("abc"));
    }

    #[test]
    fn a_duplicate_key_is_reported_and_the_first_wins() {
        let config = config("BindIP = one\nBindIP = two\n");
        assert_eq!(config.raw("BindIP"), Some("one"));
        assert!(
            config.complaints()[0]
                .message
                .contains("Duplicate key name")
        );
    }

    #[test]
    fn the_value_keeps_its_equals_signs() {
        let config = config("SomeKey = a=b\n");
        assert_eq!(config.raw("SomeKey"), Some("a=b"));
    }

    #[test]
    fn a_missing_key_answers_the_default_and_says_so() {
        let config = config("BindIP = \"0.0.0.0\"\n");
        let (value, complaints): (i32, _) = config.value("RealmServerPort", 3724).unwrap();
        assert_eq!(value, 3724);
        assert_eq!(complaints[0].severity, Severity::Warn);
    }

    #[test]
    fn a_critical_key_that_is_missing_stops_the_reader() {
        let mut config = config("BindIP = \"0.0.0.0\"\n");
        config.mark_critical("RealmServerPort");
        let answer: Result<(i32, Vec<Complaint>), ConfigError> =
            config.value("RealmServerPort", 3724);
        let error = answer.unwrap_err();
        assert!(matches!(error, ConfigError::Fatal(_)));
        assert!(
            error
                .to_string()
                .contains("cannot start without this option")
        );
    }

    #[test]
    fn a_value_that_does_not_parse_answers_the_default_at_error() {
        let config = config("RealmServerPort = not-a-number\n");
        let (value, complaints) = config.value::<i32>("RealmServerPort", 3724).unwrap();
        assert_eq!(value, 3724);
        assert_eq!(complaints[0].severity, Severity::Error);
    }

    #[test]
    fn a_boolean_is_one_else_false_and_that_is_not_an_error() {
        let config = config("SOAP.Enabled = yes\n");
        let (value, complaints) = config.value_bool("SOAP.Enabled", false).unwrap();
        assert!(!value);
        assert!(complaints.iter().all(|c| c.severity != Severity::Error));
    }

    #[test]
    fn the_environment_variable_name_is_ac_plus_upper_snake() {
        assert_eq!(ini_key_to_env_var_key("SomeConfig"), "SOME_CONFIG");
        assert_eq!(
            ini_key_to_env_var_key("myNestedConfig.opt1"),
            "MY_NESTED_CONFIG_OPT_1"
        );
        assert_eq!(
            ini_key_to_env_var_key("LogDB.Opt.ClearTime"),
            "LOG_DB_OPT_CLEAR_TIME"
        );
        assert_eq!(env_var_name("RealmServerPort"), "AC_REALM_SERVER_PORT");
    }

    #[test]
    fn a_policy_override_names_its_key_and_refuses_a_typo() {
        let mut policy = Policy::default();
        assert!(
            PolicyOverride::parse("critical_option=skip")
                .unwrap()
                .apply(&mut policy)
        );
        assert_eq!(policy.critical_option, Severity::Skip);
        assert!(
            !PolicyOverride::parse("nonsense=skip")
                .unwrap()
                .apply(&mut policy)
        );
        assert!(PolicyOverride::parse("default=shout").is_err());
    }

    #[test]
    fn a_missing_file_at_error_is_not_usable_and_at_skip_is_empty() {
        let policy = Policy {
            missing_file: Severity::Error,
            ..Policy::default()
        };
        assert!(matches!(
            Config::load("does-not-exist.conf", false, policy),
            Err(ConfigError::Unusable(_))
        ));

        let lenient = Policy {
            missing_file: Severity::Skip,
            ..Policy::default()
        };
        let config = Config::load("does-not-exist.conf", false, lenient).unwrap();
        assert_eq!(config.keys().count(), 0);
    }
    #[test]
    fn a_connection_string_has_its_password_masked_and_nothing_else() {
        let described = masked(
            "LoginDatabaseInfo",
            "127.0.0.1;5432;kryta;hunter2;kryta_login",
        );
        assert_eq!(described, "127.0.0.1;5432;kryta;********;kryta_login");
        assert!(!described.contains("hunter2"));
    }

    #[test]
    fn other_values_are_shown_as_they_are() {
        assert_eq!(masked("RealmServerPort", "6112"), "6112");
        assert_eq!(masked("LogsDir", ""), "");
        // A key that is not a connection string is not touched even when it looks like one.
        assert_eq!(masked("SomeInfo", "a;b;c;d;e"), "a;b;c;d;e");
    }

    #[test]
    fn a_configuration_describes_its_keys_with_the_password_hidden() {
        let config = Config::parse(
            "RealmServerPort = 6112\nLoginDatabaseInfo = \"127.0.0.1;5432;kryta;secret;kryta_login\"\n",
            PathBuf::from("test.conf"),
            Policy::default(),
        );
        let described = config.describe();
        assert_eq!(described.len(), 2);
        assert!(
            described
                .iter()
                .any(|(key, value)| key == "RealmServerPort" && value == "6112")
        );
        assert!(
            described
                .iter()
                .any(|(key, value)| key == "LoginDatabaseInfo" && value.contains("********"))
        );
        assert!(!described.iter().any(|(_, value)| value.contains("secret")));
    }

    /// A JSON file goes through the same getters as the reference's format, which is the whole
    /// reason the format changed and the reader did not.
    fn json_file(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_json_file_is_read_by_the_same_getters() {
        let path = json_file(
            "auth",
            r#"{
                "RealmServerPort": 6112,
                "BindIP": "0.0.0.0",
                "Portal": {"Enable": true, "Bind": "127.0.0.1:6601"},
                "LoginDatabaseInfo": "127.0.0.1;5432;kryta;secret;kryta_login"
            }"#,
        );
        let config = Config::load_json(&path, false, Policy::default()).unwrap();

        assert_eq!(
            config.value::<u16>("RealmServerPort", 3724).unwrap().0,
            6112
        );
        assert_eq!(
            config.value_string("BindIP", "0.0.0.0").unwrap().0,
            "0.0.0.0"
        );
        assert!(config.value_bool("Portal.Enable", false).unwrap().0);
        assert_eq!(
            config.value_string("Portal.Bind", "").unwrap().0,
            "127.0.0.1:6601"
        );

        // The masking is not a property of the format: a JSON file describes itself the same way.
        let described = config.describe();
        assert!(
            described
                .iter()
                .any(|(key, value)| key == "LoginDatabaseInfo" && value.contains("********"))
        );
        assert!(!described.iter().any(|(_, value)| value.contains("secret")));
    }

    #[test]
    fn a_json_file_is_read_with_the_references_lookup_order() {
        // A stated key answers its own value; a key the file does not state answers the default and
        // says so. The environment variable above the file is the same one the reference's format
        // reads, and it is not exercised here because setting one is unsafe in this edition and
        // unsafe code is forbidden in this tree.
        let path = json_file("lookup", r#"{"RealmServerPort": 6112}"#);
        let config = Config::load_json(&path, false, Policy::default()).unwrap();

        let (stated, complaints) = config.value::<u16>("RealmServerPort", 3724).unwrap();
        assert_eq!(stated, 6112);
        assert!(complaints.is_empty());

        let (absent, complaints) = config.value::<u16>("MaxPingTime", 30).unwrap();
        assert_eq!(absent, 30);
        assert_eq!(complaints[0].severity, Severity::Warn);
    }

    #[test]
    fn a_json_file_that_is_malformed_is_refused_even_when_it_is_optional() {
        // An optional file that exists and is wrong is not an absent file: a typo must not become a
        // server that quietly runs on defaults.
        let path = json_file("broken", "{ \"RealmServerPort\": }");
        let error = Config::load_json(&path, true, Policy::default()).unwrap_err();
        assert!(matches!(error, ConfigError::Malformed(_)), "{error}");
        assert!(error.to_string().contains("not valid JSON"), "{error}");
    }

    #[test]
    fn an_optional_json_file_that_is_absent_is_an_empty_configuration() {
        let config = Config::load_json("does-not-exist.json", true, Policy::default()).unwrap();
        assert_eq!(config.keys().count(), 0);
    }

    #[test]
    fn a_json_file_with_no_keys_is_the_references_empty_file() {
        let path = json_file("empty", r#"{"note": "nothing configured yet"}"#);
        let error = Config::load_json(&path, false, Policy::default()).unwrap_err();
        assert!(error.to_string().contains("Empty file"), "{error}");
    }

    #[test]
    fn a_configuration_holds_options_and_hands_out_no_payload() {
        // A byte string is not an option and this type cannot produce one: `Config` answers values a
        // program reads as settings, and nothing else. That is what stops a payload from being
        // mistaken for configuration again - the reader simply has no method for it. A payload is
        // read from the document it lives in, or held as a constant, and decoded by
        // `common::utilities::hex_bytes`.
        let path = json_file("payload", r#"{"data": {"hex": "08 00 66 21"}}"#);
        let config = Config::load_json(&path, false, Policy::default()).unwrap();
        // The key is there as text, which is all a configuration can say about it.
        assert_eq!(config.raw("data"), Some("08 00 66 21"));
    }
}
