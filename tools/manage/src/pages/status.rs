//! World status: the running world's own answer, asked through its control endpoint.
//!
//! The world is a different process. The page answers first and probes after, so a world that is
//! down, slow or refusing costs nothing to the page itself.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The world status page.
pub fn page(_manage: &Manage) -> Response {
    let body = "<p class=\"note\">The running world's own answer, through the same local control\
                endpoint <code>kryta-cli</code> uses. The page answers before the world is asked;\
                an unavailable world is reported as unavailable, never as an empty result.</p>\n\
                <div id=\"world-status\"><p>Checking the world...</p></div>\
                <script>fetch('/status/data').then(r => r.text()).then(t => { document.getElementById('world-status').innerHTML = t; }).catch(() => { document.getElementById('world-status').textContent = 'Could not reach the world status endpoint.'; });</script>\n\
                <p class=\"note\">The programmatic equivalent is <code>GET /status/data</code>, and\
                the same operation is <code>POST /v1/operations</code> with\
                <code>{\"operation\": \"world.status\"}</code>.</p>";
    layout::page("World status", "status", body)
}

/// The world's answer, as the page's own probe: three states drawn differently, because an operator
/// needs to tell them apart - the endpoint is not configured, it could not be reached or refused,
/// or it answered (and an empty answer is a valid result that says so).
pub fn fragment(manage: &Manage) -> Response {
    let html = match manage.world_status() {
        Ok(answer) if answer.trim().is_empty() => {
            components::note("The world answered the status operation, and its result is empty.")
        }
        Ok(answer) => format!(
            "<p>The running world answered:</p><pre>{}</pre>",
            components::escape(&answer)
        ),
        Err(error) => components::note(&format!(
            "The running world did not answer: {}. This is not an empty result: the world is\
             unavailable or refused the request. Check the stack with the Services page.",
            components::escape(&error)
        )),
    };
    Response::html(200, html)
}
