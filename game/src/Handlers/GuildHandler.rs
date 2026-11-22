//! The guild family: the guild block a load carries, `/guild create`, and the four requests this world
//! answers - create (`0x8036`), invite (`0x80B5`), leave (`0x80BB`) and the announcement (`0x80BE`).
//!
//! Reference: `src/server/game/Handlers/GuildHandler.cpp` - the handler reads the message, asks the guild
//! for the decision and answers. Every batch, order, value and refusal is ported 1:1 from
//! `ASURA/crates/asura-world/src/guild.rs` (819 lines): `load` `:97-107`, `badge` `:109-115`, `refuse`
//! `:220-243`, `next_revision` `:245-252`, `handle_guild_command` `:254-294`, `handle_create`
//! `:296-410`, `handle_invite` `:412-529`, `handle_leave` `:531-627`, `handle_announcement`
//! `:629-710`, and the load's splice at `ASURA/crates/asura-world/src/player.rs:740-756`. The rules are
//! `game/Guilds/mod.rs`, the rows `game/Guilds/Repository.rs`, the bytes
//! `game/Server/Packets/GuildPackets.rs`.
//!
//! What ASURA does not send, this does not send, and for the same reasons (ASURA `docs/41`, section 6):
//! no gold debit or credit (`0x014F`, `0x0140`: guilds are free), no server chat line in the creation
//! batch (its coded content is not decoded), nothing to the invited character (its `0x0123` and the
//! accept are not captured), nothing to other members on an announcement. Not built in ASURA and so not
//! here: the accept, kicks, promotions, guild chat, guild halls, capes, and the roster and history a load
//! sends beyond the founder's own row.

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::guilds::{self, Badge, DatabaseGuilds, Membership, Refused};
use crate::server::packets::guild_packets as wire;
use crate::server::world_session::{CharacterSheet, WorldSession};

impl WorldSession {
    /// The guild tables, when the program opened the character database.
    fn guild_rows(&self) -> Option<DatabaseGuilds> {
        self.characters
            .as_ref()
            .map(|pool| DatabaseGuilds::new(std::sync::Arc::clone(pool), self.log.clone()))
    }

    /// A copy of this connection's membership.
    fn membership(&self) -> Option<Membership> {
        self.guild.lock().ok().and_then(|guild| guild.clone())
    }

    /// The badge a membership is shown with in this connection's copy of the map, allocating the
    /// guild's local id there (`guild.rs:109-115`).
    fn badge_of(&self, membership: Option<&Membership>) -> Option<Badge> {
        let membership = membership?;
        let local_id = self.peers.lock().ok()?.guild_local_id(
            self.map_id(),
            self.district(),
            membership.guild.id,
        );
        Some(Badge {
            local_id,
            guild: membership.guild.clone(),
        })
    }

    /// Read this character's guild when its players are asked for, and answer the badge it is shown
    /// with (`guild.rs:97-115`, `load` and `badge`). A database error is logged and read as no guild: a
    /// load must not fail because a side table could not be read.
    pub(crate) fn load_guild(&self, peer: &str, sheet: &CharacterSheet) -> Option<Badge> {
        let found = match self.guild_rows() {
            Some(rows) => match rows.membership(&sheet.uuid) {
                Ok(found) => found.map(|(guild, member)| Membership { guild, member }),
                Err(error) => {
                    self.say(
                        LogLevel::Warn,
                        format!(
                            "Gameplay connection from {peer}: guild: could not read the guild of '{}': {error}",
                            sheet.name
                        ),
                    );
                    None
                }
            },
            None => None,
        };
        let badge = self.badge_of(found.as_ref());
        if let Ok(mut guild) = self.guild.lock() {
            *guild = found;
        }
        badge
    }

    /// The guild block of this character's own load, put in front of its party block
    /// (`ASURA/crates/asura-world/src/player.rs:740-756`): `0x011C` alone for a character in no guild.
    pub(crate) fn splice_guild_block(
        &self,
        peer: &str,
        block: &mut Vec<Vec<u8>>,
        badge: Option<&Badge>,
        agent: u32,
    ) {
        let membership = self.membership();
        let frames = guilds::load_frames(badge, membership.as_ref(), agent);
        if !guilds::splice_into_player_block(block, frames) {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: guild: the player block has no PARTY_CREATE anchor; no guild block sent"
                ),
            );
        }
    }

    /// Answer a refused request with `0x0132 {request_id, 14}` and log why (`guild.rs:220-243`).
    #[allow(clippy::too_many_arguments)]
    async fn refuse_guild(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        name: &str,
        request_id: u32,
        what: &str,
        why: impl std::fmt::Debug,
    ) {
        self.say(
            LogLevel::Debug,
            format!("Gameplay connection from {peer}: guild: {name:?} {what}: refused ({why:?})"),
        );
        let frames = [wire::guild_request_result(
            request_id,
            wire::GUILD_RESULT_REFUSED,
        )];
        self.send(io, peer, crypt, &frames, "a guild refusal").await;
    }

    /// A price the client quoted or the file states, logged and not charged (`guild.rs:208-218`).
    fn note_price(&self, peer: &str, what: &str, configured: u32, quoted: Option<u32>) {
        if configured != 0 || quoted.is_some_and(|quoted| quoted != 0) {
            self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: guild: {what} is free; the configured price {configured} is a placeholder{}",
                    quoted
                        .map(|quoted| format!(" and the client quoted {quoted}"))
                        .unwrap_or_default()
                ),
            );
        }
    }

    /// The next value of a guild's `0x0133` counter, stored. A guild that is gone keeps the value it had
    /// plus one step, unstored (`guild.rs:245-252`).
    fn next_revision(
        &self,
        rows: &DatabaseGuilds,
        guild: &guilds::Guild,
        step: u32,
    ) -> Result<u32, String> {
        Ok(rows
            .advance_revision(&guild.id, step)?
            .unwrap_or(guild.revision.wrapping_add(step)))
    }

    /// `/guild <verb>` (`guild.rs:254-294`): `/guild create` opens the Guild Registration window, owned
    /// by the player's own agent because no registrar is spawned; the form it draws sends `0x8036`. A
    /// character already in a guild is not shown the window. Everything else logs the usage.
    pub(crate) async fn answer_guild_command(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        sheet: &CharacterSheet,
        argument: &str,
    ) {
        if argument.trim() != "create" {
            self.say(
                LogLevel::Debug,
                format!("Gameplay connection from {peer}: /guild: Use: /guild create"),
            );
            return;
        }
        if let Some(membership) = self.membership() {
            self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: /guild create: {:?} is in {:?} already; no window",
                    sheet.name, membership.guild.name
                ),
            );
            return;
        }
        let frames = wire::registration_window(self.player_id());
        self.send(io, peer, crypt, &frames, "the guild registration window")
            .await;
    }

    /// `0x8036`: create a guild with this character as its leader (`guild.rs:296-410`).
    ///
    /// The batch: `0x00CC {8}`, `0x0131`, `0x0121`, property 30 on the founder's agent,
    /// `0x0118 {local, key, 5}`, `0x011F` with the founding line, `0x012F` empty, `0x0127` for the
    /// founder, `0x0133 {revision}`, `0x011C`. Nothing follows a refusal: the live answer to one is not
    /// captured, and creation carries no request id to answer. The other players in the copy are sent
    /// the founder's badge (**not captured**, ASURA's own choice).
    pub(crate) async fn answer_guild_create(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        message: &[u8],
        sheet: &CharacterSheet,
    ) {
        let Some(request) = wire::read_create_request(message.get(2..).unwrap_or_default()) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: guild: a create request could not be read"
                ),
            );
            return;
        };
        let (Some(settings), Some(rows)) = (self.world.guilds().copied(), self.guild_rows()) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: guild: no guild settings or no character database; guild requests are refused"
                ),
            );
            return;
        };
        self.note_price(
            peer,
            "creation",
            settings.creation_price,
            Some(request.quoted),
        );
        if let Err(why) = guilds::may_create(guilds::Creation {
            in_guild: self.membership().is_some(),
            name_units: request.name.encode_utf16().count(),
            name_capacity: wire::GUILD_NAME_UNITS,
            tag_units: request.tag.encode_utf16().count(),
            tag_capacity: wire::GUILD_TAG_UNITS,
        }) {
            self.say(
                LogLevel::Debug,
                format!(
                    "Gameplay connection from {peer}: guild: {:?} creates {:?} [{}]: refused ({why:?}); nothing sent",
                    sheet.name, request.name, request.tag
                ),
            );
            return;
        }
        let key = match guilds::new_key() {
            Ok(key) => key,
            Err(error) => {
                self.say(
                    LogLevel::Error,
                    format!(
                        "Gameplay connection from {peer}: guild: no key could be drawn: {error}"
                    ),
                );
                return;
            }
        };
        let created = rows.create(
            key,
            &request.name,
            &request.tag,
            &settings.new_guild(),
            &sheet.uuid,
            &sheet.name,
            wire::GUILD_MEMBER_LEADER,
        );
        let (guild, member) = match created {
            Ok(Some(created)) => created,
            Ok(None) => {
                self.say(
                    LogLevel::Debug,
                    format!(
                        "Gameplay connection from {peer}: guild: {:?} creates {:?}: refused (the name is taken, or it is in a guild); nothing sent",
                        sheet.name, request.name
                    ),
                );
                return;
            }
            Err(error) => {
                self.say(
                    LogLevel::Error,
                    format!("Gameplay connection from {peer}: guild: the creation was not written: {error}"),
                );
                return;
            }
        };
        let membership = Membership { guild, member };
        let Some(badge) = self.badge_of(Some(&membership)) else {
            return;
        };
        let agent = self.player_id();
        let date = wire::guild_date(membership.member.joined_at);

        let mut frames = vec![
            wire::trader_transaction_done(wire::GUILD_REGISTRAR_WINDOW),
            wire::guild_stream_start(),
        ];
        frames.extend(guilds::badge_frames(&badge, agent));
        frames.push(wire::guild_player_role(
            badge.local_id,
            membership.guild.id,
            membership.member.member_type,
        ));
        frames.push(wire::guild_history_entry(
            date,
            &wire::founded_line(&sheet.name),
        ));
        frames.push(wire::guild_announcement(
            &membership.guild.announcement,
            &membership.guild.announcement_author,
        ));
        frames.push(wire::guild_player_info(&wire::GuildPlayer {
            invited_name: &membership.member.invited_name,
            current_name: &sheet.name,
            invited_by: &membership.member.invited_by,
            context: "",
            minutes_since_login: 0,
            join_date: date,
            status: wire::GUILD_STATUS_ONLINE,
            member_type: membership.member.member_type,
        }));
        frames.push(wire::guild_revision(membership.guild.revision));
        frames.push(wire::guild_stream_end());

        self.say(
            LogLevel::Info,
            format!(
                "Gameplay connection from {peer}: guild: {:?} founds {:?} [{}], local id {}",
                sheet.name, membership.guild.name, membership.guild.tag, badge.local_id
            ),
        );
        if let Ok(mut peers) = self.peers.lock() {
            peers.broadcast(
                agent,
                self.map_id(),
                self.district(),
                &guilds::badge_frames(&badge, agent),
            );
            peers.wear_guild(agent, Some(badge));
        }
        if let Ok(mut guild) = self.guild.lock() {
            *guild = Some(membership);
        }
        self.send(io, peer, crypt, &frames, "the guild's creation")
            .await;
    }

    /// `0x80B5`: invite a character, by name, to this character's guild (`guild.rs:412-529`).
    ///
    /// A successful invitation is `0x0132 {request, 0}`, `0x0127` for the invitee - its name, no current
    /// name, the inviter, the date, status 0, type 1 - and `0x0133`. A refusal is `0x0132 {request, 14}`.
    /// The invited character is told nothing (**not captured**). "Online" is this world's own copies.
    pub(crate) async fn answer_guild_invite(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        message: &[u8],
        sheet: &CharacterSheet,
    ) {
        let Some(request) = wire::read_invite_request(message.get(2..).unwrap_or_default()) else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: guild: an invite request could not be read"
                ),
            );
            return;
        };
        let (Some(settings), Some(rows)) = (self.world.guilds().copied(), self.guild_rows()) else {
            self.refuse_guild(
                io,
                peer,
                crypt,
                &sheet.name,
                request.request_id,
                "invites",
                "no guild settings",
            )
            .await;
            return;
        };
        self.note_price(peer, "an invitation", settings.invite_fee, None);
        let membership = self.membership();
        let leads = membership.as_ref().map(Membership::leads);
        let looked_up = (|| -> Result<_, String> {
            let found = rows.character_named(&request.name)?;
            let target = match &found {
                None => guilds::Target::Unknown,
                Some((id, name)) => guilds::Target::Found {
                    is_self: *id == sheet.uuid,
                    online: self
                        .peers
                        .lock()
                        .map(|peers| peers.is_online(name))
                        .unwrap_or(false),
                    in_guild: rows.is_member(id)?,
                    already_invited: match &membership {
                        Some(membership) => rows.is_invited(&membership.guild.id, id)?,
                        None => false,
                    },
                },
            };
            Ok((found, target))
        })();
        let (found, target) = match looked_up {
            Ok(looked_up) => looked_up,
            Err(error) => {
                self.refuse_guild(
                    io,
                    peer,
                    crypt,
                    &sheet.name,
                    request.request_id,
                    "invites",
                    error,
                )
                .await;
                return;
            }
        };
        if let Err(why) = guilds::may_invite(leads, target) {
            self.refuse_guild(
                io,
                peer,
                crypt,
                &sheet.name,
                request.request_id,
                &format!("invites {:?}", request.name),
                why,
            )
            .await;
            return;
        }
        let (Some(membership), Some((target_id, target_name))) = (membership, found) else {
            self.refuse_guild(
                io,
                peer,
                crypt,
                &sheet.name,
                request.request_id,
                "invites",
                "no guild or no target",
            )
            .await;
            return;
        };
        let guild = membership.guild;
        let invited = rows
            .invite(&guild.id, &target_id, &target_name, &sheet.name)
            .and_then(|invited_at| match invited_at {
                Some(invited_at) => self
                    .next_revision(&rows, &guild, settings.revision_step)
                    .map(|revision| Some((invited_at, revision))),
                None => Ok(None),
            });
        let (invited_at, revision) = match invited {
            Ok(Some(invited)) => invited,
            Ok(None) => {
                self.refuse_guild(
                    io,
                    peer,
                    crypt,
                    &sheet.name,
                    request.request_id,
                    "invites",
                    Refused::AlreadyInvited,
                )
                .await;
                return;
            }
            Err(error) => {
                self.refuse_guild(
                    io,
                    peer,
                    crypt,
                    &sheet.name,
                    request.request_id,
                    "invites",
                    error,
                )
                .await;
                return;
            }
        };
        if let Ok(mut held) = self.guild.lock()
            && let Some(membership) = held.as_mut()
        {
            membership.guild.revision = revision;
        }
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: guild: {:?} invites {target_name:?} to {:?}",
                sheet.name, guild.name
            ),
        );
        let frames = [
            wire::guild_request_result(request.request_id, wire::GUILD_RESULT_OK),
            wire::guild_player_info(&wire::GuildPlayer {
                invited_name: &target_name,
                current_name: "",
                invited_by: &sheet.name,
                context: "",
                minutes_since_login: 0,
                join_date: wire::guild_date(invited_at),
                status: wire::GUILD_STATUS_OFFLINE,
                member_type: wire::GUILD_MEMBER_INVITED,
            }),
            wire::guild_revision(revision),
        ];
        self.send(io, peer, crypt, &frames, "the guild invitation")
            .await;
    }

    /// `0x80BB`: leave the guild (`guild.rs:531-627`).
    ///
    /// The batch: `0x0128` with the leaver's roster name, `0x0133`, property 30 on its agent set to 0,
    /// `0x0119`, `0x0132 {request, 0}`. A leader leaves only as the last member, and the guild goes with
    /// it (ASURA's rule: no capture shows a leader leaving). The other players in the copy are sent the
    /// property set to 0 (**not captured**).
    pub(crate) async fn answer_guild_leave(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        message: &[u8],
        sheet: &CharacterSheet,
    ) {
        let Some(request_id) = wire::read_leave_request(message.get(2..).unwrap_or_default())
        else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: guild: a leave request could not be read"
                ),
            );
            return;
        };
        let (Some(settings), Some(rows)) = (self.world.guilds().copied(), self.guild_rows()) else {
            self.refuse_guild(
                io,
                peer,
                crypt,
                &sheet.name,
                request_id,
                "leaves",
                "no guild settings",
            )
            .await;
            return;
        };
        let membership = self.membership();
        let leads = membership.as_ref().map(Membership::leads);
        let others = match &membership {
            Some(membership) => match rows.other_members(&membership.guild.id, &sheet.uuid) {
                Ok(others) => others,
                Err(error) => {
                    self.refuse_guild(io, peer, crypt, &sheet.name, request_id, "leaves", error)
                        .await;
                    return;
                }
            },
            None => 0,
        };
        let departure = match guilds::may_leave(leads, others) {
            Ok(departure) => departure,
            Err(why) => {
                self.refuse_guild(io, peer, crypt, &sheet.name, request_id, "leaves", why)
                    .await;
                return;
            }
        };
        let Some(membership) = membership else {
            self.refuse_guild(
                io,
                peer,
                crypt,
                &sheet.name,
                request_id,
                "leaves",
                "no guild",
            )
            .await;
            return;
        };
        let step = settings.revision_step;
        let revision = match departure {
            guilds::Departure::Leaves => rows
                .leave(&sheet.uuid)
                .and_then(|_| self.next_revision(&rows, &membership.guild, step)),
            guilds::Departure::Dissolves => rows
                .dissolve(&membership.guild.id)
                .map(|_| membership.guild.revision.wrapping_add(step)),
        };
        let revision = match revision {
            Ok(revision) => revision,
            Err(error) => {
                self.refuse_guild(io, peer, crypt, &sheet.name, request_id, "leaves", error)
                    .await;
                return;
            }
        };
        if let Ok(mut guild) = self.guild.lock() {
            *guild = None;
        }
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: guild: {:?} leaves {:?} ({departure:?})",
                sheet.name, membership.guild.name
            ),
        );
        let agent = self.player_id();
        let cleared = guilds::guild_property(agent, 0);
        if let Ok(mut peers) = self.peers.lock() {
            peers.wear_guild(agent, None);
            peers.broadcast(
                agent,
                self.map_id(),
                self.district(),
                std::slice::from_ref(&cleared),
            );
        }
        let frames = [
            wire::guild_player_remove(&membership.member.invited_name),
            wire::guild_revision(revision),
            cleared,
            wire::guild_none(),
            wire::guild_request_result(request_id, wire::GUILD_RESULT_OK),
        ];
        self.send(io, peer, crypt, &frames, "the guild departure")
            .await;
    }

    /// `0x80BE`: set the guild's announcement (`guild.rs:629-710`).
    ///
    /// The batch: `0x012F {text, author}`, `0x0133`, `0x0132 {request, 0}`. Sent to the one who set it;
    /// the other members online are told nothing (**not captured**).
    pub(crate) async fn answer_guild_announcement(
        &self,
        io: &mut SessionIo,
        peer: &str,
        crypt: &mut AuthCrypt,
        message: &[u8],
        sheet: &CharacterSheet,
    ) {
        let Some((request_id, text)) =
            wire::read_announcement_request(message.get(2..).unwrap_or_default())
        else {
            self.say(
                LogLevel::Warn,
                format!(
                    "Gameplay connection from {peer}: guild: an announcement request could not be read"
                ),
            );
            return;
        };
        let what = "sets the announcement";
        let (Some(settings), Some(rows)) = (self.world.guilds().copied(), self.guild_rows()) else {
            self.refuse_guild(
                io,
                peer,
                crypt,
                &sheet.name,
                request_id,
                what,
                "no guild settings",
            )
            .await;
            return;
        };
        let membership = self.membership();
        let leads = membership.as_ref().map(Membership::leads);
        if let Err(why) = guilds::may_announce(
            leads,
            text.encode_utf16().count(),
            wire::GUILD_ANNOUNCEMENT_UNITS,
        ) {
            self.refuse_guild(io, peer, crypt, &sheet.name, request_id, what, why)
                .await;
            return;
        }
        let Some(guild) = membership.map(|membership| membership.guild) else {
            self.refuse_guild(io, peer, crypt, &sheet.name, request_id, what, "no guild")
                .await;
            return;
        };
        let revision = rows
            .set_announcement(&guild.id, &text, &sheet.name)
            .and_then(|()| self.next_revision(&rows, &guild, settings.revision_step));
        let revision = match revision {
            Ok(revision) => revision,
            Err(error) => {
                self.refuse_guild(io, peer, crypt, &sheet.name, request_id, what, error)
                    .await;
                return;
            }
        };
        if let Ok(mut held) = self.guild.lock()
            && let Some(membership) = held.as_mut()
        {
            membership.guild.announcement = text.clone();
            membership.guild.announcement_author = sheet.name.clone();
            membership.guild.revision = revision;
        }
        self.say(
            LogLevel::Debug,
            format!(
                "Gameplay connection from {peer}: guild: {:?} sets the announcement of {:?}",
                sheet.name, guild.name
            ),
        );
        let frames = [
            wire::guild_announcement(&text, &sheet.name),
            wire::guild_revision(revision),
            wire::guild_request_result(request_id, wire::GUILD_RESULT_OK),
        ];
        self.send(io, peer, crypt, &frames, "the guild announcement")
            .await;
    }
}
