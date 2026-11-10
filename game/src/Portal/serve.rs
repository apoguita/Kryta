//! Serving the portal over HTTP, on a connection the socket layer hands over.
//!
//! The protocol is in [`super`]; this is the part that reads a request off a socket and writes a
//! response back. Kept here rather than in a program so the same code answers a test, and so moving
//! the portal to another process later is a listener and a dependency, not a rewrite.
//!
//! What it reads: a request line, headers, and a body of `Content-Length` bytes. What it writes: a
//! status line, `Content-Type`, `Content-Length`, and `Connection: close`. That is the whole of HTTP
//! this client needs, and guessing at more (chunked bodies, keep-alive, pipelining) would be inventing
//! behaviour nobody has seen it use.

use std::sync::Arc;

use common::logging::{Log, LogLevel};
use shared::network::socket::{Session, SessionFuture, SessionIo};

use super::{Accounts, Portal, Request, Response};

/// How much of a request this will read before giving up.
///
/// A login body and its headers are far smaller; the limit exists so a connection that sends nothing
/// but bytes cannot grow the process.
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// One connection to the portal.
pub struct PortalSession<A: Accounts + Send + Sync + 'static> {
    portal: Arc<Portal<A>>,
    log: Option<Log>,
}

impl<A: Accounts + Send + Sync + 'static> PortalSession<A> {
    /// A session answering from this portal.
    pub fn new(portal: Arc<Portal<A>>, log: Option<Log>) -> Self {
        Self { portal, log }
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        if let Some(log) = self.log.as_ref() {
            log.out(level, "portal", text);
        }
    }
}

/// Why a request could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// The peer closed before sending a complete request.
    Incomplete,
    /// A request line or a header was not shaped like one.
    Malformed(String),
    /// The request was larger than this will read.
    TooLarge,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete => f.write_str("the peer closed inside a request"),
            Self::Malformed(reason) => write!(f, "malformed request: {reason}"),
            Self::TooLarge => write!(f, "the request is larger than {MAX_REQUEST_BYTES} bytes"),
        }
    }
}

impl<A: Accounts + Send + Sync + 'static> Session for PortalSession<A> {
    fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
        Box::pin(async move {
            let peer = io
                .peer()
                .map(|address| address.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let (head, already_read) = match read_head(&mut io).await {
                Ok(head) => head,
                Err(error) => {
                    self.say(LogLevel::Warn, format!("{peer}: {error}"));
                    let _ = io.close().await;
                    return;
                }
            };

            let Some((request, body_length)) = parse_head(&head) else {
                self.say(
                    LogLevel::Warn,
                    format!("{peer}: a request this server cannot read: {head:?}"),
                );
                let _ = write_response(&mut io, &Response::bad_request("malformed request")).await;
                let _ = io.close().await;
                return;
            };

            let mut request = request;
            if body_length > 0 {
                match read_body(&mut io, body_length, already_read).await {
                    Ok(body) => request.body = body,
                    Err(error) => {
                        self.say(LogLevel::Warn, format!("{peer}: {error}"));
                        let _ = io.close().await;
                        return;
                    }
                }
            }

            let answer = self.portal.route(&request);
            self.say(
                LogLevel::Info,
                format!(
                    "{peer}: {} {} -> {} ({} bytes)",
                    request.method,
                    request.path,
                    answer.status,
                    answer.body.len()
                ),
            );

            let _ = write_response(&mut io, &answer).await;
            let _ = io.close().await;
        })
    }
}

/// Read a request's head, and answer anything that arrived after it.
///
/// The bytes after the blank line are the body, or the start of it: a read that stops at the head and
/// throws the rest away loses the body of every POST, which is every request that carries anything.
async fn read_head(io: &mut SessionIo) -> Result<(String, Vec<u8>), HttpError> {
    let mut buffer = Vec::new();
    let mut window = [0u8; 512];

    loop {
        if buffer.len() > MAX_REQUEST_BYTES {
            return Err(HttpError::TooLarge);
        }
        if let Some(end) = find_head_end(&buffer) {
            let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
            let rest = buffer[end..].to_vec();
            return Ok((head, rest));
        }
        match io.read(&mut window).await {
            Ok(0) => return Err(HttpError::Incomplete),
            Ok(count) => buffer.extend_from_slice(&window[..count]),
            Err(error) => return Err(HttpError::Malformed(error.to_string())),
        }
    }
}

/// Where a request's head ends: the index of the blank line that terminates it.
///
/// A head ends with `\r\n\r\n`, so the head's own text stops at the first `\r` of that pair and the
/// body starts four bytes later. A bare `\n\n` is accepted as well, because a test or a hand-written
/// request may use it.
fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .or_else(|| buffer.windows(2).position(|window| window == b"\n\n"))
}

/// Read the body: what already arrived after the head, then the rest.
async fn read_body(
    io: &mut SessionIo,
    length: usize,
    mut already_read: Vec<u8>,
) -> Result<Vec<u8>, HttpError> {
    if length > MAX_REQUEST_BYTES {
        return Err(HttpError::TooLarge);
    }

    // The head's read may have taken the blank line's terminator with it, so the body starts at the
    // first byte after that.
    let skip = if already_read.starts_with(b"\r\n") {
        2
    } else if already_read.starts_with(b"\n") {
        1
    } else {
        0
    };
    let mut body: Vec<u8> = already_read.split_off(skip.min(already_read.len()));

    while body.len() < length {
        let mut chunk = vec![0u8; length - body.len()];
        match io.read(&mut chunk).await {
            Ok(0) => return Err(HttpError::Incomplete),
            Ok(count) => body.extend_from_slice(&chunk[..count]),
            Err(error) => return Err(HttpError::Malformed(error.to_string())),
        }
    }
    body.truncate(length);
    Ok(body)
}

/// Turn a request head into a [`Request`] and the body length it claims.
fn parse_head(head: &str) -> Option<(Request, usize)> {
    let mut lines = head.lines().map(str::trim).filter(|line| !line.is_empty());

    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?;
    let expected_version = parts.next()?;
    if !expected_version.starts_with("HTTP/") {
        return None;
    }

    // A target may carry a query string; the portal's routes do not, and the path is what is matched.
    let path = target
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(target)
        .to_string();

    let mut request = Request::new(method, path);
    let mut body_length = 0usize;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return None;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "authorization" => request.authorization = Some(value.to_string()),
            "content-length" => body_length = value.parse().ok()?,
            _ => {}
        }
    }

    Some((request, body_length))
}

/// Write one response.
async fn write_response(io: &mut SessionIo, response: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );

    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(&response.body);
    io.write_all(&bytes).await
}

/// The reason phrase for the statuses this server answers with.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_head_is_read_into_its_parts() {
        let head = "POST /Spawned/WebGate/users/login.xml HTTP/1.1\r\nHost: localhost\r\nAuthorization: Arena ABC\r\nContent-Length: 5";
        let (request, length) = parse_head(head).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/Spawned/WebGate/users/login.xml");
        assert_eq!(request.authorization.as_deref(), Some("Arena ABC"));
        assert_eq!(length, 5);
    }

    #[test]
    fn a_query_string_is_not_part_of_the_path() {
        let head = "GET /health?x=1 HTTP/1.1";
        let (request, length) = parse_head(head).unwrap();
        assert_eq!(request.path, "/health");
        assert_eq!(length, 0);
    }

    #[test]
    fn a_head_that_is_not_http_is_refused() {
        assert!(parse_head("HELLO").is_none());
        assert!(parse_head("GET / SPDY/3").is_none());
        assert!(parse_head("GET / HTTP/1.1\r\nbroken header line").is_none());
    }

    #[test]
    fn the_head_ends_at_the_blank_line() {
        // "GET / HTTP/1.1" is 14 bytes, so the head's text is the 14 bytes and the blank line starts
        // at 14; the body starts four bytes after it.
        let buffer = b"GET / HTTP/1.1\r\n\r\nrest";
        assert_eq!(find_head_end(buffer), Some(14));
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n"), None);
    }

    #[test]
    fn a_status_has_a_reason_phrase() {
        assert_eq!(reason(200), "OK");
        assert_eq!(reason(401), "Unauthorized");
    }
    /// The whole portal over a real socket, driven by real HTTP bytes.
    #[test]
    fn a_client_walks_the_four_endpoints_over_http() {
        use crate::portal::{Account, Accounts};
        use shared::network::socket::{Session, SessionFactory};
        use shared::network::socket_mgr::SocketMgr;
        use std::io::{Read, Write};
        use std::time::Duration;

        struct One;
        impl Accounts for One {
            fn resolve(&self, login_name: &str) -> Option<Account> {
                (login_name == "player@example.com").then(|| Account {
                    user_id: "AAAA1111-2222-3333-4444-555566667777".to_string(),
                    token: "TOKEN-0000-1111-2222-333344445555".to_string(),
                })
            }
        }

        let portal = Arc::new(Portal::new(One));
        let factory: SessionFactory = {
            let portal = Arc::clone(&portal);
            Arc::new(move || Box::new(PortalSession::new(Arc::clone(&portal), None)) as Box<dyn Session>)
        };

        let mut manager = SocketMgr::new(factory);
        let port = manager.start_network("127.0.0.1", 0, 1).unwrap();

        let ask = |request: &str| -> String {
            let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            client.write_all(request.as_bytes()).unwrap();
            let mut answer = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match client.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(count) => answer.extend_from_slice(&chunk[..count]),
                    Err(_) => break,
                }
            }
            String::from_utf8_lossy(&answer).into_owned()
        };

        // 1. The session.
        let created = ask("GET /Spawned/WebGate/session/create.xml HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(created.starts_with("HTTP/1.1 200 OK"), "{created}");
        let start = created.find("<Session>").unwrap() + "<Session>".len();
        let end = created.find("</Session>").unwrap();
        let session = created[start..end].to_string();

        // 2. The login.
        let body = "<Login><Provider>Portal</Provider><LoginName>player@example.com</LoginName></Login>";
        let logged_in = ask(&format!(
            "POST /Spawned/WebGate/users/login.xml HTTP/1.1\r\nHost: localhost\r\nAuthorization: Arena {session}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        assert!(logged_in.starts_with("HTTP/1.1 200 OK"), "{logged_in}");
        assert!(logged_in.contains("AAAA1111-2222-3333-4444-555566667777"), "{logged_in}");

        // 3. The token.
        let token = ask(&format!(
            "POST /Spawned/WebGate/my_account/token.xml HTTP/1.1\r\nHost: localhost\r\nAuthorization: Arena {session}\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(token.contains("<Token>TOKEN-0000-1111-2222-333344445555</Token>"), "{token}");

        // 4. An unknown session, which is the refusal a client can act on.
        let refused = ask("POST /Spawned/WebGate/my_account/token.xml HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");

        manager.stop_network();
    }
}
