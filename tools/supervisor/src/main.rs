//! The stack: one command that starts this machine's programs and stops them together.
//!
//! What it runs is data, not code: `stack.json` lists the services - a program, its arguments and
//! the port it binds - and the supervisor starts each one, prints its output under its own name,
//! waits for its READY line, and keeps them alive together. Ctrl+C stops them all; a service that
//! dies on its own stops the rest and the supervisor answers that service's exit code.
//!
//! It also owns the control channel (`Control.rs`): a loopback line protocol with `status`,
//! `stop [service]`, `restart <service>` and `quit`, so a running stack can be asked what it is
//! doing and told to restart one thing without a second owner of its processes. And it reads the
//! reference's exit-code contract (`Main.cpp:434-439`): a world that exits `2` asked to be restarted,
//! and this program - the restarter the reference's comment names - starts it again.
//!
//! A service's port is the stack's ([`take_port`]): a listener already there that runs the same
//! program as the service is an instance left over from an earlier session, and the supervisor
//! stops it so the stack owns its port; a listener that belongs to anything else is refused with
//! the process named, never silently killed.
//!
//! This arrangement is this tree's own, taken from ASURA's `asura-supervisor` (one command for a
//! stack that is otherwise several windows) rather than from the reference, which has no such
//! program. What it still does not do, and says so: health checks beyond the READY line.
//!
//! Run it from the repository root:
//!
//! ```text
//! cargo run -p kryta-supervisor                      # start the stack; Ctrl+C stops it
//! cargo run -p kryta-supervisor -- --check           # start, wait for every READY, stop, exit
//! cargo run -p kryta-supervisor -- status            # ask the running stack (control channel)
//! cargo run -p kryta-supervisor -- restart world     # restart one service
//! ```

#[path = "Control.rs"]
mod control;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use common::configuration::json::Document;

/// Where the list lives, relative to the repository root.
const DEFAULT_STACK: &str = "tools/supervisor/stack.json";
/// How long a service may take to print its READY line.
const READY_TIMEOUT: Duration = Duration::from_secs(60);
/// How many times one service may ask to be restarted (exit code `2`) before the stack is stopped.
const MAX_RESTARTS: u32 = 5;

/// One service, as `stack.json` states it.
#[derive(Debug, Clone)]
struct Service {
    /// What the supervisor calls it in the log's prefix.
    name: String,
    /// The cargo package's binary name, built under `target/debug/`.
    program: String,
    /// Its arguments, verbatim.
    args: Vec<String>,
    /// The port it binds, for the up-report.
    port: u16,
    /// Whether it inherits the terminal's standard input: the one service whose console is the
    /// operator's. Every other service is given no input at all, so two programs cannot read one
    /// terminal (`docs/port/14-operator-surfaces.md`).
    stdin: bool,
}

/// The whole list and the supervisor's own settings.
#[derive(Debug, Clone)]
struct Stack {
    /// The services, in the file's order.
    services: Vec<Service>,
    /// The loopback port the control channel binds; `0` leaves it off.
    control_port: u16,
}

/// Read the list.
fn load(path: &std::path::Path) -> Result<Stack, String> {
    let document = Document::read(path).map_err(|error| error.to_string())?;
    let (settings, _) = document.settings();
    let control_port: u16 = match settings.get("control_port").map(String::as_str) {
        Some(value) if !value.trim().is_empty() => value
            .trim()
            .parse()
            .map_err(|_| format!("{}: 'control_port' is not a number", path.display()))?,
        _ => 0,
    };
    let console_service = settings.get("console_service").cloned().unwrap_or_default();
    let records = document.records("services")?;
    let mut services = Vec::with_capacity(records.len());
    for record in records {
        let port: u16 = record
            .required("port")?
            .parse()
            .map_err(|_| format!("{}: 'port' is not a number", record.at()))?;
        let name = record.required("name")?;
        let stdin = name == console_service;
        services.push(Service {
            name,
            program: record.required("program")?,
            args: record.strings("args")?.unwrap_or_default(),
            port,
            stdin,
        });
    }
    Ok(Stack {
        services,
        control_port,
    })
}

/// The built binary of a service, beside the repository root.
fn exe_of(program: &str) -> std::path::PathBuf {
    let root = std::env::current_dir().expect("the working directory");
    let name = if cfg!(windows) {
        format!("{program}.exe")
    } else {
        program.to_string()
    };
    root.join("target").join("debug").join(name)
}

/// Build a service's program before it starts, so the one command always runs the code that is in the
/// tree - not a binary left over from the last build. Cargo is incremental: a tree that has not
/// changed builds in a fraction of a second, and a tree that has changed says so instead of being
/// quietly ignored. The build is cargo's own, printing its own progress to this console.
fn ensure_built(program: &str) -> Result<(), String> {
    let exe = exe_of(program);
    if !exe.exists() {
        println!("{program} is not built yet; building it now (cargo build -p {program})...");
    }
    let status = Command::new("cargo")
        .args(["build", "-p", program])
        .status()
        .map_err(|error| format!("cannot run cargo build for {program}: {error}"))?;
    if exe.exists() {
        return Ok(());
    }
    Err(format!(
        "{program} did not build (cargo answered {status}); `cargo build -p {program}` says why"
    ))
}

/// Start one service, and hand every line it prints to the supervisor.
///
/// The port is claimed before anything else: see [`take_port`]. A stale copy of the service's own
/// program from an earlier session is stopped first, which also matters because a running program
/// holds its executable file open and would make `cargo build` fail to replace it.
fn start(service: &Service, lines: Sender<ServiceLine>) -> Result<Child, String> {
    take_port(service)?;
    ensure_built(&service.program)?;
    let exe = exe_of(&service.program);
    let mut child = Command::new(&exe)
        .args(&service.args)
        // This variable makes the service print its readiness handshake to the piped stdout.
        .env("KRYTA_STACK", "1")
        .stdin(if service.stdin {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        // A fatal error must reach the terminal even if the child and supervisor exit immediately.
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", service.name))?;
    let name = service.name.clone();
    let stdout = child.stdout.take().expect("the program's stdout is piped");
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                return;
            };
            if lines.send((name.clone(), line)).is_err() {
                return;
            }
        }
    });
    Ok(child)
}

/// A service's port belongs to the stack.
///
/// Before a service starts, the supervisor looks at the port `stack.json` names. When a listener is
/// already there, it asks the operating system which process owns it. A process running the same
/// program as the service is an instance left over from an earlier session - the supervisor stops
/// it, so the stack owns its port and starts cleanly. Anything else is refused with the process
/// named and the stack does not start: silently killing a program this stack does not run would be
/// a far worse surprise than a clear message. Every start goes through here, including a restart
/// from the control channel and the reference's exit-code-2 restarter.
fn take_port(service: &Service) -> Result<(), String> {
    if service.port == 0 {
        return Ok(());
    }
    let Some(pid) = listener_holder(service.port) else {
        return Ok(());
    };
    let image = process_image(pid).unwrap_or_else(|| format!("pid {pid}"));
    let own_program = if cfg!(windows) {
        format!("{}.exe", service.program)
    } else {
        service.program.clone()
    };
    if !image.eq_ignore_ascii_case(&own_program) {
        return Err(format!(
            "{}: port {} is held by {image} (pid {pid}), which is not this stack's {}; \
             stop that process or change stack.json",
            service.name, service.port, service.program
        ));
    }
    report(
        &service.name,
        &format!(
            "port {} was held by an earlier {image} (pid {pid}) from outside this stack; \
             stopping it so this stack owns the port",
            service.port
        ),
    );
    let _ = stop_process(pid);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if listener_holder(service.port).is_none() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "{}: port {} is still held after stopping pid {pid}",
        service.name, service.port
    ))
}

/// The control port is claimed like a service's: a leftover instance of this supervisor is stopped
/// so there is exactly one stack, and anything else on the port is refused with its process named.
fn take_control_port(port: u16) -> Result<(), String> {
    let Some(pid) = listener_holder(port) else {
        return Ok(());
    };
    let image = process_image(pid).unwrap_or_else(|| format!("pid {pid}"));
    let own = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "kryta-supervisor".to_string());
    if !image.eq_ignore_ascii_case(&own) {
        return Err(format!(
            "the control port {port} is held by {image} (pid {pid}), which is not this stack's \
             supervisor; stop that process or change stack.json"
        ));
    }
    println!(
        "the control port {port} is held by an earlier {image} (pid {pid}); stopping it so this \
         stack owns the channel"
    );
    let _ = stop_process(pid);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if listener_holder(port).is_none() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "the control port {port} is still held after stopping pid {pid}"
    ))
}

/// The process listening on a port, from one `netstat -ano` snapshot.
fn listener_holder(port: u16) -> Option<u32> {
    let output = Command::new("netstat").args(["-ano"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    holder_in(port, &text)
}

/// The row of a `netstat -ano` table that names a listening socket on `port`, as its owning pid.
///
/// A row is the protocol, the local address, the foreign address, the state and the pid. Only a
/// `LISTENING` TCP row owns the port; an `ESTABLISHED` row is a connection that happens to name it.
fn holder_in(port: u16, table: &str) -> Option<u32> {
    let wanted = port.to_string();
    for line in table.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 5 || !fields[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !fields[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let local = fields[1];
        if local.rsplit_once(':').map(|(_, address_port)| address_port) != Some(wanted.as_str()) {
            continue;
        }
        if let Ok(pid) = fields[4].parse::<u32>() {
            return Some(pid);
        }
    }
    None
}

/// A process's image name, from one `tasklist` CSV row.
fn process_image(pid: u32) -> Option<String> {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    image_in(&text)
}

/// The image name a `tasklist ... /FO CSV /NH` answer states, from its first line.
fn image_in(answer: &str) -> Option<String> {
    let line = answer.lines().find(|line| !line.trim().is_empty())?;
    let name = line.split(',').next()?.trim().trim_matches('"');
    (!name.is_empty() && !name.starts_with("INFO:")).then(|| name.to_string())
}

/// Stop one process, and answer whether it is gone.
fn stop_process(pid: u32) -> bool {
    Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// One standard output line and the service that printed it.
type ServiceLine = (String, String);

/// The last lines a service printed, kept so a failure can be read without a `--logs` run.
///
/// A stack's console is not a log tail: with a thousand connections the service log is on disk
/// (`Server.log`, `Errors.log`) and the terminal carries the stack's own lifecycle. What is kept here
/// is what an operator needs when a service dies: the words it died with.
type Tails = std::collections::BTreeMap<String, std::collections::VecDeque<String>>;

/// The number of lines a tail holds.
const TAIL_LINES: usize = 30;

/// Keep one line in its service's tail.
fn keep(tails: &mut Tails, name: &str, line: &str) {
    let tail = tails.entry(name.to_string()).or_default();
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line.to_string());
}

/// Print one line under its service's name.
fn report(name: &str, line: &str) {
    println!("{name:>6} | {line}");
}

/// Show a service's line if it should be shown, and keep it for a failure's tail either way.
///
/// Standard output is shown with `--logs`. Standard error goes directly to the terminal.
fn show(tails: &mut Tails, logs: bool, name: &str, line: &str) {
    if logs {
        report(name, line);
    }
    keep(tails, name, line);
}

/// Print a service's kept tail, after the line that says why it is being printed.
///
/// The tail holds stdout only. The service's stderr already reached the terminal.
fn dump_tail(tails: &mut Tails, name: &str) {
    match tails.remove(name) {
        Some(tail) if !tail.is_empty() => {
            for line in tail {
                report(name, &line);
            }
        }
        _ => report(
            name,
            "no output of its own; its log is Server.log (or Auth.log) in the working directory, and Errors.log beside it",
        ),
    }
}

/// The state word a control client reads for one service.
fn state_word(state: &control::State, name: &str, value: &str) {
    if let Ok(mut state) = state.lock() {
        state.insert(name.to_string(), value.to_string());
    }
}

fn state_all(state: &control::State, services: &[Service], value: &str) {
    for service in services {
        state_word(state, &service.name, value);
    }
}

/// Start every service and wait for the ones that have not said READY yet.
///
/// Answers whether each one's READY line was seen. A service's standard error is printed whatever the
/// mode - it carries nothing but problems - and its standard output only with `--logs` or from the tail
/// a failure dumps; the service writes its own log to disk (`Server.log`, `Errors.log`), and the stack's
/// console is for the stack's lifecycle.
fn wait_ready(
    services: &[Service],
    ready: &mut [bool],
    children: &mut [(Service, Child)],
    lines: &Receiver<ServiceLine>,
    state: &control::State,
    tails: &mut Tails,
    logs: bool,
) -> bool {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok((name, line)) => {
                show(tails, logs, &name, &line);
                if line.contains("READY: listening on")
                    && let Some(index) = services.iter().position(|service| service.name == name)
                {
                    ready[index] = true;
                    state_word(state, &name, &format!("up on {}", services[index].port));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return ready.iter().all(|up| *up);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }

        // A service that died while starting is reported with the words it died with; the others are
        // stopped so the whole command fails as one thing.
        for (service, child) in children.iter_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                report(&service.name, &format!("exited with {status}"));
                dump_tail(tails, &service.name);
                state_word(
                    state,
                    &service.name,
                    &format!("exited {}", status.code().unwrap_or(1)),
                );
                return false;
            }
        }
        if ready.iter().all(|up| *up) {
            return true;
        }
        if Instant::now() >= deadline {
            for (index, service) in services.iter().enumerate() {
                if !ready[index] {
                    report(
                        &service.name,
                        &format!("no READY line within {READY_TIMEOUT:?}"),
                    );
                    state_word(state, &service.name, "no READY line");
                }
            }
            return false;
        }
    }
}

/// Stop every child: kill it, then wait a moment for it to be gone.
fn stop_all(children: &mut [(Service, Child)]) {
    for (_, child) in children.iter_mut() {
        let _ = child.kill();
    }
    let deadline = Instant::now() + READY_TIMEOUT.min(Duration::from_secs(10));
    while Instant::now() < deadline {
        let mut running = 0usize;
        for (_, child) in children.iter_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                running += 1;
            }
        }
        if running == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Start one service again after a stop or a restart request, and wait for its READY line.
fn restart(
    service: &Service,
    children: &mut Vec<(Service, Child)>,
    lines: &Sender<ServiceLine>,
    received: &Receiver<ServiceLine>,
    state: &control::State,
    tails: &mut Tails,
    logs: bool,
) -> Result<(), String> {
    let child = start(service, lines.clone())?;
    state_word(state, &service.name, "restarting");
    children.push((service.clone(), child));
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        match received.recv_timeout(Duration::from_millis(200)) {
            Ok((name, line)) => {
                show(tails, logs, &name, &line);
                if name == service.name && line.contains("READY: listening on") {
                    state_word(state, &service.name, &format!("up on {}", service.port));
                    return Ok(());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    state_word(state, &service.name, "restarted, no READY line");
    Err(format!(
        "{} restarted but said no READY line in {READY_TIMEOUT:?}",
        service.name
    ))
}

fn usage() {
    println!("Allowed options:");
    println!("  -h [ --help ]             print usage message");
    println!(
        "  -c [ --config ] arg       use <arg> as the service list (default: {DEFAULT_STACK})"
    );
    println!("  --check                   start, wait for every READY line, stop, and say so");
    println!(
        "  -l [ --logs ]             also print each service's standard output (its log is on disk)"
    );
    println!("  status                    ask a running stack what it is doing");
    println!("  stop [service]            stop one service, or every service");
    println!("  restart <service>         start one service again");
    println!("  quit                      stop every service and exit");
}

/// The verbs a client sends to a running supervisor (`Control.rs`).
fn client_line(args: &[String]) -> Option<String> {
    match (args.first().map(String::as_str), args.len()) {
        (Some("status"), 1) => Some("status".into()),
        (Some("stop"), 1) => Some("stop".into()),
        (Some("quit"), 1) => Some("quit".into()),
        (Some("stop"), 2) => Some(format!("stop {}", args[1])),
        (Some("restart"), 2) => Some(format!("restart {}", args[1])),
        _ => None,
    }
}

fn main() -> std::process::ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();

    // A verb is a client of a running stack; everything else starts one.
    if client_line(&raw).is_some() {
        let mut config_path = std::path::PathBuf::from(DEFAULT_STACK);
        if let Some(rest) = raw
            .iter()
            .position(|argument| argument == "-c" || argument == "--config")
            && let Some(value) = raw.get(rest + 1)
        {
            config_path = std::path::PathBuf::from(value);
        }
        let control_port = match load(&config_path) {
            Ok(stack) => stack.control_port,
            Err(error) => {
                eprintln!("the service list cannot be read: {error}");
                return std::process::ExitCode::FAILURE;
            }
        };
        if control_port == 0 {
            eprintln!("the control channel is off: give stack.json a control_port");
            return std::process::ExitCode::FAILURE;
        }
        let line = client_line(&raw).expect("a verb was matched");
        return match control::request(control_port, &line) {
            Ok(answer) => {
                println!("{answer}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    let mut config_path = std::path::PathBuf::from(DEFAULT_STACK);
    let mut check_only = false;
    let mut logs = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => {
                usage();
                return std::process::ExitCode::SUCCESS;
            }
            "-c" | "--config" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--config needs a path");
                    return std::process::ExitCode::FAILURE;
                };
                config_path = std::path::PathBuf::from(value);
            }
            "--check" => check_only = true,
            "-l" | "--logs" => logs = true,
            other => {
                eprintln!("unknown option: {other}");
                usage();
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    let stack = match load(&config_path) {
        Ok(stack) => stack,
        Err(error) => {
            eprintln!("the service list cannot be read: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let services = stack.services;

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed));
    }

    let (lines, received) = channel();
    let mut tails: Tails = std::collections::BTreeMap::new();
    let (actions, requests) = channel::<control::Request>();
    let state: control::State = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
    state_all(&state, &services, "starting");
    if stack.control_port != 0 {
        // The control channel's port is claimed like a service's: a stale supervisor is replaced and
        // a stranger is named, so two stacks can never half-exist.
        if let Err(error) = take_control_port(stack.control_port) {
            eprintln!("{error}");
            return std::process::ExitCode::FAILURE;
        }
        match control::serve(stack.control_port, Arc::clone(&state), actions.clone()) {
            Ok(bound) => println!(
                "the control channel is listening on 127.0.0.1:{bound} - `status`, `restart <service>`"
            ),
            Err(error) => {
                eprintln!("{error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    let mut children: Vec<(Service, Child)> = Vec::with_capacity(services.len());
    for service in &services {
        match start(service, lines.clone()) {
            Ok(child) => children.push((service.clone(), child)),
            Err(error) => {
                eprintln!("{error}");
                stop_all(&mut children);
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    let mut ready = vec![false; services.len()];
    let mut all_up = wait_ready(
        &services,
        &mut ready,
        &mut children,
        &received,
        &state,
        &mut tails,
        logs,
    );
    if all_up {
        let list: Vec<String> = services
            .iter()
            .map(|service| format!("{} on {}", service.name, service.port))
            .collect();
        println!(
            "the stack is up: {} - {}",
            list.join(", "),
            if check_only {
                "checked, stopping now"
            } else {
                "Ctrl+C stops it"
            }
        );
    }

    if check_only || !all_up {
        stop_all(&mut children);
        state_all(&state, &services, "stopped");
        for (name, line) in received.try_iter() {
            show(&mut tails, logs, &name, &line);
        }
        return if all_up {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        };
    }

    // Supervise: Ctrl+C stops everything; a service that dies stops the rest; a service that asks to
    // be restarted (exit code `2`, the reference's `Main.cpp:434-439`) is started again here, which is
    // the "restarter" that comment names. The control channel asks for the same verbs.
    let mut exit_code = 0u8;
    let mut restarts: BTreeMap<String, u32> = BTreeMap::new();
    let mut stopped: BTreeSet<String> = BTreeSet::new();
    loop {
        for (name, line) in received.try_iter() {
            show(&mut tails, logs, &name, &line);
        }
        for request in requests.try_iter() {
            let answer = match request.action {
                control::Action::Stop(name) => {
                    if let Some(index) = children
                        .iter()
                        .position(|(service, _)| service.name == name)
                    {
                        let (_, mut child) = children.remove(index);
                        let _ = child.kill();
                        let _ = child.wait();
                        stopped.insert(name.clone());
                        state_word(&state, &name, "stopped");
                        format!("stopped {name}")
                    } else if services.iter().any(|service| service.name == name) {
                        format!("{name} is not running")
                    } else {
                        format!("no such service: {name}")
                    }
                }
                control::Action::Restart(name) => {
                    let service = services
                        .iter()
                        .find(|service| service.name == name)
                        .cloned();
                    match service {
                        None => format!("no such service: {name}"),
                        Some(service) => {
                            if let Some(index) = children
                                .iter()
                                .position(|(running, _)| running.name == name)
                            {
                                let (_, mut child) = children.remove(index);
                                let _ = child.kill();
                                let _ = child.wait();
                            }
                            stopped.remove(&name);
                            match restart(
                                &service,
                                &mut children,
                                &lines,
                                &received,
                                &state,
                                &mut tails,
                                logs,
                            ) {
                                Ok(()) => format!("restarted {name}"),
                                Err(error) => error,
                            }
                        }
                    }
                }
                control::Action::StopAll | control::Action::Quit => {
                    state_all(&state, &services, "stopped");
                    let _ = request.reply.send("stopped every service".into());
                    all_up = false;
                    exit_code = 0;
                    break;
                }
            };
            let _ = request.reply.send(answer);
        }
        if !all_up {
            break;
        }
        let mut asked: Vec<String> = Vec::new();
        for (service, child) in children.iter_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                let code = status.code().unwrap_or(1);
                state_word(&state, &service.name, &format!("exited {code}"));
                if code == 2 {
                    let count = restarts.entry(service.name.clone()).or_insert(0);
                    *count += 1;
                    let request = *count;
                    if request > MAX_RESTARTS {
                        report(
                            &service.name,
                            &format!(
                                "asked to restart more than {MAX_RESTARTS} times; stopping the stack"
                            ),
                        );
                        all_up = false;
                        exit_code = 2;
                        // This is the stack's terminal decision. Do not inspect another child in
                        // this pass: it may already be reacting to the same console event, and its
                        // Windows termination status is not the reason the stack is stopping.
                        break;
                    } else {
                        report(
                            &service.name,
                            &format!(
                                "asked to restart (exit 2), request {request} of {MAX_RESTARTS}"
                            ),
                        );
                        asked.push(service.name.clone());
                    }
                } else {
                    report(
                        &service.name,
                        &format!("exited with {status}; stopping the rest"),
                    );
                    all_up = false;
                    exit_code = code as u8;
                    // Keep the initiating service's result. The remaining children are stopped below;
                    // their termination codes are consequences, not a new stack outcome.
                    break;
                }
            }
        }
        if !all_up || stop.load(Ordering::Relaxed) {
            break;
        }
        // A restart the loop noticed: the service that asked is started again and the stack keeps
        // running with a new process in it, never two (`Main.cpp:434-439` names this restarter).
        for name in asked {
            if let Some(index) = children
                .iter()
                .position(|(service, _)| service.name == name)
            {
                let (_, mut child) = children.remove(index);
                let _ = child.wait();
            }
            if let Some(service) = services
                .iter()
                .find(|service| service.name == name)
                .cloned()
                && let Err(error) = restart(
                    &service,
                    &mut children,
                    &lines,
                    &received,
                    &state,
                    &mut tails,
                    logs,
                )
            {
                report(&name, &error);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    stop_all(&mut children);
    for (name, line) in received.try_iter() {
        show(&mut tails, logs, &name, &line);
    }
    if exit_code == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(exit_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_stack(text: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kryta-stack-{}-{}.json",
            std::process::id(),
            text.len()
        ));
        std::fs::write(&path, text).expect("the stack file is written");
        path
    }

    #[test]
    fn the_list_is_read_as_rows_with_their_arguments() {
        let path = write_stack(
            "{\"control_port\": 7612, \"console_service\": \"world\", \"services\": [\
             {\"name\": \"auth\", \"program\": \"authserver\", \"args\": [\"-c\", \"authserver/authserver.json\"], \"port\": 6112},\
             {\"name\": \"world\", \"program\": \"worldserver\", \"args\": [\"-c\", \"worldserver/worldserver.json\"], \"port\": 6113}\
             ]}",
        );
        let stack = load(&path).expect("the list reads");
        let _ = std::fs::remove_file(&path);
        assert_eq!(stack.services.len(), 2);
        assert_eq!(stack.control_port, 7612);
        assert_eq!(stack.services[0].name, "auth");
        assert_eq!(
            stack.services[1].args,
            vec!["-c", "worldserver/worldserver.json"]
        );
        assert_eq!(stack.services[1].port, 6113);
        assert!(
            !stack.services[0].stdin,
            "only the named service inherits the terminal"
        );
        assert!(stack.services[1].stdin);
    }

    #[test]
    fn a_row_without_a_port_is_refused_with_where_it_is() {
        let path = write_stack("{\"services\": [{\"name\": \"a\", \"program\": \"p\"}]}");
        let error = load(&path).expect_err("refused");
        let _ = std::fs::remove_file(&path);
        assert!(error.contains("port"), "{error}");
    }

    #[test]
    fn a_stack_without_a_control_port_leaves_the_channel_off() {
        let path =
            write_stack("{\"services\": [{\"name\": \"a\", \"program\": \"p\", \"port\": 1}]}");
        let stack = load(&path).expect("the list reads");
        let _ = std::fs::remove_file(&path);
        assert_eq!(stack.control_port, 0);
        assert!(
            !stack.services[0].stdin,
            "no service is named the terminal's"
        );
    }

    #[test]
    fn the_client_verbs_are_the_ones_the_channel_takes() {
        let verbs = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            client_line(&args)
        };
        assert_eq!(verbs(&["status"]), Some("status".into()));
        assert_eq!(verbs(&["stop"]), Some("stop".into()));
        assert_eq!(verbs(&["stop", "world"]), Some("stop world".into()));
        assert_eq!(verbs(&["restart", "world"]), Some("restart world".into()));
        assert_eq!(verbs(&["quit"]), Some("quit".into()));
        assert_eq!(verbs(&[]), None);
        assert_eq!(
            verbs(&["--check"]),
            None,
            "--check starts a stack, it does not ask one"
        );
        assert_eq!(verbs(&["restart"]), None, "restart needs a service");
    }

    #[test]
    fn a_netstat_table_names_the_owner_of_a_listening_port() {
        let table = "Active Connections\n\
                     \n  Proto  Local Address          Foreign Address        State           PID\n\
                     \x20 TCP    127.0.0.1:6602         0.0.0.0:0              LISTENING       20004\n\
                     \x20 TCP    [::1]:6602             [::]:0                 LISTENING       20004\n\
                     \x20 TCP    127.0.0.1:6113         192.168.1.2:51800      ESTABLISHED     1200\n\
                     \x20 UDP    0.0.0.0:6602           *:*                                    3000\n";
        assert_eq!(holder_in(6602, table), Some(20004));
        assert_eq!(
            holder_in(6113, table),
            None,
            "only a LISTENING row owns the port"
        );
        assert_eq!(holder_in(6603, table), None);
    }

    #[test]
    fn a_tasklist_answer_names_the_image() {
        assert_eq!(
            image_in("\"kryta-management-service.exe\",\"20004\",\"Console\",\"1\",\"48,000 K\"\n"),
            Some("kryta-management-service.exe".to_string())
        );
        assert_eq!(
            image_in("INFO: No tasks are running which match the specified criteria.\n"),
            None
        );
        assert_eq!(image_in(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_owner_of_a_live_listener_is_the_process_that_bound_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
        let port = listener.local_addr().expect("its address").port();
        let mut found = None;
        for _ in 0..40 {
            if let Some(pid) = listener_holder(port) {
                found = Some(pid);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            found,
            Some(std::process::id()),
            "the socket's owner is this test"
        );
    }
}
