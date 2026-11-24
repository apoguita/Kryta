//! HTTP/1.1, as much of it as this tree needs: a request line, headers, a body of `Content-Length`
//! bytes, and a response that closes the connection.
//!
//! Two surfaces speak it - the client-facing portal in `game/Portal/` and the administration tool in
//! `tools/manage/` - so the reader and the writer live here and each of them keeps only its routes.
//! That is the same rule as the protocol tables: the mechanism is shared, the vocabulary belongs to
//! the surface that speaks it.
//!
//! What it deliberately does not do: chunked bodies, keep-alive, pipelining. The client does not use
//! them, and guessing at the rest of HTTP would be inventing behaviour nobody has seen.
//!
//! Ported from `game/Portal/serve.rs`, where it was written against this client's own requests.

use crate::network::socket::SessionIo;

/// How much of a request this will read before giving up.
///
/// A login body and its headers are far smaller; the limit exists so a connection that sends nothing
/// but bytes cannot grow the process.
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// A request, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method, as written.
    pub method: String,
    /// The path, without a query string.
    pub path: String,
    /// The query string, when there was one, without the `?`.
    pub query: Option<String>,
    /// Every header, lowercased by name.
    pub headers: Vec<(String, String)>,
    /// The body, which may be empty.
    pub body: Vec<u8>,
}

impl Request {
    /// An empty request with this method and path.
    pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            query: None,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// One header's value, by name, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// A field of the query string, by name.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query.as_deref()?.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then_some(value)
        })
    }

    /// The body, as text, lossily decoded the way a form is.
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// A response, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The content type.
    pub content_type: &'static str,
    /// The body.
    pub body: Vec<u8>,
    /// Where a redirect sends the client, when it sends one. A 303 is an answer a client acts on:
    /// without `Location` it is a document that says "moved" and moves nothing.
    pub location: Option<String>,
}

impl Response {
    /// A response with a body.
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type,
            body: body.into(),
            location: None,
        }
    }

    /// A `text/html` page.
    pub fn html(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::new(status, "text/html; charset=utf-8", body)
    }

    /// A `text/plain` body.
    pub fn text(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::new(status, "text/plain; charset=utf-8", body)
    }

    /// A `303 See Other` that names where the client goes next.
    pub fn redirect(to: impl Into<String>) -> Self {
        let to = to.into();
        let body = format!("<p>moved to <a href=\"{to}\">{to}</a></p>");
        Self::html(303, body).with_location(to)
    }

    /// The same response, sent with a `Location` header.
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}

/// Why a request could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// The connection ended before the head was complete.
    Incomplete,
    /// The request is larger than [`MAX_REQUEST_BYTES`].
    TooLarge,
    /// The bytes are not an HTTP request this reader understands.
    Malformed(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete => write!(f, "the request ended before its head was complete"),
            Self::TooLarge => write!(f, "the request is larger than {MAX_REQUEST_BYTES} bytes"),
            Self::Malformed(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for HttpError {}

/// Read one request off `io`: its head, then the body its `Content-Length` names.
pub async fn read_request(io: &mut SessionIo) -> Result<Request, HttpError> {
    let (head, rest) = read_head(io).await?;
    let (mut request, body_length) = parse_head(&head).ok_or_else(|| {
        HttpError::Malformed("the request line or a header is not HTTP/1.1".to_string())
    })?;
    request.body = read_body(io, body_length, rest).await?;
    Ok(request)
}

/// Write one response and close nothing: the caller owns the connection.
pub async fn write_response(io: &mut SessionIo, response: &Response) -> std::io::Result<()> {
    let location = match &response.location {
        Some(location) => format!("Location: {location}\r\n"),
        None => String::new(),
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );

    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(&response.body);
    match io.write_all(&bytes).await {
        // The answer is SessionIo::write_all's own, handed back unchanged: nothing is retried
        // here.
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}

/// The reason phrase for a status this tree uses.
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Read until the blank line that ends a request's head, and answer anything that arrived after it.
async fn read_head(io: &mut SessionIo) -> Result<(String, Vec<u8>), HttpError> {
    let mut buffer = Vec::new();
    let mut window = [0u8; 512];

    loop {
        if buffer.len() > MAX_REQUEST_BYTES {
            return Err(HttpError::TooLarge);
        }
        if let Some((head_end, body_start)) = find_head_end(&buffer) {
            let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
            // The body starts *after* the blank line, not at it: a byte of the terminator left in
            // would prefix every form's first field and make it unreadable.
            let rest = buffer[body_start..].to_vec();
            return Ok((head, rest));
        }
        match io.read(&mut window).await {
            Ok(0) => return Err(HttpError::Incomplete),
            Ok(count) => buffer.extend_from_slice(&window[..count]),
            Err(error) => return Err(HttpError::Malformed(error.to_string())),
        }
    }
}

/// Where a request's head ends and its body starts: (head length, body offset).
///
/// The head's text stops at the blank line; the body begins after the four bytes of \r\n\r\n (or the
/// two of \n\n). Keeping those bytes out of the body is what makes a form's first field readable.
fn find_head_end(buffer: &[u8]) -> Option<(usize, usize)> {
    if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
        return Some((end, end + 4));
    }
    buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|end| (end, end + 2))
}

/// Read the body: what already arrived after the head, then the rest.
async fn read_body(
    io: &mut SessionIo,
    length: usize,
    already_read: Vec<u8>,
) -> Result<Vec<u8>, HttpError> {
    if length > MAX_REQUEST_BYTES {
        return Err(HttpError::TooLarge);
    }

    // What arrived after the head is the start of the body; the head's terminator was already taken
    // off by the reader.
    let mut body: Vec<u8> = already_read;

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

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };

    let mut request = Request::new(method, path);
    request.query = query;
    let mut body_length = 0usize;

    for line in lines {
        let (name, value) = line.split_once(':')?;
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "content-length" {
            body_length = value.parse().ok()?;
        }
        request.headers.push((name, value.to_string()));
    }

    Some((request, body_length))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_line_and_its_headers_are_read() {
        let (request, length) = parse_head(
            "GET /accounts?x=1 HTTP/1.1\r\nHost: localhost\r\nAuthorization: Arena abc\r\n\r\n",
        )
        .expect("parses");
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/accounts");
        assert_eq!(request.query, Some("x=1".to_string()));
        assert_eq!(request.header("authorization"), Some("Arena abc"));
        assert_eq!(request.query("x"), Some("1"));
        assert_eq!(length, 0);
    }

    #[test]
    fn a_body_length_is_read_from_its_header() {
        let (_, length) =
            parse_head("POST /accounts HTTP/1.1\r\nContent-Length: 7\r\n\r\n").expect("parses");
        assert_eq!(length, 7);
    }

    #[test]
    fn something_that_is_not_http_is_refused() {
        assert!(parse_head("hello\r\n\r\n").is_none());
        // A request line needs a method, a target and a version that starts with `HTTP/`; a lone
        // word has none of the last two.
    }

    #[test]
    fn a_head_ends_at_the_blank_line() {
        // The head is 14 bytes and the body starts right after its four-byte terminator.
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\nbody"), Some((14, 18)));
        assert_eq!(find_head_end(b"GET / HTTP/1.1\n\nbody"), Some((14, 16)));
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n"), None);
    }

    #[test]
    fn a_status_has_a_reason() {
        assert_eq!(reason(200), "OK");
        assert_eq!(reason(404), "Not Found");
    }

    #[test]
    fn a_redirect_names_where_the_client_goes() {
        let response = Response::redirect("/players/2");
        assert_eq!(response.status, 303);
        assert_eq!(response.location.as_deref(), Some("/players/2"));
        assert!(String::from_utf8_lossy(&response.body).contains("href=\"/players/2\""));
        assert!(Response::text(200, "ok").location.is_none());
    }
}
