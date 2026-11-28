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

/// The folder the configuration is in, from where a program is started: the repository's root.
pub const CONFIG_FOLDER: &str = "config";

/// Load a program's settings: the file `-c` named, or, when it named none, the program's file in
/// the flavor this machine runs (`config/flavor.json`, then
/// `config/flavors/<flavor>/<program>.json`). Either way a file in a flavor's folder has the
/// machine's own values laid over it (`docs/decisions/0016-a-server-is-a-flavor.md`).
pub fn load_settings(
    file: Option<&std::path::Path>,
    program: &str,
    policy: Policy,
) -> Result<Config, String> {
    match file {
        Some(file) => Config::load_json(file, false, policy),
        None => Config::load_active(CONFIG_FOLDER, program, policy),
    }
    .map_err(|error| error.to_string())
}

/// Read one setting that names a file or folder of the flavor's own content: the path is relative
/// to the flavor's folder ([`Config::flavor_path`]).
pub fn read_flavor_path(config: &Config, name: &str, default: &str) -> Result<String, String> {
    Ok(config.flavor_path(&read_string(config, name, default)?))
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
