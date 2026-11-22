//! Console: one registered operation, run through the world's own control endpoint.
//!
//! It is an adapter, never a dispatcher: the world validates the operation and its arguments,
//! queues it, executes it on its update thread and answers. The list of registrations is asked for
//! after the page has answered, because it too belongs to the running world.

use std::collections::BTreeMap;

use shared::network::http::{Request, Response};

use crate::Manage;
use crate::web::{components, http, layout};

/// The console page; `run` is a just-run operation and its answer or refusal.
pub fn page(
    _manage: &Manage,
    run: Option<(&str, BTreeMap<String, String>, Result<String, String>)>,
) -> Response {
    let result = match run {
        Some((operation, _arguments, Ok(answer))) => format!(
            "<h2>Result: <code>{}</code></h2><pre>{}</pre>",
            components::escape(operation),
            components::escape(&answer)
        ),
        Some((operation, _arguments, Err(error))) => format!(
            "<h2>Refused: <code>{}</code></h2>{}",
            components::escape(operation),
            components::note(&components::escape(&error))
        ),
        None => String::new(),
    };
    let body = format!(
        "<p class=\"note\">Run a registered operation through the same local API as\
         <code>kryta-cli</code>. The world validates the operation and arguments, queues it, and\
         returns the result or refusal. This page does not parse or execute a second command\
         language.</p>\n\
         <form class=\"editor-form\" method=\"post\" action=\"/console/run\">\
         {operation}{arguments}\
         <div class=\"actions\">{button}</div></form>\n\
         {result}\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>Registered operations</h2>\
         <p>The world's own list, asked after this page answered.</p></div>\
         <a class=\"button secondary\" href=\"/console/operations\">plain list</a></div>\
         <pre id=\"console-operations\">Checking the world...</pre>\
         <script>fetch('/console/operations').then(r => r.text()).then(t => {{ document.getElementById('console-operations').textContent = t; }}).catch(() => {{ document.getElementById('console-operations').textContent = 'Could not reach the world control endpoint.'; }});</script></section>",
        operation = components::field(
            "Operation",
            &components::text_input_placeholder("operation", "world.status")
        ),
        arguments = components::field(
            "Arguments, one name=value per line",
            "<textarea name=\"arguments\" placeholder=\"destination=ascalon\"></textarea>"
        ),
        button = components::submit("Run operation"),
        result = result,
    );
    layout::page("Console", "console", &body)
}

/// The world's registered operations, as the console page's probe.
pub fn operations_fragment(manage: &Manage) -> Response {
    let text = manage
        .world_operation("world.operations", &BTreeMap::new())
        .unwrap_or_else(|error| format!("World unavailable: {error}"));
    Response::text(200, text)
}

/// `POST /console/run`: hand one operation and its named values to the world.
pub fn run(manage: &Manage, request: &Request) -> Response {
    let form = http::form_fields(&request.body_text());
    let operation = http::value(&form, "operation").trim();
    if operation.is_empty() {
        return crate::pages::error(400, "console", "An operation name is required.");
    }
    let operation = operation.to_string();
    let arguments = match http::operation_arguments(http::value(&form, "arguments")) {
        Ok(arguments) => arguments,
        Err(error) => return crate::pages::error(400, "console", &error),
    };
    let result = manage.world_operation(&operation, &arguments);
    page(manage, Some((&operation, arguments, result)))
}
