//! The portal: the four HTTP answers a client takes its identity from before it logs in.
//!
//! This is one of the directories this tree's naming adds, because the reference has no equivalent.
//! The **arrangement** is the reference's: a plain request-to-response function with no socket in
//! it, so the same code serves from a program or from a test. The **protocol** is this client's,
//! recorded in `ASURA/docs/reference/client-protocol.md` section 4 and reproduced here endpoint by
//! endpoint.
//!
//! The client is launched with `-portal 127.0.0.1` and speaks plain HTTP to `localhost:6601`, in
//! this order:
//!
//! | # | request | answer |
//! |---|---|---|
//! | 1 | `GET /Spawned/WebGate/session/create.xml` | `<Reply><Session>UUID</Session></Reply>` |
//! | 2 | `POST /Spawned/WebGate/users/login.xml` | the `UserId`/`LoginName`/`Provider` block |
//! | 3 | `POST /Spawned/WebGate/my_account/game_accounts.xml` | a one-row array for the game code |
//! | 4 | `POST /Spawned/WebGate/my_account/token.xml` | `<Reply><Token>UUID</Token></Reply>` |
//!
//! The `user_id` and `token` from calls 2 and 4 are the pair the client then carries in its login
//! message, which is why this is the step between "the client reached us" and "the client can log
//! in".
//!
//! What this does not do: decide who an account is. Opening a session and answering an account's
//! identity is [`Accounts`], supplied by the caller, because what an account *is* in this game is a
//! decision this module has no business making.

#[path = "accounts.rs"]
pub mod accounts;
#[path = "serve.rs"]
pub mod serve;

pub use accounts::FileAccounts;

use std::collections::HashMap;
use std::sync::Mutex;

/// One HTTP request, as much of it as the portal reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method, as written.
    pub method: String,
    /// The path, without a query string.
    pub path: String,
    /// The `Authorization` header's value, when the client sent one. It carries `Arena <session>`.
    pub authorization: Option<String>,
    /// The body, which the client sends as XML.
    pub body: Vec<u8>,
}

impl Request {
    /// A request with a method and a path.
    pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            authorization: None,
            body: Vec::new(),
        }
    }

    /// The session this request claims, from `Authorization: Arena <session>`.
    fn claimed_session(&self) -> Option<&str> {
        let value = self.authorization.as_deref()?;
        let session = value.strip_prefix("Arena ").map(str::trim)?;
        if session.is_empty() { None } else { Some(session) }
    }

    /// One field of an XML body, by tag.
    fn field(&self, tag: &str) -> Option<String> {
        let text = String::from_utf8_lossy(&self.body);
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = text.find(&open)? + open.len();
        let end = text[start..].find(&close)? + start;
        Some(text[start..end].to_string())
    }
}

/// One HTTP answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The content type.
    pub content_type: &'static str,
    /// The body.
    pub body: Vec<u8>,
}

impl Response {
    /// An XML answer.
    ///
    /// The web gate that served this client set `application/xml` on every answer.
    pub fn xml(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "application/xml",
            body: body.into().into_bytes(),
        }
    }

    /// A bare success, for the health check.
    pub fn ok() -> Self {
        Self {
            status: 200,
            content_type: "text/plain",
            body: b"ok".to_vec(),
        }
    }

    /// `400`, with the body the reference gives it.
    pub fn bad_request(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 400,
            content_type: "text/plain",
            body: body.into(),
        }
    }

    /// `401`, with no body: the client can act on the status.
    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            content_type: "text/plain",
            body: Vec::new(),
        }
    }

    /// `404`, with no body.
    pub fn not_found() -> Self {
        Self {
            status: 404,
            content_type: "text/plain",
            body: Vec::new(),
        }
    }

    /// The body as text, for a test or a log line.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// An account, as far as the portal is concerned: an identity to hand out and a token to carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The `user_id` the client will carry in its login message.
    pub user_id: String,
    /// The `token` it will carry beside it.
    pub token: String,
}

/// Who an account is.
///
/// The web gate the client was served by answered from rows, so the identity it hands out is the
/// very pair the login service resolves. What backs this in this tree is not decided yet: the login
/// database has no account table, because what an account is in this game is a decision.
pub trait Accounts {
    /// The account behind a login name, when there is one.
    fn resolve(&self, login_name: &str) -> Option<Account>;
}

/// The portal: the sessions it has opened, and the accounts it answers from.
pub struct Portal<A: Accounts> {
    accounts: A,
    /// Session id to the account attached to it, if any.
    sessions: Mutex<HashMap<String, Option<Account>>>,
}

impl<A: Accounts> Portal<A> {
    /// A portal with no session yet.
    pub fn new(accounts: A) -> Self {
        Self {
            accounts,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// How many sessions are open, which is what a status line reports.
    pub fn open_sessions(&self) -> usize {
        self.sessions
            .lock()
            .map(|sessions| sessions.len())
            .unwrap_or(0)
    }

    /// Answer one request.
    pub fn route(&self, request: &Request) -> Response {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/Spawned/WebGate/session/create.xml") => {
                let session = new_uuid();
                if let Ok(mut sessions) = self.sessions.lock() {
                    sessions.insert(session.clone(), None);
                }
                Response::xml(xml_session_created(&session))
            }

            ("POST", "/Spawned/WebGate/users/login.xml") => self.login(request),

            ("POST", "/Spawned/WebGate/my_account/game_accounts.xml") => {
                if !self.known_session(request) {
                    return Response::unauthorized();
                }
                let game_code = request.field("GameCode").unwrap_or_default();
                Response::xml(xml_game_accounts(&game_code))
            }

            ("POST", "/Spawned/WebGate/my_account/token.xml") => {
                let Some(session) = request.claimed_session() else {
                    return Response::unauthorized();
                };
                let token = self
                    .sessions
                    .lock()
                    .ok()
                    .and_then(|sessions| sessions.get(session).cloned().flatten())
                    .map(|account| account.token);
                match token {
                    Some(token) => Response::xml(xml_token(&token)),
                    // A token asked for before a login is refused rather than invented: the client
                    // can act on a 401, and an invented token would send it to a login that cannot
                    // resolve it.
                    None => Response::unauthorized(),
                }
            }

            // This project's own addition, and not one of the reference's routes: it says the gate
            // is up and nothing else, so an operator or a supervisor can ask without a session.
            ("GET", "/health") => Response::ok(),

            _ => Response::not_found(),
        }
    }

    fn known_session(&self, request: &Request) -> bool {
        let Some(session) = request.claimed_session() else {
            return false;
        };
        self.sessions
            .lock()
            .map(|sessions| sessions.contains_key(session))
            .unwrap_or(false)
    }

    /// The login: check the session, check the provider, resolve the login name, attach the account.
    ///
    /// The order is the reference's, and each answer is the one it gives: a missing or unknown
    /// session is `401`; a provider that is not `Portal` is `400 Unsupported provider`; a login name
    /// the accounts do not know is `400 who are you?`.
    fn login(&self, request: &Request) -> Response {
        let Some(session) = request.claimed_session() else {
            return Response::unauthorized();
        };
        // Checked before the accounts are asked, so a bad session never reaches a lookup.
        if !self.known_session(request) {
            return Response::unauthorized();
        }

        let provider = request.field("Provider").unwrap_or_default();
        if provider != "Portal" {
            return Response::bad_request(b"Unsupported provider".to_vec());
        }

        let login_name = request.field("LoginName").unwrap_or_default();
        if login_name.is_empty() {
            return Response::bad_request(b"who are you?".to_vec());
        }

        let Some(account) = self.accounts.resolve(&login_name) else {
            return Response::bad_request(b"who are you?".to_vec());
        };

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(session.to_string(), Some(account.clone()));
        }

        Response::xml(xml_login(&account.user_id, &login_name))
    }
}

/// A fresh identifier, upper case, in the shape the client is handed.
///
/// The gate that served this client emits an upper-case UUID; this builds one from the operating
/// system's entropy with RFC 4122's version and variant bits, so the shape is the same and the value
/// is unpredictable.
pub fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::getrandom(&mut bytes).is_err() {
        // Without entropy there is no identifier to hand out; a fixed one is a session anyone could
        // guess, but it is at least shaped like an identifier rather than being silently empty.
        bytes = [0x5Au8; 16];
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

fn xml_session_created(session: &str) -> String {
    format!("<Reply>\n<Session>{session}</Session>\n</Reply>\n")
}

fn xml_login(user_id: &str, login_name: &str) -> String {
    format!(
        "<Reply>\n\
<UserId>{user_id}</UserId>\n\
<UserCenter>1</UserCenter>\n\
<UserName>:John.10</UserName>\n\
<Parts/>\n\
<ResumeToken>AAAAAAAA-BBBB-CCCC-DDDD-FFFFFFFFFFFF</ResumeToken>\n\
<LoginName>{login_name}</LoginName>\n\
<Provider>Portal</Provider>\n\
<EmailVerified>1</EmailVerified>\n\
</Reply>\n"
    )
}

fn xml_game_accounts(game_code: &str) -> String {
    format!(
        "<Reply type=\"array\">\n\
<Row>\n\
<GameCode>{game_code}</GameCode>\n\
<Alias>gw1</Alias>\n\
<Created>2005-01-01T12:00:00Z</Created>\n\
</Row>\n\
</Reply>\n"
    )
}

fn xml_token(token: &str) -> String {
    format!("<Reply>\n<Token>{token}</Token>\n</Reply>\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneAccount;

    impl Accounts for OneAccount {
        fn resolve(&self, login_name: &str) -> Option<Account> {
            (login_name == "player@example.com").then(|| Account {
                user_id: "0F5A2C1E-1111-2222-3333-444455556666".to_string(),
                token: "AAAAAAAA-0000-1111-2222-333344445555".to_string(),
            })
        }
    }

    fn portal() -> Portal<OneAccount> {
        Portal::new(OneAccount)
    }

    /// Take a session id out of the create answer.
    fn open_session(portal: &Portal<OneAccount>) -> String {
        let answer = portal.route(&Request::new("GET", "/Spawned/WebGate/session/create.xml"));
        assert_eq!(answer.status, 200);
        let text = answer.text();
        let start = text.find("<Session>").unwrap() + "<Session>".len();
        let end = text.find("</Session>").unwrap();
        text[start..end].to_string()
    }

    fn login_request(session: &str, provider: &str, name: &str) -> Request {
        let mut request = Request::new("POST", "/Spawned/WebGate/users/login.xml");
        request.authorization = Some(format!("Arena {session}"));
        request.body =
            format!("<Login><Provider>{provider}</Provider><LoginName>{name}</LoginName></Login>")
                .into_bytes();
        request
    }

    #[test]
    fn a_session_is_minted_in_the_reply_shape() {
        let portal = portal();
        let answer = portal.route(&Request::new("GET", "/Spawned/WebGate/session/create.xml"));
        assert_eq!(answer.content_type, "application/xml");
        let text = answer.text();
        assert!(text.starts_with("<Reply>\n<Session>"), "{text}");
        assert!(text.ends_with("</Session>\n</Reply>\n"), "{text}");
        assert_eq!(portal.open_sessions(), 1);
    }

    #[test]
    fn the_login_answers_the_identity_block() {
        let portal = portal();
        let session = open_session(&portal);
        let answer = portal.route(&login_request(&session, "Portal", "player@example.com"));

        assert_eq!(answer.status, 200);
        let text = answer.text();
        assert!(
            text.contains("<UserId>0F5A2C1E-1111-2222-3333-444455556666</UserId>"),
            "{text}"
        );
        assert!(
            text.contains("<LoginName>player@example.com</LoginName>"),
            "{text}"
        );
        assert!(text.contains("<Provider>Portal</Provider>"), "{text}");
    }

    #[test]
    fn a_login_without_a_session_is_refused() {
        let portal = portal();
        let mut request = login_request("whatever", "Portal", "player@example.com");
        request.authorization = None;
        assert_eq!(portal.route(&request).status, 401);
    }

    #[test]
    fn a_login_with_an_unknown_session_is_refused() {
        let portal = portal();
        let request = login_request("NOT-A-SESSION", "Portal", "player@example.com");
        assert_eq!(portal.route(&request).status, 401);
    }

    #[test]
    fn a_provider_that_is_not_the_portal_is_refused_with_the_reference_sentence() {
        let portal = portal();
        let session = open_session(&portal);
        let answer = portal.route(&login_request(&session, "ArenaNet", "player@example.com"));
        assert_eq!(answer.status, 400);
        assert_eq!(answer.text(), "Unsupported provider");
    }

    #[test]
    fn an_unknown_login_name_is_refused_with_the_reference_sentence() {
        let portal = portal();
        let session = open_session(&portal);
        let answer = portal.route(&login_request(&session, "Portal", "nobody@example.com"));
        assert_eq!(answer.status, 400);
        assert_eq!(answer.text(), "who are you?");
    }

    #[test]
    fn a_token_before_a_login_is_refused_rather_than_invented() {
        let portal = portal();
        let session = open_session(&portal);
        let mut request = Request::new("POST", "/Spawned/WebGate/my_account/token.xml");
        request.authorization = Some(format!("Arena {session}"));
        assert_eq!(portal.route(&request).status, 401);
    }

    #[test]
    fn a_token_after_a_login_is_the_accounts() {
        let portal = portal();
        let session = open_session(&portal);
        portal.route(&login_request(&session, "Portal", "player@example.com"));

        let mut request = Request::new("POST", "/Spawned/WebGate/my_account/token.xml");
        request.authorization = Some(format!("Arena {session}"));
        let answer = portal.route(&request);

        assert_eq!(answer.status, 200);
        assert!(
            answer
                .text()
                .contains("<Token>AAAAAAAA-0000-1111-2222-333344445555</Token>"),
            "{}",
            answer.text()
        );
    }

    #[test]
    fn the_game_accounts_answer_is_a_one_row_array_for_the_game_code() {
        let portal = portal();
        let session = open_session(&portal);
        let mut request = Request::new("POST", "/Spawned/WebGate/my_account/game_accounts.xml");
        request.authorization = Some(format!("Arena {session}"));
        request.body = b"<GameCode>gw1</GameCode>".to_vec();

        let answer = portal.route(&request);
        let text = answer.text();
        assert!(text.contains("<GameCode>gw1</GameCode>"), "{text}");
        assert!(text.contains("<Alias>gw1</Alias>"), "{text}");
    }

    #[test]
    fn an_unknown_path_is_a_404_and_health_is_a_200() {
        let portal = portal();
        assert_eq!(portal.route(&Request::new("GET", "/nothing")).status, 404);
        assert_eq!(portal.route(&Request::new("GET", "/health")).status, 200);
    }

    #[test]
    fn an_identifier_is_upper_case_and_uuid_shaped() {
        let id = new_uuid();
        assert_eq!(id.len(), 36, "{id}");
        assert!(
            id.chars()
                .all(|character| character.is_ascii_uppercase()
                    || character.is_ascii_digit()
                    || character == '-'),
            "{id}"
        );
        assert_eq!(&id[14..15], "4", "the version nibble: {id}");
    }
}
