//! The world list: every world the login may send a player to.
//!
//! Reference: `src/server/shared/Realms/RealmList.{h,cpp}`. The reference reads the table, resolves
//! each row's addresses, keeps the ones that resolve and reports the ones that do not, and repeats
//! that on `RealmsStateUpdateDelay`. This port reads and resolves the same way; the repeat belongs
//! to the program's run loop, which does not exist yet, so [`WorldList::load`] is what a program
//! calls at start-up and on a timer it owns.

use std::collections::BTreeMap;
use std::net::{IpAddr, ToSocketAddrs};

use common::logging::{Log, LogLevel};

// `Connection` is a concrete type in the database crate.
use database::database::Connection;

use super::realm::Realm;

/// The columns the load reads, in the order the reference reads them.
///
/// Reference: `LOGIN_SEL_REALMLIST`, whose fields the reference's `UpdateRealms` reads in this
/// order, and whose query leaves out a world flagged with both `VERSION_MISMATCH` and `OFFLINE`
/// (the reference's own error message says the query excludes flag 3).
pub const SELECT_REALMS: &str = "SELECT \"id\", \"name\", \"address\", \"localAddress\", \
\"localSubnetMask\", \"port\", \"icon\", \"flag\", \"timezone\", \"allowedSecurityLevel\", \
\"population\", \"gamebuild\" FROM \"realmlist\" \
WHERE (\"flag\" & 3) <> 3 ORDER BY \"id\"";

/// Every world this login knows about.
#[derive(Debug, Default)]
pub struct WorldList {
    worlds: BTreeMap<u32, Realm>,
}

impl WorldList {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the table and rebuild the list.
    ///
    /// Reference: `RealmList::UpdateRealms`. A row whose addresses do not resolve is reported and
    /// left out, which is the reference's own choice: a world a client cannot dial is not a world.
    pub fn load(&mut self, connection: &mut Connection, log: &Log) -> Result<usize, String> {
        let rows = connection.query(SELECT_REALMS)?;

        let mut worlds = BTreeMap::new();

        for row in &rows {
            let id: i32 = row.get(0);
            let name: String = row.get(1);
            let address_text: String = row.get(2);
            let local_text: String = row.get(3);
            let subnet: String = row.get(4);
            let port: i16 = row.get(5);
            let icon: i16 = row.get(6);
            let flag: i16 = row.get(7);
            let timezone: i16 = row.get(8);
            let security: i16 = row.get(9);
            let population: f32 = row.get(10);
            let build: i32 = row.get(11);

            let Ok(address) = resolve(&address_text) else {
                log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!(
                        "Could not resolve address {address_text} for world \"{name}\" id {id}"
                    ),
                );
                continue;
            };

            let Ok(local_address) = resolve(&local_text) else {
                log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!(
                        "Could not resolve localAddress {local_text} for world \"{name}\" id {id}"
                    ),
                );
                continue;
            };

            worlds.insert(
                id as u32,
                Realm {
                    id: id as u32,
                    name,
                    address,
                    address_text,
                    local_address,
                    local_subnet_mask: subnet,
                    port: port as u16,
                    icon: icon as u8,
                    flags: flag as u8,
                    timezone: timezone as u8,
                    allowed_security_level: security as u8,
                    population,
                    build: build as u32,
                },
            );
        }

        let count = worlds.len();
        self.worlds = worlds;
        Ok(count)
    }

    /// Every world, by id.
    pub fn worlds(&self) -> &BTreeMap<u32, Realm> {
        &self.worlds
    }

    /// Put one world in the list, replacing any world with the same id.
    pub fn insert(&mut self, realm: Realm) {
        self.worlds.insert(realm.id, realm);
    }

    /// Read a world list from a settings file and merge it over the table.
    ///
    /// A world's address is the operator's choice, so it can be set in `data/realms.json` as well as
    /// in the `realmlist` table: a file row with an id the table already holds overrides it field by
    /// field, and a row with a new id is added. A missing file is nothing to merge, not an error.
    ///
    /// Reference: there is no file in the reference - its world list is the table alone. This is the
    /// settings rule applied to it (`docs/port/04-configuration.md`): the table is the reference's
    /// home for the list, and the file is where an operator writes the same settings down.
    pub fn merge_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
        log: &Log,
    ) -> Result<usize, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(0);
        }

        let document =
            common::configuration::json::Document::read(path).map_err(|error| error.to_string())?;
        let records = document.records("realms")?;

        let mut merged = 0usize;
        for record in records {
            let id: u32 = record
                .required("id")?
                .parse()
                .map_err(|_| format!("{}: 'id' is not a number", record.at()))?;

            // A row the table already holds is the starting point, so a file states only what it
            // changes; a new id starts from the reference's own defaults.
            let mut realm = self.worlds.get(&id).cloned().unwrap_or(Realm {
                id,
                name: String::new(),
                address: IpAddr::from([127, 0, 0, 1]),
                address_text: "127.0.0.1".to_string(),
                local_address: IpAddr::from([127, 0, 0, 1]),
                local_subnet_mask: "255.255.255.0".to_string(),
                port: 8085,
                icon: 0,
                flags: 0,
                timezone: 0,
                allowed_security_level: 0,
                population: 0.0,
                build: 0,
            });

            if let Some(text) = record.text("name")? {
                realm.name = text;
            }
            if let Some(text) = record.text("address")? {
                realm.address = resolve(&text)
                    .map_err(|_| format!("{}: 'address' {text} does not resolve", record.at()))?;
                realm.address_text = text;
            }
            if let Some(text) = record.text("localAddress")? {
                realm.local_address = resolve(&text).map_err(|_| {
                    format!("{}: 'localAddress' {text} does not resolve", record.at())
                })?;
            }
            if let Some(text) = record.text("localSubnetMask")? {
                realm.local_subnet_mask = text;
            }
            if let Some(text) = record.text("port")? {
                realm.port = number(&text, record.at(), "port")? as u16;
            }
            if let Some(text) = record.text("icon")? {
                realm.icon = number(&text, record.at(), "icon")? as u8;
            }
            if let Some(text) = record.text("flag")? {
                realm.flags = number(&text, record.at(), "flag")? as u8;
            }
            if let Some(text) = record.text("timezone")? {
                realm.timezone = number(&text, record.at(), "timezone")? as u8;
            }
            if let Some(text) = record.text("allowedSecurityLevel")? {
                realm.allowed_security_level =
                    number(&text, record.at(), "allowedSecurityLevel")? as u8;
            }
            if let Some(text) = record.text("population")? {
                realm.population = text
                    .parse()
                    .map_err(|_| format!("{}: 'population' {text} is not a number", record.at()))?;
            }
            if let Some(text) = record.text("gamebuild")? {
                realm.build = number(&text, record.at(), "gamebuild")? as u32;
            }

            self.worlds.insert(id, realm);
            merged += 1;
        }

        if merged > 0 {
            log.out(
                LogLevel::Info,
                "server.authserver",
                format!("World list: {merged} row(s) from {}.", path.display()),
            );
        }
        Ok(merged)
    }

    /// One world, by id.
    pub fn get(&self, id: u32) -> Option<&Realm> {
        self.worlds.get(&id)
    }

    /// How many worlds the list holds.
    pub fn len(&self) -> usize {
        self.worlds.len()
    }

    /// Whether the list is empty, which is what a login cannot work with.
    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty()
    }
}

/// Resolve one address, answering the first result.
///
/// Reference: the resolver call in `UpdateRealms`, which asks for IPv4.
fn resolve(text: &str) -> Result<IpAddr, ()> {
    if let Ok(address) = text.parse::<IpAddr>() {
        return Ok(address);
    }

    match (text, 0u16).to_socket_addrs() {
        Ok(mut resolved) => resolved.next().map(|socket| socket.ip()).ok_or(()),
        Err(_) => Err(()),
    }
}

/// One of a file row's numbers, with the row named when it is not one.
fn number(text: &str, at: &str, field: &str) -> Result<u64, String> {
    text.parse()
        .map_err(|_| format!("{at}: '{field}' {text} is not a number"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_literal_address_resolves_to_itself() {
        assert_eq!(resolve("127.0.0.1"), Ok(IpAddr::from([127, 0, 0, 1])));
    }

    #[test]
    fn a_name_that_cannot_exist_does_not_resolve() {
        assert!(resolve("this-name-does-not-exist.invalid").is_err());
    }

    #[test]
    fn a_new_list_is_empty_and_answers_nothing() {
        let list = WorldList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert!(list.get(1).is_none());
    }

    fn temp_file(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-realms-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("realms.json");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_file_row_is_a_world_and_a_new_id_is_added() {
        let log = Log::new();
        let mut list = WorldList::new();
        let path = temp_file(
            "one",
            r#"{"realms": [{"id": 2, "name": "Kryta", "address": "127.0.0.1", "port": 6113}]}"#,
        );

        assert_eq!(list.merge_file(&path, &log).unwrap(), 1);
        let world = list.get(2).expect("the world the file states");
        assert_eq!(world.name, "Kryta");
        assert_eq!(world.port, 6113);
        assert_eq!(world.address, IpAddr::from([127, 0, 0, 1]));

        log.shutdown();
    }

    #[test]
    fn a_file_row_overrides_only_the_fields_it_states() {
        let log = Log::new();
        let mut list = WorldList::new();
        let first = temp_file(
            "base",
            r#"{"realms": [{"id": 2, "name": "Kryta", "address": "127.0.0.1", "port": 6113, "icon": 1}]}"#,
        );
        list.merge_file(&first, &log).unwrap();

        let second = temp_file("override", r#"{"realms": [{"id": 2, "port": 7000}]}"#);
        list.merge_file(&second, &log).unwrap();

        let world = list.get(2).unwrap();
        assert_eq!(world.port, 7000, "the field the file states");
        assert_eq!(world.name, "Kryta", "the fields it leaves alone");
        assert_eq!(world.icon, 1);

        log.shutdown();
    }

    #[test]
    fn a_missing_file_merges_nothing() {
        let log = Log::new();
        let mut list = WorldList::new();
        let missing = std::env::temp_dir().join("kryta-realms-nothing.json");
        assert_eq!(list.merge_file(&missing, &log).unwrap(), 0);
        assert!(list.is_empty());
        log.shutdown();
    }

    #[test]
    fn a_row_without_an_id_names_itself() {
        let log = Log::new();
        let mut list = WorldList::new();
        let path = temp_file("bad", r#"{"realms": [{"name": "no id"}]}"#);
        let error = list.merge_file(&path, &log).unwrap_err();
        assert!(error.contains("realms[1]"), "{error}");
        log.shutdown();
    }
}
