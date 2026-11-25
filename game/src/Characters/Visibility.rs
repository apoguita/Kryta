//! The inventory panel's display mode: whether the cape, the headgear and the two costume slots are
//! shown, and where.
//!
//! Ported from Rurik, `research/Rurik/toolkit/authsrv/visstatus.py` (293 lines, the whole module but
//! its menu and label tables, which only its log lines read), which read the rule out of the client:
//! one byte of flags per character, two bits per slot kind, the low bit of a pair meaning "shown in
//! an explorable area" and the high bit "shown in a town or outpost". The client never changes the
//! byte itself: its drop-down sends `0x8057 [value, mask]` and waits for `0x00EF [value, mask]`.
//!
//! Rurik had no retail capture of the exchange and marked its reply a reconstruction. The retail
//! server's own answer is now recorded, in a town
//! (`recordings/client/raw/creation-armor-in-game-setting-unfiltered.jsonl`), and it is Rurik's:
//!
//! | the headgear's menu | client `0x8057` | server `0x00EF` | server `0x006F` |
//! |---|---|---|---|
//! | Always Hide | `0, 0x0C` (seq 482) | `0, 0x0C` (489) | agent, 6, 0 (490) |
//! | Hide in Combat Areas | `8, 0x0C` (776) | `8, 0x0C` (780) | agent, 6, the head item (781) |
//! | Hide in Towns and Outposts | `4, 0x0C` (1027) | `4, 0x0C` (1031) | agent, 6, 0 (1032) |
//! | Always Show | `0x0C, 0x0C` (1159) | `0x0C, 0x0C` (1163) | agent, 6, the head item (1164) |
//!
//! What no capture shows, and is Rurik's reconstruction here too: the load of a character whose
//! byte is not `0xFF` (`0x00EF [flags, 0xFF]`, and a `0x006E` without the hidden pieces), and any of
//! it in an explorable area.

/// The cape: it has no visual position (it rides `0x0048`).
pub const KIND_CAPE: u8 = 0;
/// The headgear.
pub const KIND_HEADGEAR: u8 = 1;
/// The costume's body.
pub const KIND_COSTUME_BODY: u8 = 2;
/// The costume's head.
pub const KIND_COSTUME_HEAD: u8 = 3;

/// The eight bits the client keeps (`visstatus.py`, `FLAG_BITS`).
pub const FLAG_BITS: u32 = 0xFF;
/// The retail server's load for a character that never chose: every slot Always Show
/// (`recordings/client/raw/creation-armor-head-hidden-unfiltered.jsonl` seq 2383, `ff, ff`).
pub const DEFAULT_FLAGS: u8 = 0xFF;

/// The position each kind occupies in the nine-dword visual list (`visstatus.py`,
/// `KIND_VISUAL_SLOT`); the head's 6 is the slot the retail `0x006F` names.
const KIND_VISUAL_SLOT: [(u8, usize); 3] = [
    (KIND_HEADGEAR, 6),
    (KIND_COSTUME_BODY, 7),
    (KIND_COSTUME_HEAD, 8),
];

/// The client's own writer: `(flags & !mask) | value`, kept to the eight bits (`apply`).
pub fn apply(flags: u8, value: u32, mask: u32) -> u8 {
    (((u32::from(flags) & !mask) | value) & FLAG_BITS) as u8
}

/// The byte a load starts with: the stored one, or the default when none was ever stored
/// (`load_flags`: "absence is the default, never zero").
pub fn load_flags(stored: Option<u8>) -> u8 {
    stored.unwrap_or(DEFAULT_FLAGS)
}

/// The `0x00EF` a load sends: the whole byte under the full mask (`load_message`).
pub fn load_message(flags: u8) -> (u32, u32) {
    (u32::from(flags), FLAG_BITS)
}

/// Why a `0x8057 [value, mask]` is refused, or `None` (`check_request`): the drop-down sends
/// `bits & mask, mask` with one kind's pair as the mask, so an empty mask, bits outside the eight, or
/// a value carrying bits its mask does not is not the client's request.
pub fn check_request(value: u32, mask: u32) -> Option<String> {
    if mask == 0 {
        return Some("an empty mask changes nothing".to_string());
    }
    if mask & !FLAG_BITS != 0 || value & !FLAG_BITS != 0 {
        return Some(format!(
            "bits outside the eight visibility flags: value 0x{value:x} mask 0x{mask:x}"
        ));
    }
    if value & !mask != 0 {
        return Some(format!(
            "value 0x{value:x} carries bits outside its mask 0x{mask:x}"
        ));
    }
    None
}

/// Whether this kind is shown here (`shown`): the low bit of its pair in an explorable area, the
/// high bit in a town.
pub fn shown(flags: u8, kind: u8, explorable: bool) -> bool {
    let bit = 2 * u32::from(kind) + u32::from(!explorable);
    (u32::from(flags) >> bit) & 1 == 1
}

/// The visual list the world is told: `worn` with each hidden kind's position zeroed
/// (`strip_visual`).
pub fn strip_visual(worn: [u32; 9], flags: u8, explorable: bool) -> [u32; 9] {
    let mut out = worn;
    for (kind, slot) in KIND_VISUAL_SLOT {
        if !shown(flags, kind, explorable) {
            out[slot] = 0;
        }
    }
    out
}

/// After a mode change: the `0x006F` writes the world needs, as (visual slot, item id or 0), for
/// every kind whose shown state here changed and whose slot holds a piece (`slot_changes`). A kind
/// whose change only concerns the other kind of map sends nothing now.
pub fn slot_changes(old: u8, new: u8, worn: [u32; 9], explorable: bool) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for (kind, slot) in KIND_VISUAL_SLOT {
        if worn[slot] == 0 {
            continue;
        }
        let before = shown(old, kind, explorable);
        let after = shown(new, kind, explorable);
        if before != after {
            out.push((slot as u32, if after { worn[slot] } else { 0 }));
        }
    }
    out
}

/// The headgear's bit of the character list's packed word: the client packs "shown in a town" there
/// (`visstatus.py`: "the roster blob's helm_shown bit 14 is CHAR_STATS_VIS(3) packed by UiGame's
/// summary packer"), and its character select draws the headgear only while it is set
/// (`research/Eden/crates/eden-account/src/msg/summary.rs`, `helm_shown`).
pub fn helm_status(flags: u8) -> u8 {
    u8::from(shown(flags, KIND_HEADGEAR, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four choices of the headgear's menu as the retail client sent them, in a town, and what
    /// the retail server did to the head's visual slot after each
    /// (`recordings/client/raw/creation-armor-in-game-setting-unfiltered.jsonl` seq 482-1164).
    #[test]
    fn the_recorded_town_session_is_reproduced() {
        let mut worn = [0u32; 9];
        worn[6] = 0x1EF5;
        let mut flags = DEFAULT_FLAGS;
        let steps: [(u32, Vec<(u32, u32)>); 4] = [
            (0x0, vec![(6, 0)]),
            (0x8, vec![(6, 0x1EF5)]),
            (0x4, vec![(6, 0)]),
            (0xC, vec![(6, 0x1EF5)]),
        ];
        for (value, expected) in steps {
            assert_eq!(check_request(value, 0x0C), None);
            let next = apply(flags, value, 0x0C);
            assert_eq!(slot_changes(flags, next, worn, false), expected);
            flags = next;
        }
        assert_eq!(flags, DEFAULT_FLAGS);
    }

    #[test]
    fn a_mode_hides_only_where_it_says() {
        let mut worn = [0u32; 9];
        worn[6] = 7;
        // Hide in Combat Areas: the high bit of the headgear's pair only.
        let flags = apply(DEFAULT_FLAGS, 0x8, 0x0C);
        assert_eq!(strip_visual(worn, flags, false)[6], 7);
        assert_eq!(strip_visual(worn, flags, true)[6], 0);
        // A change that only concerns the other kind of map writes nothing here.
        assert!(slot_changes(DEFAULT_FLAGS, flags, worn, false).is_empty());
        assert_eq!(helm_status(flags), 1);
        assert_eq!(helm_status(apply(flags, 0x4, 0x0C)), 0);
    }

    #[test]
    fn a_request_that_is_not_the_drop_downs_is_refused() {
        assert!(check_request(0, 0).is_some());
        assert!(check_request(0x100, 0x100).is_some());
        assert!(check_request(0x3, 0x0C).is_some());
        assert_eq!(load_message(load_flags(None)), (0xFF, 0xFF));
        assert_eq!(load_message(load_flags(Some(0xF3))), (0xF3, 0xFF));
    }
}
