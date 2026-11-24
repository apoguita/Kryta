//! The two owners the panel asks and never replaces: the running world, through its control
//! endpoint, and the supervisor, through its line protocol.
//!
//! The panel is a client of both. It starts no process, keeps no second service list and runs no
//! second command language (`docs/port/14-operator-surfaces.md`).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::state::Manage;

/// Where the running world answers operator operations: `worldserver`'s loopback control endpoint.
pub struct Control {
    /// The loopback port the world program listens on (`Control.Port` in its own settings).
    pub(crate) port: u16,
    /// The service credential the world program requires, read from its restricted file.
    pub(crate) credential: String,
    /// How long a request waits for the world before reporting it unavailable.
    pub(crate) timeout: Duration,
}

/// Where the running supervisor accepts the lifecycle verbs it owns.
pub struct Supervisor {
    /// Its `control_port`, read from `stack.json`.
    pub(crate) port: u16,
    /// How long a request waits for its answer.
    pub(crate) timeout: Duration,
}

impl Manage {
    /// Whether `manage.json` names a world control endpoint.
    pub fn control_configured(&self) -> bool {
        self.control.is_some()
    }

    /// Call one registered operation through the same endpoint as `kryta-cli`. The world keeps
    /// argument validation, queue ownership and execution; this is only an adapter.
    pub fn world_operation(
        &self,
        operation: &str,
        arguments: &BTreeMap<String, String>,
    ) -> Result<String, String> {
        let Some(control) = self.control.as_ref() else {
            return Err(
                "no world control endpoint is configured: set ControlPort and ControlCredentialFile \
                 in manage.json, and Control.Enable with a credential file in worldserver.json"
                    .into(),
            );
        };
        let arguments: serde_json::Map<String, serde_json::Value> = arguments
            .iter()
            .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let body = serde_json::json!({
            "request_id": "manage-console",
            "operation": operation,
            "arguments": arguments
        })
        .to_string();
        let request = format!(
            "POST /v1/operations HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            control.credential,
            body.len()
        );
        let address = SocketAddr::from(([127, 0, 0, 1], control.port));
        let response = exchange(address, control.timeout, request.as_bytes())
            .map_err(|error| format!("the world control endpoint at {address}: {error}"))?;
        read_operation_answer(&response)
    }

    /// Ask the running supervisor for the state or one of its documented lifecycle verbs.
    pub fn supervisor_request(&self, line: &str) -> Result<String, String> {
        let Some(supervisor) = self.supervisor.as_ref() else {
            return Err(
                "no supervisor control channel is configured: set SupervisorStackFile to a stack.json \
                 with control_port, then start kryta-supervisor"
                    .into(),
            );
        };
        let address = SocketAddr::from(([127, 0, 0, 1], supervisor.port));
        let answer = exchange(
            address,
            supervisor.timeout,
            format!("{}\n", line.trim()).as_bytes(),
        )
        .map_err(|error| format!("the supervisor at {address}: {error}"))?;
        let answer = answer.trim().to_string();
        if answer.starts_with("err:") {
            return Err(answer);
        }
        Ok(answer)
    }
}

/// Send one request to a loopback owner and read everything it says before it closes.
fn exchange(address: SocketAddr, timeout: Duration, request: &[u8]) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|error| format!("is unavailable: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(request)
        .map_err(|error| format!("the request failed: {error}"))?;
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .map_err(|error| format!("the answer could not be read: {error}"))?;
    Ok(answer)
}

/// The world's answer to one operation: its result, or the sentence it refused with.
fn read_operation_answer(response: &str) -> Result<String, String> {
    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or("the world answered with something that is not an HTTP response")?;
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| "the world answered with something that is not JSON")?;
    if value.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        return Ok(value
            .get("result")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string());
    }
    Err(value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("the world refused the operation")
        .to_string())
}

/// Read only the existing supervisor control port from its stack file. The service rows remain owned
/// by the supervisor and are never parsed into manager state.
pub fn supervisor_port(path: &str) -> Result<u16, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read SupervisorStackFile {path}: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("SupervisorStackFile {path} is not JSON: {error}"))?;
    let port = value
        .get("control_port")
        .and_then(serde_json::Value::as_u64)
        .filter(|port| *port > 0 && *port <= u64::from(u16::MAX))
        .ok_or_else(|| format!("SupervisorStackFile {path} has no usable control_port"))?;
    Ok(port as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_control_port_is_read_from_a_stack_file() {
        let path =
            std::env::temp_dir().join(format!("kryta-manage-stack-{}.json", std::process::id()));
        std::fs::write(&path, "{\"control_port\":7612,\"services\":[]}").expect("stack is written");
        assert_eq!(
            supervisor_port(&path.to_string_lossy()).expect("port reads"),
            7612
        );
        std::fs::write(&path, "{\"control_port\":0}").expect("stack is rewritten");
        assert!(supervisor_port(&path.to_string_lossy()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_worlds_answer_is_its_result_or_its_refusal() {
        let ok = "HTTP/1.1 200 OK\r\n\r\n{\"ok\":true,\"result\":\"World 2\"}";
        assert_eq!(read_operation_answer(ok), Ok("World 2".to_string()));
        let refused = "HTTP/1.1 400 Bad\r\n\r\n{\"ok\":false,\"error\":{\"message\":\"no such operation\"}}";
        assert_eq!(read_operation_answer(refused), Err("no such operation".to_string()));
        assert!(read_operation_answer("garbage").is_err());
    }
}
