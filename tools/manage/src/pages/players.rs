//! Players: the characters the server hosts, listed across accounts or for one account, and one
//! player with the data that belongs to it.
//!
//! A player (a character) belongs to one account; the data below the player belongs to the player:
//! its campaign and professions, its level and stored experience, its carried gold, where it stands,
//! what it carries (inventory and equipment), its titles, and the maps it has unlocked. The account
//! that owns it is shown here and linked back, and the account's own page lists the same players.

use shared::network::http::{Request, Response};

use crate::Manage;
use crate::view;
use crate::web::{components, http, layout};

/// The creation screen's own list of campaigns: the three a role-playing character may choose.
/// A PvP character (campaign 0) is refused because the world pack names no PvP starting map.
const CREATION_CAMPAIGNS: [u8; 3] = [1, 2, 3];

/// The players: every character across accounts, or one account's players when `?account=` selects it.
pub fn list(manage: &Manage, request: &Request) -> Response {
    let accounts = match manage.list_accounts() {
        Ok(accounts) => accounts,
        Err(error) => {
            return crate::pages::error(
                500,
                "players",
                &format!("The account table could not be read: {error}"),
            );
        }
    };

    let filter = request
        .query("account")
        .and_then(|value| value.parse::<i64>().ok());
    let filter_account = match filter {
        Some(id) => match accounts.iter().find(|account| account.id == id) {
            Some(account) => Some(account),
            None => return crate::pages::error(404, "players", "No account has that id."),
        },
        None => None,
    };

    let players: Vec<(Option<game::accounts::Account>, game::characters::Character)> =
        match filter_account {
            Some(account) => match manage.characters_of_account(&account.user_id) {
                Some(characters) => characters
                    .into_iter()
                    .map(|character| (Some(account.clone()), character))
                    .collect(),
                None => {
                    return crate::pages::error(
                        503,
                        "players",
                        "No character database is configured in manage.json, so there is nothing to list.",
                    );
                }
            },
            None => match manage.characters() {
                Some(characters) => characters
                    .all_with_account()
                    .into_iter()
                    .map(|(account_uuid, character)| {
                        (
                            accounts
                                .iter()
                                .find(|account| account.user_id == account_uuid)
                                .cloned(),
                            character,
                        )
                    })
                    .collect(),
                None => {
                    return crate::pages::error(
                        503,
                        "players",
                        "No character database is configured in manage.json, so there is nothing to list.",
                    );
                }
            },
        };

    let names = view::map_names(manage);
    let rows = players
        .iter()
        .map(|(account, character)| {
            let row = view::PlayerRow::of(character, &names);
            let href = format!("/players/{}", row.uuid);
            let account_cell = match account {
                Some(account) => format!(
                    "<a href=\"/accounts/{}\">{}</a>",
                    account.id,
                    components::escape(&account.username)
                ),
                None => "<em>account not recorded</em>".to_string(),
            };
            components::row(&[
                format!("<a href=\"{href}\">{}</a>", components::escape(&row.name)),
                account_cell,
                row.level.to_string(),
                components::escape(&row.campaign),
                components::escape(&row.professions),
                components::escape(&row.outpost),
                if row.pvp { "yes" } else { "no" }.to_string(),
            ])
        })
        .collect::<Vec<_>>();
    let table_body = if rows.is_empty() {
        components::note("No player exists yet. Create one below.")
    } else {
        components::table(
            &[
                "player",
                "account",
                "level",
                "campaign",
                "professions",
                "last outpost",
                "PvP",
            ],
            &rows,
        )
    };

    let scope = match filter_account {
        Some(account) => format!(
            "<p class=\"note\">Players of account <a href=\"/accounts/{}\">{}</a>. <a href=\"/players\">All players</a>.</p>",
            account.id,
            components::escape(&account.username)
        ),
        None => "<p class=\"note\">Every player the server hosts, with the account that owns it. A\
                 player's page owns its progress, position, inventory, titles and unlocks.</p>"
            .to_string(),
    };

    let crumbs = match filter_account {
        Some(account) => components::crumbs(&[
            ("/players", "Players"),
            ("", &components::escape(&account.username)),
        ]),
        None => components::crumbs(&[("/players", "Players")]),
    };

    let body = format!(
        "{crumbs}\n{scope}\n\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>Players</h2>\
         <p>{count} player(s) listed. A player is a character; it belongs to one account.</p></div>\
         </div>{table}</section>\n\
         {create}",
        count = players.len(),
        table = table_body,
        create = create_form(&accounts, filter_account),
    );
    layout::page("Players", "players", &body)
}

/// One player: its own data, the account it belongs to, and the edits an operator may make.
pub fn detail(manage: &Manage, uuid: &str) -> Response {
    let Some(characters) = manage.characters() else {
        return crate::pages::error(
            503,
            "players",
            "No character database is configured in manage.json, so this player cannot be read.",
        );
    };
    let Some((account_uuid, character)) = characters
        .all_with_account()
        .into_iter()
        .find(|(_, character)| game::characters::uuid_text(&character.uuid) == uuid)
    else {
        return crate::pages::error(404, "players", "No player has that id.");
    };
    let accounts = manage.list_accounts().unwrap_or_default();
    let account = accounts
        .iter()
        .find(|account| account.user_id == account_uuid)
        .cloned();
    let names = view::map_names(manage);
    let row = view::PlayerRow::of(&character, &names);
    let sheet = characters.sheet(uuid, character.uuid);
    let gold = sheet.gold;
    let experience = characters.experience(uuid).ok().flatten();
    let position = characters.position(uuid).ok().flatten();
    let primary = ((character.settings.appearance >> 20) & 0xF) as u8;

    let unlocked_professions = view::PROFESSIONS
        .iter()
        .filter(|(id, _)| *id > 0 && (sheet.unlocked_professions >> id) & 1 == 1)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>();
    let unlocked_professions = if unlocked_professions.is_empty() {
        "none".to_string()
    } else {
        unlocked_professions.join(", ")
    };

    let position_text = match position {
        Some(position) => format!(
            "x {:.2} y {:.2} on plane {} in {}",
            position.x,
            position.y,
            position.plane,
            view::outpost_label(position.map_id as u16, &names)
        ),
        None => "never saved - the player has not been placed since it was played".to_string(),
    };

    let inventory = match characters.items_of(uuid) {
        Ok(items) if items.is_empty() => components::note("The player carries nothing."),
        Ok(items) => {
            let rows = items
                .iter()
                .map(|item| {
                    let name = crate::view::item_name(&item.name);
                    let name = if name.trim().is_empty() {
                        "(unnamed)".to_string()
                    } else {
                        name
                    };
                    components::row(&[
                        components::escape(&name),
                        item.bag_id.to_string(),
                        item.slot.to_string(),
                        item.quantity.to_string(),
                        format!("<code>{}</code>", item.file_id),
                        item.model_id.to_string(),
                    ])
                })
                .collect::<Vec<_>>();
            components::table(
                &["item", "bag", "slot", "quantity", "file id", "model"],
                &rows,
            )
        }
        Err(error) => components::note(&format!(
            "The inventory could not be read: {}",
            components::escape(&error)
        )),
    };

    let unlocks = match characters.unlocked_maps(uuid) {
        Ok(maps) if maps.is_empty() => {
            components::note("The player has no unlocked map yet (R12).")
        }
        Ok(maps) => {
            let cells = maps
                .iter()
                .map(|map_id| match names.get(map_id) {
                    Some(name) => format!("<code>{map_id}</code> {}", components::escape(name)),
                    None => format!("<code>{map_id}</code>"),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("<p>{cells}</p>")
        }
        Err(error) => components::note(&format!(
            "The unlocked maps could not be read: {}",
            components::escape(&error)
        )),
    };

    let name = components::escape(&character.name);
    let owner = match &account {
        Some(account) => format!(
            "<p class=\"note\">Owned by account <a href=\"/accounts/{}\">{}</a>.</p>",
            account.id,
            components::escape(&account.username)
        ),
        None => components::note(
            "The account that owns this player is not in the account table; the player row is kept.",
        ),
    };
    let experience_text = match experience {
        Some(value) => value.to_string(),
        None => "not stated by the row".to_string(),
    };
    let metric_rows: String = game::characters::metrics::sheet()
        .iter()
        .map(|metric| {
            format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                components::escape(metric.key),
                components::escape(metric.label),
                metric.value,
                components::escape(metric.unit)
            )
        })
        .collect();
    let title_rows: String = match manage.world_pack_path().and_then(|pack| {
        game::characters::titles::VanillaTitles::load(
            std::path::Path::new(&pack).join("titles.json"),
        )
    }) {
        Ok(titles) => titles
            .sheet()
            .map(|title| {
                format!(
                    "<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    components::escape(title.name),
                    components::escape(title.metric),
                    title.points,
                    title.current_rank,
                    title.next_minimum
                )
            })
            .collect(),
        Err(error) => format!(
            "<tr><td colspan=\"5\">{}</td></tr>",
            components::escape(&error)
        ),
    };
    let uuid_escaped = components::escape(uuid);
    let body = format!(
        "{crumbs}\n{owner}\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>{name}</h2>\
         <p>Player id <code>{uuid}</code></p></div></div>\
         <table><tbody>\
         <tr><th>campaign</th><td>{campaign}</td></tr>\
         <tr><th>professions</th><td>{professions} (primary {primary})</td></tr>\
         <tr><th>level</th><td>{level}</td></tr>\
         <tr><th>experience</th><td>{experience}</td></tr>\
         <tr><th>gold</th><td>{gold}</td></tr>\
         <tr><th>PvP</th><td>{pvp}</td></tr>\
         <tr><th>last outpost</th><td>{outpost}</td></tr>\
         <tr><th>position</th><td>{position}</td></tr>\
         <tr><th>appearance</th><td>{appearance}</td></tr>\
         <tr><th>professions unlocked</th><td>{unlocked_professions}</td></tr>\
         </tbody></table>\
         <p class=\"note\">Changes are not synchronized with an online player. Log the player\
         out before editing or deleting it.</p>\
         <div class=\"field-grid\">\
         <form class=\"editor-form\" method=\"post\" action=\"/players/{uuid}/rename\">\
         {rename_field}<div class=\"actions\">{rename_button}</div></form>\
         <form class=\"editor-form\" method=\"post\" action=\"/players/{uuid}/level\">\
         {level_field}<div class=\"actions\">{level_button}</div></form>\
         </div></section>\n\
         <section class=\"panel\"><h2>Inventory and equipment</h2>\
         <p>The item rows the player's load draws from, with the bag and slot each sits in.</p>{inventory}</section>\n\
         <section class=\"panel\"><h2>Metrics</h2>\
         <p>Every metric the platform keeps. All report zero until a source writes them; this is\
         the character sheet's inventory, not a claim that the values have been earned.</p>\
         <table><thead><tr><th>metric</th><th>label</th><th>value</th><th>unit</th></tr></thead>\
         <tbody>{metric_rows}</tbody></table></section>\n\
         <section class=\"panel\"><h2>Titles</h2>\
         <p>All 48 client title rows, each displayed at 1 point while its metric is zero. The\
         extra row is <code>Jump Master</code>, projected from <code>jumps</code>; its next rank\
         is 10,000.</p>\
         <table><thead><tr><th>title</th><th>metric</th><th>points</th><th>rank</th><th>next</th></tr></thead>\
         <tbody>{title_rows}</tbody></table></section>\n\
         <section class=\"panel\"><h2>Unlocked maps</h2>\
         <p>Where a player may travel (R12), as the load's gate reads it.</p>{unlocks}</section>\n\
         <section class=\"panel\"><div class=\"panel-head\"><div><h2>Permanent removal</h2>\
         <p>Removes the player's row and the records that are its own: its items and its\
         unlocks.</p></div></div>\
         <form method=\"post\" action=\"/players/{uuid}/delete\" \
         onsubmit=\"return confirm('Remove {confirm} and its items and unlocks?')\">{remove}</form></section>",
        crumbs = components::crumbs(&[("/players", "Players"), ("", &name)]),
        owner = owner,
        name = name,
        uuid = uuid_escaped,
        campaign = components::escape(&row.campaign),
        professions = components::escape(&row.professions),
        primary = view::profession_label(primary),
        level = row.level,
        experience = experience_text,
        gold = gold,
        pvp = if row.pvp { "yes" } else { "no" },
        metric_rows = metric_rows,
        title_rows = title_rows,
        outpost = components::escape(&row.outpost),
        position = components::escape(&position_text),
        appearance = {
            let fields = view::appearance_fields(character.settings.appearance);
            let sex = view::sex_label(character.settings.appearance);
            format!(
                "{sex}, height {}, skin {}, hair colour {}, face {}, hair style {}",
                fields[1].1, fields[2].1, fields[3].1, fields[4].1, fields[5].1
            )
        },
        unlocked_professions = unlocked_professions,
        rename_field = components::field("Name", &components::text_input("name", &character.name)),
        rename_button = components::submit("Rename"),
        level_field = components::field(
            "Level (1 to 20, R48)",
            &components::number_input("level", row.level, 1, 20)
        ),
        level_button = components::submit("Save level"),
        inventory = inventory,
        unlocks = unlocks,
        confirm = name,
        remove = components::submit_class("Remove player", "danger"),
    );
    layout::page("Players", "players", &body)
}

/// `POST /players/create`: write a player for the account the form names.
pub fn create(manage: &Manage, request: &Request) -> Response {
    let form = http::form_fields(&request.body_text());
    let account_id = match http::value(&form, "account").parse::<i64>() {
        Ok(id) => id,
        Err(_) => {
            return crate::pages::error(
                400,
                "players",
                "A player is created for one account: select its account.",
            );
        }
    };
    let account = match manage.list_accounts() {
        Ok(accounts) => accounts
            .into_iter()
            .find(|account| account.id == account_id),
        Err(error) => {
            return crate::pages::error(
                500,
                "players",
                &format!("The account table could not be read: {error}"),
            );
        }
    };
    let Some(account) = account else {
        return crate::pages::error(404, "players", "No account has that id.");
    };

    let run = || -> Result<String, String> {
        let characters = manage
            .characters()
            .ok_or("The character database is not configured.")?;
        let name = http::value(&form, "name").trim();
        if !valid_character_name(name) {
            return Err("Name must be 3 to 19 English letters, a first name and a surname.".into());
        }
        let number = |key: &str, max: u32| -> Result<u32, String> {
            let value = http::value(&form, key)
                .parse::<u32>()
                .map_err(|_| format!("Invalid {key}."))?;
            if value > max {
                return Err(format!("Invalid {key}."));
            }
            Ok(value)
        };
        let campaign = number("campaign", 3)? as u8;
        if campaign == 0 {
            return Err(
                "PvP creation is unavailable: the world pack names no PvP starting map.".into(),
            );
        }
        let profession = number("profession", 10)? as u8;
        let valid = (1..=6).contains(&profession)
            || (campaign == 2 && (7..=8).contains(&profession))
            || (campaign == 3 && (9..=10).contains(&profession));
        if !valid {
            return Err("This profession is unavailable in the selected campaign.".into());
        }
        // docs/game-rules.md E74: bit positions in the client's appearance word.
        let appearance = number("sex", 1)?
            | (number("height", 15)? << 1)
            | (number("skin", 31)? << 5)
            | (number("hair_color", 31)? << 10)
            | (number("face", 31)? << 15)
            | (u32::from(profession) << 20)
            | (number("hair_style", 63)? << 24)
            | (u32::from(campaign) << 30);

        let world_config = manage
            .world_config()
            .ok_or("The world program's settings are not named in manage.json.")?;
        let config = common::configuration::Config::load_json(
            world_config,
            false,
            common::configuration::Policy::default(),
        )
        .map_err(|error| error.to_string())?;
        let (inventory_path, _) = config
            .value_string("InventoryFile", "")
            .map_err(|e| e.to_string())?;
        let (equipment_path, _) = config
            .value_string("EquipmentFile", "")
            .map_err(|e| e.to_string())?;
        let (worn_path, _) = config
            .value_string("EquippedSlotsFile", "")
            .map_err(|e| e.to_string())?;
        let (pack_path, _) = config
            .value_string("WorldPack", "")
            .map_err(|e| e.to_string())?;
        let log = manage.log();
        let tables = game::creation::Tables::load(
            &inventory_path,
            &equipment_path,
            Some(std::path::Path::new(&worn_path)),
            log,
        )?;
        if tables.set(campaign, profession).is_none() {
            return Err("The world pack has no equipment for this choice.".into());
        }

        let spawn_path = std::path::Path::new(&pack_path).join("character_spawn.json");
        let spawn_data: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&spawn_path)
                .map_err(|error| format!("{}: {error}", spawn_path.display()))?,
        )
        .map_err(|error| error.to_string())?;
        let map = spawn_data["spawns"]
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["campaign"].as_u64() == Some(u64::from(campaign)))
            })
            .and_then(|row| row["map_id"].as_u64())
            .and_then(|id| u16::try_from(id).ok())
            .ok_or("The world pack names no starting map for this campaign.")?;
        let pack = game::maps::pack::Pack::load(&pack_path, log)?;
        let (maps, _) = pack.load_maps()?;
        if !maps
            .iter()
            .any(|entry| entry.id == u32::from(map) && entry.is_hostable())
        {
            return Err(format!("The world pack cannot host starting map {map}."));
        }

        let templates = manage
            .items()
            .ok_or("The world database is not configured, so item templates are not available.")?
            .list()?;
        let mut id = [0u8; 16];
        getrandom::getrandom(&mut id)
            .map_err(|error| format!("Cannot create a player id: {error}"))?;
        let uuid = game::characters::uuid_text(&id);
        if !characters.insert_creation(
            &uuid,
            &account.user_id,
            name,
            appearance,
            profession,
            campaign,
            map,
        )? {
            return Err("That player name is already in use.".into());
        }
        let initialized = characters
            .write_starting_items(&uuid, &tables.inventory(campaign, profession), &templates)
            .and_then(|_| characters.unlock_map(&uuid, u32::from(map)).map(|_| ()));
        if let Err(error) = initialized {
            return match characters.remove(&uuid) {
                Ok(_) => Err(format!("Creation failed and was rolled back: {error}")),
                Err(cleanup) => Err(format!(
                    "Creation failed: {error}; cleanup failed for {uuid}: {cleanup}"
                )),
            };
        }
        manage.say(format!("account {account_id}: created player {uuid}"));
        Ok(uuid)
    };
    match run() {
        Ok(uuid) => http::redirect(&format!("/players/{uuid}")),
        Err(error) => crate::pages::error(400, "players", &error),
    }
}

/// `POST /players/{uuid}/rename`: rename one player.
pub fn rename(manage: &Manage, request: &Request, uuid: &str) -> Response {
    let form = http::form_fields(&request.body_text());
    let name = http::value(&form, "name").trim();
    if !valid_character_name(name) {
        return crate::pages::error(
            400,
            "players",
            "Name must be 3 to 19 English letters, a first name and a surname.",
        );
    }
    let Some(characters) = manage.characters() else {
        return crate::pages::error(503, "players", "No character database is configured.");
    };
    let Some((account_uuid, _)) = characters
        .all_with_account()
        .into_iter()
        .find(|(_, character)| game::characters::uuid_text(&character.uuid) == uuid)
    else {
        return crate::pages::error(404, "players", "No player has that id.");
    };
    match characters.rename_for_account(&account_uuid, uuid, name) {
        Ok(true) => {
            manage.say(format!("player {uuid}: renamed to {name}"));
            http::redirect(&format!("/players/{uuid}"))
        }
        Ok(false) => crate::pages::error(404, "players", "The player is already gone."),
        Err(error) => crate::pages::error(
            500,
            "players",
            &format!("The character database said: {error}"),
        ),
    }
}

/// `POST /players/{uuid}/level`: set a level in the game's own range (R48).
pub fn set_level(manage: &Manage, request: &Request, uuid: &str) -> Response {
    let form = http::form_fields(&request.body_text());
    let level = match http::value(&form, "level").parse::<u8>() {
        Ok(level) if (1..=20).contains(&level) => level,
        _ => {
            return crate::pages::error(
                400,
                "players",
                "A level is a number from 1 to 20, the game's own range (R48).",
            );
        }
    };
    let Some(characters) = manage.characters() else {
        return crate::pages::error(503, "players", "No character database is configured.");
    };
    if !characters
        .all_with_account()
        .iter()
        .any(|(_, character)| game::characters::uuid_text(&character.uuid) == uuid)
    {
        return crate::pages::error(404, "players", "No player has that id.");
    }
    match characters.set_level(uuid, level) {
        Ok(true) => {
            manage.say(format!("player {uuid}: level {level}"));
            http::redirect(&format!("/players/{uuid}"))
        }
        Ok(false) => crate::pages::error(404, "players", "The player is already gone."),
        Err(error) => crate::pages::error(
            500,
            "players",
            &format!("The character database said: {error}"),
        ),
    }
}

/// `POST /players/{uuid}/delete`: remove the player and the records that are its own.
pub fn delete(manage: &Manage, request: &Request, uuid: &str) -> Response {
    let _ = request;
    let Some(characters) = manage.characters() else {
        return crate::pages::error(503, "players", "No character database is configured.");
    };
    if !characters
        .all_with_account()
        .iter()
        .any(|(_, character)| game::characters::uuid_text(&character.uuid) == uuid)
    {
        return crate::pages::error(404, "players", "No player has that id.");
    }
    match characters.remove(uuid) {
        Ok(true) => {
            manage.say(format!(
                "player {uuid}: removed, with its items and unlocks"
            ));
            http::redirect("/players")
        }
        Ok(false) => crate::pages::error(404, "players", "The player is already gone."),
        Err(error) => crate::pages::error(
            500,
            "players",
            &format!("The character database said: {error}"),
        ),
    }
}

/// The creation form, drawn from the creation record's own choices.
fn create_form(
    accounts: &[game::accounts::Account],
    selected: Option<&game::accounts::Account>,
) -> String {
    if accounts.is_empty() {
        return components::note(
            "A player is created for one account. Create an account first, on the Accounts page.",
        );
    }
    let account_options = accounts
        .iter()
        .map(|account| {
            let selected = if selected.map(|selected| selected.id) == Some(account.id) {
                " selected"
            } else {
                ""
            };
            format!(
                "<option value=\"{}\"{selected}>{}</option>",
                account.id,
                components::escape(&account.username)
            )
        })
        .collect::<String>();
    let campaigns = view::CAMPAIGNS
        .iter()
        .filter(|(id, _)| CREATION_CAMPAIGNS.contains(id))
        .map(|(id, name)| format!("<option value=\"{id}\">{name}</option>"))
        .collect::<String>();
    format!(
        "<section class=\"panel\"><h2>Create player</h2>\
         <p>Choose the account that will own the player, then a campaign, profession and\
         appearance. PvP creation is unavailable until the world pack names its starting map. The\
         player is written with the creation record's inventory and its first map unlocked.</p>\
         <form class=\"editor-form\" method=\"post\" action=\"/players/create\">\
         <div class=\"field-grid\">\
         {account}{name}\
         </div><div class=\"field-grid\">\
         {campaign}{profession}\
         </div><div class=\"field-grid\">\
         {sex}{height}\
         </div><div class=\"field-grid\">\
         {skin}{hair_color}\
         </div><div class=\"field-grid\">\
         {face}{hair_style}\
         </div>\
         <div class=\"actions\">{button}</div></form></section>",
        account = components::field(
            "Account",
            &format!("<select name=\"account\">{account_options}</select>")
        ),
        name = components::field(
            "Name",
            &components::text_input_placeholder("name", "First Last")
        ),
        campaign = components::field(
            "Campaign",
            &format!("<select name=\"campaign\">{campaigns}</select>")
        ),
        profession = components::field(
            "Profession",
            &format!(
                "<select name=\"profession\">{}</select>",
                view::profession_options(1)
            )
        ),
        sex = components::field(
            "Sex",
            "<select name=\"sex\"><option value=\"0\">Male</option><option value=\"1\">Female</option></select>"
        ),
        height = components::field(
            "Height (0-15)",
            &components::number_input("height", 0, 0, 15)
        ),
        skin = components::field("Skin (0-31)", &components::number_input("skin", 0, 0, 31)),
        hair_color = components::field(
            "Hair colour (0-31)",
            &components::number_input("hair_color", 0, 0, 31)
        ),
        face = components::field("Face (0-31)", &components::number_input("face", 0, 0, 31)),
        hair_style = components::field(
            "Hair style (0-63)",
            &components::number_input("hair_style", 3, 0, 63)
        ),
        button = components::submit("Create player"),
    )
}

/// `docs/game-rules.md` R65: a first and last name made of English letters.
pub fn valid_character_name(name: &str) -> bool {
    let words: Vec<_> = name.split(' ').collect();
    (3..=19).contains(&name.len())
        && words.len() >= 2
        && words
            .iter()
            .all(|word| !word.is_empty() && word.bytes().all(|c| c.is_ascii_alphabetic()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_name_is_the_ledgers_own_format() {
        assert!(valid_character_name("Asura Warrior"));
        assert!(!valid_character_name("Asura"));
        assert!(!valid_character_name("Asura  Warrior"));
        assert!(!valid_character_name("Asura Warrior2"));
        assert!(!valid_character_name("An Excessively Long Character"));
    }
}
