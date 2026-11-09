//! The hook registry.
//!
//! Reference: `src/server/scripts/ScriptMgr.h`. Its shape is what content is written
//! against: the engine calls a hook, the hook calls whatever registered for it. Not
//! ported yet.

/// The registry itself.
///
/// The reference is a singleton (`sScriptMgr`). Here it is a value the caller owns, so a
/// test can build one without a process around it.
#[derive(Debug, Default)]
pub struct ScriptMgr {
    _private: (),
}