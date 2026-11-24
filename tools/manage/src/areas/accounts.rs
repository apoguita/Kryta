//! Accounts: the login identities the server hosts.
//!
//! An account owns its players (the characters) and the state that belongs to the person. Every
//! read and change goes through `game::accounts::DatabaseAccounts`, the repository the login uses.

use serde_json::{Value, json};

use crate::api::{Call, Failure, Reply, Route, bad, broken, conflict, get, missing, post};
use crate::state::Manage;

/// The EULA version recorded when an operator marks an account as having accepted. The panel has
/// always written this number (`tools/manage/src/pages/accounts.rs` before the rebuild).
const ACCEPTED_EULA: i64 = 26;

/// The accounts area's routes.
pub fn routes() -> Vec<Route<Manage>> {
    vec![
        get("/api/v1/accounts", list),
        post("/api/v1/accounts", create),
        get("/api/v1/accounts/{id}", detail),
        get("/api/v1/accounts/{id}/token", token),
        post("/api/v1/accounts/{id}/update", update),
        post("/api/v1/accounts/{id}/active", set_active),
        post("/api/v1/accounts/{id}/eula", set_eula),
        post("/api/v1/accounts/{id}/delete", delete),
    ]
}

/// A portal token as a list shows it: hidden, with its last four characters when it is long enough
/// for those to identify it without giving it away.
pub fn mask_token(token: &str) -> String {
    let length = token.chars().count();
    if length == 0 {
        return String::new();
    }
    if length < 12 {
        return "********".to_string();
    }
    let tail: String = token.chars().skip(length - 4).collect();
    format!("********{tail}")
}

/// One account as the API states it. The token is never in it; the `token` route reveals one.
fn account_json(account: &game::accounts::Account, players: usize) -> Value {
    json!({
        "id": account.id,
        "username": account.username,
        "user_id": account.user_id,
        "token_masked": mask_token(&account.token),
        "eula": account.eula,
        "active": account.active,
        "players": players,
    })
}

fn table_error(error: String) -> Failure {
    broken(format!("The account table said: {error}"))
}

/// The row with this id.
pub fn find(manage: &Manage, id: i64) -> Result<game::accounts::Account, Failure> {
    manage
        .accounts()
        .list()
        .map_err(table_error)?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| missing(format!("No account has id {id}.")))
}

/// `GET /api/v1/accounts`: every account with its player count.
fn list(manage: &Manage, _: &Call) -> Reply {
    let accounts = manage.accounts().list().map_err(table_error)?;
    let counts = manage
        .characters()
        .ok()
        .and_then(|characters| characters.counts_by_account().ok())
        .unwrap_or_default();
    let rows: Vec<Value> = accounts
        .iter()
        .map(|account| account_json(account, counts.get(&account.user_id).copied().unwrap_or(0)))
        .collect();
    Ok(json!({ "accounts": rows }))
}

/// `GET /api/v1/accounts/{id}`: one account and the players it owns.
fn detail(manage: &Manage, call: &Call) -> Reply {
    let account = find(manage, call.id(0)?)?;
    let players = manage.characters().ok().map(|characters| {
        let names = manage.map_names();
        characters
            .for_account(&account.user_id)
            .iter()
            .map(|character| super::players::summary(character, &names))
            .collect::<Vec<Value>>()
    });
    let count = players.as_ref().map_or(0, Vec::len);
    Ok(json!({
        "account": account_json(&account, count),
        "players": players,
    }))
}

/// `GET /api/v1/accounts/{id}/token`: the portal token in clear, when the operator asks for it.
fn token(manage: &Manage, call: &Call) -> Reply {
    let account = find(manage, call.id(0)?)?;
    Ok(json!({ "token": account.token }))
}

/// `POST /api/v1/accounts`: record a new login identity. A blank portal id or token is generated.
fn create(manage: &Manage, call: &Call) -> Reply {
    let username = call.text("username");
    if username.is_empty() {
        return Err(bad("An account needs a login name."));
    }
    let or_generated = |value: String| if value.is_empty() { generated_id() } else { value };
    let user_id = or_generated(call.text("user_id"));
    let token = or_generated(call.text("token"));
    match manage.accounts().create(&username, &user_id, &token) {
        Ok(true) => {
            manage.say(format!("{username}: account created"));
            let created = manage
                .accounts()
                .find(&username)
                .map_err(table_error)?
                .ok_or_else(|| broken("The account was created and could not be read back."))?;
            Ok(json!({ "account": account_json(&created, 0) }))
        }
        Ok(false) => Err(conflict(format!("An account is already called {username}."))),
        Err(error) => Err(table_error(error)),
    }
}

/// `POST /api/v1/accounts/{id}/update`: change the values the portal login checks. A blank token
/// keeps the one the row has, so the page never needs the token to save a name.
fn update(manage: &Manage, call: &Call) -> Reply {
    let id = call.id(0)?;
    let current = find(manage, id)?;
    let username = call.text("username");
    if username.is_empty() {
        return Err(bad("An account login name is required."));
    }
    let user_id = call.text("user_id");
    if user_id.is_empty() {
        return Err(bad("A portal user id is required."));
    }
    let token = match call.text("token") {
        blank if blank.is_empty() => current.token,
        stated => stated,
    };
    match manage.accounts().update(id, &username, &user_id, &token) {
        Ok(true) => {
            manage.say(format!("account {id}: updated as {username}"));
            Ok(json!({ "id": id }))
        }
        Ok(false) => Err(missing(format!("No account has id {id}."))),
        Err(error) => Err(table_error(error)),
    }
}

/// `POST /api/v1/accounts/{id}/active`: admit or refuse future logins, keeping the row and players.
fn set_active(manage: &Manage, call: &Call) -> Reply {
    let id = call.id(0)?;
    let active = call
        .flag("active")
        .ok_or_else(|| bad("State whether the account is active: true or false."))?;
    match manage.accounts().set_active(id, active) {
        Ok(true) => {
            let state = if active { "activated" } else { "deactivated" };
            manage.say(format!("account {id}: {state}"));
            Ok(json!({ "id": id, "active": active }))
        }
        Ok(false) => Err(missing(format!("No account has id {id}."))),
        Err(error) => Err(table_error(error)),
    }
}

/// `POST /api/v1/accounts/{id}/eula`: record that the client accepted the EULA, or make it ask
/// again. The column is the version the login answer carries back; zero makes the client ask.
fn set_eula(manage: &Manage, call: &Call) -> Reply {
    let id = call.id(0)?;
    let accepted = call
        .flag("accepted")
        .ok_or_else(|| bad("State whether the EULA is accepted: true or false."))?;
    let account = find(manage, id)?;
    let version = if accepted { ACCEPTED_EULA } else { 0 };
    match manage.accounts().set_eula(&account.username, version) {
        Ok(true) => {
            manage.say(format!("account {id}: the EULA version is now {version}"));
            Ok(json!({ "id": id, "eula": version }))
        }
        Ok(false) => Err(missing("The account disappeared while it was read.")),
        Err(error) => Err(table_error(error)),
    }
}

/// `POST /api/v1/accounts/{id}/delete`: remove the login identity only; player records are kept.
fn delete(manage: &Manage, call: &Call) -> Reply {
    let account = find(manage, call.id(0)?)?;
    match manage.accounts().delete(&account.username) {
        Ok(true) => {
            manage.say(format!(
                "{}: account removed (its players are untouched)",
                account.username
            ));
            Ok(json!({ "id": account.id }))
        }
        Ok(false) => Err(missing("The account is already gone.")),
        Err(error) => Err(table_error(error)),
    }
}

/// A portal identifier for a field the form left blank: the shape the account table already holds.
fn generated_id() -> String {
    let mut bytes = [0u8; 8];
    let number = match shared::cryptography::handshake::random_into(&mut bytes) {
        Ok(()) => u64::from_le_bytes(bytes),
        Err(_) => 1,
    };
    format!("00000000-0000-0000-0000-{:012}", number % 1_000_000_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_portal_id_keeps_the_tables_shape() {
        let id = generated_id();
        assert!(id.starts_with("00000000-0000-0000-0000-"));
        assert_eq!(id.len(), 36);
    }

    #[test]
    fn a_portal_token_is_never_listed_in_clear() {
        assert_eq!(mask_token(""), "");
        assert_eq!(mask_token("short"), "********");
        let masked = mask_token("00000000-0000-0000-0000-000000001234");
        assert_eq!(masked, "********1234");
        let account = game::accounts::Account {
            id: 3,
            username: "a@b".into(),
            user_id: "u".into(),
            token: "a-very-secret-portal-token".into(),
            eula: 0,
            active: true,
        };
        let text = account_json(&account, 2).to_string();
        assert!(!text.contains("a-very-secret"));
        assert!(text.contains("\"players\":2"));
    }
}
