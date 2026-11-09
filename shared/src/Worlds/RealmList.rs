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
                    format!("Could not resolve address {address_text} for world \"{name}\" id {id}"),
                );
                continue;
            };

            let Ok(local_address) = resolve(&local_text) else {
                log.out(
                    LogLevel::Error,
                    "server.authserver",
                    format!("Could not resolve localAddress {local_text} for world \"{name}\" id {id}"),
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
}
