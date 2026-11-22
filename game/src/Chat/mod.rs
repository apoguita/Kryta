//! `game/Chat/` - the reference's `src/server/game/Chat/`: the channel rule, and the command surface.
//!
//! What a player types into a chat line arrives as `GAME_CMSG_SEND_CHAT_MESSAGE` (`0x8064`); the first
//! unit of its text is the marker that decides what happens - `!` speaks on the all channel, `#` on the
//! group channel, `$` on trade, `/` starts a command, and the reference drops `@`, `%` and `"` and
//! refuses the rest (`GmChat.c`, as the working implementation ports it at
//! `ASURA/crates/asura-world/src/chat.rs:193-353`). The connection reads that frame; this module holds
//! the alias rule and the answer `/maps` prints.
//!
//! **The command surface is a file, not this module.** Which names exist, what each takes and which
//! operation it runs are rows in `config/Server/commands.json`, read by [`commands::Table`]; the reference's own
//! two dispatch points - `GameSrv_TryHandleGmCommand` (`GmCommands.c`, which owns `/tp`, `/teleport` and
//! `/maps`) and `GameSrv_HandleChatCommand` (`GmChat.c`, which owns `/stuck`) - are that file's rows, and
//! Rust keeps only the operations they name (`docs/decisions/0011-the-command-surface-is-content.md`).

#[path = "Commands.rs"]
pub mod commands;

#[path = "Operations.rs"]
pub mod operations;

use crate::maps::travel_targets::{TRAVEL_TARGETS, TravelTarget};
use crate::world::MapEntry;

/// What `/maps` produces: the lines the reference would put on the chat channel, and the table it puts
/// in its log. Two shapes, because the reference has two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedMaps {
    /// The chat half, wrapped the way the reference wraps it. Printed, never sent
    /// (`chat.rs:288-324`).
    pub chat: Vec<String>,
    /// The table the log gets: the alias count in the header, then every alias.
    pub table: Vec<String>,
}

/// How a map alias is compared: lower case, runs of spaces collapsed. The reference's own rule for
/// `/tp` (`GmCommand_ToAscii`), as the working implementation writes it (`chat.rs:115-120`).
pub fn alias_key(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The reference's `/tp` lookup: normalise the argument, find it in its table.
///
/// `GmCommand_ToAscii` refuses anything that is not printable ASCII, which is why a non-ASCII argument is
/// an unknown map name rather than a fuzzy match; and the table is exact, so `lakesidecounty` is not
/// `lakeside county` even though the reference's own comment says otherwise (`chat.rs:169-182`).
pub fn travel_target(argument: &str) -> Option<&'static TravelTarget> {
    let wanted = alias_key(argument);
    if wanted.is_empty() {
        return None;
    }
    TRAVEL_TARGETS.iter().find(|target| target.alias == wanted)
}

/// The reference's `/maps`, line for line (`GmCommand_ListTargets`), as the working implementation
/// prints it (`chat.rs:41-102`).
///
/// It produces two things and they are **not the same text**:
///
/// * the **chat** side is one alias per map - the first in the table, because that is the one to type -
///   comma-separated and wrapped inside one chat fragment, then the reference's fixed
///   "`Use: /tp <name>, e.g. /tp lakeside county`";
/// * the **table** side is **every alias**, one row each, with the map it names, in the reference's own
///   column widths and in its table order.
///
/// The header's count is the **alias** count, not the map count, and the loop below prints a row per
/// alias. Only maps **this world hosts** are listed, so a world with one map does not advertise a
/// target `/tp` would refuse; a hosted map the table does not name gets one row of its own, named by its
/// own alias, rather than vanishing.
pub fn maps_answer(maps: &[MapEntry]) -> MappedMaps {
    let hosted: Vec<u32> = maps.iter().map(|map| map.id).collect();

    // The rows, in the reference's table order: one per alias.
    let mut rows: Vec<(String, u32, String)> = Vec::new();
    let mut chat: Vec<String> = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    for target in TRAVEL_TARGETS {
        if !hosted.contains(&target.map_id) {
            continue;
        }
        rows.push((
            target.alias.to_string(),
            target.map_id,
            target.label.to_string(),
        ));
        if !seen.contains(&target.map_id) {
            seen.push(target.map_id);
            chat.push(target.alias.to_string());
        }
    }
    for map in maps {
        // A creation instance is not a place: it is where a character is made, it has no alias, and
        // nobody travels to it. It stays out of the teleport table.
        if map.is_creation() {
            continue;
        }
        if seen.contains(&map.id) {
            continue;
        }
        seen.push(map.id);
        let alias = map.alias();
        chat.push(alias.clone());
        rows.push((alias, map.id, map.name.clone()));
    }

    // The chat line, split the way the reference splits it: `100 <= used + needed` starts a continuation
    // line indented by six spaces, each entry followed by a comma.
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::from("Maps:");
    for alias in &chat {
        if 100 <= current.len() + alias.len() + 3 {
            lines.push(current);
            current = String::from("      ");
        }
        current.push_str(&format!(" {alias},"));
    }
    lines.push(current);
    // The reference's own sentence, literal: it names `lakeside county` whatever is in the table.
    lines.push("Use: /tp <name>, e.g. /tp lakeside county".to_string());

    // The table the log gets: the alias count in the header, then every alias.
    let mut table = vec![format!(
        "Teleport targets ({} aliases, every one with a map config):",
        rows.len()
    )];
    for (alias, map_id, label) in &rows {
        table.push(format!("  /tp {alias:<15} -> map {map_id:>3}  {label}"));
    }

    MappedMaps { chat: lines, table }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::map_kind;

    fn map(id: u32, kind: u8, name: &str) -> MapEntry {
        MapEntry {
            id,
            kind: Some(kind),
            file_id: 0,
            name: name.to_string(),
            instance_value: None,
            name_id: None,
        }
    }

    /// `GmCommand_ToAscii`'s rule, on the three shapes it is asked about.
    #[test]
    fn an_alias_is_lower_case_with_single_spaces() {
        assert_eq!(alias_key("Lakeside County"), "lakeside county");
        assert_eq!(alias_key("  lakeside   county "), "lakeside county");
        assert_eq!(alias_key("ASCALON"), "ascalon");
        assert_eq!(alias_key("   "), "");
    }

    #[test]
    fn a_target_is_found_by_the_alias_the_reference_lists() {
        // Two spellings, one map: the reference's `s_teleport_targets` lists both.
        assert_eq!(travel_target("lakeside").map(|t| t.map_id), Some(146));
        assert_eq!(
            travel_target("Lakeside County").map(|t| t.map_id),
            Some(146)
        );
        assert_eq!(travel_target("kamadan").map(|t| t.map_id), Some(449));
        // The table is exact, and an empty argument names nothing.
        assert!(travel_target("lakesidecounty").is_none());
        assert!(travel_target("").is_none());
        assert!(travel_target("   ").is_none());
    }

    /// `/maps` with this world's one map: the table's own alias for it, and no row for the creation
    /// instance it also hosts.
    #[test]
    fn the_answer_lists_the_aliases_of_the_maps_this_world_hosts() {
        let maps = vec![
            map(148, map_kind::OUTPOST, "Ascalon City (pre-Searing)"),
            map(0, map_kind::CREATION, "Character creation"),
        ];
        let answer = maps_answer(&maps);
        // Both of map 148's aliases are rows, and the header counts aliases rather than maps.
        assert_eq!(
            answer.table[0],
            "Teleport targets (2 aliases, every one with a map config):"
        );
        assert_eq!(
            answer.table[1],
            "  /tp ascalon         -> map 148  Ascalon City (pre-Searing)"
        );
        assert_eq!(
            answer.table[2],
            "  /tp ascalon city    -> map 148  Ascalon City (pre-Searing)"
        );
        assert_eq!(answer.table.len(), 3);
        // The chat half names one alias per map - the first - and ends with the reference's sentence.
        assert_eq!(answer.chat[0], "Maps: ascalon,");
        assert_eq!(answer.chat[1], "Use: /tp <name>, e.g. /tp lakeside county");
    }

    /// A hosted map the table does not name gets one row of its own, with the alias made from its name,
    /// and it is added after every table row.
    #[test]
    fn a_map_the_table_does_not_name_is_listed_by_its_own_alias() {
        let maps = vec![
            map(148, map_kind::OUTPOST, "Ascalon City (pre-Searing)"),
            map(999, map_kind::EXPLORABLE, "Somewhere New"),
        ];
        let answer = maps_answer(&maps);
        assert_eq!(
            answer.table[0],
            "Teleport targets (3 aliases, every one with a map config):"
        );
        assert_eq!(
            answer.table[3],
            "  /tp somewhere new   -> map 999  Somewhere New"
        );
        assert!(answer.chat[0].contains("ascalon"));
        assert!(answer.chat[0].contains("somewhere new"));
    }

    /// The chat half wraps at the reference's own point: `100 <= used + needed` starts a continuation
    /// line indented by six spaces.
    #[test]
    fn the_chat_half_wraps_the_way_the_reference_wraps_it() {
        let maps: Vec<MapEntry> = TRAVEL_TARGETS
            .iter()
            .map(|target| map(target.map_id, 0, target.label))
            .collect();
        let answer = maps_answer(&maps);
        // Eleven aliases plus `Maps:` passes a hundred characters, so there is more than one line.
        assert!(answer.chat.len() > 2);
        assert!(answer.chat[0].starts_with("Maps:"));
        for line in &answer.chat[1..answer.chat.len() - 1] {
            assert!(line.starts_with("      "), "{line} is not a continuation");
        }
        assert_eq!(
            answer.chat.last().map(String::as_str),
            Some("Use: /tp <name>, e.g. /tp lakeside county")
        );
    }
}
