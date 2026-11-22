//! Services: the running stack, through the supervisor's existing lifecycle channel.
//!
//! The page draws before the supervisor is asked: its status is another process's answer and may
//! be missing, slow or refused, so it is fetched after the page has been served and never blocks
//! the shell. The manager asks; the supervisor remains the only owner of its processes.

use shared::network::http::{Request, Response};

use crate::Manage;
use crate::web::{components, http, layout};

/// The services page; `action` is a lifecycle verb's own result when a form just ran one.
pub fn page(_manage: &Manage, action: Option<(&str, &str, Result<String, String>)>) -> Response {
    let action_result = match action {
        Some((_verb, service, Ok(answer))) => components::note(&format!(
            "Supervisor: <strong>{}</strong> <code>{}</code>.",
            components::escape(&answer),
            components::escape(service)
        )),
        Some((verb, service, Err(error))) => components::note(&format!(
            "Supervisor could not {verb} <code>{}</code>: {}",
            components::escape(service),
            components::escape(&error)
        )),
        None => String::new(),
    };
    let body = format!(
        "<p class=\"note\">Service state and lifecycle actions come from the supervisor's existing\
         loopback channel. The manager asks; the supervisor remains the only owner of its\
         processes.</p>\n\
         {action_result}\n\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>Running stack</h2>\
         <p>Checking the supervisor after this page has answered; a missing supervisor is the\
         stack not running, not an empty list.</p></div>\
         <a class=\"button secondary\" href=\"/services/status\">plain status</a></div>\
         <div id=\"service-status\"><p>Checking the running supervisor...</p></div>\
         <script>fetch('/services/status').then(r => r.text()).then(t => {{ document.getElementById('service-status').innerHTML = t; }}).catch(() => {{ document.getElementById('service-status').textContent = 'Could not ask the supervisor.'; }});</script></section>",
    );
    layout::page("Services", "services", &body)
}

/// The supervisor's own status, as a fragment, for the page's own probe.
pub fn status_fragment(manage: &Manage) -> Response {
    let html = match manage.supervisor_request("status") {
        Ok(answer) => format!(
            "<pre>{}</pre>{}",
            components::escape(&answer),
            service_actions(&answer)
        ),
        Err(error) => components::note(&format!(
            "The supervisor did not answer: {}. Start the stack with <code>cargo run -p\
             kryta-supervisor</code>, then ask again. The manager starts no second supervisor.",
            components::escape(&error)
        )),
    };
    Response::html(200, html)
}

/// `POST /services/stop` or `/services/restart`: one of the supervisor's own verbs, for one service.
pub fn action(manage: &Manage, request: &Request, verb: &str) -> Response {
    let form = http::form_fields(&request.body_text());
    let name = http::value(&form, "service");
    if !http::service_name(name) {
        return crate::pages::error(
            400,
            "services",
            "A service name contains only letters, digits, dot, dash, or underscore.",
        );
    }
    let result = manage.supervisor_request(&format!("{verb} {name}"));
    page(manage, Some((verb, name, result)))
}

/// One restart and stop control for every service the supervisor currently reports. The status
/// answer is the supervisor's data, not a second service inventory maintained by the manager.
fn service_actions(status: &str) -> String {
    let controls = status
        .lines()
        .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim()))
        .filter(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
        .map(|name| {
            let name = components::escape(name);
            format!(
                "<tr><td><code>{name}</code></td><td><div class=\"actions\">\
                 <form method=\"post\" action=\"/services/restart\">{restart}</form>\
                 <form method=\"post\" action=\"/services/stop\">{stop}</form>\
                 </div></td></tr>",
                restart = format!(
                    "<input type=\"hidden\" name=\"service\" value=\"{name}\"><button>Restart</button>"
                ),
                stop = format!(
                    "<input type=\"hidden\" name=\"service\" value=\"{name}\"><button class=\"danger\">Stop</button>"
                ),
            )
        })
        .collect::<Vec<_>>();
    if controls.is_empty() {
        return String::new();
    }
    format!(
        "<h2>Actions</h2><table><thead><tr><th>service</th><th>supervisor action</th></tr></thead><tbody>{}</tbody></table>",
        controls.join("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_action_is_built_only_from_the_supervisors_reported_names() {
        let body = service_actions("auth: up on 6112\nworld: up on 6113\nnot a service line");
        assert!(body.contains("/services/restart"));
        assert!(body.contains("/services/stop"));
        assert!(body.contains("name=\"service\" value=\"world\""));
        assert!(!body.contains("not a service line"));
    }
}
