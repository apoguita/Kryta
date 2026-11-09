//! The library both programs link, translated from the reference's `src/common/`.
//!
//! Ten of the reference's sixteen directories are here. Six are not, and each has a
//! named home or a named reason:
//!
//! - `Asio` does not translate: the runtime is tokio, used directly by `shared/Network/`.
//! - `Collision` and `Navigation` are geometry: they belong to `game/Terrain/`.
//! - `Dynamic` (the Lua bridge) arrives with `scripts/`, when scripts exist.
//! - `DataStores` is `shared/DataStores/`, where this server's structure puts it.
//! - `PrecompiledHeaders` is a C++ build concern and has no Rust equivalent.
//!
//! The directory names are the reference's; the module identifiers are snake_case and
//! the two are joined by an explicit `#[path]`.

#[path = "Configuration/mod.rs"]
pub mod configuration;
#[path = "Cryptography/mod.rs"]
pub mod cryptography;
#[path = "Debugging/mod.rs"]
pub mod debugging;
#[path = "Encoding/mod.rs"]
pub mod encoding;
#[path = "IPLocation/mod.rs"]
pub mod ip_location;
#[path = "Logging/mod.rs"]
pub mod logging;
#[path = "Metric/mod.rs"]
pub mod metric;
#[path = "Platform/mod.rs"]
pub mod platform;
#[path = "Threading/mod.rs"]
pub mod threading;
#[path = "Utilities/mod.rs"]
pub mod utilities;
