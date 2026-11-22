//! Where a character enters, when the map a client names is not a place it can be put in.
//!
//! Reference: `AuthSrv_HandleRequestGameInstance` decides the destination with the character's own
//! row, not with the client's number alone - a client has no business knowing which outpost its
//! character stands in. The rule is ported whole from the working implementation's rule layer
//! (`ASURA/crates/asura-rules/src/lib.rs`, 283 lines: `placement`, `name_is_acceptable`, `Requested`,
//! `Character`, `Substituted`), in its own order:
//!
//! 1. **A creation visit is its own destination.** `map_id == CreationMap` *and* the asked-for type is
//!    `map_kind::CREATION` is the character-creation instance, and it is not substituted.
//! 2. **The mistaken creation request.** A client that names the creation map as a *place* - which is
//!    what this client does when a character is clicked, `0x8029` with map 0 and type 0 - is placed in
//!    the map its character's row names, then in its last outpost, and only then in the fallback
//!    outpost the caller supplies. Measured 2026-11-12: without this, the login hands the client the
//!    creation instance again and the client waits on a loading screen that never ends.
//! 3. **The outpost rule, for every destination that is not a main town.**
//!    `destination_type is None -> the client's own map`; `== main_town -> the client's own map`;
//!    no last outpost at all -> the fallback; the last outpost *is* the destination -> the client's
//!    own map; otherwise -> the last outpost, `Substituted::NotAnOutpost`.
//!
//! `destination_type` is the **catalogue's** type for the map being asked for, or `None` when the map
//! is not in a catalogue the caller holds - an unknown map is left alone for the caller to refuse. The
//! login program holds no world catalogue (its configuration opens the login and character databases
//! only), so it passes `None`, which is rule 3's own first branch; giving it one is the decision named
//! in `docs/port/09-parity-audit.md` §7a, and nothing here invents it.
//!
//! `kinds` are the client's own map types (`game/World/mod.rs`, `map_kind`: 0 an outpost, 1 an
//! explorable, 11 the creation instance), which ASURA reads from its `defaults.json` for the same
//! reason - they are the wire layer's numbers, not a rule's.

use crate::world::map_kind;

/// The client's map kinds, as the wire states them: what a server compares a destination against.
///
/// `ASURA/crates/asura-protocol/src/game/mod.rs`, `MapKinds`, whose values are 0, 1 and 11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapKinds {
    /// A town or outpost.
    pub main_town: u8,
    /// An explorable area.
    pub main_explorable: u8,
    /// The character-creation instance.
    pub character_creation: u8,
}

/// The client's kinds, as this tree states them.
pub const MAP_KINDS: MapKinds = MapKinds {
    main_town: map_kind::OUTPOST,
    main_explorable: map_kind::EXPLORABLE,
    character_creation: map_kind::CREATION,
};

/// What the client asked for, in its own numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requested {
    /// The map the client named.
    pub map_id: u16,
    /// The type it named with it: 0 an outpost, 1 an explorable, 11 the creation instance.
    pub map_type: u32,
}

/// What the character's own row says, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Character {
    /// The map its row names, or the creation map when it names none.
    pub map_id: u16,
    /// The outpost it was last in, or the creation map when it has never been in one.
    pub last_outpost: u16,
}

/// Why the destination is not the one the client named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Substituted {
    /// The client named the creation instance as a place, and the character's row was the answer.
    CreationNamedAsPlace(&'static str),
    /// The destination is not an outpost, and the outpost rule put the character in its last one.
    NotAnOutpost,
    /// Neither the row nor the last outpost names a map, so the caller's fallback is the answer.
    NothingToPlace,
}

/// Whether a name the creation confirm parsed is one to create a character with.
///
/// A name with nothing in it is refused: a character the client cannot list is not a character.
/// Whether the name is *free* is the database's question and is not asked here
/// (`ASURA/crates/asura-rules/src/lib.rs:49-56`).
pub fn name_is_acceptable(name: &str) -> bool {
    !name.trim().is_empty()
}

/// The map the character enters, and why it is not the one the client asked for.
///
/// `fallback_outpost` and `creation_map` are the caller's - both are settings, because which outpost a
/// character with no history enters and which map id means "nothing written yet" are the operator's
/// numbers, not this rule's (`docs/port/04-configuration.md`).
pub fn place(
    requested: Requested,
    character: Character,
    destination_type: Option<u8>,
    creation_map: u16,
    fallback_outpost: u16,
    kinds: MapKinds,
) -> (u16, Option<Substituted>) {
    // 1. A creation visit is its own destination: the screen has to be reachable.
    if requested.map_id == creation_map && requested.map_type == u32::from(kinds.character_creation)
    {
        return (requested.map_id, None);
    }

    // 2. The mistaken creation request: the creation map named as a place to stand.
    if requested.map_id == creation_map {
        if character.map_id != creation_map {
            return (
                character.map_id,
                Some(Substituted::CreationNamedAsPlace("the character's row")),
            );
        }
        if character.last_outpost != creation_map {
            return (
                character.last_outpost,
                Some(Substituted::CreationNamedAsPlace("its last outpost")),
            );
        }
        return (fallback_outpost, Some(Substituted::NothingToPlace));
    }

    // 3. The outpost rule, for every destination that is not a main town.
    let Some(map_type) = destination_type else {
        // An unknown map is left alone for the caller to refuse (`lib.rs:119-122`).
        return (requested.map_id, None);
    };
    if map_type == kinds.main_town {
        return (requested.map_id, None);
    }
    if character.last_outpost == creation_map {
        // Never placed, so there is no last outpost to substitute: the fallback.
        return (fallback_outpost, Some(Substituted::NothingToPlace));
    }
    if character.last_outpost == requested.map_id {
        (requested.map_id, None)
    } else {
        (character.last_outpost, Some(Substituted::NotAnOutpost))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The creation instance's map id, which the world's own catalogue names.
    const CREATION: u16 = 0;
    /// The outpost a character with no history enters, as the auth program's setting states it.
    const FALLBACK: u16 = 148;

    fn fresh() -> Character {
        Character {
            map_id: CREATION,
            last_outpost: CREATION,
        }
    }

    /// `ASURA/crates/asura-rules/src/lib.rs:156-166`.
    #[test]
    fn a_creation_visit_reaches_the_creation_instance() {
        let requested = Requested {
            map_id: CREATION,
            map_type: u32::from(map_kind::CREATION),
        };
        assert_eq!(
            place(
                requested,
                fresh(),
                Some(map_kind::CREATION),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (CREATION, None)
        );
    }

    /// What this client really sends when a character is clicked: map 0, type 0, district 0 and
    /// language -1 (measured 2026-11-12, `Auth.log`). The row is the answer.
    #[test]
    fn a_character_click_that_names_the_creation_map_enters_the_characters_own_map() {
        let requested = Requested {
            map_id: CREATION,
            map_type: map_kind::OUTPOST.into(),
        };
        let played = Character {
            map_id: 148,
            last_outpost: 148,
        };
        assert_eq!(
            place(
                requested,
                played,
                Some(map_kind::CREATION),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (
                148,
                Some(Substituted::CreationNamedAsPlace("the character's row"))
            )
        );
    }

    #[test]
    fn a_row_that_names_no_map_falls_back_to_its_last_outpost() {
        let requested = Requested {
            map_id: CREATION,
            map_type: map_kind::OUTPOST.into(),
        };
        let somewhere = Character {
            map_id: CREATION,
            last_outpost: 148,
        };
        assert_eq!(
            place(
                requested,
                somewhere,
                Some(map_kind::CREATION),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (
                148,
                Some(Substituted::CreationNamedAsPlace("its last outpost"))
            )
        );
    }

    #[test]
    fn a_character_that_has_never_been_anywhere_gets_the_callers_fallback() {
        let requested = Requested {
            map_id: CREATION,
            map_type: map_kind::OUTPOST.into(),
        };
        assert_eq!(
            place(
                requested,
                fresh(),
                Some(map_kind::CREATION),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (FALLBACK, Some(Substituted::NothingToPlace))
        );
    }

    /// `ASURA/crates/asura-rules/src/lib.rs:218-232`: an outpost asked for is an outpost answered.
    #[test]
    fn an_outpost_request_is_answered_as_asked() {
        let requested = Requested {
            map_id: 163,
            map_type: map_kind::OUTPOST.into(),
        };
        let played = Character {
            map_id: 163,
            last_outpost: 163,
        };
        assert_eq!(
            place(
                requested,
                played,
                Some(map_kind::OUTPOST),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (163, None)
        );
    }

    /// `ASURA/crates/asura-rules/src/lib.rs:234-248`: rule 3's substitution.
    #[test]
    fn an_explorable_request_goes_to_the_last_outpost() {
        let requested = Requested {
            map_id: 146,
            map_type: u32::from(map_kind::EXPLORABLE),
        };
        let played = Character {
            map_id: 146,
            last_outpost: 148,
        };
        assert_eq!(
            place(
                requested,
                played,
                Some(map_kind::EXPLORABLE),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (148, Some(Substituted::NotAnOutpost))
        );
    }

    /// `ASURA/crates/asura-rules/src/lib.rs:250-260`: rule 3 with no history at all.
    #[test]
    fn a_character_that_never_saw_an_outpost_and_asks_for_an_explorable_gets_the_fallback() {
        let requested = Requested {
            map_id: 146,
            map_type: u32::from(map_kind::EXPLORABLE),
        };
        assert_eq!(
            place(
                requested,
                fresh(),
                Some(map_kind::EXPLORABLE),
                CREATION,
                FALLBACK,
                MAP_KINDS
            ),
            (FALLBACK, Some(Substituted::NothingToPlace))
        );
    }

    /// `ASURA/crates/asura-rules/src/lib.rs:272-282`: a map no catalogue names is the caller's to
    /// refuse, and the rule leaves it exactly as asked.
    #[test]
    fn an_unknown_map_is_left_for_the_caller_to_refuse() {
        let requested = Requested {
            map_id: 999,
            map_type: u32::from(map_kind::EXPLORABLE),
        };
        assert_eq!(
            place(requested, fresh(), None, CREATION, FALLBACK, MAP_KINDS),
            (999, None)
        );
    }

    /// `ASURA/crates/asura-rules/src/lib.rs:262-270`.
    #[test]
    fn a_name_with_nothing_in_it_is_refused() {
        assert!(!name_is_acceptable(""));
        assert!(!name_is_acceptable("   "));
        assert!(!name_is_acceptable("\u{00a0}"));
        assert!(name_is_acceptable("Asura"));
        assert!(name_is_acceptable("A"));
        assert!(name_is_acceptable("Asura One"));
    }
}
