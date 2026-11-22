//! World content: the maps a world can put a client in, and the data a load reads from them.
//!
//! The content of a world is its pack (`config/World/Vanilla/`, read at world start-up -
//! `docs/decisions/0014-the-world-content-is-a-pack.md`), so this page reads the pack through the
//! same reader the world uses. A database row is state a server writes while it runs, never content:
//! the only rows shown here are the running instances, which the world itself writes.
//!
//! Editing content is the roadmap's generic asset workflow, not implemented yet; nothing on this
//! page pretends to change what the world serves.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The world content page.
pub fn page(manage: &Manage) -> Response {
    let pack = match manage.world_pack() {
        Ok(pack) => pack,
        Err(error) => {
            return crate::pages::error(
                503,
                "content",
                &format!("The world pack could not be read: {error}"),
            );
        }
    };
    let (maps, spawns) = match pack.load_maps() {
        Ok(loaded) => loaded,
        Err(error) => {
            return crate::pages::error(
                503,
                "content",
                &format!("The world pack's maps could not be read: {error}"),
            );
        }
    };
    let arrivals = pack.arrivals().unwrap_or_default();

    let map_rows = maps
        .iter()
        .map(|map| {
            let points = spawns
                .iter()
                .filter(|spawn| spawn.map_id == map.id)
                .map(|spawn| {
                    format!(
                        "x {:.2} y {:.2} plane {}{}",
                        spawn.x,
                        spawn.y,
                        spawn.plane,
                        match spawn.angle {
                            Some(angle) => format!(" facing {angle:.2}"),
                            None => String::new(),
                        }
                    )
                })
                .collect::<Vec<_>>();
            let arrival_points = arrivals
                .iter()
                .filter(|arrival| arrival.map_id == map.id)
                .map(|arrival| {
                    format!(
                        "from {}: x {:.2} y {:.2} plane {}",
                        arrival.from_map, arrival.x, arrival.y, arrival.plane
                    )
                })
                .collect::<Vec<_>>();
            components::row(&[
                format!("<code>{}</code>", map.id),
                map.kind_name().to_string(),
                format!("<code>{}</code>", map.file_id),
                components::escape(&map.name),
                if points.is_empty() {
                    "<em>none</em>".to_string()
                } else {
                    points.join("<br>")
                },
                if arrival_points.is_empty() {
                    "<em>none</em>".to_string()
                } else {
                    arrival_points.join("<br>")
                },
            ])
        })
        .collect::<Vec<_>>();

    let tables = manage.creation_tables().ok();
    let definitions = tables
        .as_ref()
        .map(|tables| tables.definitions())
        .unwrap_or_default();
    let item_rows = definitions
        .iter()
        .map(|item| {
            components::row(&[
                format!("<code>{}</code>", item.file_id),
                item.item_type.to_string(),
                format!("{}/{}", item.dye_tint, item.dye_colors),
                item.model_id.to_string(),
                item.value.to_string(),
                item.profession.to_string(),
                item.quantity.to_string(),
                components::escape(&crate::view::item_name(&item.name)),
            ])
        })
        .collect::<Vec<_>>();
    let sets = tables
        .as_ref()
        .map(|tables| tables.sets().to_vec())
        .unwrap_or_default();
    let set_rows = sets
        .iter()
        .map(|set| {
            let files = set
                .items
                .iter()
                .take(6)
                .map(|item| item.file_id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            components::row(&[
                set.campaign.to_string(),
                set.profession.to_string(),
                set.items.len().to_string(),
                files,
            ])
        })
        .collect::<Vec<_>>();

    let instances = manage.list_instances().unwrap_or_default();
    let instance_rows = instances
        .iter()
        .map(|instance| {
            components::row(&[
                instance.map_id.to_string(),
                instance.district.to_string(),
                instance.language.to_string(),
                if instance.kind == 0 {
                    "outpost"
                } else {
                    "explorable"
                }
                .to_string(),
                instance.opened_at.clone().unwrap_or_default(),
                instance
                    .empty_since
                    .clone()
                    .unwrap_or_else(|| "somebody inside".to_string()),
            ])
        })
        .collect::<Vec<_>>();

    let body = format!(
        "<p class=\"note\">The maps a world can put a client in, and where a character may stand on\
         each - the pack files under <code>config/World/Vanilla/maps/</code> the world reads at its\
         start-up (<code>0x0196</code>), read here through the same pack reader. A character's entry\
         point is scattered per R13, and a portal crossing lands on the arrival point stated for its\
         source map (R21).</p>\n\
         {maps_table}\n\
         <p class=\"note\">{maps} map(s), {spawns} entry point(s), {arrivals} arrival point(s).</p>\n\
         <section class=\"panel\"><h2>Item definitions</h2>\
         <p>What an item is, once: the creation record lists the world starts from\
         (<code>config/World/Vanilla/creation/</code>). A character's own items are written from\
         these at its confirm.</p>{item_table}\
         <p class=\"note\">{items} definition(s).</p></section>\n\
         <section class=\"panel\"><h2>Profession sets</h2>\
         <p>The sets a creation screen may choose from: a campaign and a profession, how many items\
         the set carries, and the first few of their <code>Gw.dat</code> file ids.</p>{set_table}\
         <p class=\"note\">{sets} set(s).</p></section>\n\
         <section class=\"panel\"><h2>Running instances</h2>\
         <p>The copies the world has open - the rows its tick disposes once they have been empty past\
         the hold (R37/R40). A copy with a time in the last column is waiting for that.</p>{instance_table}\
         <p class=\"note\">{instances} copy(ies).</p></section>\n\
         <p class=\"note\">Editing content is the roadmap's generic asset workflow, not implemented\
         yet: the world's pack is changed as JSON files and read at its next start, and this page\
         does not write them.</p>",
        maps_table = components::table(
            &["id", "kind", "file id", "name", "entry points", "arrivals"],
            &map_rows
        ),
        maps = maps.len(),
        spawns = spawns.len(),
        arrivals = arrivals.len(),
        item_table = components::table(
            &[
                "file id",
                "type",
                "dyes",
                "model",
                "value",
                "profession",
                "stack",
                "name"
            ],
            &item_rows
        ),
        items = definitions.len(),
        set_table = components::table(
            &["campaign", "profession", "items", "first file ids"],
            &set_rows
        ),
        sets = sets.len(),
        instance_table = components::table(
            &[
                "map",
                "district",
                "language",
                "kind",
                "opened",
                "empty since"
            ],
            &instance_rows
        ),
        instances = instances.len(),
    );
    layout::page("World content", "content", &body)
}
