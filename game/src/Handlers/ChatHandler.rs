//! The chat family: `GAME_CMSG_SEND_CHAT_MESSAGE` (`0x8064`), a spoken line or one of the reference's
//! own commands.
//!
//! Reference: `src/server/game/Handlers/ChatHandler.cpp`, which reads the message, applies the language
//! and command gates, and calls into the chat object. The behaviour is the GameServer's, ported whole:
//! `research/OpenTyria/code/OpenTyria/GmChat.c:183-210` (`GameSrv_HandleChatMessage`) for the channel
//! rule, `:163-181` (`GameSrv_HandleLocalMessage`) for a spoken line, `:145-161`
//! (`GameSrv_HandleChatCommand`) for a command and `:116-143` (`GameSrv_SendInvalidCommand`) for the
//! answer. The command table and the answers are `game/src/Chat/`; a command that teleports ends in
//! `answer_teleport_command`, which is the travel family's (`Handlers/TravelHandler.rs`).
//!
//! **A line is a pair and it is sent.** The text body (`0x005D`, fragmented at 121 units and wrapped as
//! the coded string the client parses) then exactly one commit tag: `0x0061` for a player's line,
//! `0x005E` for one the server composed. A lone body renders nothing, and the client renders no local
//! copy of a typed line either, so the echo is the only render path
//! (`research/Rurik/studies/chat/FINDINGS.md:263-268`). `docs/decisions/0012-chat-text-is-sent.md`
//! records why the guessed table this family carried before is wrong.

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::server::packets::chat_packets;
use crate::server::world_session::{CharacterSheet, WorldSession};

impl WorldSession {
    /// `GAME_CMSG_SEND_CHAT_MESSAGE` (`0x8064`): the client speaks.
    ///
    /// The frame is `{agent u32, text String16}` - the table's own field list
    /// (`ASURA/docs/reference/client-protocol.md:920-926`), which is why the text's count sits four
    /// bytes in - and the **first unit of the text is the channel**: `!` all, `#` group, `$` trade,
    /// `/` a command, `@`/`%`/`"` dropped as the reference drops them, and anything else refused
    /// (`GmChat.c:193-207`).
    pub(crate) async fn answer_chat_message(
        &self,
        io: &mut SessionIo,
        peer: &str,
        message: &[u8],
        crypt: &mut AuthCrypt,
        sheet: &CharacterSheet,
    ) {
        let body = &message[2..];
        if body.len() < 6 {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: a chat line is {} bytes and carries no text; it cannot be read.",
                    message.len()
                ),
            );
            return;
        }
        let units = u16::from_le_bytes([body[4], body[5]]) as usize;
        let text: Vec<u16> = (0..units)
            .map(|index| {
                let at = 6 + index * 2;
                u16::from_le_bytes([
                    body.get(at).copied().unwrap_or(0),
                    body.get(at + 1).copied().unwrap_or(0),
                ])
            })
            .collect();
        let Some(channel) = text.first().copied() else {
            // An empty message, which the reference drops.
            return;
        };
        let spoken: String = char::decode_utf16(text[1.min(text.len())..].iter().copied())
            .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();

        match u8::try_from(channel).unwrap_or(0) as char {
            // `Channel_All`, `Channel_Group`, `Channel_Trade`: the three channels the reference speaks
            // on. The body is the coded string the client parses, so a line carrying a terminator or a
            // literal mark would end the run early and be read as a different message: the reference
            // screens the user's text for exactly those (`GmText.c:32-41`, `GmText_ValidateUserMessage`).
            speaker @ ('!' | '#' | '$') => {
                if !chat_packets::valid_user_message(&text[1.min(text.len())..]) {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Gameplay connection from {peer}: {} sent a chat line carrying a coded-string terminator; it is refused, as the reference refuses it.",
                            sheet.name
                        ),
                    );
                    return;
                }
                let channel = match speaker {
                    '#' => chat_packets::CHANNEL_GROUP,
                    '$' => chat_packets::CHANNEL_TRADE,
                    _ => chat_packets::CHANNEL_ALL,
                };
                let Some(player) = self.player_tag(peer) else {
                    return;
                };
                let mut frames = chat_packets::all_chat_frames(&spoken);
                frames.push(chat_packets::chat_message_local(player, channel));
                // Everyone in the map and district, the speaker included: the reference broadcasts the
                // body and the tag (`GmChat.c:176-177`, `GameSrv_BroadcastChatMessage*`), and the
                // speaker's own client shows nothing unless it is told.
                if let Ok(peers) = self.peers.lock() {
                    peers.broadcast(self.player_id(), self.map_id(), self.district(), &frames);
                }
                self.send(io, peer, crypt, &frames, "a spoken line").await;
                self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: {} spoke {spoken:?} on channel {channel}.",
                        sheet.name
                    ),
                );
            }
            '/' => {
                // The command surface is a file (`config/Server/commands.json`): the names, whether a name takes
                // an argument, and the operation it runs are data, and Rust keeps the operations
                // (`docs/decisions/0011-the-command-surface-is-content.md`). The reference's two
                // dispatch points are that file's rows - `GmCommands.c` for `/tp`, `/teleport` and
                // `/maps`, `GmChat.c` for `/stuck` - and every one of them names a registration in the
                // world's own registry. There is no match on a command here.
                let spoken = spoken.trim();
                let Some((row, values)) = self.commands.resolve_chat(spoken) else {
                    // `GameSrv_SendInvalidCommand`: the reference's text is "Unknown command"
                    // (`GmChat.c:130-140`). A command asked with the wrong shape is this answer too:
                    // the reference checks `argument.is_empty()` per command (`GmCommand_ToAscii`).
                    self.answer_line(io, peer, crypt, "Unknown command").await;
                    return;
                };
                let Some(tag) = self.player_tag(peer) else {
                    return;
                };
                let registry = self.world.operations();
                let invocation = match registry.invocation(
                    &row.operation,
                    crate::chat::operations::Arguments::of(values),
                    crate::chat::operations::Caller::chat(tag),
                ) {
                    Ok(invocation) => invocation,
                    Err(refusal) => {
                        self.answer_line(io, peer, crypt, &refusal.message).await;
                        return;
                    }
                };
                // The agent this connection holds, taken through `with_agent` so the standing it
                // records stays recorded, and handed to the operation as the caller's own.
                let agent = self.with_agent(|agent| (*agent).clone());
                let outcome = match registry.invoke(&invocation, &self.world, agent.as_ref()) {
                    Ok(outcome) => outcome,
                    Err(refusal) => {
                        self.answer_line(io, peer, crypt, &refusal.message).await;
                        return;
                    }
                };
                match outcome {
                    // Text: the wire half is answered, the log half is logged - the reference's two
                    // outputs for `/maps` (`chat.rs:288-324`).
                    crate::chat::operations::Outcome::Text { wire, log } => {
                        for line in &wire {
                            self.answer_line(io, peer, crypt, line).await;
                        }
                        for line in &log {
                            self.say(
                                LogLevel::Debug,
                                format!("Gameplay connection from {peer}: {line}"),
                            );
                        }
                    }
                    crate::chat::operations::Outcome::Frames(frames) => {
                        self.send(io, peer, crypt, &frames, "an operation's frames")
                            .await;
                    }
                    crate::chat::operations::Outcome::Transfer { argument } => {
                        self.answer_teleport_command(io, peer, crypt, sheet, &argument)
                            .await;
                    }
                    crate::chat::operations::Outcome::Resign => {
                        self.answer_resign(io, peer, crypt).await;
                    }
                    crate::chat::operations::Outcome::Guild { argument } => {
                        self.answer_guild_command(io, peer, crypt, sheet, &argument)
                            .await;
                    }
                    crate::chat::operations::Outcome::Experience { argument } => {
                        self.answer_experience(io, peer, crypt, sheet, argument.as_deref())
                            .await;
                    }
                }
            }
            other => {
                // `@`, `%` and `"` are ignored by the reference; anything else it refuses
                // (`GmChat.c:195-206`).
                if matches!(other, '@' | '%' | '"') {
                    self.say(
                        LogLevel::Debug,
                        format!(
                            "Gameplay connection from {peer}: channel {other:?} ignored, as the reference ignores it."
                        ),
                    );
                } else {
                    self.say(
                        LogLevel::Debug,
                        format!(
                            "Gameplay connection from {peer}: chat on channel {other:?} refused, as the reference refuses it."
                        ),
                    );
                }
            }
        }
    }

    /// One line the server composed, on the wire and in the log.
    ///
    /// Reference: `GameSrv_SendInvalidCommand` (`GmChat.c:116-143`) - the body, then `0x005E` on
    /// `Channel_Global`. **One divergence, named:** the reference broadcasts the body to every
    /// connection but sends the tag to the asker alone, which leaves an orphaned body in every other
    /// client's buffer; both frames go to the asker here, because an unanswered body is what the client
    /// silently holds (`Rurik/studies/chat/FINDINGS.md:49-50`).
    pub(crate) async fn answer_line(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        text: &str,
    ) {
        let Some(player) = self.player_tag(peer) else {
            return;
        };
        let mut frames = chat_packets::all_chat_frames(text);
        frames.push(chat_packets::chat_message_server(
            player,
            chat_packets::CHANNEL_GLOBAL,
        ));
        self.send(io, peer, crypt, &frames, "an answer").await;
        self.say(
            LogLevel::Debug,
            format!("Gameplay connection from {peer}: {text}"),
        );
    }

    /// `/resign`: mark this member, then die after the pack's delay. When every present member has
    /// resigned, the party is defeated (`0x01D9`) and its leader's `0x80A7` becomes live.
    pub(crate) async fn answer_resign(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
    ) {
        // `/resign` belongs to an explorable, not an outpost. A town has nothing to resign from, and
        // the live client does not put a member into the resign flow there.
        if self.world.map(self.map_id()).and_then(|map| map.kind)
            != Some(crate::world::map_kind::EXPLORABLE)
        {
            self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: /resign is ignored outside an explorable area"
                ),
            );
            return;
        }
        let me = self.player_id();
        let map_id = self.map_id();
        let district = self.district();
        let party_id = match self.world.parties().lock() {
            Ok(mut parties) => {
                let table = parties.table(map_id, district);
                let Some(party) = table.party_of(me) else {
                    return;
                };
                if party.moving || party.resigned.contains(&me) {
                    return;
                }
                let party_id = party.party_id;
                if let Some(party) = table.parties.get_mut(&party_id) {
                    party.resigned.insert(me);
                }
                party_id
            }
            Err(_) => return,
        };
        let seconds = u64::from(self.settings.resign_secs);
        let world = std::sync::Arc::clone(&self.world);
        let peers = std::sync::Arc::clone(&self.peers);
        let agent = me;
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: player {me} resigned; its death is due in {seconds} s"
            ),
        );
        // The live client gets a server chat line after the `/resign` request; the captured pair is
        // `0x005D` then `0x005E` (`multiplay-live-20261106-200638.jsonl` seq 16-17).
        self.answer_line(io, peer, crypt, "You have resigned.")
            .await;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            let (present, all, first_defeat) = {
                let parties_arc = world.parties();
                let Ok(mut parties) = parties_arc.lock() else {
                    return;
                };
                let Some(party) = parties.table(map_id, district).parties.get_mut(&party_id) else {
                    return;
                };
                if !party.members.contains(&me) || !party.dead.insert(me) {
                    return;
                }
                let present = party.present();
                let all = !present.is_empty()
                    && present.iter().all(|member| party.resigned.contains(member));
                let first_defeat = all && !party.defeated;
                if first_defeat {
                    party.defeated = true;
                }
                (present, all, first_defeat)
            };
            let death = [
                crate::server::packets::agent_packets::agent_player_die(agent),
                crate::server::packets::agent_packets::agent_update_flags(agent, 4),
            ];
            if let Ok(peers) = peers.lock() {
                for member in &present {
                    peers.send_to(*member, &death);
                }
                if all && first_defeat {
                    for member in &present {
                        peers.send_to(
                            *member,
                            &[crate::server::packets::party_packets::party_defeated()],
                        );
                    }
                }
            }
        });
    }

    /// This connection's player id as the chat tag carries it: `0x0061` and `0x005E` hold a `u16`
    /// (`research/OpenTyria/code/OpenTyria/GameMsg.h:591-595`, `msgdefs.info` `GAME_SMSG_0097`), and the
    /// ids this world allocates start at one and climb per map (`Districts/Peers.rs`, `allocate`), so the
    /// bound holds. A connection that ever outgrew it is refused rather than truncated.
    fn player_tag(&self, peer: &str) -> Option<u16> {
        let id = self.player_id();
        match u16::try_from(id) {
            Ok(tag) => Some(tag),
            Err(_) => {
                self.say(
                    LogLevel::Warn,
                    format!(
                        "Gameplay connection from {peer}: player id {id} does not fit the chat tag's 16 bits; nothing is sent."
                    ),
                );
                None
            }
        }
    }
}
