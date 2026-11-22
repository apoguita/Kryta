//! Every alias `/tp` accepts, and the map each one names.
//!
//! Ported, whole, from the working implementation's own generated table
//! (`ASURA/crates/asura-world/src/travel_targets.rs`, itself generated from the reference's
//! `GmCommands.c`): the reference's `s_teleport_targets` is the table its `/tp` resolves through, its
//! comment calls it "every map the server has a config for", and it is written out rather than derived
//! from `GmMaps.h` for a stated reason - a command that quietly lands on Kamadan is not a testing tool.
//!
//! The reference's aliases are not the maps' names: `lakeside` and `lakeside county` both name map 146,
//! and `ascalon city` names a map whose own name is `Ascalon City (pre-Searing)`. That is why this is a
//! table of its own and not a lookup on [`crate::world::MapEntry::name`].
//!
//! `hosted` is the working implementation's answer to "do I have a row and a blob for it", because the
//! reference refuses a target with no map config rather than letting its map load fall back. This world
//! answers the same question from its own map rows, and asks it in the same order the working
//! implementation does: the alias, then the entry's own `hosted` flag, then whether a map with that id
//! exists (`ASURA/crates/asura-world/src/travel.rs:201-247`).

/// One alias of the teleport table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelTarget {
    /// What the player types, lower case with single spaces (`GmCommand_ToAscii` normalises).
    pub alias: &'static str,
    /// What the log calls it, and what `/maps` prints once per map.
    pub label: &'static str,
    /// The map the alias names.
    pub map_id: u32,
    /// `MapType` (`GmMaps.h`): 0 town, 1 explorable, 11 character creation.
    pub map_type: u32,
    /// Whether the world that wrote this table has a row and a blob for it.
    pub hosted: bool,
}

/// Every alias of the reference's table, in its order.
pub const TRAVEL_TARGETS: &[TravelTarget] = &[
    TravelTarget {
        alias: "ascalon",
        label: "Ascalon City (pre-Searing)",
        map_id: 148,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "ascalon city",
        label: "Ascalon City (pre-Searing)",
        map_id: 148,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "lakeside",
        label: "Lakeside County",
        map_id: 146,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "lakeside county",
        label: "Lakeside County",
        map_id: 146,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "ashford",
        label: "Ashford Abbey",
        map_id: 164,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "ashford abbey",
        label: "Ashford Abbey",
        map_id: 164,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "catacombs",
        label: "The Catacombs",
        map_id: 145,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "the catacombs",
        label: "The Catacombs",
        map_id: 145,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "northlands",
        label: "The Northlands",
        map_id: 147,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "the northlands",
        label: "The Northlands",
        map_id: 147,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "green hills",
        label: "Green Hills County",
        map_id: 160,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "barradin",
        label: "The Barradin Estate",
        map_id: 163,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "barradin estate",
        label: "The Barradin Estate",
        map_id: 163,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "wizards folly",
        label: "Wizard's Folly",
        map_id: 161,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "foibles",
        label: "Foibles Fair",
        map_id: 165,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "foibles fair",
        label: "Foibles Fair",
        map_id: 165,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "regent",
        label: "Regent Valley (pre-Searing)",
        map_id: 162,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "regent valley",
        label: "Regent Valley (pre-Searing)",
        map_id: 162,
        map_type: 1,
        hosted: true,
    },
    TravelTarget {
        alias: "fort ranik",
        label: "Fort Ranik",
        map_id: 166,
        map_type: 0,
        hosted: true,
    },
    TravelTarget {
        alias: "kamadan",
        label: "Kamadan, Jewel of Istan",
        map_id: 449,
        map_type: 0,
        hosted: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is the working implementation's, entry for entry and in its order
    /// (`ASURA/crates/asura-world/src/travel_targets.rs:25-166`).
    #[test]
    fn the_table_is_the_one_it_was_copied_from() {
        assert_eq!(TRAVEL_TARGETS.len(), 20);
        assert_eq!(TRAVEL_TARGETS[0].alias, "ascalon");
        assert_eq!(TRAVEL_TARGETS[0].map_id, 148);
        assert_eq!(TRAVEL_TARGETS[19].alias, "kamadan");
        assert_eq!(TRAVEL_TARGETS[19].map_id, 449);
        // The reference's own two spellings of one map, and its three kinds.
        assert_eq!(
            TRAVEL_TARGETS[2].map_id, TRAVEL_TARGETS[3].map_id,
            "lakeside and lakeside county name the same map"
        );
        assert!(TRAVEL_TARGETS.iter().all(|target| target.map_type <= 11));
        assert!(TRAVEL_TARGETS.iter().all(|target| target.hosted));
    }

    /// `GmCommand_ToAscii`'s rule: an alias is lower case with single spaces, and no two entries are the
    /// same string - a duplicate would make the table's order meaningless.
    #[test]
    fn every_alias_is_already_normalised_and_unique() {
        for (index, target) in TRAVEL_TARGETS.iter().enumerate() {
            assert_eq!(
                target.alias,
                target
                    .alias
                    .to_lowercase()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                "{} is not in the form a player types",
                target.alias
            );
            assert!(
                !TRAVEL_TARGETS[..index]
                    .iter()
                    .any(|earlier| earlier.alias == target.alias),
                "{} appears twice",
                target.alias
            );
        }
    }
}
