//! The client's instance table: the destinations it can resolve at all.
//!
//! Ported from `ASURA/crates/asura-world/src/instance_table.rs`. A client cannot ask to travel to a map
//! it does not know exists: `0x01A0` is how it learns, as `{instance, value}` pairs, ten batches of
//! them in the load of one map (`recordings/client/feature/maps/pre-searing-load-3.jsonl`, seq 751-...).
//!
//! Where the working implementation walks a pack of `maps/<campaign>/<area>/<subarea>/<map id>/basic.json`
//! files, this tree's map content is rows (`docs/port/04-configuration.md`, "three kinds of data"), so
//! the value a map states is a column: `map.instance_value` (`data/sql/base/db_world/0007-map-instance-value.sql`).
//!
//! **What a map without a value means:** it is left out, and the log says so. Answering with a wrong
//! value is worse than a destination the client cannot resolve yet.

use crate::server::packets::instance_packets;
use crate::world::MapEntry;

/// The pairs one `0x01A0` message carries, which is the batch size a live server sends
/// (`ASURA/crates/asura-world/src/instance_table.rs:26-27`).
pub const BATCH: usize = 15;

/// The pairs to send, in the batch shape a load uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstanceTable {
    /// `(map id, instance value)` per map, ordered by map id.
    pub pairs: Vec<(u16, u32)>,
}

impl InstanceTable {
    /// The frames: `0x01A0` messages of at most [`BATCH`] pairs each.
    pub fn frames(&self) -> Vec<Vec<u8>> {
        self.pairs
            .chunks(BATCH)
            .map(instance_packets::instance_table)
            .collect()
    }
}

/// The table this world sends: every map whose row states a value, in map-id order, in the batches a
/// load uses. `ASURA/crates/asura-world/src/instance_table.rs:131-142` (`table_for`).
///
/// A map id that does not fit the wire's `u16` is reported and left out, as a map without a value is -
/// the working implementation's own rule for a record it cannot state.
pub fn table_for(maps: &[MapEntry]) -> (InstanceTable, Vec<String>) {
    let mut notes = Vec::new();
    let mut pairs: Vec<(u16, u32)> = Vec::new();
    for map in maps {
        let Some(value) = map.instance_value else {
            continue;
        };
        match u16::try_from(map.id) {
            Ok(id) => pairs.push((id, value)),
            Err(_) => notes.push(format!(
                "map {} does not fit the instance table's 16-bit id; left out",
                map.id
            )),
        }
    }
    pairs.sort_by_key(|pair| pair.0);
    (InstanceTable { pairs }, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::map_kind;

    fn map(id: u32, value: Option<u32>) -> MapEntry {
        MapEntry {
            id,
            kind: Some(map_kind::OUTPOST),
            file_id: 0,
            name: format!("map {id}"),
            instance_value: value,
            name_id: None,
        }
    }

    #[test]
    fn a_map_without_a_value_is_left_out() {
        let (table, notes) = table_for(&[map(148, Some(0xBB14_F539)), map(0, None)]);
        assert_eq!(table.pairs, vec![(148, 0xBB14_F539)]);
        assert!(
            notes.is_empty(),
            "leaving a map out is not a fault: {notes:?}"
        );
    }

    #[test]
    fn the_pairs_are_ordered_by_map_id_whatever_the_rows_arrive_like() {
        let (table, _) = table_for(&[map(779, Some(1)), map(148, Some(2)), map(145, Some(3))]);
        assert_eq!(table.pairs, vec![(145, 3), (148, 2), (779, 1)]);
    }

    /// A batch is a `u8` count and that many `{u16 instance, u32 value}` pairs: two header bytes, the
    /// count, six bytes each (`ASURA/crates/asura-protocol/src/game/mod.rs:868-878`).
    #[test]
    fn the_table_frames_in_batches_a_client_expects() {
        let maps: Vec<MapEntry> = (1..=40).map(|id| map(id, Some(id * 7))).collect();
        let (table, _) = table_for(&maps);
        let frames = table.frames();
        assert_eq!(frames.len(), 40usize.div_ceil(BATCH), "three batches");
        for (index, frame) in frames.iter().enumerate() {
            let expected = table.pairs.chunks(BATCH).nth(index).expect("a batch").len();
            assert_eq!(&frame[..2], &[0xA0, 0x01], "the instance table");
            assert_eq!(frame[2] as usize, expected, "the batch's own count");
            assert_eq!(
                frame.len(),
                3 + expected * 6,
                "a count and six bytes a pair"
            );
        }
    }

    /// The one value this tree has measured: map 148's, from the live load that carried it.
    #[test]
    fn a_pair_is_the_map_and_its_value() {
        let (table, _) = table_for(&[map(148, Some(0xBB14_F539))]);
        let frame = &table.frames()[0];
        assert_eq!(&frame[3..5], &148u16.to_le_bytes(), "the instance");
        assert_eq!(&frame[5..9], &0xBB14_F539u32.to_le_bytes(), "its value");
    }
}
