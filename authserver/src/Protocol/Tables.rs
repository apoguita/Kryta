//! The login family of this client's protocol: 100 rows, ported from ASURA's table.
//!
//! One row per header: the wire value, the index the reference addresses its format array
//! with, the declared size, the name and the field layout a decoder walks. The walker itself
//! is `shared::packets::Frame` - the mechanism both connections use - and the other family's
//! rows live in the program that serves the world.
//!
//! Source: `ASURA/crates/asura-protocol/src/packets.rs`, split by
//! `tools/split-protocol-table.py`. `docs/port/02-wire.md` says how a stream is split with it.

use shared::packets::types::{Conn, Field, FieldKind, Packet, Status};

/// The family's rows: `100`, sorted by client-to-server first, then header.
#[rustfmt::skip]
static ROWS: &[Packet] = &[
    Packet { conn: Conn::Auth, header: 0x8000, index:   0, size:     6, name: Some("AUTH_CMSG_HEARTBEAT"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8001, index:   1, size:   138, name: Some("AUTH_CMSG_SEND_COMPUTER_INFO"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 1 }, Field { kind: FieldKind::String16, length: 32 }, Field { kind: FieldKind::String16, length: 32 }] },
    Packet { conn: Conn::Auth, header: 0x8002, index:   2, size:    22, name: Some("AUTH_CMSG_SEND_COMPUTER_HASH"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 2 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }] },
    Packet { conn: Conn::Auth, header: 0x8003, index:   3, size:   214, name: Some("AUTH_CMSG_ACCOUNT_CREATE"), from_client: true, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 3 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 64 }, Field { kind: FieldKind::Blob, length: 20 }, Field { kind: FieldKind::String16, length: 26 }] },
    Packet { conn: Conn::Auth, header: 0x8004, index:   4, size:     6, name: Some("AUTH_CMSG_ACCOUNT_LOGIN"), from_client: true, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 4 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8005, index:   5, size:   182, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 5 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 20 }, Field { kind: FieldKind::Blob, length: 20 }, Field { kind: FieldKind::String16, length: 64 }] },
    Packet { conn: Conn::Auth, header: 0x8006, index:   6, size:    10, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 6 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8007, index:   7, size:    50, name: Some("AUTH_CMSG_DELETE_CHARACTER"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 7 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x8008, index:   8, size:    50, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 8 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x8009, index:   9, size:   118, name: Some("AUTH_CMSG_UPDATE_CHARACTER_SETTINGS"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 9 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::Array8, length: 64 }] },
    Packet { conn: Conn::Auth, header: 0x800A, index:  10, size:    50, name: Some("AUTH_CMSG_CHANGE_PLAY_CHARACTER"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 10 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x800B, index:  11, size:    18, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 11 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x800C, index:  12, size:    26, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 12 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }] },
    Packet { conn: Conn::Auth, header: 0x800D, index:  13, size:     6, name: Some("AUTH_CMSG_DISCONNECT"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 13 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x800E, index:  14, size:     6, name: Some("AUTH_CMSG_SET_PLAYER_STATUS"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 14 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x800F, index:  15, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 15 }] },
    Packet { conn: Conn::Auth, header: 0x8010, index:  16, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 16 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8011, index:  17, size:   518, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 17 }, Field { kind: FieldKind::String16, length: 256 }] },
    Packet { conn: Conn::Auth, header: 0x8012, index:  18, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 18 }] },
    Packet { conn: Conn::Auth, header: 0x8013, index:  19, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 19 }] },
    Packet { conn: Conn::Auth, header: 0x8014, index:  20, size:   790, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 20 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 64 }, Field { kind: FieldKind::String16, length: 64 }, Field { kind: FieldKind::String16, length: 256 }] },
    Packet { conn: Conn::Auth, header: 0x8015, index:  21, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 21 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8016, index:  22, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 22 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8017, index:  23, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 23 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8018, index:  24, size:   138, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 24 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 64 }] },
    Packet { conn: Conn::Auth, header: 0x8019, index:  25, size:    50, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 25 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 20 }, Field { kind: FieldKind::Blob, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x801A, index:  26, size:    98, name: Some("AUTH_CMSG_FRIEND_ADD"), from_client: true, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 26 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x801B, index:  27, size:    66, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 27 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x801C, index:  28, size:    62, name: Some("AUTH_CMSG_ADD_ACCESS_KEY"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 28 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 26 }] },
    Packet { conn: Conn::Auth, header: 0x801D, index:  29, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 29 }] },
    Packet { conn: Conn::Auth, header: 0x801E, index:  30, size:    66, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 30 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 26 }] },
    Packet { conn: Conn::Auth, header: 0x801F, index:  31, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 31 }] },
    Packet { conn: Conn::Auth, header: 0x8020, index:  32, size:   522, name: Some("AUTH_CMSG_SETTING_UPDATE_CONTENT"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 32 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 512 }] },
    Packet { conn: Conn::Auth, header: 0x8021, index:  33, size:    10, name: Some("AUTH_CMSG_SETTING_UPDATE_SIZE"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 33 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8022, index:  34, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 34 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8023, index:  35, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 35 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8024, index:  36, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 36 }] },
    Packet { conn: Conn::Auth, header: 0x8025, index:  37, size:    38, name: Some("AUTH_CMSG_REQUEST_GUILD_HALL"), from_client: true, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 37 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }] },
    Packet { conn: Conn::Auth, header: 0x8026, index:  38, size:     3, name: Some("AUTH_CMSG_ACCEPT_EULA"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 38 }, Field { kind: FieldKind::Byte, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8027, index:  39, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 39 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8028, index:  40, size:   410, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 40 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 28 }, Field { kind: FieldKind::String16, length: 28 }, Field { kind: FieldKind::String16, length: 32 }, Field { kind: FieldKind::String16, length: 32 }, Field { kind: FieldKind::String16, length: 12 }, Field { kind: FieldKind::String16, length: 28 }, Field { kind: FieldKind::String16, length: 28 }] },
    Packet { conn: Conn::Auth, header: 0x8029, index:  41, size:    26, name: Some("AUTH_CMSG_REQUEST_GAME_INSTANCE"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 41 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x802A, index:  42, size:    94, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 42 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x802B, index:  43, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 43 }] },
    Packet { conn: Conn::Auth, header: 0x802C, index:  44, size:  1050, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 44 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array32, length: 128 }, Field { kind: FieldKind::Array32, length: 128 }] },
    Packet { conn: Conn::Auth, header: 0x802D, index:  45, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 45 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x802E, index:  46, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 46 }] },
    Packet { conn: Conn::Auth, header: 0x802F, index:  47, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 47 }] },
    Packet { conn: Conn::Auth, header: 0x8030, index:  48, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 48 }] },
    Packet { conn: Conn::Auth, header: 0x8031, index:  49, size:     6, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 49 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8032, index:  50, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 50 }] },
    Packet { conn: Conn::Auth, header: 0x8033, index:  51, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 51 }] },
    Packet { conn: Conn::Auth, header: 0x8034, index:  52, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 52 }] },
    Packet { conn: Conn::Auth, header: 0x8035, index:  53, size:     6, name: Some("AUTH_CMSG_ASK_SERVER_RESPONSE"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 53 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x8036, index:  54, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 54 }] },
    Packet { conn: Conn::Auth, header: 0x8037, index:  55, size:    94, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 55 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x8038, index:  56, size:   150, name: Some("AUTH_CMSG_PORTAL_ACCOUNT_LOGIN"), from_client: true, status: Status::Handled, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 56 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 32 }] },
    Packet { conn: Conn::Auth, header: 0x8039, index:  57, size:     2, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 57 }] },
    Packet { conn: Conn::Auth, header: 0x803A, index:  58, size:   526, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 58 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 512 }] },
    Packet { conn: Conn::Auth, header: 0x803B, index:  59, size:   522, name: None, from_client: true, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 59 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 512 }] },
    Packet { conn: Conn::Auth, header: 0x0000, index:   0, size:     6, name: Some("AUTH_SMSG_HEARTBEAT"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0001, index:   1, size:    10, name: Some("AUTH_SMSG_SESSION_INFO"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 1 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0002, index:   2, size:    10, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 2 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0003, index:   3, size:    10, name: Some("AUTH_SMSG_REQUEST_RESPONSE"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 3 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0004, index:   4, size:  4106, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 4 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 2048 }] },
    Packet { conn: Conn::Auth, header: 0x0005, index:   5, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 5 }] },
    Packet { conn: Conn::Auth, header: 0x0006, index:   6, size:    42, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 6 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0007, index:   7, size:   138, name: Some("AUTH_SMSG_CHARACTER_INFO"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 7 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::Array8, length: 64 }] },
    Packet { conn: Conn::Auth, header: 0x0008, index:   8, size:    38, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 8 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 24 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0009, index:   9, size:    42, name: Some("AUTH_SMSG_GAME_SERVER_INFO"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 9 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 24 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x000A, index:  10, size:    70, name: Some("AUTH_SMSG_FRIEND_UPDATE_INFO"), from_client: false, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 10 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x000B, index:  11, size:   110, name: Some("AUTH_SMSG_FRIEND_UPDATE_STATUS"), from_client: false, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 11 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x000C, index:  12, size:   566, name: Some("AUTH_SMSG_WHISPER_RECEIVED"), from_client: false, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 12 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 256 }] },
    Packet { conn: Conn::Auth, header: 0x000D, index:  13, size:  1042, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 13 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::String16, length: 256 }, Field { kind: FieldKind::String16, length: 256 }] },
    Packet { conn: Conn::Auth, header: 0x000E, index:  14, size:    46, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 14 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 32 }] },
    Packet { conn: Conn::Auth, header: 0x000F, index:  15, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 15 }] },
    Packet { conn: Conn::Auth, header: 0x0010, index:  16, size:   630, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 16 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 256 }] },
    Packet { conn: Conn::Auth, header: 0x0011, index:  17, size:   275, name: Some("AUTH_SMSG_ACCOUNT_INFO"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 17 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 200 }, Field { kind: FieldKind::Byte, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0012, index:  18, size:   630, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 18 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 20 }, Field { kind: FieldKind::String16, length: 256 }] },
    Packet { conn: Conn::Auth, header: 0x0013, index:  19, size:     6, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 19 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0014, index:  20, size:    10, name: Some("AUTH_SMSG_FRIEND_STREAM_END"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 20 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0015, index:  21, size:    34, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 21 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0016, index:  22, size:  2058, name: Some("AUTH_SMSG_ACCOUNT_SETTINGS"), from_client: false, status: Status::Emitted, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 22 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 2048 }] },
    Packet { conn: Conn::Auth, header: 0x0017, index:  23, size:  4102, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 23 }, Field { kind: FieldKind::Array8, length: 4096 }] },
    Packet { conn: Conn::Auth, header: 0x0018, index:  24, size:   226, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 24 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Blob, length: 8 }, Field { kind: FieldKind::Array8, length: 200 }] },
    Packet { conn: Conn::Auth, header: 0x0019, index:  25, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 25 }] },
    Packet { conn: Conn::Auth, header: 0x001A, index:  26, size:    32, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 26 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::NestedStruct, length: 11 }, Field { kind: FieldKind::Byte, length: 0 }, Field { kind: FieldKind::Byte, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x001B, index:  27, size:    18, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 27 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x001C, index:  28, size:    10, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 28 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x001D, index:  29, size:    14, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 29 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x001E, index:  30, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 30 }] },
    Packet { conn: Conn::Auth, header: 0x001F, index:  31, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 31 }] },
    Packet { conn: Conn::Auth, header: 0x0020, index:  32, size:    54, name: Some("AUTH_SMSG_FRIEND_UPDATE_LOCATION"), from_client: false, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 32 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Word, length: 0 }, Field { kind: FieldKind::Byte, length: 0 }, Field { kind: FieldKind::Byte, length: 0 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x0021, index:  33, size:    54, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 33 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Word, length: 0 }, Field { kind: FieldKind::Byte, length: 0 }, Field { kind: FieldKind::Byte, length: 0 }, Field { kind: FieldKind::String16, length: 20 }] },
    Packet { conn: Conn::Auth, header: 0x0022, index:  34, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 34 }] },
    Packet { conn: Conn::Auth, header: 0x0023, index:  35, size:    26, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 35 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Blob, length: 16 }] },
    Packet { conn: Conn::Auth, header: 0x0024, index:  36, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 36 }] },
    Packet { conn: Conn::Auth, header: 0x0025, index:  37, size:     2, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 37 }] },
    Packet { conn: Conn::Auth, header: 0x0026, index:  38, size:    10, name: Some("AUTH_SMSG_SERVER_RESPONSE"), from_client: false, status: Status::Known, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 38 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Dword, length: 0 }] },
    Packet { conn: Conn::Auth, header: 0x0027, index:  39, size:  4106, name: None, from_client: false, status: Status::Unnamed, evidence: &[], fields: &[Field { kind: FieldKind::MsgHeader, length: 39 }, Field { kind: FieldKind::Dword, length: 0 }, Field { kind: FieldKind::Array8, length: 4096 }] },
];

/// The row for one header in one direction, or `None` if this family has no such message.
///
/// A binary search over the family's rows: framing asks this for every message.
pub fn lookup(header: u16, from_client: bool) -> Option<&'static Packet> {
    ROWS.binary_search_by(|row| {
        // The rows are sorted client-to-server first, then by header, so a client row sorts before a
        // server row of a nearby header.
        let row_key = (u8::from(!row.from_client), row.header);
        let want = (u8::from(!from_client), header);
        row_key.cmp(&want)
    })
    .ok()
    .map(|index| &ROWS[index])
}

/// Every row of the family.
pub fn rows() -> impl Iterator<Item = &'static Packet> {
    ROWS.iter()
}

/// The names the reference's opcodes.h gives the login connection's headers, and the lookups a
/// dispatcher and a framer ask of this family.
pub mod auth {
    pub use shared::packets::types::Packet as Entry;

    /// `AUTH_CMSG_MASK`: set on client-to-server headers, and therefore also the direction bit.
    pub const AUTH_CMSG_MASK: u16 = 0x8000;
    /// `AUTH_SMSG_MASK`: server-to-client headers carry no direction bit.
    pub const AUTH_SMSG_MASK: u16 = 0x0000;

    /// Number of client-to-server auth headers (`AUTH_CMSG_COUNT`).
    pub const AUTH_CMSG_COUNT: usize = 60;
    /// Number of server-to-client auth headers (`AUTH_SMSG_COUNT`).
    pub const AUTH_SMSG_COUNT: usize = 40;

    // ---------------------------------------------------------------- named headers
    // The names are GWPS's own (`opcodes.h`), kept verbatim so a Rust message can be diffed
    // against a C one by name rather than by number.
    //
    // A constant is an alias of a row's `header`, not a second declaration of it: the value is
    // copied here once because `match` needs a constant, and `every_header_agrees_with_its_index`
    // checks that the two columns cannot drift.

    /// `AUTH_CMSG_HEARTBEAT` (6 bytes on the wire).
    pub const AUTH_CMSG_HEARTBEAT: u16 = 0x8000;

    /// `AUTH_CMSG_SEND_COMPUTER_INFO` (138 bytes on the wire).
    pub const AUTH_CMSG_SEND_COMPUTER_INFO: u16 = 0x8001;

    /// `AUTH_CMSG_SEND_COMPUTER_HASH` (22 bytes on the wire).
    pub const AUTH_CMSG_SEND_COMPUTER_HASH: u16 = 0x8002;

    /// `AUTH_CMSG_ACCOUNT_CREATE` (214 bytes on the wire).
    pub const AUTH_CMSG_ACCOUNT_CREATE: u16 = 0x8003;

    /// `AUTH_CMSG_ACCOUNT_LOGIN` (6 bytes on the wire).
    pub const AUTH_CMSG_ACCOUNT_LOGIN: u16 = 0x8004;

    /// `AUTH_CMSG_DELETE_CHARACTER` (50 bytes on the wire).
    pub const AUTH_CMSG_DELETE_CHARACTER: u16 = 0x8007;

    /// `AUTH_CMSG_UPDATE_CHARACTER_SETTINGS` (118 bytes on the wire).
    pub const AUTH_CMSG_UPDATE_CHARACTER_SETTINGS: u16 = 0x8009;

    /// `AUTH_CMSG_CHANGE_PLAY_CHARACTER` (50 bytes on the wire).
    pub const AUTH_CMSG_CHANGE_PLAY_CHARACTER: u16 = 0x800A;

    /// `AUTH_CMSG_DISCONNECT` (6 bytes on the wire).
    pub const AUTH_CMSG_DISCONNECT: u16 = 0x800D;

    /// `AUTH_CMSG_SET_PLAYER_STATUS` (6 bytes on the wire).
    pub const AUTH_CMSG_SET_PLAYER_STATUS: u16 = 0x800E;

    /// `AUTH_CMSG_FRIEND_ADD` (98 bytes on the wire).
    pub const AUTH_CMSG_FRIEND_ADD: u16 = 0x801A;

    /// `AUTH_CMSG_ADD_ACCESS_KEY` (62 bytes on the wire).
    pub const AUTH_CMSG_ADD_ACCESS_KEY: u16 = 0x801C;

    /// `AUTH_CMSG_SETTING_UPDATE_CONTENT` (522 bytes on the wire).
    pub const AUTH_CMSG_SETTING_UPDATE_CONTENT: u16 = 0x8020;

    /// `AUTH_CMSG_SETTING_UPDATE_SIZE` (10 bytes on the wire).
    pub const AUTH_CMSG_SETTING_UPDATE_SIZE: u16 = 0x8021;

    /// `AUTH_CMSG_REQUEST_GUILD_HALL` (38 bytes on the wire).
    pub const AUTH_CMSG_REQUEST_GUILD_HALL: u16 = 0x8025;

    /// `AUTH_CMSG_ACCEPT_EULA` (3 bytes on the wire).
    pub const AUTH_CMSG_ACCEPT_EULA: u16 = 0x8026;

    /// `AUTH_CMSG_REQUEST_GAME_INSTANCE` (26 bytes on the wire).
    pub const AUTH_CMSG_REQUEST_GAME_INSTANCE: u16 = 0x8029;

    /// `AUTH_CMSG_ASK_SERVER_RESPONSE` (6 bytes on the wire).
    pub const AUTH_CMSG_ASK_SERVER_RESPONSE: u16 = 0x8035;

    /// `AUTH_CMSG_PORTAL_ACCOUNT_LOGIN` (150 bytes on the wire).
    pub const AUTH_CMSG_PORTAL_ACCOUNT_LOGIN: u16 = 0x8038;

    /// `AUTH_SMSG_HEARTBEAT` (6 bytes on the wire).
    pub const AUTH_SMSG_HEARTBEAT: u16 = 0x0000;

    /// `AUTH_SMSG_SESSION_INFO` (10 bytes on the wire).
    pub const AUTH_SMSG_SESSION_INFO: u16 = 0x0001;

    /// `AUTH_SMSG_REQUEST_RESPONSE` (10 bytes on the wire).
    pub const AUTH_SMSG_REQUEST_RESPONSE: u16 = 0x0003;

    /// `AUTH_SMSG_CHARACTER_INFO` (138 bytes on the wire).
    pub const AUTH_SMSG_CHARACTER_INFO: u16 = 0x0007;

    /// `AUTH_SMSG_GAME_SERVER_INFO` (42 bytes on the wire).
    pub const AUTH_SMSG_GAME_SERVER_INFO: u16 = 0x0009;

    /// `AUTH_SMSG_FRIEND_UPDATE_INFO` (70 bytes on the wire).
    pub const AUTH_SMSG_FRIEND_UPDATE_INFO: u16 = 0x000A;

    /// `AUTH_SMSG_FRIEND_UPDATE_STATUS` (110 bytes on the wire).
    pub const AUTH_SMSG_FRIEND_UPDATE_STATUS: u16 = 0x000B;

    /// `AUTH_SMSG_WHISPER_RECEIVED` (566 bytes on the wire).
    pub const AUTH_SMSG_WHISPER_RECEIVED: u16 = 0x000C;

    /// `AUTH_SMSG_ACCOUNT_INFO` (275 bytes on the wire).
    pub const AUTH_SMSG_ACCOUNT_INFO: u16 = 0x0011;

    /// `AUTH_SMSG_FRIEND_STREAM_END` (10 bytes on the wire).
    pub const AUTH_SMSG_FRIEND_STREAM_END: u16 = 0x0014;

    /// `AUTH_SMSG_ACCOUNT_SETTINGS` (2058 bytes on the wire).
    pub const AUTH_SMSG_ACCOUNT_SETTINGS: u16 = 0x0016;

    /// `AUTH_SMSG_FRIEND_UPDATE_LOCATION` (54 bytes on the wire).
    pub const AUTH_SMSG_FRIEND_UPDATE_LOCATION: u16 = 0x0020;

    /// `AUTH_SMSG_SERVER_RESPONSE` (10 bytes on the wire).
    pub const AUTH_SMSG_SERVER_RESPONSE: u16 = 0x0026;

    /// Headers whose declared size cannot be recomputed from their field layout, because a
    /// `TYPE_NESTED_STRUCT` makes the length depend on a count carried on the wire.
    ///
    /// One of them, `0x001A`, declares 32 bytes where its own layout can only occupy 31: the
    /// reference's maximum is not exactly its packer's output. It is recorded rather than guessed
    /// at (`docs/reference/client-protocol.md`, open questions).
    pub const VARIABLE_LENGTH_HEADERS: &[u16] = &[0x001A];

    /// The row for an auth header, whichever direction. The family's two directions use disjoint
    /// header ranges, so the header alone identifies it: `0x80xx` comes from the client.
    pub fn entry(header: u16) -> Option<&'static Entry> {
        let from_client = header & AUTH_CMSG_MASK != 0;
        crate::protocol::tables::lookup(header, from_client)
    }

    /// The fixed on-wire size for a header, or `None` if it is not an auth header at all.
    pub fn expected_size(header: u16) -> Option<usize> {
        entry(header).map(|entry| entry.size)
    }
}
