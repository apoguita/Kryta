//! `game/Districts/` - the reference's `src/server/game/Instances/` (4 files).
//!
//! Renamed: the reference calls this directory `Instances/`.
//!
//! An outpost is **one running copy per district** (R1, R4); the first district opens when an
//! outpost is entered and the next when one fills its allotment (R5, R16); a joining character is
//! placed in the first district with room (R22) unless the travel named one, in which case it is
//! honoured (R57) - and a district that is full cannot be entered (R58). The allotment is a setting,
//! never a constant (R5); it comes from `game/World/WorldConfig.rs` at the moment a character enters.
//!
//! An explorable is **private per party** (R3): with no parties yet, one connection is one party, so
//! each entry opens its own copy.
//!
//! A copy outlives its members by the hold (R37: fifteen minutes of nobody inside, for an outpost
//! and an explorable alike) and is disposed when the hold passes (`sweep`, R40). Its row in
//! `kryta_world.instance` is what says which district holds which number and when it last emptied
//! (`Instances.rs`, `data/sql/base/db_world/0003-instances.sql`).

#[path = "Instances.rs"]
pub mod instances;

#[path = "Peers.rs"]
pub mod peers;

pub use instances::{DatabaseInstances, InstanceRow};
pub use peers::{PeerView, Peers, Standing, farewell, introduction};

use common::logging::{Log, LogLevel};
use std::sync::Arc;

use crate::world::MapEntry;
use crate::world::map_kind::OUTPOST;

/// One running copy of a map: which map, which district number, who is inside, and what the world
/// database holds about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Copy {
    map_id: u32,
    district: u32,
    members: u32,
    /// The row this copy owns, when the world opened one for it.
    db_id: Option<i64>,
    /// When the last member left, in unix seconds; `None` while somebody is inside (R37).
    empty_since: Option<u64>,
}

/// The live copies, and the district numbers they hold.
///
/// One world owns this: a copy is the world's state, not a session's.
#[derive(Clone)]
pub struct Districts {
    copies: Vec<Copy>,
    store: Option<Arc<DatabaseInstances>>,
    log: Option<Log>,
}

impl Default for Districts {
    fn default() -> Self {
        Self::new()
    }
}

impl Districts {
    /// A world with no copy open and no row store.
    pub fn new() -> Self {
        Self {
            copies: Vec::new(),
            store: None,
            log: None,
        }
    }

    /// Give the copies a table to be recorded in, and a log to say what happened to them.
    pub fn attach(&mut self, store: Arc<DatabaseInstances>, log: Log) {
        self.store = Some(store);
        self.log = Some(log);
    }

    /// Which district a joining character enters, and the copy it enters.
    ///
    /// The rules, in the order they act:
    /// - a named district (`1..=n`) is honoured - an outpost's copies are numbered, and a number the
    ///   world does not hold yet is opened as that district (R4, R57);
    /// - a copy whose members have reached the allotment cannot be entered (R58), and a request for
    ///   one is refused rather than moved: the client asked for that district;
    /// - a request with no district takes the first copy with room (R22), and when every copy is full
    ///   the next one is opened (R5, R16).
    pub fn enter(
        &mut self,
        map: &MapEntry,
        district_hint: u32,
        language: u32,
        allotment: u32,
    ) -> Result<u32, String> {
        if map.kind != Some(OUTPOST) {
            // R3: an explorable copy belongs to one party. A party crossing names the district it
            // chose, and every member must land in that same copy; a connection with no hint gets a
            // fresh copy, numbered after the copies this world already holds for the map.
            if district_hint >= 1 {
                match self.copy_mut(map.id, district_hint) {
                    Some(copy) => copy.members += 1,
                    None => self.open_copy(map, district_hint, language),
                }
                return Ok(district_hint);
            }
            let district = self.next_number(map.id);
            self.open_copy(map, district, language);
            return Ok(district);
        }

        let allotment = allotment.max(1);
        if district_hint >= 1 {
            match self.copy_mut(map.id, district_hint) {
                Some(copy) => {
                    if copy.members >= allotment {
                        return Err(format!(
                            "district {} of map {} is full ({} of {}) and cannot be entered",
                            district_hint, map.id, copy.members, allotment
                        ));
                    }
                    copy.members += 1;
                    return Ok(district_hint);
                }
                None => {
                    self.open_copy(map, district_hint, language);
                    return Ok(district_hint);
                }
            }
        }

        if let Some(copy) = self.first_with_room(map.id, allotment) {
            copy.members += 1;
            return Ok(copy.district);
        }

        // R5: the outpost is full, so the next district opens - the number after the highest held.
        let district = self.next_number(map.id);
        self.open_copy(map, district, language);
        Ok(district)
    }

    /// One connection ends, and its place is given back.
    ///
    /// When the last member leaves, the copy is marked empty: R37's hold counts from that moment,
    /// and `sweep` disposes it once the hold passes. An outpost that empties is kept - a returning
    /// client finds the same district - and so is an explorable for its reconnection window.
    pub fn leave(&mut self, map_id: u32, district: u32, now: u64) {
        let (db_id, emptied) = match self.copy_mut(map_id, district) {
            Some(copy) => {
                copy.members = copy.members.saturating_sub(1);
                if copy.members == 0 {
                    copy.empty_since = Some(now);
                    (copy.db_id, true)
                } else {
                    (None, false)
                }
            }
            None => return,
        };
        if emptied
            && let (Some(store), Some(db_id)) = (self.store.as_ref(), db_id)
            && let Err(error) = store.mark_empty(db_id)
        {
            self.say(format!(
                "Instance on map {map_id}, district {district}, empties but its row cannot be marked: {error}."
            ));
        }
    }

    /// Dispose every copy that has been empty for at least `hold` seconds, and answer which were
    /// disposed - the teardown R40 acts on, once per tick.
    pub fn sweep(&mut self, now: u64, hold: u64) -> Vec<(u32, u32)> {
        let mut disposed = Vec::new();
        let mut kept = Vec::with_capacity(self.copies.len());
        for copy in &self.copies {
            let expired = copy
                .empty_since
                .is_some_and(|since| now.saturating_sub(since) >= hold);
            if !expired {
                kept.push(*copy);
                continue;
            }
            if let (Some(store), Some(db_id)) = (self.store.as_ref(), copy.db_id)
                && let Err(error) = store.dispose(db_id)
            {
                self.say(format!(
                    "Instance on map {}, district {}, is empty past the hold but its row cannot be deleted: {error}.",
                    copy.map_id, copy.district
                ));
            }
            disposed.push((copy.map_id, copy.district));
        }
        self.copies = kept;
        disposed
    }

    /// How many members a copy holds, for a test or a status line.
    pub fn members(&self, map_id: u32, district: u32) -> u32 {
        self.copies
            .iter()
            .find(|copy| copy.map_id == map_id && copy.district == district)
            .map_or(0, |copy| copy.members)
    }

    /// How many copies a map has open.
    pub fn count_of(&self, map_id: u32) -> usize {
        self.copies
            .iter()
            .filter(|copy| copy.map_id == map_id)
            .count()
    }

    /// Every open copy, in the order it was opened - for a log line.
    pub fn open(&self) -> Vec<(u32, u32, u32)> {
        self.copies
            .iter()
            .map(|copy| (copy.map_id, copy.district, copy.members))
            .collect()
    }

    /// Open a copy, and write the row that records it.
    fn open_copy(&mut self, map: &MapEntry, district: u32, language: u32) {
        let db_id = match self.store.as_ref() {
            Some(store) => match store.open(map.id, district, language, map.kind.unwrap_or(0)) {
                Ok(id) => Some(id),
                Err(error) => {
                    self.say(format!(
                        "Instance on map {}, district {district}, opens but its row cannot be written: {error}.",
                        map.id
                    ));
                    None
                }
            },
            None => None,
        };
        self.copies.push(Copy {
            map_id: map.id,
            district,
            members: 1,
            db_id,
            empty_since: None,
        });
    }

    fn say(&self, text: String) {
        if let Some(log) = self.log.as_ref() {
            log.out(LogLevel::Warn, "server.worldserver", text);
        }
    }

    fn copy_mut(&mut self, map_id: u32, district: u32) -> Option<&mut Copy> {
        self.copies
            .iter_mut()
            .find(|copy| copy.map_id == map_id && copy.district == district)
    }

    /// The lowest-numbered copy of a map with room left (R22).
    fn first_with_room(&mut self, map_id: u32, allotment: u32) -> Option<&mut Copy> {
        self.copies
            .iter_mut()
            .filter(|copy| copy.map_id == map_id && copy.members < allotment)
            .min_by_key(|copy| copy.district)
    }

    /// The next district number a map would open: one past the highest it holds.
    fn next_number(&self, map_id: u32) -> u32 {
        self.copies
            .iter()
            .filter(|copy| copy.map_id == map_id)
            .map(|copy| copy.district)
            .max()
            .map_or(1, |highest| highest + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::MapEntry;
    use crate::world::map_kind;

    fn outpost() -> MapEntry {
        MapEntry {
            id: 148,
            kind: Some(OUTPOST),
            file_id: 112_509,
            name: "Ascalon City (pre-Searing)".to_string(),
            // The value a live load of map 148 carried (`0xBB14F539`,
            // `recordings/client/feature/maps/pre-searing-load-3.jsonl` seq 751).
            instance_value: Some(0xBB14_F539),
            name_id: None,
        }
    }

    fn explorable() -> MapEntry {
        MapEntry {
            id: 5,
            kind: Some(map_kind::EXPLORABLE),
            file_id: 1,
            name: "Regent Valley".to_string(),
            instance_value: None,
            name_id: None,
        }
    }

    #[test]
    fn the_first_entry_opens_district_one() {
        let mut districts = Districts::new();
        assert_eq!(districts.enter(&outpost(), 0, 0, 100).unwrap(), 1);
        assert_eq!(districts.count_of(148), 1);
        assert_eq!(districts.members(148, 1), 1);
    }

    #[test]
    fn a_named_district_is_honoured_even_before_it_exists() {
        let mut districts = Districts::new();
        assert_eq!(districts.enter(&outpost(), 3, 0, 100).unwrap(), 3);
        assert_eq!(districts.members(148, 3), 1);
        assert_eq!(districts.count_of(148), 1);
    }

    #[test]
    fn a_filled_district_is_left_for_the_first_with_room() {
        let mut districts = Districts::new();
        districts.enter(&outpost(), 0, 0, 2).unwrap();
        districts.enter(&outpost(), 0, 0, 2).unwrap();
        // District 1 is full (2 of 2): the next joining character gets the next district (R5).
        assert_eq!(districts.enter(&outpost(), 0, 0, 2).unwrap(), 2);
        assert_eq!(districts.count_of(148), 2);
        assert_eq!(districts.members(148, 1), 2);
        assert_eq!(districts.members(148, 2), 1);
    }

    #[test]
    fn a_named_full_district_is_refused() {
        let mut districts = Districts::new();
        districts.enter(&outpost(), 1, 0, 1).unwrap();
        let refusal = districts.enter(&outpost(), 1, 0, 1).unwrap_err();
        assert!(
            refusal.contains("district 1 of map 148 is full"),
            "{refusal}"
        );
    }

    #[test]
    fn leaving_gives_the_place_back() {
        let mut districts = Districts::new();
        districts.enter(&outpost(), 0, 0, 2).unwrap();
        districts.enter(&outpost(), 0, 0, 2).unwrap();
        districts.leave(148, 1, 0);
        assert_eq!(districts.members(148, 1), 1);
        // The place that opened is the one the next entry takes.
        assert_eq!(districts.enter(&outpost(), 0, 0, 2).unwrap(), 1);
    }

    #[test]
    fn an_explorable_gets_a_copy_of_its_own() {
        let mut districts = Districts::new();
        assert_eq!(districts.enter(&explorable(), 0, 0, 100).unwrap(), 1);
        assert_eq!(districts.enter(&explorable(), 0, 0, 100).unwrap(), 2);
        assert_eq!(districts.count_of(5), 2);
        assert_eq!(districts.members(5, 1), 1);
    }

    #[test]
    fn a_copy_is_held_for_the_hold_and_then_disposed() {
        // R37: fifteen minutes of nobody inside, then R40's disposal.
        let mut districts = Districts::new();
        let started = 1_000;
        districts.enter(&outpost(), 0, 0, 100).unwrap();
        assert!(
            districts.sweep(started, 900).is_empty(),
            "a loaded copy is not empty"
        );
        districts.leave(148, 1, started);
        assert!(
            districts.sweep(started + 899, 900).is_empty(),
            "still inside the hold"
        );
        assert_eq!(districts.sweep(started + 900, 900), vec![(148, 1)]);
        assert_eq!(districts.count_of(148), 0, "and the copy is gone");
    }
}
