//! One world: what the login is allowed to send a player to.
//!
//! Reference: `src/server/shared/Realms/Realm.h`, renamed for this game: a realm there is the
//! server a player picks, and in Guild Wars that thing is a world.

use std::net::{IpAddr, SocketAddr};

/// The flags a `realmlist` row carries.
///
/// Reference: `enum RealmFlags`, whose numbers are what the client is told about a world.
pub mod flags {
    /// Reference: `REALM_FLAG_NONE`.
    pub const NONE: u8 = 0x00;
    /// Reference: `REALM_FLAG_VERSION_MISMATCH`. The world runs a build the client does not have.
    pub const VERSION_MISMATCH: u8 = 0x01;
    /// Reference: `REALM_FLAG_OFFLINE`. The world is listed and marked down.
    pub const OFFLINE: u8 = 0x02;
    /// Reference: `REALM_FLAG_SPECIFYBUILD`.
    pub const SPECIFY_BUILD: u8 = 0x04;
    /// Reference: `REALM_FLAG_UNK1`.
    pub const UNK1: u8 = 0x08;
    /// Reference: `REALM_FLAG_UNK2`.
    pub const UNK2: u8 = 0x10;
    /// Reference: `REALM_FLAG_RECOMMENDED`.
    pub const RECOMMENDED: u8 = 0x20;
    /// Reference: `REALM_FLAG_NEW`.
    pub const NEW: u8 = 0x40;
    /// Reference: `REALM_FLAG_FULL`.
    pub const FULL: u8 = 0x80;
}

/// One world, as the `realmlist` table describes it.
///
/// Reference: `struct Realm`. The reference keeps resolved `address` objects; here the two
/// addresses are kept as the strings the table holds and as what they resolved to, so a name that
/// does not resolve is visible rather than silently dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Realm {
    /// The row's primary key.
    pub id: u32,
    /// What a player sees.
    pub name: String,
    /// The address a client is told to dial, resolved.
    pub address: IpAddr,
    /// The same address as the table spells it.
    pub address_text: String,
    /// The address a client on the world's own network is told to dial, resolved.
    pub local_address: IpAddr,
    /// The network that counts as local.
    pub local_subnet_mask: String,
    /// The port the world's program listens on.
    pub port: u16,
    /// The icon the client shows.
    pub icon: u8,
    /// The world's flags, from [`flags`].
    pub flags: u8,
    /// The clock the world runs on.
    pub timezone: u8,
    /// The lowest account level this world accepts.
    pub allowed_security_level: u8,
    /// How full the world is, as the client is shown it.
    pub population: f32,
    /// The build the world runs.
    pub build: u32,
}

impl Realm {
    /// Whether this world is listed as up.
    pub fn is_offline(&self) -> bool {
        self.flags & flags::OFFLINE != 0
    }

    /// Whether the world is full.
    pub fn is_full(&self) -> bool {
        self.flags & flags::FULL != 0
    }

    /// Where this world's program listens, as an address to dial.
    pub fn endpoint(&self) -> SocketAddr {
        SocketAddr::new(self.address, self.port)
    }

    /// Where to dial from the same network as the world, when the caller is on it.
    ///
    /// Reference: `Realm::GetAddressForClient`, which answers the local address to a client inside
    /// the subnet and the external one to everybody else. The subnet test needs the mask and the
    /// client's address; a caller that does not know it takes the external address.
    pub fn endpoint_for(external: SocketAddr, local: SocketAddr, is_local: bool) -> SocketAddr {
        if is_local { local } else { external }
    }
}
