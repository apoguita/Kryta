//! Content behaviour, written against hooks.
//!
//! Reference: `src/server/scripts/` - the two functions a program hands the script manager as its
//! loaders (`AddScripts`, `AddModulesScripts`) and the `AddSC_*` functions they call. The manager
//! itself is not here: the reference keeps it in the game library
//! (`src/server/game/Scripting/ScriptMgr.h`), which is also what breaks the `game` <-> `scripts`
//! cycle - the manager is a game symbol, the scripts that fill it are these. So this crate depends on
//! `game` and registers into `game::scripting::ScriptMgr`.
//!
//! The engine never names a quest, a district or a boss; content does, here and in `modules/`.

#[path = "AddModulesScripts.rs"]
mod add_modules_scripts;
#[path = "AddScripts.rs"]
mod add_scripts;

pub use add_modules_scripts::add_modules_scripts;
pub use add_scripts::add_scripts;
