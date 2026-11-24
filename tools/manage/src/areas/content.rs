//! Content: what the world serves, read from where the world reads it.
//!
//! The content of a world is its pack (`docs/decisions/0014-the-world-content-is-a-pack.md`), so
//! the maps, the creation lists and the titles are read through the readers the world uses. The
//! only database rows here are the running instances, which the world itself writes, and the world
//! list the login reads. Nothing here writes: content is changed as JSON files and read at the
//! world's next start.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Value, json};

use crate::api::{Call, Reply, Route, get, unavailable};
use crate::state::Manage;
use crate::view;

/// The content area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/content/maps", maps),
        get("/api/v1/content/creation", creation),
        get("/api/v1/content/instances", instances),
        get("/api/v1/content/worlds", worlds),
        get("/api/v1/content/titles", titles),
    ]
}

/// Which campaign folder each map of a pack sits in.
///
/// The pack is `<pack>/maps/<campaign>/<area>/<kind>/<map id>/` (`game/src/Maps/Pack.rs`); the map
/// record itself states no campaign, so the folder is the only place the pack says it. A map
/// outside a campaign folder (`unplaced`) is listed under that folder's name.
pub fn campaigns_by_folder(maps_root: &Path) -> HashMap<u32, String> {
    let mut found = HashMap::new();
    let Ok(campaigns) = std::fs::read_dir(maps_root) else {
        return found;
    };
    for campaign in campaigns.flatten().filter(|entry| entry.path().is_dir()) {
        let name = campaign.file_name().to_string_lossy().into_owned();
        collect_map_folders(&campaign.path(), &name, &mut found);
    }
    found
}

/// Every folder under `dir` that holds a `basic.json` and is named by a map id.
fn collect_map_folders(dir: &Path, campaign: &str, found: &mut HashMap<u32, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()) {
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok());
        match id {
            Some(id) if path.join("basic.json").is_file() => {
                found.insert(id, campaign.to_string());
            }
            _ => collect_map_folders(&path, campaign, found),
        }
    }
}

/// `GET /api/v1/content/maps`: every map of the pack with its entry points and arrivals.
fn maps(manage: &Manage, _: &Call) -> Reply {
    let pack_path = manage.world_pack_path().map_err(unavailable)?;
    let pack = manage
        .world_pack()
        .map_err(|error| unavailable(format!("The world pack could not be read: {error}")))?;
    let (maps, spawns) = pack
        .load_maps()
        .map_err(|error| unavailable(format!("The world pack's maps could not be read: {error}")))?;
    let arrivals = pack.arrivals().unwrap_or_default();
    let campaigns = campaigns_by_folder(&Path::new(&pack_path).join("maps"));

    let rows: Vec<Value> = maps
        .iter()
        .map(|map| {
            let points: Vec<Value> = spawns
                .iter()
                .filter(|spawn| spawn.map_id == map.id)
                .map(|spawn| json!({ "x": spawn.x, "y": spawn.y, "plane": spawn.plane, "angle": spawn.angle }))
                .collect();
            let landings: Vec<Value> = arrivals
                .iter()
                .filter(|arrival| arrival.map_id == map.id)
                .map(|arrival| json!({
                    "from": arrival.from_map, "x": arrival.x, "y": arrival.y, "plane": arrival.plane,
                }))
                .collect();
            json!({
                "id": map.id,
                "name": map.name,
                "kind": map.kind_name(),
                "campaign": campaigns.get(&map.id),
                "file_id": map.file_id,
                "hostable": map.is_hostable(),
                "spawns": points,
                "arrivals": landings,
            })
        })
        .collect();
    Ok(json!({
        "pack": pack_path,
        "maps": rows,
        "spawn_count": spawns.len(),
        "arrival_count": arrivals.len(),
    }))
}

/// `GET /api/v1/content/creation`: the item definitions and profession sets a new character is
/// made from.
fn creation(manage: &Manage, _: &Call) -> Reply {
    let tables = manage
        .creation_tables()
        .map_err(|error| unavailable(format!("The creation lists could not be read: {error}")))?;
    let items: Vec<Value> = tables
        .definitions()
        .iter()
        .map(|item| json!({
            "file_id": item.file_id,
            "item_type": item.item_type,
            "dyes": format!("{}/{}", item.dye_tint, item.dye_colors),
            "model_id": item.model_id,
            "value": item.value,
            "profession": view::profession_label(item.profession),
            "quantity": item.quantity,
            "name": view::item_name(&item.name),
        }))
        .collect();
    let sets: Vec<Value> = tables
        .sets()
        .iter()
        .map(|set| json!({
            "campaign": view::campaign_label(set.campaign),
            "profession": view::profession_label(set.profession),
            "items": set.items.len(),
            "file_ids": set.items.iter().map(|item| item.file_id).collect::<Vec<u32>>(),
        }))
        .collect();
    Ok(json!({ "items": items, "sets": sets }))
}

/// `GET /api/v1/content/instances`: the copies the world has open, from its instance table.
fn instances(manage: &Manage, _: &Call) -> Reply {
    let Some(table) = manage.instances.as_ref() else {
        return Ok(json!({ "configured": false, "instances": [] }));
    };
    let names = manage.map_names();
    let rows: Vec<Value> = table
        .rows()
        .map_err(|error| unavailable(format!("The instance table said: {error}")))?
        .iter()
        .map(|row| json!({
            "map_id": row.map_id,
            "map": names.get(&row.map_id),
            "district": row.district,
            "language": row.language,
            "kind": if row.kind == 0 { "outpost" } else { "explorable" },
            "opened_at": row.opened_at,
            "empty_since": row.empty_since,
        }))
        .collect();
    Ok(json!({ "configured": true, "instances": rows }))
}

/// `GET /api/v1/content/worlds`: the list a login may send a player to, as it was read when the
/// panel started (`data/realms.json` merged over the `realmlist` table).
fn worlds(manage: &Manage, _: &Call) -> Reply {
    let rows: Vec<Value> = manage
        .worlds
        .worlds()
        .values()
        .map(|world| json!({
            "id": world.id,
            "name": world.name,
            "address": format!("{}:{}", world.address_text, world.port),
            "local_address": format!("{}:{}", world.local_address, world.port),
        }))
        .collect();
    Ok(json!({ "worlds": rows }))
}

/// `GET /api/v1/content/titles`: the pack's title rows and the metric catalogue.
///
/// This is one sheet, the same for every player: no metric is recorded yet, so every value is the
/// platform's zero and every title shows the rank zero gives it. It is content, not a player's
/// progress, which is why it lives here and not on a player's page.
fn titles(manage: &Manage, _: &Call) -> Reply {
    let pack_path = manage.world_pack_path().map_err(unavailable)?;
    let titles = game::characters::titles::VanillaTitles::load(Path::new(&pack_path).join("titles.json"))
        .map_err(|error| unavailable(format!("The pack's titles could not be read: {error}")))?;
    let title_rows: Vec<Value> = titles
        .sheet()
        .map(|title| json!({
            "name": title.name,
            "metric": title.metric,
            "points": title.points,
            "rank": title.current_rank,
            "next": title.next_minimum,
        }))
        .collect();
    let metric_rows: Vec<Value> = game::characters::metrics::sheet()
        .iter()
        .map(|metric| json!({
            "key": metric.key, "label": metric.label, "value": metric.value, "unit": metric.unit,
        }))
        .collect();
    Ok(json!({ "titles": title_rows, "metrics": metric_rows }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_maps_campaign_is_the_folder_the_pack_keeps_it_in() {
        let root = std::env::temp_dir().join(format!("kryta-manage-maps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["Prophecies/Ascalon/Outposts/148", "Factions/Shing Jea/Explorables/249", "unplaced/010"] {
            let dir = root.join(folder);
            std::fs::create_dir_all(&dir).expect("the folder is made");
            std::fs::write(dir.join("basic.json"), "{}").expect("the record is written");
        }
        // A numbered folder with no record is not a map.
        std::fs::create_dir_all(root.join("Prophecies/Ascalon/Outposts/999")).expect("made");

        let campaigns = campaigns_by_folder(&root);
        assert_eq!(campaigns.get(&148).map(String::as_str), Some("Prophecies"));
        assert_eq!(campaigns.get(&249).map(String::as_str), Some("Factions"));
        assert_eq!(campaigns.get(&10).map(String::as_str), Some("unplaced"));
        assert_eq!(campaigns.get(&999), None);
        assert!(campaigns_by_folder(&root.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
