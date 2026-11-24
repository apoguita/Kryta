//! What both programs do with their configuration while they start: print a complaint, read one
//! setting and print what the lookup had to say, take the severity policy from the command line,
//! and print the options they share.
//!
//! Reference: `src/common/Configuration/Config.cpp` at 30a36ec4d (`LogWithSeverity`,
//! `GetValueDefault<T>`) and the `options_description` both `Main.cpp` files build in
//! `GetConsoleArguments`. Nothing here names a program: each program keeps its own `Main`.

use std::str::FromStr;

use super::{Complaint, Config, Policy, PolicyOverride, Severity};

/// Print a complaint at the severity the policy gave it.
///
/// Reference: `LogWithSeverity`, whose first act is to say nothing at `Skip`.
pub fn report(complaint: &Complaint) {
    match complaint.severity {
        Severity::Skip => {}
        Severity::Warn => println!("warning: {}", complaint.message),
        Severity::Error => eprintln!("error: {}", complaint.message),
        Severity::Fatal => eprintln!("fatal: {}", complaint.message),
    }
}

/// Read one setting through [`Config::value`] and print what the lookup complained about.
pub fn read<T>(config: &Config, name: &str, default: T) -> Result<T, String>
where
    T: FromStr + std::fmt::Display + Copy,
{
    let (value, notes) = config
        .value::<T>(name, default)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    Ok(value)
}

/// Read one boolean setting through [`Config::value_bool`] and print what the lookup complained
/// about.
pub fn read_bool(config: &Config, name: &str, default: bool) -> Result<bool, String> {
    let (value, notes) = config
        .value_bool(name, default)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    Ok(value)
}

/// Read one string setting through [`Config::value_string`] and print what the lookup complained
/// about.
pub fn read_string(config: &Config, name: &str, default: &str) -> Result<String, String> {
    let (value, notes) = config
        .value_string(name, default)
        .map_err(|error| error.to_string())?;
    for note in notes {
        report(&note);
    }
    Ok(value)
}

/// The severity policy a program loads its configuration with: the default, changed by
/// `--config-policy` when the command line gave one.
///
/// A key the policy does not have is refused rather than ignored - the reference reports it the
/// same way.
pub fn policy(config_policy: Option<&str>) -> Result<Policy, String> {
    let mut policy = Policy::default();
    if let Some(text) = config_policy {
        let control = PolicyOverride::parse(text).map_err(|error| error.to_string())?;
        if !control.apply(&mut policy) {
            return Err(format!(
                "--config-policy names no value this program has: '{}'",
                control.key
            ));
        }
    }
    Ok(policy)
}

/// Print the options both programs take, naming the configuration file read when `-c` names none.
pub fn print_usage(default_config: &str) {
    println!("Allowed options:");
    println!("  -h [ --help ]             print usage message");
    println!("  -v [ --version ]          print version build info");
    println!("  -d [ --dry-run ]          Dry run");
    println!("  -c [ --config ] arg       use <arg> as configuration file (default: {default_config})");
    println!("  --config-policy policy    override config severity policy");
}
