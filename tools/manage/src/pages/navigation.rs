//! Navigation: the server-owned extracted path-graph cache.
//!
//! The world reads this cache but never a client archive. A cache may intentionally omit maps;
//! an omitted map has no graph. This page inspects the cache and can replace one selected map
//! from an explicit offline archive source.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The navigation-cache page; `action` is a just-run dump's own result.
pub fn page(manage: &Manage, action: Option<Result<String, String>>, inspect: bool) -> Response {
    let outcome = match action {
        Some(Ok(message)) => components::note(&components::escape(&message)),
        Some(Err(error)) => components::note(&components::escape(&error)),
        None => String::new(),
    };
    let cache = if !inspect {
        "<p>Inspect the cache to see which maps have navigation data.</p>\
         <a class=\"button secondary\" href=\"/navigation?inspect=1\">Inspect navigation data</a>"
            .to_string()
    } else {
        match manage.inspect_navigation() {
            Ok(inspection) => {
                let names = manage.map_names();
                let rows = inspection
                    .maps
                    .iter()
                    .map(|graph| {
                        let name = if graph.map_id >= 0 {
                            names.get(&(graph.map_id as u32)).cloned()
                        } else {
                            None
                        };
                        components::row(&[
                            if graph.map_id < 0 {
                                "Unresolved".to_string()
                            } else {
                                graph.map_id.to_string()
                            },
                            name.map(|name| components::escape(&name))
                                .unwrap_or_else(|| "Unknown map".to_string()),
                            graph.planes.to_string(),
                            graph.trapezoids.to_string(),
                        ])
                    })
                    .collect::<Vec<_>>();
                format!(
                    "<p><code>{}</code>: {} bytes, {} graph(s). The JSON form is\
                     <code>/api/navigation/maps</code>.</p>\n{}",
                    components::escape(&inspection.path),
                    inspection.bytes,
                    inspection.maps.len(),
                    components::table(&["map id", "map", "layers", "walkable areas"], &rows)
                )
            }
            Err(error) => components::note(&components::escape(&error)),
        }
    };
    let body = format!(
        "<p class=\"note\">This is the server-owned extracted navigation cache. The world reads it\
         but never a client archive. A cache may intentionally omit maps; an omitted map has no\
         graph. The manager API can inspect it and its dump action replaces one selected map from\
         its explicit offline archive source.</p>\n\
         {outcome}\
         <section class=\"panel\"><h2>Path-graph cache</h2>{cache}</section>\n\
         <section class=\"panel\"><h2>Dump one map</h2>\
         <form class=\"editor-form\" method=\"post\" action=\"/navigation/dump\">\
         {field}<div class=\"actions\">{button}</div></form>\
         <p class=\"note\">The programmatic equivalent is <code>POST /api/navigation/dump</code>\
         with form field <code>map_id</code>. It needs <code>MapArchiveFile</code> and the world\
         database in <code>manage.json</code>.</p></section>",
        field = components::field(
            "Map id",
            &components::text_input_placeholder("map_id", "148")
        ),
        button = components::submit("Dump map into cache"),
    );
    layout::page("Navigation", "navigation", &body)
}
