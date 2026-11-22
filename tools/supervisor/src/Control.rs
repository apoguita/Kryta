//! The supervisor's control channel: a loopback line protocol, one verb per line.
//!
//! Reference: `ASURA/crates/asura-supervisor/src/control.rs:3-4` for the verbs and `:113` for the
//! client side - `status`, `stop <service>`, `stop`, `restart <service>`, `quit`. The reference
//! (AzerothCore) has no supervisor at all, so this arrangement is taken from this tree's own lineage
//! (`docs/port/05-tooling.md`) and it is the one channel that owns process state: the world's console
//! asks the world to stop and exits with a code, and this channel turns that code into a process.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

/// What the supervisor calls each service: `up`, `exited 2`, `stopped`, `starting`.
pub type State = std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>;

/// How a control thread asks the supervisor's own loop to act. The loop owns the children, so the
/// control thread never touches a process: it asks, and the loop answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Stop one service and leave it stopped.
    Stop(String),
    /// Start one service again, replacing the process that ran it.
    Restart(String),
    /// Stop everything; the supervisor exits.
    StopAll,
    /// Answer, stop everything, and exit.
    Quit,
}

/// One request from a client, and where the answer goes.
pub struct Request {
    /// What to do.
    pub action: Action,
    /// The text the client is waiting for.
    pub reply: mpsc::Sender<String>,
}

/// How long a client waits for its answer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);

/// Bind the channel on loopback and answer requests on it. Answers the port it bound, which is the
/// port the caller asked for unless it asked for `0`.
pub fn serve(port: u16, state: State, send: mpsc::Sender<Request>) -> Result<u16, String> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
        .map_err(|error| format!("the control channel cannot bind 127.0.0.1:{port}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("the control channel cannot read its own address: {error}"))?
        .port();
    std::thread::Builder::new()
        .name("supervisor-control".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                handle(stream, &state, &send);
            }
        })
        .map_err(|error| format!("the control thread cannot start: {error}"))?;
    Ok(bound)
}

fn handle(mut stream: TcpStream, state: &State, send: &mpsc::Sender<Request>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let answer = answer(&line, state, send);
    let _ = writeln!(stream, "{answer}");
    let _ = stream.flush();
}

/// One line in, one answer out.
fn answer(line: &str, state: &State, send: &mpsc::Sender<Request>) -> String {
    let line = line.trim();
    let mut words = line.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some("status"), None, None) => match state.lock() {
            Ok(state) => table(&state),
            Err(_) => "err: the supervisor's state is unavailable".into(),
        },
        (Some("stop"), None, None) => ask(send, Action::StopAll, "stopping every service"),
        (Some("stop"), Some(name), None) => ask(send, Action::Stop(name.to_string()), "stopping"),
        (Some("restart"), Some(name), None) => {
            ask(send, Action::Restart(name.to_string()), "restarting")
        }
        (Some("quit"), None, None) => ask(send, Action::Quit, "the stack is stopping"),
        _ => "err: expected: status | stop [service] | restart service | quit".into(),
    }
}

/// Send an action to the supervisor's loop and wait for its answer.
fn ask(send: &mpsc::Sender<Request>, action: Action, doing: &str) -> String {
    let (reply, answer) = mpsc::channel();
    if send.send(Request { action, reply }).is_err() {
        return "err: the supervisor is not running".into();
    }
    match answer.recv_timeout(REPLY_TIMEOUT) {
        Ok(text) => text,
        Err(_) => format!("err: no answer from the supervisor; it may still be {doing}"),
    }
}

/// The state, one service per line.
fn table(state: &BTreeMap<String, String>) -> String {
    if state.is_empty() {
        return "no services".into();
    }
    state
        .iter()
        .map(|(name, state)| format!("{name}: {state}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The client: send one line to a running supervisor and answer everything it said.
///
/// The answer is read to the end of the connection, not one line of it: `status` answers one line per
/// service and only the first of them would be printed otherwise.
pub fn request(port: u16, line: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_secs(5),
    )
    .map_err(|error| format!("no supervisor is listening on 127.0.0.1:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(REPLY_TIMEOUT))
        .map_err(|error| error.to_string())?;
    writeln!(stream, "{}", line.trim()).map_err(|error| format!("the request failed: {error}"))?;
    stream.flush().map_err(|error| error.to_string())?;
    let mut answer = String::new();
    std::io::Read::read_to_string(&mut stream, &mut answer)
        .map_err(|error| format!("the answer could not be read: {error}"))?;
    let answer = answer.trim_end().to_string();
    if answer.starts_with("err:") {
        return Err(answer);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn state() -> State {
        let mut map = BTreeMap::new();
        map.insert("auth".to_string(), "up on 6112".to_string());
        map.insert("world".to_string(), "up on 6113".to_string());
        std::sync::Arc::new(std::sync::Mutex::new(map))
    }

    #[test]
    fn status_lists_every_service_and_needs_no_action() {
        let (send, requests) = mpsc::channel();
        let answer = answer("status", &state(), &send);
        assert_eq!(answer, "auth: up on 6112\nworld: up on 6113");
        assert!(
            requests.try_recv().is_err(),
            "status does not act on a process"
        );
    }

    #[test]
    fn the_verbs_are_the_ones_the_channel_documents() {
        for (line, expected) in [
            ("stop", Action::StopAll),
            ("stop world", Action::Stop("world".into())),
            ("restart world", Action::Restart("world".into())),
            ("quit", Action::Quit),
        ] {
            let (send, requests) = mpsc::channel();
            let state = state();
            let asked = std::thread::spawn(move || answer(line, &state, &send));
            let request = requests.recv().unwrap();
            assert_eq!(request.action, expected, "{line}");
            request.reply.send(format!("{line}: done")).unwrap();
            assert_eq!(asked.join().unwrap(), format!("{line}: done"));
        }
    }

    #[test]
    fn a_line_that_is_not_a_verb_is_refused_with_the_ones_that_are() {
        let (send, requests) = mpsc::channel();
        for line in ["", "nonsense", "stop a b", "restart", "status extra"] {
            let answer = answer(line, &state(), &send);
            assert!(answer.starts_with("err: expected:"), "{line}: {answer}");
            assert!(requests.try_recv().is_err(), "{line} must not act");
        }
    }

    #[test]
    fn a_client_and_the_supervisor_meet_over_the_channel() {
        let state = state();
        let (send, requests) = mpsc::channel();
        let port = serve(0, Arc::clone(&state), send).expect("the channel binds");

        // The supervisor's own loop, faked: it takes one action and answers it.
        let supervisor = std::thread::spawn(move || {
            let request = requests.recv().expect("an action arrives");
            assert_eq!(request.action, Action::Stop("world".into()));
            request
                .reply
                .send("stopped world".into())
                .expect("the answer is delivered");
        });

        assert_eq!(
            request(port, "stop world").expect("the action answers"),
            "stopped world"
        );
        supervisor.join().expect("the supervisor's loop finished");
        assert_eq!(
            request(port, "status").expect("status answers"),
            "auth: up on 6112\nworld: up on 6113",
            "one line per service, and the client reads all of them"
        );
        assert!(
            request(port, "nonsense").is_err(),
            "a line that is not a verb is an error"
        );
    }

    #[test]
    fn an_action_is_answered_by_the_supervisors_loop() {
        let (send, requests) = mpsc::channel();
        let state = state();
        let asked = std::thread::spawn(move || answer("restart world", &state, &send));
        let request = requests.recv().unwrap();
        request.reply.send("world restarted".into()).unwrap();
        assert_eq!(asked.join().unwrap(), "world restarted");
    }
}
