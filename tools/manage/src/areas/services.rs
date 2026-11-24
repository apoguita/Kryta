//! Services: the running stack, through the supervisor's lifecycle channel.
//!
//! The panel asks; the supervisor remains the only owner of its processes. The verbs are the
//! supervisor's own - `status`, `restart <service>`, `stop <service>`, `stop` - and it has no start
//! verb, so a stopped service is started again with `restart`.

use serde_json::{Value, json};

use crate::api::{Call, Failure, Reply, Route, bad, get, post, unavailable};
use crate::state::Manage;

/// The services area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/services", status),
        post("/api/v1/services/stop-all", stop_all),
        post("/api/v1/services/{name}/restart", restart),
        post("/api/v1/services/{name}/stop", stop),
    ]
}

/// A stack service is an identifier, not a free-form supervisor command. Keeping that boundary here
/// prevents a request value from becoming a second command language.
pub fn service_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

/// The supervisor's `status` answer, one service per `name: state` line, as rows. A line that is
/// not a service line is left out: the answer is the supervisor's data, not a list kept here.
pub fn parse_status(answer: &str) -> Vec<Value> {
    answer
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, state)| (name.trim(), state.trim()))
        .filter(|(name, _)| service_name(name))
        .map(|(name, state)| {
            let port = state
                .strip_prefix("up on ")
                .and_then(|port| port.parse::<u16>().ok());
            json!({ "name": name, "state": state, "up": state.starts_with("up"), "port": port })
        })
        .collect()
}

/// `GET /api/v1/services`: what the supervisor says is running. A supervisor that does not answer
/// is a state the page shows, not a failed request.
fn status(manage: &Manage, _: &Call) -> Reply {
    Ok(match manage.supervisor_request("status") {
        Ok(answer) => json!({ "available": true, "services": parse_status(&answer), "raw": answer }),
        Err(error) => json!({ "available": false, "services": [], "error": error }),
    })
}

fn ask(manage: &Manage, line: &str) -> Reply {
    let answer = manage.supervisor_request(line).map_err(unavailable)?;
    manage.say(format!("supervisor: {line} -> {answer}"));
    Ok(json!({ "answer": answer }))
}

fn named<'a>(call: &'a Call) -> Result<&'a str, Failure> {
    Some(call.param(0)).filter(|name| service_name(name)).ok_or_else(|| {
        bad("A service name contains only letters, digits, dot, dash, or underscore.")
    })
}

/// `POST /api/v1/services/{name}/restart`.
fn restart(manage: &Manage, call: &Call) -> Reply {
    ask(manage, &format!("restart {}", named(call)?))
}

/// `POST /api/v1/services/{name}/stop`.
fn stop(manage: &Manage, call: &Call) -> Reply {
    ask(manage, &format!("stop {}", named(call)?))
}

/// `POST /api/v1/services/stop-all`: the supervisor's `stop`, which stops every service.
fn stop_all(manage: &Manage, _: &Call) -> Reply {
    ask(manage, "stop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_name_cannot_change_the_supervisor_line() {
        assert!(service_name("world"));
        assert!(service_name("world-2"));
        assert!(!service_name("world restart auth"));
        assert!(!service_name("world\nquit"));
        assert!(!service_name(""));
    }

    #[test]
    fn rows_are_built_only_from_the_supervisors_reported_names() {
        let rows = parse_status("auth: up on 6112\nworld: restarting\nnot a service line\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], json!({"name": "auth", "state": "up on 6112", "up": true, "port": 6112}));
        assert_eq!(rows[1]["up"], false);
        assert_eq!(rows[1]["port"], Value::Null);
        assert!(parse_status("no services").is_empty());
    }
}
