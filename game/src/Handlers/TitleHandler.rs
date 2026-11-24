//! The title family: `GAME_CMSG_TITLE_DISPLAY` and `GAME_CMSG_TITLE_HIDE`.
//!
//! Reference: `src/server/game/Handlers/`, whose members read a message and call into the nouns. The
//! decision - which rank a title displays - is `VanillaTitles::selectable_rank`
//! (`game/src/Characters/`); what is here is the translation and the answer. The handler is an
//! `impl WorldSession` method the session's dispatch calls by header, on a loaded map only.

use common::logging::LogLevel;
use shared::cryptography::AuthCrypt;
use shared::network::socket::SessionIo;

use crate::server::packets::title_packets;
use crate::server::protocol::tables::game::GAME_CMSG_TITLE_HIDE;
use crate::server::world_session::{Connection, MapLoad, WorldSession};

impl WorldSession {
    /// The client displays one of its titles, or hides the one it displays: the player's displayed
    /// rank changes, and the player and everyone in its copy of the map are told.
    ///
    /// ASURA crates/asura-protocol/src/packets.rs:422-423,772 supplies the layouts.
    /// Retail login-unfiltered-9292 seq 659-661 replies with the selected rank.
    /// Preforged packets-1360-15821281.jsonl:36252 shows rank zero for no title.
    pub(crate) async fn answer_title_display(
        &self,
        state: &mut Connection,
        io: &mut SessionIo,
        peer: &str,
        header: u16,
        message: &[u8],
        crypt: &mut AuthCrypt,
        load: &MapLoad<'_>,
    ) {
        let map = load.map;
        let player = load.state.player;
        let rank = if header == GAME_CMSG_TITLE_HIDE {
            Some(0)
        } else {
            let Some(title) = title_id(message) else {
                self.say(
                    LogLevel::Warn,
                    format!(
                        "Gameplay connection from {peer}: a title display of {} bytes cannot be read.",
                        message.len()
                    ),
                );
                return;
            };
            self.world.titles().selectable_rank(title)
        };
        if let Some(rank) = rank {
            let reply = [title_packets::rank_display(player as u16, rank)];
            if let Ok(mut peers) = self.world.peers().lock() {
                peers.set_displayed_rank(player, rank);
                peers.broadcast(player, map.id, state.district(), &reply);
            }
            self.send(io, peer, crypt, &reply, "the displayed title")
                .await;
        }
    }
}

/// The title a display message names: the dword after the header.
fn title_id(message: &[u8]) -> Option<u32> {
    let bytes = message.get(2..6)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}
