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
//! **No frame, order or size changed when this moved out of `WorldSession.rs`.**

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::server::packets::{character_packets, item_packets, misc_packets};
use crate::server::protocol::GameVersion;
use crate::server::world_session::WorldSession;

/// What a character-creation connection has chosen so far.
///
/// Reference: `char_creation_campaign` and `primary_profession` on the reference's player, which is
/// the whole of what a creation screen keeps until its confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Creation {
    pub(crate) campaign: u8,
    pub(crate) profession: u8,
}

impl Creation {
    /// The reference's starting state: Warrior, and no campaign until the client says so.
    pub(crate) fn start() -> Self {
        Self {
            campaign: 0,
            profession: character_packets::DEFAULT_PROFESSION,
        }
    }
}

/// The player-data block a creation screen is drawn from, in the reference's order.
///
/// Reference: `GameSrv_HandleCharCreationRequestPlayer` - the player-data start, the inventory stream,
/// the four weapon sets and the gold, the faction caps, the attribute and profession state, the
/// skillbar, the faction record and the agent's properties, then the player-data end. The block is
/// `crate::creation::player_data_block`, because the inventory it carries is content.
pub(crate) fn creation_player_block(
    tables: Option<&crate::creation::Tables>,
    creation: &Creation,
) -> Vec<Vec<u8>> {
    match tables {
        Some(tables) => {
            crate::creation::player_data_block(tables, creation.campaign, creation.profession)
        }
        // A session with no creation data still answers: the block without the inventory, which is
        // what says a profession's equipment is missing rather than that the screen failed.
        None => crate::creation::player_data_block(
            &crate::creation::Tables::default(),
            creation.campaign,
            creation.profession,
        ),
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
        // a profession; the reference reads them and re-draws from that choice.
        if message.len() >= 4 {
            creation.campaign = message[2];
            creation.profession = message[3];
        }
        let frames = creation_player_block(self.creation.as_ref(), creation);
        self.send(
            io,
            peer,
            crypt,
            &frames,
            "the creation screen's player data",
        )
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
    /// (`handle_change_profession`), in the reference's order - the previous set off the client,
    /// the profession, then the new set created and placed.
    pub(crate) async fn answer_creation_change_prof(
        &self,
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
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: the creation screen chose campaign {} and profession {}.",
                creation.campaign, creation.profession
            ),
        );
        // The reference's own answer (`GameSrv_HandleCharCreationChangeProf`), in its order:
        // the previous set comes off the client first, then the profession, then the new set
        // is created and placed. A client still holding the old set asserts on an item created
        // into a slot it thinks is occupied (`Assertion: !bagParent->GetItem(slot)`,
        // `ItCliInv.cpp(105)`, which `ASURA/crates/asura-world/src/creation.rs` records
        // against build 39112 when it omitted the removal).
        let mut frames = Vec::new();
        let tables = self.creation.clone().unwrap_or_default();
        for item in tables
            .inventory(previous.campaign, previous.profession)
            .equipped_items()
        {
            frames.push(item_packets::item_remove(
                item_packets::ITEM_STREAM,
                item.id,
            ));
        }
        frames.push(character_packets::update_unlocked_skills(&[]));
        frames.push(character_packets::player_update_profession(
            self.player_id(),
            creation.profession,
            0,
            0,
        ));
        let equipped_bag = crate::creation::equipped_bag(&tables)
            .map(|bag| bag.id)
            .unwrap_or(0);
        for item in tables
            .inventory(creation.campaign, creation.profession)
            .equipped_items()
        {
            let slot = item.slot;
            frames.push(item_packets::create_named_item(
                &crate::creation::named_item(item),
            ));
            if let Some(slot) = slot {
                frames.push(item_packets::item_move_to_location(
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

    /// The creation screen's confirm: write the character, and say whether it was written.
    ///
    /// Reference: `GameSrv_HandleCharCreationConfirm` - read the name and the packed appearance, write
    /// the rows, answer `0x0188` with the character's id, its name, the reference's own `0x2211` and
    /// the settings the list will carry; a name that is taken is answered with `0x018B` and the
    /// taken-name code. The row is this tree's own (`docs/decisions/0007-character-schema.md`), and
    /// the creation defaults are the reference's: level 1, the helmet shown, the unlocked-profession
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
        let Some(count) = message
            .get(2..4)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
        else {
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
                format!(
                    "Gameplay connection from {peer}: no entropy for a character id; using zero."
                ),
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
            std::sync::Arc::clone(pool),
            self.log.clone(),
        );
        let reply = characters.insert_creation(
            &crate::characters::uuid_text(&id),
            &account,
            &name,
            appearance,
            creation.profession,
            creation.campaign,
            start_map,
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
                    .creation
                    .as_ref()
                    .map(|tables| tables.inventory(creation.campaign, creation.profession))
                    .unwrap_or_default();
                let characters = crate::characters::DatabaseCharacters::new(
                    std::sync::Arc::clone(pool),
                    self.log.clone(),
                );
                match characters.write_starting_items(
                    &crate::characters::uuid_text(&id),
                    &inventory,
                    self.item_templates(),
                ) {
                    Ok(written) => self.say(
                        LogLevel::Debug,
                        format!(
                            "Gameplay connection from {peer}: the character's {written} starting item row(s) are written."
                        ),
                    ),
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

impl WorldSession {
    /// `/.exp <total>` and `/xp`: the one place this world changes a character's experience, and the
    /// line that reads it.
    ///
    /// Ported from `ASURA/crates/asura-world/src/player.rs:462-569` (`set_experience`) and
    /// `ASURA/crates/asura-world/src/chat.rs:612-648` (the two command arms). The curve is the pack's
    /// (`World::experience`); the level is what the total says, not a number the caller chooses
    /// (`characters::experience::apply`). The row is written first, and a refused write changes
    /// nothing. Then the client is told in one of two shapes (`characters::experience::told`): a rise of
    /// one level or less is one `0x00EE {0, delta}`, which moves the bar; a decrease, or a jump of more
    /// than one level, restates the whole `0x00E9` block. **Nothing else is sent**: ASURA's client
    /// crashed on a change that also sent the public level (`0x009F` 36) and the attribute points
    /// (`0x0037`), so the server's level and points are read at the next load (`player.rs:528-539`).
    /// The answer line is ASURA's `tell`, which here is the server's chat pair (`answer_line`).
    pub(crate) async fn answer_experience(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        sheet: &crate::characters::CharacterSheet,
        argument: Option<&str>,
    ) {
        let before = self
            .progress
            .lock()
            .ok()
            .and_then(|progress| *progress)
            .unwrap_or_else(|| sheet.progress());
        let Some(argument) = argument else {
            // `/xp` without a total: the reading half (`chat.rs:624-631`).
            let line = format!("Experience {}; level {}.", before.experience, before.level);
            self.answer_line(io, peer, crypt, &line).await;
            return;
        };
        let Ok(total) = argument.trim().parse::<u32>() else {
            self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: /.exp: {argument:?} is not a total experience"
                ),
            );
            self.answer_line(
                io,
                peer,
                crypt,
                "Usage: /.exp <total experience>, e.g. /.exp 2000",
            )
            .await;
            return;
        };
        let Some(curve) = self.world.experience().copied() else {
            self.answer_line(
                io,
                peer,
                crypt,
                "Experience is unavailable: the pack has no experience curve.",
            )
            .await;
            return;
        };
        let after = crate::characters::experience::apply(&curve, before, total);
        let written = match self.characters() {
            Some(characters) => characters
                .set_progress(&crate::characters::uuid_text(&sheet.uuid), &after)
                .unwrap_or(false),
            None => false,
        };
        if !written {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: /.exp: the database refused the change of {}'s experience",
                    sheet.name
                ),
            );
            self.answer_line(
                io,
                peer,
                crypt,
                "Experience was not changed: the character row refused the write.",
            )
            .await;
            return;
        }
        if let Ok(mut progress) = self.progress.lock() {
            *progress = Some(after);
        }
        self.say(
            LogLevel::Info,
            format!(
                "Gameplay connection from {peer}: {} experience {}, level {} (was {}), skill points {}/{}",
                sheet.name,
                after.experience,
                after.level,
                before.level,
                after.skill_points,
                after.skill_points_total
            ),
        );
        let frames = match crate::characters::experience::told(before, after) {
            crate::characters::experience::Told::Award(delta) => {
                vec![character_packets::agent_kill_reward(
                    character_packets::ATTRIBUTE_EXPERIENCE,
                    delta,
                )]
            }
            crate::characters::experience::Told::Block => {
                vec![crate::agents::player::progress_record(&after)]
            }
            crate::characters::experience::Told::Nothing => Vec::new(),
        };
        if !frames.is_empty() {
            self.send(io, peer, crypt, &frames, "the experience change")
                .await;
        }
        let line = format!("Experience {}; level {}.", after.experience, after.level);
        self.answer_line(io, peer, crypt, &line).await;
    }
}
