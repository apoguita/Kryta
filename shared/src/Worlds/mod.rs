//! The world list: the reference's Realms/, renamed for this game.
//!
//! Reference: `src/server/shared/Realms/{Realm,RealmList}.{h,cpp}`. A realm there is the server a
//! player picks; in Guild Wars that thing is a world, and the table it comes from is `realmlist`.

#[path = "Realm.rs"]
pub mod realm;
#[path = "RealmList.rs"]
pub mod realm_list;

pub use realm::{flags, Realm};
pub use realm_list::WorldList;