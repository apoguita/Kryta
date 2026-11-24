//! What every area shares: a route, the call a handler is given, the one JSON answer, the guard
//! that decides whether a request is answered at all, and the router that puts them together.
//!
//! The service answers data. A read is a `GET`, a change is a `POST` with a JSON body, and every
//! answer is JSON built here: `{"ok":true, ...}` or `{"ok":false,"error":"<plain sentence>"}` with a
//! fitting status. A handler is a function from the service's state and a [`Call`] to a [`Reply`];
//! it never builds a response itself, so no area can answer in a second shape.
//!
//! Nothing here knows what an account is, and nothing here needs a database: the router is generic
//! over its state, which is how the tests drive it with a stub.

use serde_json::{Map, Value, json};
use shared::network::http::{Request, Response};

/// The header every changing request must carry. A plain HTML form on another site cannot send a
/// custom header, and this service never answers a CORS preflight, so a page that is not the panel's
/// own cannot make a `POST` that passes (`docs/port/23-control-panel.md`, "Security").
pub const PANEL_HEADER: &str = "x-kryta-panel";

/// The address prefix every area's routes live under.
#[cfg(test)]
pub const API_PREFIX: &str = "/api/v1/";

/// Why a request was not answered with its data: a status and one plain sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The HTTP status.
    pub status: u16,
    /// The sentence the operator reads.
    pub error: String,
}

/// What a handler answers: the fields of a success, or a failure.
pub type Reply = Result<Value, Failure>;

/// A failure with this status.
pub fn fail(status: u16, error: impl Into<String>) -> Failure {
    Failure {
        status,
        error: error.into(),
    }
}

/// 400: the request itself is wrong.
pub fn bad(error: impl Into<String>) -> Failure {
    fail(400, error)
}

/// 404: nothing has that id.
pub fn missing(error: impl Into<String>) -> Failure {
    fail(404, error)
}

/// 409: the change collides with what exists.
pub fn conflict(error: impl Into<String>) -> Failure {
    fail(409, error)
}

/// 500: a repository refused.
pub fn broken(error: impl Into<String>) -> Failure {
    fail(500, error)
}

/// 503: the thing asked is not configured or not running.
pub fn unavailable(error: impl Into<String>) -> Failure {
    fail(503, error)
}

/// The one place an answer is built: a reply becomes the JSON response the browser reads.
///
/// A success is the handler's own object with `"ok": true` added; a value that is not an object is
/// carried as `"result"`.
pub fn answer(reply: Reply) -> Response {
    let (status, body) = match reply {
        Ok(Value::Object(mut fields)) => {
            fields.insert("ok".to_string(), Value::Bool(true));
            (200, Value::Object(fields))
        }
        Ok(other) => (200, json!({"ok": true, "result": other})),
        Err(failure) => (failure.status, json!({"ok": false, "error": failure.error})),
    };
    Response::new(status, "application/json; charset=utf-8", body.to_string())
}

/// One request, as a handler reads it: the path's captured segments, the JSON body and the query.
pub struct Call<'a> {
    /// The request itself.
    pub request: &'a Request,
    /// The `{name}` segments of the route's pattern, in order, URL-decoded.
    pub params: Vec<String>,
    /// The JSON body of a `POST`; `Null` when there was none.
    pub body: Value,
}

impl Call<'_> {
    /// One captured path segment.
    pub fn param(&self, index: usize) -> &str {
        self.params.get(index).map(String::as_str).unwrap_or("")
    }

    /// A captured segment as a row id; a word is a 404, because no row has that address.
    pub fn id(&self, index: usize) -> Result<i64, Failure> {
        self.param(index)
            .parse::<i64>()
            .map_err(|_| missing("No row has that id."))
    }

    /// A body field as trimmed text. A number is read as its digits, so a form may send either.
    pub fn text(&self, name: &str) -> String {
        match self.body.get(name) {
            Some(Value::String(text)) => text.trim().to_string(),
            Some(Value::Number(number)) => number.to_string(),
            Some(Value::Bool(flag)) => flag.to_string(),
            _ => String::new(),
        }
    }

    /// A body field as a whole number no larger than `max`.
    pub fn number(&self, name: &str, max: u32) -> Result<u32, Failure> {
        self.text(name)
            .parse::<u32>()
            .ok()
            .filter(|value| *value <= max)
            .ok_or_else(|| bad(format!("Invalid {name}.")))
    }

    /// A body field as a yes or no.
    pub fn flag(&self, name: &str) -> Option<bool> {
        match self.body.get(name) {
            Some(Value::Bool(flag)) => Some(*flag),
            Some(Value::String(text)) if text == "true" => Some(true),
            Some(Value::String(text)) if text == "false" => Some(false),
            _ => None,
        }
    }

    /// A query field, URL-decoded.
    pub fn query(&self, name: &str) -> Option<String> {
        self.request.query(name).map(unescape)
    }
}

/// A handler: the service's state and one call in, a reply out.
pub type Handler<S> = fn(&S, &Call) -> Reply;

/// One address an area answers.
pub struct Route<S> {
    /// `GET` reads, `POST` changes.
    pub method: &'static str,
    /// The path, with `{name}` for a captured segment: `/api/v1/accounts/{id}/active`.
    pub pattern: &'static str,
    /// What answers it.
    pub handler: Handler<S>,
}

/// A route that reads.
pub fn get<S>(pattern: &'static str, handler: Handler<S>) -> Route<S> {
    Route {
        method: "GET",
        pattern,
        handler,
    }
}

/// A route that changes something.
pub fn post<S>(pattern: &'static str, handler: Handler<S>) -> Route<S> {
    Route {
        method: "POST",
        pattern,
        handler,
    }
}

/// The non-empty segments of a path or a pattern.
fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

/// Whether a pattern segment captures.
fn captures(segment: &str) -> bool {
    segment.starts_with('{') && segment.ends_with('}')
}

/// The captured segments when `path` is an address `pattern` names, or `None`.
pub fn matches(pattern: &str, path: &str) -> Option<Vec<String>> {
    let wanted = segments(pattern);
    let given = segments(path);
    if wanted.len() != given.len() {
        return None;
    }
    let mut params = Vec::new();
    for (want, have) in wanted.iter().zip(&given) {
        if captures(want) {
            params.push(unescape(have));
        } else if want != have {
            return None;
        }
    }
    Some(params)
}

/// Whether two patterns could both answer one address: the same length, and no position where both
/// are fixed words that differ. The registration test refuses any such pair.
#[cfg(test)]
pub fn collide(left: &str, right: &str) -> bool {
    let (left, right) = (segments(left), segments(right));
    left.len() == right.len()
        && left
            .iter()
            .zip(&right)
            .all(|(a, b)| captures(a) || captures(b) || a == b)
}

/// What the route table says about one method and path.
pub enum Resolved<'a, S> {
    /// A route answers it, with these captured segments.
    Found(&'a Route<S>, Vec<String>),
    /// The address exists and answers another method.
    WrongMethod,
    /// No route names the address.
    NotFound,
}

/// Find the route for a method and a path.
pub fn resolve<'a, S>(routes: &'a [Route<S>], method: &str, path: &str) -> Resolved<'a, S> {
    let mut other_method = false;
    for route in routes {
        if let Some(params) = matches(route.pattern, path) {
            if route.method == method {
                return Resolved::Found(route, params);
            }
            other_method = true;
        }
    }
    if other_method {
        Resolved::WrongMethod
    } else {
        Resolved::NotFound
    }
}

/// Who may be answered: the token, when one is set, and the addresses this service is reached by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    /// The `Token` setting; empty requires none.
    pub token: String,
    /// The port the service listens on.
    pub port: u16,
    /// Whether the service is bound to this machine only. When it is, a request must name this
    /// machine in its `Host` header, which is what stops another site's name being pointed at
    /// 127.0.0.1 (DNS rebinding).
    pub loopback: bool,
}

impl Guard {
    /// Whether a bind address is this machine only.
    pub fn is_loopback(bind: &str) -> bool {
        bind.eq_ignore_ascii_case("localhost")
            || bind
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    }

    /// Refuse a request this service must not answer.
    ///
    /// In order: the `Host` header names this machine (a loopback bind only); a `POST` carries
    /// [`PANEL_HEADER`]; and an `/api/` request carries the token when one is set. The static files
    /// and `/health` need no token, so the page that asks for it can load.
    pub fn check(&self, request: &Request) -> Result<(), Failure> {
        if self.loopback && !self.host_is_ours(request.header("host")) {
            return Err(fail(
                403,
                "This panel answers only requests addressed to this machine.",
            ));
        }
        if request.method != "GET" && request.header(PANEL_HEADER) != Some("1") {
            return Err(fail(
                403,
                "A request that changes something must come from the panel's own pages.",
            ));
        }
        if request.path.starts_with("/api/") && !self.token_matches(request) {
            return Err(fail(401, "This panel wants its token."));
        }
        Ok(())
    }

    fn host_is_ours(&self, host: Option<&str>) -> bool {
        let Some(host) = host else {
            return false;
        };
        ["127.0.0.1", "localhost", "[::1]"]
            .iter()
            .any(|name| host.eq_ignore_ascii_case(&format!("{name}:{}", self.port)))
    }

    fn token_matches(&self, request: &Request) -> bool {
        if self.token.is_empty() {
            return true;
        }
        let Some(given) = request
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
        else {
            return false;
        };
        same_bytes(given.trim().as_bytes(), self.token.as_bytes())
    }
}

/// Compare two secrets without stopping at the first difference.
fn same_bytes(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// Answer one `/api/` request: guard it, find its route, read its body, call its handler.
pub fn serve<S>(state: &S, guard: &Guard, routes: &[Route<S>], request: &Request) -> Response {
    if let Err(failure) = guard.check(request) {
        return answer(Err(failure));
    }
    let (route, params) = match resolve(routes, &request.method, &request.path) {
        Resolved::Found(route, params) => (route, params),
        Resolved::WrongMethod => {
            return answer(Err(fail(405, "This address does not answer that method.")));
        }
        Resolved::NotFound => {
            return answer(Err(missing(format!(
                "Nothing answers at {}.",
                request.path
            ))));
        }
    };
    let body = match read_body(request) {
        Ok(body) => body,
        Err(failure) => return answer(Err(failure)),
    };
    let call = Call {
        request,
        params,
        body,
    };
    answer((route.handler)(state, &call))
}

/// The JSON object a `POST` carries. An empty body is an empty object: an action with no fields.
fn read_body(request: &Request) -> Result<Value, Failure> {
    if request.body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Object(Map::new()));
    }
    match serde_json::from_slice::<Value>(&request.body) {
        Ok(body @ Value::Object(_)) => Ok(body),
        _ => Err(bad("The request body must be a JSON object.")),
    }
}

/// Decode one URL-encoded value: `+` is a space, and `%XX` is a byte.
pub fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                index += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                index += 1;
            }
            (byte, _) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stub state: the router needs no database to be driven.
    struct Stub;

    fn list(_: &Stub, _: &Call) -> Reply {
        Ok(json!({"handler": "list"}))
    }
    fn one(_: &Stub, call: &Call) -> Reply {
        Ok(json!({"handler": "one", "id": call.id(0)?}))
    }
    fn create(_: &Stub, call: &Call) -> Reply {
        Ok(json!({"handler": "create", "name": call.text("name")}))
    }
    fn active(_: &Stub, call: &Call) -> Reply {
        Ok(json!({"handler": "active", "id": call.id(0)?, "active": call.flag("active")}))
    }
    fn refuse(_: &Stub, _: &Call) -> Reply {
        Err(conflict("Already there."))
    }

    fn routes() -> Vec<Route<Stub>> {
        vec![
            get("/api/v1/things", list),
            get("/api/v1/things/{id}", one),
            post("/api/v1/things", create),
            post("/api/v1/things/{id}/active", active),
            post("/api/v1/things/{id}/refuse", refuse),
        ]
    }

    fn guard(token: &str) -> Guard {
        Guard {
            token: token.to_string(),
            port: 6602,
            loopback: true,
        }
    }

    fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
        let mut request = Request::new(method, path);
        request.headers = headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        request.body = body.as_bytes().to_vec();
        request
    }

    const HOST: (&str, &str) = ("host", "127.0.0.1:6602");
    const PANEL: (&str, &str) = ("x-kryta-panel", "1");

    fn body(response: &Response) -> Value {
        serde_json::from_slice(&response.body).expect("every answer is JSON")
    }

    #[test]
    fn each_path_reaches_its_own_handler() {
        let cases = [
            ("GET", "/api/v1/things", "", "list"),
            ("GET", "/api/v1/things/7", "", "one"),
            ("POST", "/api/v1/things", "{\"name\":\" a \"}", "create"),
            ("POST", "/api/v1/things/7/active", "{\"active\":true}", "active"),
        ];
        for (method, path, text, handler) in cases {
            let response = serve(
                &Stub,
                &guard(""),
                &routes(),
                &request(method, path, &[HOST, PANEL], text),
            );
            assert_eq!(response.status, 200, "{method} {path}");
            assert_eq!(body(&response)["handler"], handler, "{method} {path}");
            assert_eq!(body(&response)["ok"], true);
        }
    }

    #[test]
    fn a_call_carries_its_segments_and_its_body() {
        let response = serve(
            &Stub,
            &guard(""),
            &routes(),
            &request("POST", "/api/v1/things/42/active", &[HOST, PANEL], "{\"active\":false}"),
        );
        assert_eq!(body(&response)["id"], 42);
        assert_eq!(body(&response)["active"], false);
        let response = serve(
            &Stub,
            &guard(""),
            &routes(),
            &request("POST", "/api/v1/things", &[HOST, PANEL], "{\"name\":\" a \"}"),
        );
        assert_eq!(body(&response)["name"], "a");
    }

    #[test]
    fn an_unknown_address_a_wrong_method_and_a_word_for_an_id_are_refused_in_json() {
        let ask = |method: &str, path: &str| {
            serve(&Stub, &guard(""), &routes(), &request(method, path, &[HOST, PANEL], ""))
        };
        assert_eq!(ask("GET", "/api/v1/nothing").status, 404);
        assert_eq!(ask("POST", "/api/v1/things/7").status, 405);
        let response = ask("GET", "/api/v1/things/seven");
        assert_eq!(response.status, 404);
        assert_eq!(body(&response)["ok"], false);
    }

    #[test]
    fn a_post_without_the_panel_header_is_403() {
        let response = serve(
            &Stub,
            &guard(""),
            &routes(),
            &request("POST", "/api/v1/things", &[HOST], "{\"name\":\"a\"}"),
        );
        assert_eq!(response.status, 403);
        assert_eq!(body(&response)["ok"], false);
        // A form on another site can set a content type, never this header.
        let response = serve(
            &Stub,
            &guard(""),
            &routes(),
            &request("POST", "/api/v1/things", &[HOST, ("x-kryta-panel", "0")], "{}"),
        );
        assert_eq!(response.status, 403);
    }

    #[test]
    fn a_host_that_is_not_this_machine_is_refused() {
        let ask = |host: &[(&str, &str)]| {
            serve(&Stub, &guard(""), &routes(), &request("GET", "/api/v1/things", host, "")).status
        };
        assert_eq!(ask(&[("host", "127.0.0.1:6602")]), 200);
        assert_eq!(ask(&[("host", "localhost:6602")]), 200);
        assert_eq!(ask(&[("host", "[::1]:6602")]), 200);
        assert_eq!(ask(&[("host", "evil.example:6602")]), 403);
        assert_eq!(ask(&[("host", "127.0.0.1:9999")]), 403);
        assert_eq!(ask(&[("host", "127.0.0.1")]), 403);
        assert_eq!(ask(&[]), 403);
    }

    #[test]
    fn a_panel_bound_beyond_this_machine_does_not_check_the_host() {
        let open = Guard {
            token: String::new(),
            port: 6602,
            loopback: false,
        };
        let response = serve(
            &Stub,
            &open,
            &routes(),
            &request("GET", "/api/v1/things", &[("host", "panel.lan:6602")], ""),
        );
        assert_eq!(response.status, 200);
        assert!(Guard::is_loopback("127.0.0.1"));
        assert!(Guard::is_loopback("::1"));
        assert!(Guard::is_loopback("localhost"));
        assert!(!Guard::is_loopback("0.0.0.0"));
        assert!(!Guard::is_loopback("192.168.1.4"));
    }

    #[test]
    fn a_token_is_required_when_set_and_not_when_unset() {
        let ask = |token: &str, headers: &[(&str, &str)]| {
            serve(&Stub, &guard(token), &routes(), &request("GET", "/api/v1/things", headers, ""))
                .status
        };
        assert_eq!(ask("", &[HOST]), 200);
        assert_eq!(ask("secret", &[HOST]), 401);
        assert_eq!(ask("secret", &[HOST, ("authorization", "Bearer wrong!")]), 401);
        assert_eq!(ask("secret", &[HOST, ("authorization", "Bearer secre")]), 401);
        assert_eq!(ask("secret", &[HOST, ("authorization", "secret")]), 401);
        assert_eq!(ask("secret", &[HOST, ("authorization", "Bearer secret")]), 200);
    }

    #[test]
    fn the_token_guards_the_api_and_not_the_static_files() {
        let guard = guard("secret");
        assert!(guard.check(&request("GET", "/", &[HOST], "")).is_ok());
        assert!(guard.check(&request("GET", "/ui/app.js", &[HOST], "")).is_ok());
        assert!(guard.check(&request("GET", "/health", &[HOST], "")).is_ok());
        assert!(guard.check(&request("GET", "/api/v1/things", &[HOST], "")).is_err());
    }

    #[test]
    fn the_answer_helper_builds_both_shapes() {
        let success = answer(Ok(json!({"count": 3})));
        assert_eq!(success.status, 200);
        assert_eq!(success.content_type, "application/json; charset=utf-8");
        assert_eq!(body(&success), json!({"ok": true, "count": 3}));

        let bare = answer(Ok(json!([1, 2])));
        assert_eq!(body(&bare), json!({"ok": true, "result": [1, 2]}));

        let failure = answer(Err(conflict("An account is already called x.")));
        assert_eq!(failure.status, 409);
        assert_eq!(
            body(&failure),
            json!({"ok": false, "error": "An account is already called x."})
        );
    }

    #[test]
    fn a_handlers_refusal_keeps_its_status_and_a_bad_body_is_400() {
        let response = serve(
            &Stub,
            &guard(""),
            &routes(),
            &request("POST", "/api/v1/things/1/refuse", &[HOST, PANEL], ""),
        );
        assert_eq!(response.status, 409);
        assert_eq!(body(&response)["error"], "Already there.");
        let response = serve(
            &Stub,
            &guard(""),
            &routes(),
            &request("POST", "/api/v1/things", &[HOST, PANEL], "name=a"),
        );
        assert_eq!(response.status, 400);
    }

    #[test]
    fn patterns_match_and_collide_by_shape() {
        assert_eq!(matches("/api/v1/a/{id}", "/api/v1/a/9"), Some(vec!["9".to_string()]));
        assert_eq!(matches("/api/v1/a/{id}", "/api/v1/a/x%20y"), Some(vec!["x y".to_string()]));
        assert_eq!(matches("/api/v1/a/{id}", "/api/v1/a"), None);
        assert_eq!(matches("/api/v1/a", "/api/v1/b"), None);
        assert!(collide("/api/v1/a/{id}", "/api/v1/a/create"));
        assert!(collide("/api/v1/a/{id}", "/api/v1/a/{uuid}"));
        assert!(!collide("/api/v1/a/{id}", "/api/v1/a/{id}/x"));
        assert!(!collide("/api/v1/a/x", "/api/v1/a/y"));
    }

    #[test]
    fn a_url_value_is_decoded() {
        assert_eq!(unescape("a%40b.c"), "a@b.c");
        assert_eq!(unescape("x+y"), "x y");
        assert_eq!(unescape("50%"), "50%");
        assert_eq!(unescape("client%2Fraw%2Fa.jsonl"), "client/raw/a.jsonl");
    }
}
