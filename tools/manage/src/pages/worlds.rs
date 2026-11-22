//! Worlds: the list a login may send a player to, after `data/realms.json` is merged over the
//! `realmlist` table. The two programs read the same list; the file row wins when both name a
//! world.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The worlds page.
pub fn page(manage: &Manage) -> Response {
    let Some(worlds) = manage.list_worlds() else {
        return crate::pages::error(
            500,
            "worlds",
            "The login database could not be read, so the world list is not available.",
        );
    };
    let rows = worlds
        .iter()
        .map(|world| {
            components::row(&[
                world.id.to_string(),
                components::escape(&world.name),
                format!(
                    "<code>{}:{}</code>",
                    components::escape(&world.address_text),
                    world.port
                ),
                format!("<code>{}:{}</code>", world.local_address, world.port),
            ])
        })
        .collect::<Vec<_>>();
    let body = format!(
        "<p class=\"note\">The worlds a login may send a player to, after <code>data/realms.json</code>\
         is merged over the <code>realmlist</code> table. The two programs read the same list; the\
         file row wins when both name a world.</p>\n\
         {table}\n\
         <p class=\"note\">{count} world(s).</p>",
        table = components::table(&["id", "name", "public address", "local address"], &rows),
        count = worlds.len(),
    );
    layout::page("Worlds", "worlds", &body)
}
