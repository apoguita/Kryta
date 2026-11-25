//! The character family: the creation screen - its three asks and its confirm.
//!
//! Reference: `src/server/game/Handlers/CharacterHandler.cpp` - the creation, the delete and
//! the name the client asks about are one handler file there, and the handler reads the message,
//! calls the character object and answers. The behaviour is ASURA's, ported whole:
//! `ASURA/crates/asura-world/src/creation.rs` - the player data (`handle_request_player`,
//! `:190-255`), the PvP half (`handle_request_armors`, `:259-271`), a changed profession
//! (`handle_change_profession`, `:275-341`), and the confirm that writes the row
//! (`handle_confirm`, `:346-519`) - dispatched in that order by
//! `ASURA/crates/asura-world/src/dispatch.rs:88-103`.
//!
//! The decisions are the nouns': the rows are `crate::characters::DatabaseCharacters`
//! (`write_starting_items`, `unlock_map`), the block a screen is drawn from is
//! `crate::creation::player_data_block`, and where a character starts is the world's own first
//! outpost (`WorldSession::starting_outpost`, from R12).
//!
//! Each handler is an `impl WorldSession` method the session's dispatch calls by header.

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::server::packets::{character_packets, item_packets, misc_packets};
use crate::server::protocol::GameVersion;
use crate::server::world_session::{Connection, WorldSession};

/// What a character-creation connection has chosen so far.
///
/// Reference: `char_creation_campaign` and `primary_profession` on GWPS's player, which is
/// the whole of what a creation screen keeps until its confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Creation {
    pub(crate) campaign: u8,
    pub(crate) profession: u8,
    /// The colour the client chose for each equipped slot of the set on screen (`0x8084`), by slot.
    /// `None` is a slot it has not coloured since the set was sent, which keeps the set's own dye.
    pub(crate) colors: [Option<u8>; CREATION_COLOR_SLOTS],
}

/// The equipped slots a creation screen colours: the client sends one `0x8084` for each of slots
/// `4, 2, 3, 6, 5, 0, 1` after every profession change (`research/Eden/docs/re/char-create-protocol.md`
/// 3.2; `recordings/client/feature/character_creation/creation-armor-color-20261124.jsonl` seq 659-665).
pub(crate) const CREATION_COLOR_SLOTS: usize = 7;

/// The item flag that says an item takes no dye. The retail server answers a colour for every
/// worn item but the one that carries it: the Warrior's head (flags `0x20001006`) is sent again
/// re-dyed and the Elementalist's (`0x20001202`) is not, in the same capture (`recordings/client/feature/character_creation/creation-armor-color-20261124.jsonl`
/// seq 1130 and 1138 against seq 1218, which no later message re-sends). The client's own dye rule
/// refuses the same bit (`research/preforged-source/src/runtime/gw_dye_transactions.h:20-30`,
/// "target forbids interaction bit200"), and its creation screen gives such a head a Show checkbox
/// and no colours (`research/Eden/docs/re/char-create-ui.md`, "Head is `HeadNoColor`").
const ITEM_FLAG_TAKES_NO_DYE: u32 = 0x200;

impl Creation {
    /// GWPS's starting state: Warrior, and no campaign until the client says so.
    pub(crate) fn start() -> Self {
        Self {
            campaign: 0,
            profession: character_packets::DEFAULT_PROFESSION,
            colors: [None; CREATION_COLOR_SLOTS],
        }
    }
}

/// The blob a `0x8045` carries: the bytes after the header and the `u16` count, or `None` when the
/// count runs past the message or past the 256 bytes the message declares.
///
/// Measured on the retail client, where the capture holds the client's own struct - the header, the
/// count as a dword, then the bytes: `45000000 32000000 0500661100...` is a count of 50 and a blob
/// that begins `05 00 66 11` (`recordings/client/feature/preferences/weapon-sets-toggle-20261124.jsonl` seq 476). On the
/// wire the count is the `u16` every `Array8` carries.
pub(crate) fn preferences_of(message: &[u8]) -> Option<&[u8]> {
    let count = usize::from(u16::from_le_bytes(message.get(2..4)?.try_into().ok()?));
    if count > character_packets::PREFERENCES_BYTES {
        return None;
    }
    message.get(4..4 + count)
}

impl WorldSession {
    /// `0x8045`: keep the character's preferences blob, and answer nothing.
    pub(crate) fn answer_store_preferences(
        &self,
        peer: &str,
        message: &[u8],
        sheet: &crate::characters::CharacterSheet,
    ) {
        let Some(preferences) = preferences_of(message) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a preferences upload of {} bytes cannot be read; not stored.",
                    message.len()
                ),
            );
            return;
        };
        let Some(characters) = self.characters() else {
            self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: {} bytes of preferences arrived with no character database; not stored.",
                    preferences.len()
                ),
            );
            return;
        };
        let uuid = crate::characters::uuid_text(&sheet.uuid);
        match characters.record_preferences(&uuid, preferences) {
            Ok(true) => self.say(
                LogLevel::Info,
                format!(
                    "Gameplay connection from {peer}: stored {} bytes of preferences for '{}'.",
                    preferences.len(),
                    sheet.name
                ),
            ),
            Ok(false) => self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a preferences upload for '{}' matched no character row.",
                    sheet.name
                ),
            ),
            Err(error) => self.say(
                LogLevel::Error,
                format!("Gameplay connection from {peer}: the preferences could not be stored: {error}"),
            ),
        }
    }
}

/// The value and the mask a `0x8057` carries: the two dwords after the header.
pub(crate) fn equip_visibility_of(message: &[u8]) -> Option<(u32, u32)> {
    let value = u32::from_le_bytes(message.get(2..6)?.try_into().ok()?);
    let mask = u32::from_le_bytes(message.get(6..10)?.try_into().ok()?);
    Some((value, mask))
}

impl WorldSession {
    /// `0x8057`: the inventory panel's display mode chose an option for one slot kind.
    ///
    /// Ported from Rurik's `handle_visibility_flags`
    /// (`research/Rurik/toolkit/authsrv/authsrv.py:36904-36951`), whose reply the retail server's
    /// own now confirms (`recordings/client/raw/creation-armor-in-game-setting-unfiltered.jsonl` seq 482-1164): the byte takes `(flags & !mask) | value`, the client is
    /// answered `0x00EF [value, mask]` - the one message that writes its copy of the byte - and the
    /// body follows in `0x006F [agent, position, item or 0]` for each kind whose shown state on this
    /// kind of map changed. A request the drop-down cannot send is refused with nothing sent.
    /// The byte is kept on the character row, so the next load states it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn answer_equip_visibility(
        &self,
        state: &mut Connection,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        crypt: &mut AuthCrypt,
        sheet: &crate::characters::CharacterSheet,
        map: &crate::world::MapEntry,
    ) {
        use crate::characters::visibility;
        let Some((value, mask)) = equip_visibility_of(message) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a display mode change of {} bytes cannot be read; nothing sent.",
                    message.len()
                ),
            );
            return;
        };
        if let Some(why) = visibility::check_request(value, mask) {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: the display mode change (0x{value:x}, mask 0x{mask:x}) is refused: {why}; nothing sent."
                ),
            );
            return;
        }
        let uuid = crate::characters::uuid_text(&sheet.uuid);
        let characters = self.characters();
        let old = visibility::load_flags(
            characters
                .as_ref()
                .and_then(|characters| characters.visibility_flags(&uuid).unwrap_or(None)),
        );
        let new = visibility::apply(old, value, mask);
        let worn = characters
            .as_ref()
            .and_then(|characters| characters.equipped_item_ids(&uuid, &self.bags()).ok())
            .unwrap_or([0u32; 9]);
        let explorable = crate::server::world_session::is_explorable(map);

        let mut frames = vec![misc_packets::char_visibility_flags(value, mask)];
        for (slot, item) in visibility::slot_changes(old, new, worn, explorable) {
            frames.push(misc_packets::update_agent_visual_equipment_slot(
                state.player_id(),
                slot,
                item,
            ));
        }
        let writes = frames.len() - 1;
        self.send(io, peer, crypt, &frames, "the display mode").await;

        let kept = match characters.map(|characters| characters.record_visibility_flags(&uuid, new)) {
            Some(Ok(true)) => "stored".to_string(),
            Some(Ok(false)) => "NOT stored: no character row matched".to_string(),
            Some(Err(error)) => format!("NOT stored: {error}"),
            None => "NOT stored: no character database".to_string(),
        };
        self.say(
            LogLevel::Info,
            format!(
                "Gameplay connection from {peer}: the display mode of '{}' is 0x{old:02x} -> 0x{new:02x} (value 0x{value:x}, mask 0x{mask:x}); {writes} body slot write(s) in {}; {kept}.",
                sheet.name,
                if explorable { "an explorable area" } else { "a town" }
            ),
        );
    }
}

/// The player-data block a creation screen is drawn from, in GWPS's order.
///
/// Reference: `GameSrv_HandleCharCreationRequestPlayer` - the player-data start, the inventory stream,
/// the four weapon sets and the gold, the faction caps, the attribute and profession state, the
/// skillbar, the faction record and the agent's properties, then the player-data end. The block is
/// `crate::creation::player_data_block`, because the inventory it carries is content.
pub(crate) fn creation_player_block(tables: Option<&crate::creation::Tables>, creation: &Creation) -> Vec<Vec<u8>> {
    match tables {
        Some(tables) => {
            crate::creation::player_data_block(tables, creation.campaign, creation.profession)
        }
        // A session with no creation data still answers: the block without the inventory, which is
        // what says a profession's equipment is missing rather than that the screen failed.
        None => crate::creation::player_data_block(&crate::creation::Tables::default(), creation.campaign, creation.profession),
    }
}

impl WorldSession {
    /// The creation screen's first ask: the block it draws its body and bags from.
    ///
    /// `ASURA/crates/asura-world/src/creation.rs:190-255` (`handle_request_player`): the body is
    /// the two bytes of the client's current choice - a campaign and a profession - the screen
    /// sets, and the block is rebuilt from that choice.
    pub(crate) async fn answer_creation_request_player(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        creation: &mut Creation,
        crypt: &mut AuthCrypt,
    ) {
        // The body is the two bytes of the client's current choice, which is a campaign and
        // a profession; GWPS reads them and re-draws from that choice.
        if message.len() >= 4 {
            creation.campaign = message[2];
            creation.profession = message[3];
        }
        let frames = creation_player_block(Some(self.world.creation()), creation);
        self.send(io, peer, crypt, &frames, "the creation screen's player data")
            .await;
    }

    /// The creation screen's PvP half: `ASURA/crates/asura-world/src/creation.rs:259-271`
    /// (`handle_request_armors`) - the unlocked skills and heroes, both empty, and the item
    /// stream's end.
    pub(crate) async fn answer_creation_request_armors(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
    ) {
        let frames = vec![
            misc_packets::pvp_update_unlocked_skills(&[]),
            misc_packets::pvp_update_unlocked_heroes(&[]),
            misc_packets::pvp_item_stream_end(),
        ];
        self.send(io, peer, crypt, &frames, "the creation screen's PvP half")
            .await;
    }

    /// A chosen profession: `ASURA/crates/asura-world/src/creation.rs:275-341`
    /// (`handle_change_profession`), in GWPS's order - the previous set off the client,
    /// the profession, then the new set created and placed.
    pub(crate) async fn answer_creation_change_prof(
        &self,
        state: &mut Connection,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        creation: &mut Creation,
        crypt: &mut AuthCrypt,
    ) {
        let previous = *creation;
        if message.len() >= 4 {
            creation.campaign = message[2];
            creation.profession = message[3];
        }
        // A new set is on screen with its own dyes. The client says its colours again with the
        // seven `0x8084` that follow this message, and those fill the list back in.
        creation.colors = [None; CREATION_COLOR_SLOTS];
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: the creation screen chose campaign {} and profession {}.",
                creation.campaign, creation.profession
            ),
        );
        // The retail server's answer, message for message (`recordings/client/raw/creation-armor-head-color-unfiltered.jsonl` seq 1194-1225, a change to the
        // Elementalist): every piece of the previous set removed (`0x014D`), the profession
        // (`0x00B7`), the unlocked skills (`0x00DB`), and then for each piece of the new set, the
        // weapon included: the item (`0x0161`), the profession on screen (`0x015A`), the item added
        // to the backpack's slot 0 (`0x013E`), and the item moved from there to its slot of the
        // equipped bag (`0x014B`). The backpack of a creation screen is empty, so slot 0 is free
        // for each piece in turn.
        //
        // A client still holding the old set asserts on an item created into a slot it thinks is
        // occupied (`Assertion: !bagParent->GetItem(slot)`, `ItCliInv.cpp(105)`, which
        // `ASURA/crates/asura-world/src/creation.rs` records against build 39112 when it omitted the
        // removal).
        let mut frames = Vec::new();
        let tables = self.world.creation();
        for item in tables
            .inventory(previous.campaign, previous.profession)
            .equipped_items()
        {
            frames.push(item_packets::item_remove(item_packets::ITEM_STREAM, item.id));
        }
        frames.push(character_packets::player_update_profession(
            state.player_id(),
            creation.profession,
            0,
            0,
        ));
        frames.push(character_packets::update_unlocked_skills(&[]));
        let equipped_bag =
            crate::creation::equipped_bag(tables).map(|bag| bag.id).unwrap_or(0);
        let backpack = crate::creation::backpack(tables).map(|bag| bag.id).unwrap_or(0);
        for item in tables
            .inventory(creation.campaign, creation.profession)
            .equipped_items()
        {
            frames.push(item_packets::create_named_item(
                &crate::creation::named_item(item),
            ));
            frames.push(item_packets::item_set_profession(
                item.id,
                creation.profession,
            ));
            if let Some(slot) = item.slot {
                frames.push(item_packets::item_move_to_location(
                    item_packets::ITEM_STREAM,
                    item.id,
                    backpack,
                    0,
                ));
                frames.push(item_packets::item_change_location(
                    item_packets::ITEM_STREAM,
                    item.id,
                    equipped_bag,
                    slot,
                ));
            }
        }
        self.send(io, peer, crypt, &frames, "the new profession")
            .await;
    }

    /// `0x8084` on the creation screen: the client coloured one equipped slot, and the item in it is
    /// sent again with that colour.
    ///
    /// No research project answers this message - GWPS's handler is an empty function
    /// (`research/OpenTyria/code/OpenTyria/GameSrv.c:1684-1690`) and ASURA has none - so it is ported
    /// from a retail recording of 2026-11-24, `recordings/client/feature/character_creation/creation-armor-color-20261124.jsonl`. The client sends `{u8 slot, u8 colour}` (seq
    /// 1272: `84000000 02000000 05000000` in the client's struct, slot 2 and colour 5), and the
    /// server answers with three messages (seq 1273-1275): the item's `0x0161` exactly as the set
    /// created it but with the colour as its dye colours (`0b000000` at seq 1202, `05000000` at seq
    /// 1273), then `0x015A` with the item and the profession on screen, then `0x015B` with the item's
    /// id twice. The same three answer each of the seven the client sends after a profession change,
    /// for the slots that hold an item that takes dye (`ITEM_FLAG_TAKES_NO_DYE`). The colour is
    /// kept: the created character's items carry it (seq 1314-1330 after the confirm at 1296).
    ///
    /// A client left without this answer shows no colour change and never enables its Create button
    /// (the owner, 2026-11-24, against this tree).
    pub(crate) async fn answer_creation_item_color(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        creation: &mut Creation,
        crypt: &mut AuthCrypt,
    ) {
        let (Some(&slot), Some(&color)) = (message.get(2), message.get(3)) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: an item colour of {} bytes cannot be read.",
                    message.len()
                ),
            );
            return;
        };
        if usize::from(slot) >= CREATION_COLOR_SLOTS {
            return;
        }
        let inventory = self
            .world
            .creation()
            .inventory(creation.campaign, creation.profession);
        let Some(item) = inventory
            .equipped_items()
            .into_iter()
            .find(|item| item.slot == Some(slot))
        else {
            // Nothing is worn there, and the retail server says nothing either (slot 1 above).
            return;
        };
        if item.flags & ITEM_FLAG_TAKES_NO_DYE != 0 {
            return;
        }
        creation.colors[usize::from(slot)] = Some(color);
        let mut named = crate::creation::named_item(item);
        named.dye_colors = u16::from(color);
        let frames = [
            item_packets::create_named_item(&named),
            item_packets::item_set_profession(item.id, creation.profession),
            item_packets::item_creation_redyed(item.id),
        ];
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: the creation screen coloured slot {slot} with colour {color}; item {} is sent again.",
                item.id
            ),
        );
        self.send(io, peer, crypt, &frames, "the item's new colour")
            .await;
    }

    /// The creation screen's confirm: write the character, and say whether it was written.
    ///
    /// Reference: `GameSrv_HandleCharCreationConfirm` - read the name and the packed appearance, write
    /// the rows, answer `0x0188` with the character's id, its name, GWPS's own `0x2211` and
    /// the settings the list will carry; a name that is taken is answered with `0x018B` and the
    /// taken-name code. The row is this tree's own (`docs/decisions/0007-character-schema.md`), and
    /// the creation defaults are GWPS's: level 1, the helmet shown, the unlocked-profession
    /// bit set to the profession that was chosen.
    ///
    /// The confirm's layout is the reference's message table: the `u16` header, a counted name whose
    /// field holds up to twenty units, then the packed appearance word (`ASURA`,
    /// `crates/asura-protocol/src/packets.rs`, the `0x808B` row: header, `String16` of twenty, then
    /// the body's dword).
    pub(crate) async fn confirm_creation(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        creation: &Creation,
        version: &GameVersion,
        crypt: &mut AuthCrypt,
    ) {
        let Some(count) = message.get(2..4).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) else {
            return;
        };
        let name_at = 4;
        let end = name_at + count * 2;
        if message.len() < end + 4 {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a creation confirm is {} bytes and its name needs {}; it cannot be read.",
                    message.len(),
                    end + 4
                ),
            );
            let _ = io.close().await;
            return;
        }
        let units: Vec<u16> = (0..count)
            .map(|index| {
                let at = name_at + index * 2;
                u16::from_le_bytes([message[at], message[at + 1]])
            })
            .collect();
        let name = String::from_utf16_lossy(&units);
        let appearance = u32::from_le_bytes([
            message[end],
            message[end + 1],
            message[end + 2],
            message[end + 3],
        ]);

        let Some(pool) = self.characters.as_ref() else {
            self.say(
                LogLevel::Error,
                format!(
                    "Gameplay connection from {peer}: a character was confirmed, but this world has no character database; it cannot be written."
                ),
            );
            let refusal = character_packets::char_creation_error(
                character_packets::GM_ERROR_CHARACTER_NAME_ALREADY_EXIST,
            );
            let _ = self.send(io, peer, crypt, &[refusal], "the refusal").await;
            return;
        };

        // A fresh identifier for the character: sixteen bytes the client will use from now on.
        let mut id = [0u8; 16];
        if getrandom::getrandom(&mut id).is_err() {
            self.say(
                LogLevel::Warn,
                format!("Gameplay connection from {peer}: no entropy for a character id; using zero."),
            );
        }
        let account = crate::characters::uuid_text(&version.account);

        // Where the character starts, which is not where it was made: the first outpost this world
        // hosts. `GameSrv_HandleCharCreationConfirm` writes the character's starting map into its own
        // row, and the login's list reads it back, so a row without one is a character the client can
        // only be sent back into the creation instance for.
        let starting_outpost = self.starting_outpost();
        let start_map = starting_outpost.map(|map| map.id as u16).unwrap_or(0);

        let characters = crate::characters::DatabaseCharacters::new(
            std::sync::Arc::clone(pool), self.log.clone(),
        );
        let reply = characters.insert_creation(
            &crate::characters::uuid_text(&id), &account, &name, appearance,
            creation.profession, creation.campaign, start_map,
        );

        match reply {
            Ok(false) => {
                // `ON CONFLICT (name) DO NOTHING` wrote nothing: the name is taken.
                self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: the name '{name}' is taken; the creation screen is told so."
                    ),
                );
                let refusal = character_packets::char_creation_error(
                    character_packets::GM_ERROR_CHARACTER_NAME_ALREADY_EXIST,
                );
                let _ = self.send(io, peer, crypt, &[refusal], "the refusal").await;
            }
            Ok(true) => {
                self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: the character '{name}' was created as {} (profession {}, campaign {}).",
                        crate::characters::uuid_text(&id),
                        creation.profession,
                        creation.campaign
                    ),
                );

                // The character's starting items are its own rows now: what a load draws from. The
                // set comes from the creation record list, which is a setting; the rows are state.
                let inventory = self
                    .world
                    .creation()
                    .inventory(creation.campaign, creation.profession);
                let characters = crate::characters::DatabaseCharacters::new(
                    std::sync::Arc::clone(pool),
                    self.log.clone(),
                );
                match characters.write_starting_items(
                    &crate::characters::uuid_text(&id),
                    &inventory,
                    self.item_templates(),
                ) {
                    Ok(written) => {
                        self.say(
                            LogLevel::Debug,
                            format!(
                                "Gameplay connection from {peer}: the character's {written} starting item row(s) are written."
                            ),
                        );
                        // The colours chosen on the creation screen are the items' own from here on:
                        // the retail server's first load after a creation draws them with the colour
                        // (`recordings/client/feature/character_creation/creation-armor-color-20261124.jsonl` seq 1314-1330). Written after the rows, so a row still finds its
                        // template by the dye the set gave it.
                        for item in inventory.equipped_items() {
                            let Some(color) = item
                                .slot
                                .and_then(|slot| creation.colors.get(usize::from(slot)).copied())
                                .flatten()
                            else {
                                continue;
                            };
                            if let Err(error) = characters.dye_item(
                                &crate::characters::uuid_text(&id),
                                item.id,
                                u16::from(color),
                            ) {
                                self.say(
                                    LogLevel::Error,
                                    format!(
                                        "Gameplay connection from {peer}: item {} keeps its set's colour, not the one chosen: {error}",
                                        item.id
                                    ),
                                );
                            }
                        }
                    }
                    Err(error) => self.say(
                        LogLevel::Error,
                        format!(
                            "Gameplay connection from {peer}: the character was created but its items are not written: {error}"
                        ),
                    ),
                }

                // R12: a character starts with one map unlocked. The world's first outpost is the
                // one - the lowest map id this world hosts of that kind - and the gate that reads
                // these rows arrives with the travel answer.
                if let Some(outpost) = starting_outpost {
                    match characters.unlock_map(&crate::characters::uuid_text(&id), outpost.id) {
                        Ok(true) => self.say(
                            LogLevel::Debug,
                            format!(
                                "Gameplay connection from {peer}: map {} ('{}') is unlocked for the new character (R12).",
                                outpost.id, outpost.name
                            ),
                        ),
                        Ok(false) => {}
                        Err(error) => self.say(
                            LogLevel::Error,
                            format!(
                                "Gameplay connection from {peer}: the character was created but its starting map is not unlocked: {error}"
                            ),
                        ),
                    }
                }

                let settings = crate::characters::CharacterSettings {
                    last_outpost: start_map,
                    appearance,
                    campaign: creation.campaign,
                    level: 1,
                    is_pvp: creation.campaign == 0,
                    secondary_profession: 0,
                    helm_status: 1,
                    pieces: Vec::new(),
                };
                let mut answer = vec![character_packets::char_creation_success(
                    id,
                    &name,
                    &settings.to_blob(),
                )];
                // And where the character now is, in the **five-byte login shape**. The client
                // reconnects naming the map it believes it is in, and that belief came from the
                // transfer, which named the creation instance: without this it dials back into the
                // creation screen and waits there forever (`ASURA/crates/asura-world/src/creation.rs`,
                // `handle_confirm`, measured against a real client). The four-byte-header redirect
                // generation is ignored by this client, so it is not the frame to send here.
                match starting_outpost {
                    Some(outpost) => answer.push(misc_packets::map_update_current(
                        outpost.id as u16,
                        0,
                    )),
                    None => self.say(
                        LogLevel::Warn,
                        format!(
                            "Gameplay connection from {peer}: the character '{name}' is created and this world hosts no outpost, so the client is told no map and will dial the creation instance again."
                        ),
                    ),
                }
                let _ = self
                    .send(io, peer, crypt, &answer, "the character creation")
                    .await;
            }
            Err(error) => {
                self.say(
                    LogLevel::Error,
                    format!(
                        "Gameplay connection from {peer}: the character '{name}' could not be written: {error}"
                    ),
                );
                let refusal = character_packets::char_creation_error(
                    character_packets::GM_ERROR_CHARACTER_NAME_ALREADY_EXIST,
                );
                let _ = self.send(io, peer, crypt, &[refusal], "the refusal").await;
            }

        }
    }
}

#[cfg(test)]
mod preferences_tests {
    use super::preferences_of;

    #[test]
    fn a_preferences_upload_is_its_counted_bytes() {
        // On the wire: the header, a `u16` count, the bytes. The blob is the one the retail client
        // uploaded with its weapon sets unticked
        // (`recordings/client/feature/preferences/weapon-sets-toggle-20261124.jsonl` seq 476), which begins `05 00 66 11`.
        let mut message = vec![0x45, 0x80, 50, 0];
        message.extend([0x05, 0x00, 0x66, 0x11]);
        message.extend([0u8; 46]);
        let blob = preferences_of(&message).expect("a whole upload");
        assert_eq!(blob.len(), 50);
        assert_eq!(&blob[..4], &[0x05, 0x00, 0x66, 0x11]);

        // A count past the message, or past the 256 bytes the message declares, is not read.
        assert_eq!(preferences_of(&message[..30]), None);
        assert_eq!(preferences_of(&[0x45, 0x80, 0x01, 0x01]), None);
        assert_eq!(preferences_of(&[0x45, 0x80]), None);
        // An empty upload is an empty blob.
        assert_eq!(preferences_of(&[0x45, 0x80, 0, 0]), Some(&[][..]));
    }
}
