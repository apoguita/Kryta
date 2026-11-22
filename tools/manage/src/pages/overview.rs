//! The overview: a work entry point, not a database inventory.
//!
//! It shows the numbers an operator checks first, the live server's own answer (asked after the
//! page has already answered, because it belongs to another process), and the working areas the
//! rail leads to. Unfinished destinations are not listed here: the rail is not a roadmap.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The overview page.
pub fn page(manage: &Manage) -> Response {
    let accounts = match manage.list_accounts() {
        Ok(accounts) => accounts,
        Err(error) => {
            return crate::pages::error(
                500,
                "overview",
                &format!("The account table could not be read: {error}"),
            );
        }
    };
    let active = accounts.iter().filter(|account| account.active).count();
    let characters = manage
        .characters()
        .and_then(|characters| characters.count().ok());
    let maps = manage
        .world_pack()
        .ok()
        .and_then(|pack| pack.load_maps().ok())
        .map(|(maps, _)| maps.len());

    let character_metric = match characters {
        Some(count) => components::metric(count, "players"),
        None => components::metric("-", "players (no character database)"),
    };
    let map_metric = match maps {
        Some(count) => components::metric(count, "map definitions"),
        None => components::metric("-", "map definitions (no world database)"),
    };

    let areas = components::table(
        &["area", "whose data", "open"],
        &[
            components::row(&[
                "Accounts".to_string(),
                "The login identities the server hosts, and the state that belongs to a person."
                    .to_string(),
                "<a href=\"/accounts\">open accounts</a>".to_string(),
            ]),
            components::row(&[
                "Players".to_string(),
                "The characters the accounts own: progress, position, inventory, titles, unlocks."
                    .to_string(),
                "<a href=\"/players\">open players</a>".to_string(),
            ]),
            components::row(&[
                "Services".to_string(),
                "The supervisor's running stack: status, stop, restart.".to_string(),
                "<a href=\"/services\">open services</a>".to_string(),
            ]),
            components::row(&[
                "Console".to_string(),
                "The world's registered operations, through its own control endpoint.".to_string(),
                "<a href=\"/console\">open console</a>".to_string(),
            ]),
            components::row(&[
                "World content".to_string(),
                "The maps, their entry points, item definitions and running copies.".to_string(),
                "<a href=\"/content\">open world content</a>".to_string(),
            ]),
            components::row(&[
                "Worlds".to_string(),
                "The realm list a login may send a player to.".to_string(),
                "<a href=\"/worlds\">open worlds</a>".to_string(),
            ]),
            components::row(&[
                "Settings & schema".to_string(),
                "The programs' own JSON files, and the schema each database records.".to_string(),
                "<a href=\"/settings\">open settings</a> <a href=\"/schema\">open schema</a>"
                    .to_string(),
            ]),
        ],
    );

    let body = format!(
        "<section class=\"panel\"><div class=\"panel-head\"><div><h2>Operator workspace</h2>\
         <p>Manage the server through the owners that already keep its state. Accounts hold the\
         login identities and what belongs to the person; Players are the characters those\
         accounts own, with their own progress, inventory, titles and unlocks. Service lifecycles\
         stay with the supervisor; the running world answers its own operations.</p></div>\
         <a class=\"button\" href=\"/accounts\">Open accounts</a></div></section>\n\
         <section class=\"metric-grid\">\n{accounts}{active}{characters}{maps}</section>\n\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>Live server</h2>\
         <p>The running world is asked for its own status after this page has answered; it is a\
         different process and this page never waits for it.</p></div>\
         <a class=\"button secondary\" href=\"/status\">open world status</a></div>\
         <div id=\"world-status\"><p>Checking the world...</p></div>\
         <script>fetch('/status/data').then(r => r.text()).then(t => {{ document.getElementById('world-status').innerHTML = t; }}).catch(() => {{ document.getElementById('world-status').textContent = 'Could not reach the world status endpoint.'; }});</script></section>\n\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>Working areas</h2>\
         <p>Only destinations that answer today are listed. A pending capability is a roadmap\
         entry, not a page that pretends to exist.</p></div></div>{areas}</section>\n\
         <p class=\"note\">The manager serves local operator use by default. A token in\
         <code>manage.json</code> is required when one is configured.</p>",
        accounts = components::metric(accounts.len(), "accounts"),
        active = components::metric(active, "active login accounts"),
        characters = character_metric,
        maps = map_metric,
    );
    layout::page("Overview", "overview", &body)
}
