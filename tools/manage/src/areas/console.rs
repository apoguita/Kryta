//! Console: the operations the running world registers, run through its own control endpoint.
//!
//! It is an adapter, never a dispatcher: the world validates the operation and its arguments,
//! queues it, executes it on its update thread and answers. The list of registrations is the
//! world's answer to `world.operations`.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::api::{Call, Failure, Reply, Route, bad, fail, get, post};
use crate::state::Manage;

/// The console area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/console/operations", operations),
        get("/api/v1/console/status", status),
        post("/api/v1/console/run", run),
    ]
}

/// The column widths `world.operations` prints its rows with: the name padded to 18, the help to
/// 44, then the argument names (`worldserver/src/CommandLine/mod.rs`, `list_operations`).
const NAME_WIDTH: usize = 18;
const HELP_WIDTH: usize = 44;

/// One line of the world's `world.operations` answer, as a row a form can be built from.
///
/// `arguments` is the list of names when the line keeps the columns, and `null` when a long name or
/// help text pushed them out of place; the page then offers the free `name=value` area instead of
/// guessing.
pub fn parse_operation(line: &str) -> Option<Value> {
    let name = line.split_whitespace().next()?;
    let help_end = NAME_WIDTH + 1 + HELP_WIDTH;
    let columns = line.is_ascii()
        && name.len() <= NAME_WIDTH
        && line.len() > help_end + 1
        && line.as_bytes()[NAME_WIDTH] == b' '
        && line.as_bytes()[help_end] == b' ';
    if !columns {
        let help = line[name.len()..].trim();
        return Some(json!({ "name": name, "help": help, "arguments": Value::Null }));
    }
    let help = line[NAME_WIDTH + 1..help_end].trim();
    let values = line[help_end + 1..].trim();
    let arguments: Vec<&str> = if values == "no values" {
        Vec::new()
    } else {
        values.split(',').map(str::trim).filter(|value| !value.is_empty()).collect()
    };
    Some(json!({ "name": name, "help": help, "arguments": arguments }))
}

/// `GET /api/v1/console/operations`: the world's own list. A world that does not answer is a state
/// the page shows.
fn operations(manage: &Manage, _: &Call) -> Reply {
    Ok(match manage.world_operation("world.operations", &BTreeMap::new()) {
        Ok(answer) => json!({
            "available": true,
            "operations": answer.lines().filter_map(parse_operation).collect::<Vec<Value>>(),
            "raw": answer,
        }),
        Err(error) => json!({ "available": false, "operations": [], "error": error }),
    })
}

/// `GET /api/v1/console/status`: the running world's answer to `world.status`. Three states an
/// operator must tell apart: not configured, did not answer, answered.
fn status(manage: &Manage, _: &Call) -> Reply {
    let configured = manage.control_configured();
    Ok(match manage.world_operation("world.status", &BTreeMap::new()) {
        Ok(answer) => json!({ "configured": configured, "available": true, "result": answer }),
        Err(error) => json!({ "configured": configured, "available": false, "error": error }),
    })
}

/// The free argument area: one `name=value` assignment per line, the same named values the world
/// endpoint accepts. The endpoint still decides whether the names and count are valid.
pub fn operation_arguments(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut arguments = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once('=') else {
            return Err(format!("An argument must be name=value: {line}"));
        };
        insert_argument(&mut arguments, name.trim(), value.trim())?;
    }
    Ok(arguments)
}

fn insert_argument(
    arguments: &mut BTreeMap<String, String>,
    name: &str,
    value: &str,
) -> Result<(), String> {
    let well_formed = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !well_formed {
        return Err(format!(
            "An argument name is letters, digits, or underscore: {name}"
        ));
    }
    if arguments.insert(name.to_string(), value.to_string()).is_some() {
        return Err(format!("An argument is named twice: {name}"));
    }
    Ok(())
}

/// The arguments of a run: the form's named fields (`arguments`, an object) and the free area
/// (`arguments_text`), together.
fn read_arguments(call: &Call) -> Result<BTreeMap<String, String>, Failure> {
    let mut arguments = operation_arguments(&call.text("arguments_text")).map_err(bad)?;
    if let Some(Value::Object(fields)) = call.body.get("arguments") {
        for (name, value) in fields {
            let value = match value {
                Value::String(text) => text.trim().to_string(),
                other => other.to_string(),
            };
            insert_argument(&mut arguments, name, &value).map_err(bad)?;
        }
    }
    Ok(arguments)
}

/// `POST /api/v1/console/run`: hand one operation and its named values to the world.
fn run(manage: &Manage, call: &Call) -> Reply {
    let operation = call.text("operation");
    if operation.is_empty() {
        return Err(bad("An operation name is required."));
    }
    let arguments = read_arguments(call)?;
    manage.say(format!("console: {operation}"));
    match manage.world_operation(&operation, &arguments) {
        Ok(result) => Ok(json!({ "operation": operation, "result": result })),
        // The world refused, or is not there: its own sentence is the answer.
        Err(error) => Err(fail(502, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_arguments_are_named_once_and_left_for_the_world_to_validate() {
        let arguments =
            operation_arguments("destination=ascalon\nmode=hard").expect("arguments parse");
        assert_eq!(arguments.get("destination"), Some(&"ascalon".to_string()));
        assert_eq!(arguments.get("mode"), Some(&"hard".to_string()));
        assert!(operation_arguments("destination=ascalon\ndestination=kryta").is_err());
        assert!(operation_arguments("not an assignment").is_err());
        assert!(operation_arguments("bad name=1").is_err());
    }

    #[test]
    fn an_operation_line_becomes_a_form_row() {
        let line = format!("{:<18} {:<44} {}", "world.status", "what this world is running", "no values");
        assert_eq!(
            parse_operation(&line),
            Some(json!({"name": "world.status", "help": "what this world is running", "arguments": []}))
        );
        let line = format!("{:<18} {:<44} {}", "chat.teleport", "move a character", "destination, mode");
        assert_eq!(
            parse_operation(&line).expect("a row")["arguments"],
            json!(["destination", "mode"])
        );
    }

    #[test]
    fn a_line_that_lost_its_columns_offers_no_guessed_arguments() {
        let line = format!(
            "{:<18} {:<44} {}",
            "an.operation.with.a.very.long.name", "help", "no values"
        );
        let row = parse_operation(&line).expect("a row");
        assert_eq!(row["name"], "an.operation.with.a.very.long.name");
        assert_eq!(row["arguments"], Value::Null);
        assert_eq!(parse_operation("   "), None);
    }
}
