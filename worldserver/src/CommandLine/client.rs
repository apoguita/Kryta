//! `kryta-cli`: the standalone operator client for a running world.
//!
//! Reference: the world's local control endpoint is the reference's own arrangement for a separate
//! operator process (`ACSoap.cpp:81-140`: authenticate, queue the command, wait for the world thread).
//! This is the command a person types. **It holds no command of its own**: it names a registered
//! operation and the world decides what that means, so a new operation is a registration in the world
//! and nothing here (`docs/port/14-operator-surfaces.md`).
//!
//! ```text
//! kryta-cli help                                    this usage
//! kryta-cli list                                    the operations the running world registers
//! kryta-cli <operation> [--arg name=value]...       call one
//! kryta-cli init-credential <file>                  create the service credential, restricted
//! ```
//!
//! The port and the credential come from the world program's own settings - `Control.Port` and
//! `Control.CredentialFile` in `worldserver/worldserver.json` - so an operator types no plumbing.
//! `--config`, `--port` and `--credential-file` override each of them.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::Duration;

use common::configuration::{Config, Policy};

/// Where the world program's own settings are, unless told otherwise.
const DEFAULT_CONFIG: &str = "worldserver/worldserver.json";

/// What one command line asked for.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    /// Print the usage.
    Help,
    /// Create a restricted service credential.
    InitCredential(std::path::PathBuf),
    /// Ask the world which operations it registers.
    List,
    /// Call one operation.
    Call(Call),
}

/// One operation to call.
#[derive(Debug, PartialEq, Eq)]
struct Call {
    /// The world program's settings file, for the port and the credential.
    config: std::path::PathBuf,
    /// An explicit port, when the caller wants one.
    port: Option<u16>,
    /// An explicit credential file, when the caller wants one.
    credential_file: Option<std::path::PathBuf>,
    /// The registered operation's name.
    operation: String,
    /// Its values, by name, in the order they were typed.
    arguments: BTreeMap<String, String>,
}

fn usage() -> String {
    [
        "kryta-cli - ask a running world for one of its operations",
        "",
        "usage:",
        "  kryta-cli help                                  this text",
        "  kryta-cli list                                  the operations the world registers",
        "  kryta-cli <operation> [--arg name=value]...      call one",
        "  kryta-cli init-credential <file>                create the service credential, restricted",
        "",
        "options for a call:",
        "  --config <file>            the world program's settings (default: worldserver/worldserver.json)",
        "  --port <n>                the control port, instead of Control.Port in that file",
        "  --credential-file <file>  the service credential, instead of Control.CredentialFile",
        "",
        "The world must be running with Control.Enable and a credential file. The answer is the",
        "operation's own text; a refusal prints its reason and exits 1.",
    ]
    .join("\n")
}

/// Read the command line into what it asked for.
fn parse(args: &[String]) -> Result<Mode, String> {
    let mut config = std::path::PathBuf::from(DEFAULT_CONFIG);
    let mut port = None;
    let mut credential_file = None;
    let mut words: Vec<String> = Vec::new();
    let mut arguments: BTreeMap<String, String> = BTreeMap::new();
    let mut rest = args.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "-h" | "--help" | "help" => return Ok(Mode::Help),
            "-c" | "--config" => {
                let value = rest.next().ok_or("--config needs a path")?;
                config = std::path::PathBuf::from(value);
            }
            "-p" | "--port" => {
                let value = rest.next().ok_or("--port needs a number")?;
                port = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| "--port must be a number")?,
                );
            }
            "--credential-file" => {
                let value = rest.next().ok_or("--credential-file needs a path")?;
                credential_file = Some(std::path::PathBuf::from(value));
            }
            "--arg" => {
                let value = rest.next().ok_or("--arg needs name=value")?;
                let Some((name, value)) = value.split_once('=') else {
                    return Err(format!("--arg {value:?} is not name=value"));
                };
                if arguments
                    .insert(name.to_string(), value.to_string())
                    .is_some()
                {
                    return Err(format!("--arg {name} is given twice"));
                }
            }
            other if other.starts_with("--") => return Err(format!("unknown option: {other}")),
            other => words.push(other.to_string()),
        }
    }
    match words.as_slice() {
        [] => Err("no operation named; `kryta-cli help` lists the usage".into()),
        [only] if only == "init-credential" => {
            Err("init-credential needs the file to create".into())
        }
        [only] if only == "list" => Ok(Mode::List),
        [only, path] if only == "init-credential" => {
            Ok(Mode::InitCredential(std::path::PathBuf::from(path)))
        }
        [operation, ..] => {
            if words.len() > 1 {
                return Err(format!(
                    "{} is not an option; values are given as --arg name=value",
                    words[1]
                ));
            }
            Ok(Mode::Call(Call {
                config,
                port,
                credential_file,
                operation: operation.clone(),
                arguments,
            }))
        }
    }
}

/// The port and credential a call needs: the settings, then the credential file, validated.
fn target(
    config_path: &std::path::Path,
    port: Option<u16>,
    credential_file: Option<&std::path::Path>,
) -> Result<(u16, String), String> {
    let config = Config::load_json(config_path, false, Policy::default())
        .map_err(|error| format!("cannot read {}: {error}", config_path.display()))?;
    let port = match port {
        Some(port) => port,
        None => {
            config
                .value::<u16>("Control.Port", 7878)
                .map_err(|error| format!("Control.Port: {error}"))?
                .0
        }
    };
    let file = match credential_file {
        Some(file) => file.to_path_buf(),
        None => {
            let (file, _) = config
                .value_string("Control.CredentialFile", "")
                .map_err(|error| format!("Control.CredentialFile: {error}"))?;
            if file.trim().is_empty() {
                return Err(format!(
                    "{} states no Control.CredentialFile; create one with `kryta-cli init-credential <file>` and name it there",
                    config_path.display()
                ));
            }
            std::path::PathBuf::from(file.trim())
        }
    };
    let credential = std::fs::read_to_string(&file)
        .map_err(|error| format!("cannot read the credential {}: {error}", file.display()))?;
    let credential = credential.trim().to_string();
    if credential.len() < 32 || !credential.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(format!(
            "the credential {} is not at least 32 ASCII letters or digits",
            file.display()
        ));
    }
    Ok((port, credential))
}

/// Ask the running world for one operation, and answer what it said.
fn call(
    port: u16,
    credential: &str,
    operation: &str,
    arguments: &BTreeMap<String, String>,
) -> Result<String, String> {
    let arguments = serde_json::Value::Object(
        arguments
            .iter()
            .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
            .collect(),
    );
    let body =
        serde_json::json!({"request_id": "cli", "operation": operation, "arguments": arguments})
            .to_string();
    let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .map_err(|error| format!("no world control is listening on 127.0.0.1:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let request = format!(
        "POST /v1/operations HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {credential}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("the request failed: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("the answer could not be read: {error}"))?;
    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or("the world answered with something that is not an HTTP response")?;
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| "the world answered with something that is not JSON")?;
    if value.get("ok").and_then(|ok| ok.as_bool()) != Some(true) {
        let code = value
            .pointer("/error/code")
            .and_then(|code| code.as_str())
            .unwrap_or("refused");
        let message = value
            .pointer("/error/message")
            .and_then(|message| message.as_str())
            .unwrap_or("the world refused the operation");
        return Err(format!("{message} ({code})"));
    }
    Ok(value
        .get("result")
        .and_then(|result| result.as_str())
        .unwrap_or_default()
        .to_string())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args)? {
        Mode::Help => {
            println!("{}", usage());
            Ok(())
        }
        Mode::InitCredential(path) => init_credential(&path),
        Mode::List => {
            let (port, credential) = target(std::path::Path::new(DEFAULT_CONFIG), None, None)?;
            let answer = call(port, &credential, "world.operations", &BTreeMap::new())?;
            println!("{answer}");
            Ok(())
        }
        Mode::Call(call_options) => {
            let (port, credential) = target(
                &call_options.config,
                call_options.port,
                call_options.credential_file.as_deref(),
            )?;
            let answer = call(
                port,
                &credential,
                &call_options.operation,
                &call_options.arguments,
            )?;
            println!("{answer}");
            Ok(())
        }
    }
}

#[cfg(windows)]
fn init_credential(path: &std::path::Path) -> Result<(), String> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create credential file: {error}"))?;
    let script = r#"try { $ErrorActionPreference='Stop'; $p=$env:KRYTA_CONTROL_FILE; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User; $a=Get-Acl -LiteralPath $p; $a.SetAccessRuleProtection($true,$false); $a.SetOwner($sid); $rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','Allow'); $a.AddAccessRule($rule); Set-Acl -LiteralPath $p -AclObject $a; exit 0 } catch { exit 1 }"#;
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("KRYTA_CONTROL_FILE", path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| "cannot set credential file permissions")?;
    if !status.success() {
        return Err("cannot restrict credential file permissions; empty file left in place".into());
    }
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| "cannot generate a credential")?;
    let secret: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    file.write_all(secret.as_bytes())
        .map_err(|_| "cannot write credential")?;
    println!("Created restricted credential file {}.", path.display());
    Ok(())
}

#[cfg(not(windows))]
fn init_credential(_: &std::path::Path) -> Result<(), String> {
    Err("credential initialization is not implemented on this platform".into())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kryta-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn help_needs_no_operation_and_names_the_ones_that_exist() {
        for words in [&["help"][..], &["-h"], &["--help"]] {
            assert_eq!(parse(&args(words)).unwrap(), Mode::Help);
        }
        let text = usage();
        assert!(text.contains("kryta-cli list"), "{text}");
        assert!(text.contains("--arg name=value"), "{text}");
    }

    #[test]
    fn a_call_takes_the_operation_and_its_values_by_name() {
        let mode = parse(&args(&["world.status", "--arg", "destination=ascalon"])).unwrap();
        match mode {
            Mode::Call(call) => {
                assert_eq!(call.operation, "world.status");
                assert_eq!(
                    call.arguments.get("destination").map(String::as_str),
                    Some("ascalon")
                );
                assert_eq!(call.config, std::path::PathBuf::from(DEFAULT_CONFIG));
                assert_eq!(call.port, None);
            }
            other => panic!("a call, not {other:?}"),
        }
    }

    #[test]
    fn the_port_and_the_credential_file_can_be_named() {
        let mode = parse(&args(&[
            "--config",
            "somewhere.json",
            "--port",
            "7878",
            "--credential-file",
            "cred",
            "world.status",
        ]))
        .unwrap();
        match mode {
            Mode::Call(call) => {
                assert_eq!(call.config, std::path::PathBuf::from("somewhere.json"));
                assert_eq!(call.port, Some(7878));
                assert_eq!(call.credential_file, Some(std::path::PathBuf::from("cred")));
            }
            other => panic!("a call, not {other:?}"),
        }
    }

    #[test]
    fn what_is_not_a_command_line_is_refused_with_a_reason() {
        for (words, expected) in [
            (&[][..], "no operation named"),
            (&["--port", "x", "world.status"], "--port must be a number"),
            (&["world.status", "--arg", "half"], "is not name=value"),
            (&["world.status", "--nonsense"], "unknown option"),
            (&["world.status", "extra"], "values are given as --arg"),
            (&["init-credential"], "needs the file"),
        ] {
            let error = parse(&args(words)).unwrap_err();
            assert!(error.contains(expected), "{words:?}: {error}");
        }
        let twice = parse(&args(&["world.status", "--arg", "a=1", "--arg", "a=2"])).unwrap_err();
        assert!(twice.contains("given twice"), "{twice}");
    }

    #[test]
    fn list_and_init_credential_are_their_own_modes() {
        assert_eq!(parse(&args(&["list"])).unwrap(), Mode::List);
        assert_eq!(
            parse(&args(&[
                "init-credential",
                "worldserver/control.credential"
            ]))
            .unwrap(),
            Mode::InitCredential(std::path::PathBuf::from("worldserver/control.credential"))
        );
    }
}
