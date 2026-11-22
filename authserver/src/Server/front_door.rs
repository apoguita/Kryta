//! The front door: a gameplay connection arrives here and is copied to the world it names.
//!
//! Measured, not assumed: this client answers the `0x0009` that hands it a world address by opening
//! its gameplay connection **to the address it was launched with** - the auth port - whatever address
//! that message named. So the gameplay connection arrives on the login port, and this program has to
//! hand it on. The reference does the same thing with its control channel; a socket handle cannot
//! cross a process boundary here, so the crossing is a copy
//! (`docs/decisions/0004-gameplay-connections-arrive-at-the-front-door.md`).
//!
//! What this is: a router. It reads the client's 64-byte version message only to learn which world
//! the client names, forwards those bytes **whole and uninterpreted**, and then carries bytes
//! between the client and that world until both are done. What it is not: a decoder and not an
//! authority. The world reads the same 64 bytes itself, compares the token pair against the travel
//! the login recorded, and closes the connection when they do not match.
//!
//! Reference: `AuthSrv_DoPostHandshake` and `CtrlMsgId_TransferConnection`, in the reference's auth
//! service.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use common::logging::{Log, LogLevel};
use game::server::protocol::{GAME_CMSG_VERSION_LEN, GameVersion};
use shared::network::relay::RelayError;
use shared::network::socket::SessionIo;
use shared::worlds::WorldList;

use crate::server::session::{GameServer, read_exactly};

/// How long the front door waits for a world's port before closing the client's connection.
///
/// Reference: the reference's transfer is immediate because it hands over an open socket; a copy has
/// to make a connection of its own, and a client should not wait for the system's connect timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// The worlds this program may hand a client to, and what it does with a connection that arrives for
/// one.
pub struct FrontDoor {
    /// Behind a lock because the list is refreshed on a timer (`RealmsStateUpdateDelay`): a world
    /// that comes back online is handed out again without a restart.
    worlds: Arc<std::sync::Mutex<WorldList>>,
    log: Log,
}

impl FrontDoor {
    /// A front door onto these worlds.
    pub fn new(worlds: Arc<std::sync::Mutex<WorldList>>, log: Log) -> Self {
        Self { worlds, log }
    }

    /// The world a login hands out: the first one that is not marked offline.
    ///
    /// Reference: the reference picks a live game server out of its own registry and answers
    /// `0x0009` with it.
    pub fn pick(&self) -> Option<GameServer> {
        let worlds = self.worlds.lock().ok()?;
        worlds
            .worlds()
            .values()
            .find(|world| !world.is_offline())
            .map(|world| GameServer {
                token: world.id,
                address: world.endpoint(),
            })
    }

    /// Where a gameplay connection naming this token should go.
    ///
    /// The token is the world's `map_token` in the `0x0009` this program sent, which is the world's
    /// id, so the lookup is by id. A world the list does not hold is a world this program never
    /// offered, and the connection is not routed.
    pub fn endpoint_for(&self, map_token: u32) -> Option<SocketAddr> {
        let worlds = self.worlds.lock().ok()?;
        worlds
            .get(map_token)
            .filter(|world| !world.is_offline())
            .map(|world| world.endpoint())
    }

    /// Take a gameplay connection over: read the rest of its version message, find the world, copy.
    ///
    /// `first_four` is what the connection was classified with. It is part of the message, not
    /// separate from it, so it is the head of what is forwarded.
    pub async fn route(&self, io: &mut SessionIo, first_four: [u8; 4]) {
        let peer = io
            .peer()
            .map(|address| address.to_string())
            .unwrap_or_else(|| "an unknown address".to_string());

        let mut message = [0u8; GAME_CMSG_VERSION_LEN];
        message[..4].copy_from_slice(&first_four);
        if let Err(error) = read_exactly(io, &mut message[4..]).await {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer} ended inside its version message: {error}"
                ),
            );
            let _ = io.close().await;
            return;
        }

        let version = match GameVersion::decode(&message) {
            Ok((version, _)) => version,
            Err(error) => {
                self.say(
                    LogLevel::Warn,
                    format!("Gameplay connection from {peer}: {error}; closing it."),
                );
                let _ = io.close().await;
                return;
            }
        };

        self.say(
            LogLevel::Info,
            format!(
                "Gameplay connection from {peer}: build {}, world {}, map {}, player token 0x{:08X} - this is the client asking for the world it was told about.",
                version.version, version.map_token, version.map_id, version.player_token
            ),
        );

        let Some(target) = self.endpoint_for(version.map_token) else {
            self.say(
                LogLevel::Error,
                format!(
                    "Gameplay connection from {peer} names world {}, which is not in the world list; closing it. A world is added in data/realms.json or the realmlist table.",
                    version.map_token
                ),
            );
            let _ = io.close().await;
            return;
        };

        match io.relay_to(target, &message, CONNECT_TIMEOUT).await {
            Ok(relayed) => self.say(
                LogLevel::Info,
                format!(
                    "Gameplay connection from {peer}: world {} at {target}, {} byte(s) in, {} out, until the client left.",
                    version.map_token, relayed.to_world, relayed.from_world
                ),
            ),
            Err(RelayError::Unreachable(reason)) => self.say(
                LogLevel::Error,
                format!(
                    "Gameplay connection from {peer}: world {} at {target} did not answer ({reason}); the client is closed rather than left waiting.",
                    version.map_token
                ),
            ),
            Err(RelayError::Broken(reason)) => self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: the copy to world {} at {target} ended early: {reason}",
                    version.map_token
                ),
            ),
        }
        let _ = io.close().await;
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        self.log.out(level, "server.authserver", text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::worlds::Realm;

    fn realm(id: u32, port: u16, flags: u8) -> Realm {
        Realm {
            id,
            name: format!("World {id}"),
            address: "127.0.0.1".parse().unwrap(),
            address_text: "127.0.0.1".to_string(),
            local_address: "127.0.0.1".parse().unwrap(),
            local_subnet_mask: "255.255.255.0".to_string(),
            port,
            icon: 0,
            flags,
            timezone: 0,
            allowed_security_level: 0,
            population: 0.0,
            build: 39167,
        }
    }

    fn door(realms: Vec<Realm>) -> FrontDoor {
        let mut worlds = WorldList::new();
        for realm in realms {
            worlds.insert(realm);
        }
        FrontDoor::new(Arc::new(std::sync::Mutex::new(worlds)), Log::new())
    }

    #[test]
    fn the_world_a_login_hands_out_is_the_first_one_that_is_not_offline() {
        let door = door(vec![realm(1, 6000, 0x02), realm(2, 6113, 0x00)]);
        let picked = door.pick().expect("a world is up");
        assert_eq!(picked.token, 2);
        assert_eq!(picked.address.port(), 6113);
    }

    #[test]
    fn a_token_is_routed_to_the_world_that_owns_it() {
        let door = door(vec![realm(2, 6113, 0x00), realm(3, 7000, 0x00)]);
        assert_eq!(door.endpoint_for(2).unwrap().port(), 6113);
        assert_eq!(door.endpoint_for(3).unwrap().port(), 7000);
    }

    #[test]
    fn a_token_the_list_does_not_hold_is_not_routed() {
        let door = door(vec![realm(2, 6113, 0x00)]);
        assert!(door.endpoint_for(99).is_none());
    }

    #[test]
    fn an_offline_world_is_neither_handed_out_nor_routed_to() {
        let door = door(vec![realm(2, 6113, 0x02)]);
        assert!(door.pick().is_none());
        assert!(door.endpoint_for(2).is_none());
    }

    #[test]
    fn nothing_is_offered_when_the_list_is_empty() {
        let door = door(Vec::new());
        assert!(door.pick().is_none());
    }
}
