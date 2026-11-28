//! The rows the server states for its maps, held so the client can be answered with them.
//!
//! The client keeps one row for each map id - its name, kind, region and world-map place - in a
//! table compiled into it, and reads it through one function (`maps.row_lookup`,
//! `config/flavors/<flavor>/Client/offsets/maps.json`). The server states a map's row in the map's
//! record (`client_row` in `basic.json`) and sends it over the link (`Message::MapRows`); the hook
//! on that function (`runtime/map_row_hooks.rs`) answers from here.
//!
//! No research project does this: each puts a custom map on a stock map id and lets the client
//! show the stock row (`docs/port/24-custom-maps.md`, "The map's row in the client"). It follows
//! this tree's own title hook, which answers the same kind of table lookup
//! (`runtime/title_hooks.rs`, `metadata_detour`).
//!
//! **A row answered once stays where it is.** The client's own rows are in its image and never
//! move, so its code may keep the pointer it was given. A row handed out here is therefore never
//! freed or moved: it is allocated once for its map id and changed in place when the server states
//! it again, and a map the server no longer states goes back to the client's own row.

use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use client_link::{MAP_ROW_FIELDS, MapRow};

/// How many four-byte fields a row has.
pub const FIELDS: usize = MAP_ROW_FIELDS.len();

/// One map's row as the client is given it.
struct Held {
    /// Whether the server states this map now.
    stated: AtomicBool,
    /// The row: the client's own with the stated fields written over it.
    row: [AtomicU32; FIELDS],
}

/// The rows the server states, by map id.
#[derive(Default)]
pub struct MapRows {
    held: RwLock<HashMap<u32, &'static Held>>,
}

impl MapRows {
    /// Take the rows the server states. `own` answers the client's own row for a map id, or
    /// `None` for an id its table does not have; such a map's unstated fields are zero. A map
    /// stated before and not now is answered by the client again. Returns how many are stated.
    pub fn set(&self, rows: &[MapRow], own: &dyn Fn(u32) -> Option<[u32; FIELDS]>) -> usize {
        // The client's own rows are read before the lock is taken: `own` calls into the client.
        let built: Vec<(u32, [u32; FIELDS])> = rows
            .iter()
            .map(|row| (row.map_id, row.over(own(row.map_id).unwrap_or([0; FIELDS]))))
            .collect();
        let Ok(mut held) = self.held.write() else {
            return 0;
        };
        for entry in held.values() {
            entry.stated.store(false, Ordering::Release);
        }
        for (map_id, row) in &built {
            let entry = held.entry(*map_id).or_insert_with(|| {
                Box::leak(Box::new(Held {
                    stated: AtomicBool::new(false),
                    row: std::array::from_fn(|_| AtomicU32::new(0)),
                }))
            });
            for (slot, value) in entry.row.iter().zip(row) {
                slot.store(*value, Ordering::Release);
            }
            entry.stated.store(true, Ordering::Release);
        }
        built.len()
    }

    /// The row to answer the client with for a map id, when the server states one: 124 bytes that
    /// stay where they are for as long as the process runs.
    pub fn answer(&self, map_id: u32) -> Option<*const u32> {
        let held = self.held.read().ok()?;
        let entry = held.get(&map_id).filter(|entry| entry.stated.load(Ordering::Acquire))?;
        // An `AtomicU32` has the layout of a `u32`, so the array is the row as the client reads it.
        Some(entry.row.as_ptr().cast::<u32>())
    }

    /// The row held for a map id, as numbers.
    pub fn row(&self, map_id: u32) -> Option<[u32; FIELDS]> {
        let held = self.held.read().ok()?;
        let entry = held.get(&map_id).filter(|entry| entry.stated.load(Ordering::Acquire))?;
        Some(std::array::from_fn(|field| entry.row[field].load(Ordering::Acquire)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client's own row of a map, as a test stands in for it: every field is the map id, and
    /// a map from 898 up has no row.
    fn own(map_id: u32) -> Option<[u32; FIELDS]> {
        (map_id < 898).then_some([map_id; FIELDS])
    }

    #[test]
    fn a_stated_row_is_the_clients_own_with_the_stated_fields_over_it() {
        let rows = MapRows::default();
        assert!(rows.answer(148).is_none(), "nothing is stated yet");

        let stated = [
            MapRow { map_id: 148, fields: vec![(3, 2), (16, 500)], ..MapRow::default() },
            MapRow { map_id: 900, fields: vec![(29, 7)], ..MapRow::default() },
        ];
        assert_eq!(rows.set(&stated, &own), 2);

        let row = rows.row(148).expect("148 is stated");
        assert_eq!((row[0], row[3], row[16], row[30]), (148, 2, 500, 148));
        // A map the client's table does not have starts from zeros.
        let row = rows.row(900).expect("900 is stated");
        assert_eq!((row[0], row[29]), (0, 7));
        assert!(rows.answer(146).is_none(), "a map that is not stated is the client's to answer");

        // What the client reads through the pointer is the row, field by field.
        let pointer = rows.answer(148).expect("148 is answered");
        // SAFETY: the pointer is to 31 values that are never freed.
        let read = unsafe { std::slice::from_raw_parts(pointer, FIELDS) };
        assert_eq!((read[3], read[16]), (2, 500));
    }

    #[test]
    fn a_row_stays_where_it_is_and_a_map_no_longer_stated_goes_back_to_the_client() {
        let rows = MapRows::default();
        rows.set(&[MapRow { map_id: 148, fields: vec![(16, 500)], ..MapRow::default() }], &own);
        let first = rows.answer(148).expect("answered");

        // Stated again with another value: the same place, the new value, and the field stated
        // before and not now is the client's own again.
        rows.set(&[MapRow { map_id: 148, fields: vec![(17, 600)], ..MapRow::default() }], &own);
        assert_eq!(rows.answer(148), Some(first), "the client may still hold the pointer");
        let row = rows.row(148).unwrap();
        assert_eq!((row[16], row[17]), (148, 600));

        assert_eq!(rows.set(&[], &own), 0);
        assert!(rows.answer(148).is_none());
        // And stated once more, it is answered from the same place.
        rows.set(&[MapRow { map_id: 148, fields: vec![(16, 1)], ..MapRow::default() }], &own);
        assert_eq!(rows.answer(148), Some(first));
    }
}
