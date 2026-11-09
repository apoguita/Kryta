//! Content behaviour, written against hooks.
//!
//! Reference: `src/server/scripts/`. Two pieces are the structure: `ScriptMgr`, the
//! registry of hooks, and `AddScripts`, the list of what is written against them. The
//! engine never names a quest, a district or a boss; content does, here and in `modules/`.

#[path = "ScriptMgr/mod.rs"]
pub mod script_mgr;

use script_mgr::ScriptMgr;

/// The list of scripts this build carries.
///
/// Reference: `AddScripts`, handed to `sScriptMgr->SetScriptLoader`. Empty until the
/// script layer exists; a module adds its own through `add_modules_scripts`.
pub fn add_scripts(_mgr: &mut ScriptMgr) {}

/// The list of module scripts this build carries.
///
/// Reference: `AddModulesScripts`, handed to `sScriptMgr->SetModulesLoader`.
pub fn add_modules_scripts(_mgr: &mut ScriptMgr) {}
