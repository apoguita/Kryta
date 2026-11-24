//! Players: the characters the server hosts, and one player with the data that belongs to it.
//!
//! A player (a character) belongs to one account. Every read and change goes through
//! `game::characters::DatabaseCharacters`, the repository the login and the world use.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::api::{Call, Failure, Reply, Route, bad, broken, get, missing, post};
use crate::state::Manage;
use crate::view;

/// The players area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/players", list),
        post("/api/v1/players", create),
        get("/api/v1/players/{uuid}", detail),
        post("/api/v1/players/{uuid}/rename", rename),
        post("/api/v1/players/{uuid}/level", set_level),
        post("/api/v1/players/{uuid}/delete", delete),
    ]
}

/// Whether a character's item rows can be loaded by a client.
///
/// An item row whose `item_type` is 0 names no kind of item. Rows like that crashed the client when
/// the character was loaded (owner, 2026-11-23), so a character holding one is reported as unsound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Soundness {
    /// How many item rows the character has.
    pub items: usize,
    /// How many of them have `item_type` 0.
    pub untyped: usize,
}

impl Soundness {
    /// Check a character's rows by their `item_type` values.
    pub fn of(item_types: impl IntoIterator<Item = u8>) -> Self {
        let mut soundness = Self { items: 0, untyped: 0 };
        for item_type in item_types {
            soundness.items += 1;
            if item_type == 0 {
                soundness.untyped += 1;
            }
        }
        soundness
    }

    /// Whether every row has a type.
    pub fn sound(&self) -> bool {
        self.untyped == 0
    }

    /// The check as the API states it.
    pub fn json(&self) -> Value {
        let text = if self.sound() {
            "sound".to_string()
        } else {
            format!("{} of {} item rows have no type", self.untyped, self.items)
        };
        json!({ "items": self.items, "untyped": self.untyped, "sound": self.sound(), "text": text })
    }
}

/// The soundness of one character's rows, or `None` when they could not be read.
pub fn soundness_of(
    characters: &game::characters::DatabaseCharacters,
    uuid: &str,
) -> Option<Soundness> {
    let items = characters.items_of(uuid).ok()?;
    Some(Soundness::of(items.iter().map(|item| item.item_type)))
}

/// One character as a roster lists it.
pub fn summary(character: &game::characters::Character, names: &HashMap<u32, String>) -> Value {
    let settings = &character.settings;
    json!({
        "uuid": game::characters::uuid_text(&character.uuid),
        "name": character.name,
        "campaign": view::campaign_label(settings.campaign),
        "professions": view::professions(
            view::primary_profession(settings.appearance),
            settings.secondary_profession,
        ),
        "level": settings.level,
        "outpost": view::outpost_label(u32::from(settings.last_outpost), names),
        "pvp": settings.is_pvp,
    })
}

fn database_error(error: String) -> Failure {
    broken(format!("The character database said: {error}"))
}

/// The character with this id, and the portal id of the account that owns it.
fn find(
    characters: &game::characters::DatabaseCharacters,
    uuid: &str,
) -> Result<(String, game::characters::Character), Failure> {
    characters
        .all_with_account()
        .into_iter()
        .find(|(_, character)| game::characters::uuid_text(&character.uuid) == uuid)
        .ok_or_else(|| missing("No player has that id."))
}

/// What the creation form offers: the campaigns a role-playing character may choose, the
/// professions, and the appearance ranges the appearance word can hold.
fn choices() -> Value {
    let named = |pairs: &[(u8, &str)]| -> Vec<Value> {
        pairs
            .iter()
            .map(|(id, name)| json!({ "id": id, "name": name }))
            .collect()
    };
    json!({
        "campaigns": named(&view::CAMPAIGNS[1..=3]),
        "professions": named(&view::PROFESSIONS[1..]),
        "appearance": [
            { "name": "height", "label": "Height", "max": 15, "value": 0 },
            { "name": "skin", "label": "Skin", "max": 31, "value": 0 },
            { "name": "hair_color", "label": "Hair colour", "max": 31, "value": 0 },
            { "name": "face", "label": "Face", "max": 31, "value": 0 },
            { "name": "hair_style", "label": "Hair style", "max": 63, "value": 3 },
        ],
    })
}

/// `GET /api/v1/players`: every character with its account and its data check.
fn list(manage: &Manage, _: &Call) -> Reply {
    let accounts = manage
        .accounts()
        .list()
        .map_err(|error| broken(format!("The account table said: {error}")))?;
    let account_rows: Vec<Value> = accounts
        .iter()
        .map(|account| json!({ "id": account.id, "username": account.username }))
        .collect();
    let Ok(characters) = manage.characters() else {
        return Ok(json!({
            "configured": false, "players": [], "accounts": account_rows, "choices": choices(),
        }));
    };
    let names = manage.map_names();
    let players: Vec<Value> = characters
        .all_with_account()
        .iter()
        .map(|(account_uuid, character)| {
            let mut row = summary(character, &names);
            let owner = accounts.iter().find(|account| &account.user_id == account_uuid);
            let uuid = game::characters::uuid_text(&character.uuid);
            row["account_id"] = json!(owner.map(|account| account.id));
            row["account"] = json!(owner.map(|account| account.username.clone()));
            row["data"] = json!(soundness_of(characters, &uuid).map(|check| check.json()));
            row
        })
        .collect();
    Ok(json!({
        "configured": true, "players": players, "accounts": account_rows, "choices": choices(),
    }))
}

/// `GET /api/v1/players/{uuid}`: one player's identity, position, items and unlocked maps.
fn detail(manage: &Manage, call: &Call) -> Reply {
    let uuid = call.param(0);
    let characters = manage.characters()?;
    let (account_uuid, character) = find(characters, uuid)?;
    let owner = manage
        .accounts()
        .list()
        .unwrap_or_default()
        .into_iter()
        .find(|account| account.user_id == account_uuid);
    let names = manage.map_names();
    let sheet = characters.sheet(uuid, character.uuid);
    let appearance = character.settings.appearance;

    let mut player = summary(&character, &names);
    player["primary"] = json!(view::profession_label(view::primary_profession(appearance)));
    player["experience"] = json!(characters.experience(uuid).ok().flatten());
    player["gold"] = json!(sheet.gold);
    player["sex"] = json!(view::sex_label(appearance));
    player["appearance"] = view::appearance_fields(appearance)[1..]
        .iter()
        .map(|(name, value)| json!({ "name": name, "value": value }))
        .collect();
    player["unlocked_professions"] = view::PROFESSIONS
        .iter()
        .filter(|(id, _)| *id > 0 && (sheet.unlocked_professions >> id) & 1 == 1)
        .map(|(_, name)| json!(name))
        .collect();
    player["position"] = json!(characters.position(uuid).ok().flatten().map(|position| json!({
        "x": position.x,
        "y": position.y,
        "plane": position.plane,
        "map": view::outpost_label(position.map_id, &names),
    })));

    let (items, items_error) = match characters.items_of(uuid) {
        Ok(items) => (items, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    let unlocks: Vec<Value> = characters
        .unlocked_maps(uuid)
        .unwrap_or_default()
        .iter()
        .map(|map_id| json!({ "map_id": map_id, "name": names.get(map_id) }))
        .collect();
    Ok(json!({
        "player": player,
        "account": owner.map(|account| json!({ "id": account.id, "username": account.username })),
        "data": Soundness::of(items.iter().map(|item| item.item_type)).json(),
        "items": items.iter().map(item_json).collect::<Vec<Value>>(),
        "items_error": items_error,
        "unlocks": unlocks,
    }))
}

/// One item row as the player page lists it.
fn item_json(item: &game::characters::HeldItem) -> Value {
    json!({
        "name": view::item_name(&item.name).trim(),
        "item_id": item.item_id,
        "item_type": item.item_type,
        "bag": item.bag_id,
        "slot": item.slot,
        "quantity": item.quantity,
        "file_id": item.file_id,
        "model_id": item.model_id,
        "value": item.value,
    })
}

/// What the creation form states about a new character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCharacter {
    /// The name the client will show.
    pub name: String,
    /// The campaign, in the client's numbering.
    pub campaign: u8,
    /// The primary profession, in the client's numbering.
    pub profession: u8,
    /// Sex, height, skin, hair colour, face, hair style, as the appearance word packs them.
    pub look: [u32; 6],
}

impl NewCharacter {
    /// Read the form, with the ranges the appearance word can hold.
    fn read(call: &Call) -> Result<Self, Failure> {
        Ok(Self {
            name: call.text("name"),
            campaign: call.number("campaign", 3)? as u8,
            profession: call.number("profession", 10)? as u8,
            look: [
                call.number("sex", 1)?,
                call.number("height", 15)?,
                call.number("skin", 31)?,
                call.number("hair_color", 31)?,
                call.number("face", 31)?,
                call.number("hair_style", 63)?,
            ],
        })
    }
}

/// `POST /api/v1/players`: write a player for the account the form names.
fn create(manage: &Manage, call: &Call) -> Reply {
    let account_id = call
        .text("account")
        .parse::<i64>()
        .map_err(|_| bad("A player is created for one account: select its account."))?;
    let account = super::accounts::find(manage, account_id)?;
    let new = NewCharacter::read(call)?;
    let uuid = create_character_duplicating_the_server(manage, &account.user_id, &new).map_err(bad)?;
    manage.say(format!("account {account_id}: created player {uuid}"));
    Ok(json!({ "uuid": uuid }))
}

/// Create a character row with its starting items and its first unlocked map.
///
/// **This duplicates the server's character creation and is to be replaced by the server's own**
/// (`docs/port/23-control-panel.md`, "A second character creation"). It validates the name, the
/// campaign and the profession, packs the appearance word, reads `character_spawn.json` and writes
/// the rows with its own code, where the world does the same at a creation confirm. Until the
/// server's creation can be called from here, every rule this function states is kept in this one
/// place (with [`appearance_word`] and [`starting_map`]) and nowhere else in the panel.
fn create_character_duplicating_the_server(
    manage: &Manage,
    account_uuid: &str,
    new: &NewCharacter,
) -> Result<String, String> {
    let characters = manage
        .characters()
        .map_err(|_| "The character database is not configured.".to_string())?;
    let appearance = appearance_word(new)?;
    let tables = manage.creation_tables()?;
    if tables.set(new.campaign, new.profession).is_none() {
        return Err("The world pack has no equipment for this choice.".into());
    }
    let map = starting_map(manage, new.campaign)?;

    // The item definitions are the creation content's own, as the world builds them at start-up
    // (`worldserver/src/main.rs`, `definitions_as_templates`): the pack says what an item is.
    let templates = game::data_stores::definitions_as_templates(tables.definitions());
    let mut id = [0u8; 16];
    getrandom::getrandom(&mut id).map_err(|error| format!("Cannot create a player id: {error}"))?;
    let uuid = game::characters::uuid_text(&id);
    if !characters.insert_creation(
        &uuid,
        account_uuid,
        &new.name,
        appearance,
        new.profession,
        new.campaign,
        map,
    )? {
        return Err("That player name is already in use.".into());
    }
    let initialized = characters
        .write_starting_items(&uuid, &tables.inventory(new.campaign, new.profession), &templates)
        .and_then(|_| characters.unlock_map(&uuid, u32::from(map)).map(|_| ()));
    if let Err(error) = initialized {
        return match characters.remove(&uuid) {
            Ok(_) => Err(format!("Creation failed and was rolled back: {error}")),
            Err(cleanup) => Err(format!(
                "Creation failed: {error}; cleanup failed for {uuid}: {cleanup}"
            )),
        };
    }
    Ok(uuid)
}

/// Part of the duplicated creation: validate the choice and pack the client's appearance word
/// (`docs/game-rules.md` E74 for the bit positions, R65 for the name).
pub fn appearance_word(new: &NewCharacter) -> Result<u32, String> {
    if !valid_character_name(&new.name) {
        return Err("Name must be 3 to 19 English letters, a first name and a surname.".into());
    }
    if new.campaign == 0 || new.campaign > 3 {
        return Err("PvP creation is unavailable: the world pack names no PvP starting map.".into());
    }
    let profession = new.profession;
    let valid = (1..=6).contains(&profession)
        || (new.campaign == 2 && (7..=8).contains(&profession))
        || (new.campaign == 3 && (9..=10).contains(&profession));
    if !valid {
        return Err("This profession is unavailable in the selected campaign.".into());
    }
    let [sex, height, skin, hair_color, face, hair_style] = new.look;
    if sex > 1 || height > 15 || skin > 31 || hair_color > 31 || face > 31 || hair_style > 63 {
        return Err("An appearance value is outside the range the client packs.".into());
    }
    Ok(sex
        | (height << 1)
        | (skin << 5)
        | (hair_color << 10)
        | (face << 15)
        | (u32::from(profession) << 20)
        | (hair_style << 24)
        | (u32::from(new.campaign) << 30))
}

/// Part of the duplicated creation: the map the pack's `character_spawn.json` names for a
/// campaign, when the pack can host it.
fn starting_map(manage: &Manage, campaign: u8) -> Result<u16, String> {
    let pack_path = manage.world_pack_path()?;
    let spawn_path = std::path::Path::new(&pack_path).join("character_spawn.json");
    let bytes =
        std::fs::read(&spawn_path).map_err(|error| format!("{}: {error}", spawn_path.display()))?;
    let spawn_data: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let map = spawn_data["spawns"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["campaign"].as_u64() == Some(u64::from(campaign)))
        })
        .and_then(|row| row["map_id"].as_u64())
        .and_then(|id| u16::try_from(id).ok())
        .ok_or("The world pack names no starting map for this campaign.")?;
    let (maps, _) = manage.world_pack()?.load_maps()?;
    if !maps
        .iter()
        .any(|entry| entry.id == u32::from(map) && entry.is_hostable())
    {
        return Err(format!("The world pack cannot host starting map {map}."));
    }
    Ok(map)
}

/// `POST /api/v1/players/{uuid}/rename`: rename one player.
fn rename(manage: &Manage, call: &Call) -> Reply {
    let uuid = call.param(0);
    let name = call.text("name");
    if !valid_character_name(&name) {
        return Err(bad(
            "Name must be 3 to 19 English letters, a first name and a surname.",
        ));
    }
    let characters = manage.characters()?;
    let (account_uuid, _) = find(characters, uuid)?;
    match characters.rename_for_account(&account_uuid, uuid, &name) {
        Ok(true) => {
            manage.say(format!("player {uuid}: renamed to {name}"));
            Ok(json!({ "uuid": uuid, "name": name }))
        }
        Ok(false) => Err(missing("The player is already gone.")),
        Err(error) => Err(database_error(error)),
    }
}

/// `POST /api/v1/players/{uuid}/level`: set a level in the game's own range (R48).
fn set_level(manage: &Manage, call: &Call) -> Reply {
    let uuid = call.param(0);
    let level = call
        .text("level")
        .parse::<u8>()
        .ok()
        .filter(|level| (1..=20).contains(level))
        .ok_or_else(|| bad("A level is a number from 1 to 20, the game's own range (R48)."))?;
    let characters = manage.characters()?;
    find(characters, uuid)?;
    match characters.set_level(uuid, level) {
        Ok(true) => {
            manage.say(format!("player {uuid}: level {level}"));
            Ok(json!({ "uuid": uuid, "level": level }))
        }
        Ok(false) => Err(missing("The player is already gone.")),
        Err(error) => Err(database_error(error)),
    }
}

/// `POST /api/v1/players/{uuid}/delete`: remove the player and the records that are its own.
fn delete(manage: &Manage, call: &Call) -> Reply {
    let uuid = call.param(0);
    let characters = manage.characters()?;
    find(characters, uuid)?;
    match characters.remove(uuid) {
        Ok(true) => {
            manage.say(format!("player {uuid}: removed, with its items and unlocks"));
            Ok(json!({ "uuid": uuid }))
        }
        Ok(false) => Err(missing("The player is already gone.")),
        Err(error) => Err(database_error(error)),
    }
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

    fn new(name: &str, campaign: u8, profession: u8) -> NewCharacter {
        NewCharacter {
            name: name.to_string(),
            campaign,
            profession,
            look: [1, 7, 3, 9, 12, 33],
        }
    }

    #[test]
    fn a_character_name_is_the_ledgers_own_format() {
        assert!(valid_character_name("Asura Warrior"));
        assert!(!valid_character_name("Asura"));
        assert!(!valid_character_name("Asura  Warrior"));
        assert!(!valid_character_name("Asura Warrior2"));
        assert!(!valid_character_name("An Excessively Long Character"));
    }

    #[test]
    fn a_character_with_an_untyped_item_row_is_unsound() {
        let sound = Soundness::of([3u8, 7, 24]);
        assert!(sound.sound());
        assert_eq!(sound.json()["text"], "sound");

        let broken = Soundness::of([0u8, 0, 5]);
        assert!(!broken.sound());
        assert_eq!((broken.items, broken.untyped), (3, 2));
        assert_eq!(broken.json()["text"], "2 of 3 item rows have no type");
        assert_eq!(broken.json()["sound"], false);

        // A character that carries nothing has nothing wrong with it.
        assert!(Soundness::of([]).sound());
    }

    #[test]
    fn the_appearance_word_is_packed_as_the_page_reads_it_back() {
        let word = appearance_word(&new("Test Subject", 1, 3)).expect("a valid choice");
        assert_eq!(view::primary_profession(word), 3);
        assert_eq!(word >> 30, 1);
        let fields = view::appearance_fields(word);
        assert_eq!(
            fields.map(|(_, value)| value),
            [1, 7, 3, 9, 12, 33],
            "sex, height, skin, hair colour, face, hair style"
        );
    }

    #[test]
    fn a_creation_refuses_what_the_old_form_refused() {
        assert!(appearance_word(&new("Solo", 1, 1)).is_err());
        assert!(appearance_word(&new("Test Subject", 0, 1)).is_err());
        // Assassin and Ritualist are Factions'; Paragon and Dervish are Nightfall's.
        assert!(appearance_word(&new("Test Subject", 1, 7)).is_err());
        assert!(appearance_word(&new("Test Subject", 2, 7)).is_ok());
        assert!(appearance_word(&new("Test Subject", 2, 9)).is_err());
        assert!(appearance_word(&new("Test Subject", 3, 10)).is_ok());
        let mut tall = new("Test Subject", 1, 1);
        tall.look[1] = 16;
        assert!(appearance_word(&tall).is_err());
    }
}
