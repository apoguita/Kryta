//! `kryta-manage`: the control panel - the operator's front door to the state the server programs
//! keep.
//!
//! It is not a game server. No client talks to it, and neither server program knows it exists. The
//! service answers data, and the pages are static files that ask for it
//! (`docs/port/23-control-panel.md`):
//!
//! - `api` - the one answer shape, the route type, the guard and the router. Shared by every area.
//! - `areas` - one module per area of the panel, each declaring its own routes. The table there is
//!   the single registration point; adding an area is one module and one word in that table.
//! - `ui` - the static pages (`tools/manage/ui/`), embedded in the binary.
//! - `state`, `clients`, `startup` - what the service answers from, the two owners it asks (the
//!   supervisor and the running world), and how it starts.
//! - `view`, `path_graphs` - the game's names for stored numbers, and the path-graph cache.
//!
//! The data access is the nouns' own code (`game::accounts`, `game::characters`, `game::maps`), so
//! what the panel changes is exactly what the server reads, and the panel writes no SQL. It writes
//! account rows, character rows, the path-graph cache and `recordings/index.jsonl`.
//!
//! Settings are in `manage.json` (`docs/port/05-tooling.md`).

mod api;
mod areas;
mod clients;
mod path_graphs;
mod startup;
mod state;
mod ui;
mod view;

use shared::network::http::{Request, Response};

pub use startup::main_entry;
pub use state::Manage;

/// One request, answered: the static pages, `/health`, or an area's route.
fn answer(manage: &Manage, request: &Request) -> Response {
    if request.path.starts_with("/api/") {
        return api::serve(manage, &manage.guard, &areas::routes(), request);
    }
    if let Err(failure) = manage.guard.check(request) {
        return api::answer(Err(failure));
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => Response::text(200, "ok\n"),
        ("GET", path) => ui::file(path, areas::AREAS).unwrap_or_else(|| {
            api::answer(Err(api::missing(format!("Nothing answers at {path}."))))
        }),
        _ => api::answer(Err(api::fail(405, "This address does not answer that method."))),
    }
}
