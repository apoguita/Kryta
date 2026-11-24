//! The protocol's vocabulary: a message, its fields, and which connection it belongs to.
//!
//! Shared, because both connections are framed by the same walk (`Frame` beside this file) and the
//! auth program does not link the game library. The *rows* are not here: a program owns the rows of
//! the connection it serves - `game/src/Server/Protocol/Tables.rs` for a world connection and
//! `authserver/src/Protocol/Tables.rs` for the login - which is the reference's arrangement.
//!
//! Ported from `ASURA/crates/asura-protocol/src/packets.rs`; `tools/split-protocol-table.py` records
//! how the merged file was separated again.

/// Which connection a message belongs to. The two families share header values (`0x8000` is a
/// different message in each), so the connection is part of every key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Conn {
    /// The login connection: key exchange, session, character list, travel.
    Auth = 0,
    /// A world connection: the map, agents, items, movement.
    Game = 1,
}

impl Conn {
    /// The family's name, for a log line or an error.
    pub fn label(self) -> &'static str {
        match self {
            Conn::Auth => "auth",
            Conn::Game => "game",
        }
    }
}

/// How far a packet is known. Ordered from least to most proven, so a table can be sorted or a
/// progress line can be read as a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// OpenTyria's `opcodes.h` does not name this header. It is still framed, so an
    /// unexpected-but-real message is recognised rather than mistaken for a malformed one.
    Unnamed = 0,
    /// OpenTyria's `opcodes.h` names it, and nothing else is proven yet.
    Known = 1,
    /// Seen in a capture. Nothing acts on it.
    Observed = 2,
    /// This server sends it. In ASURA, where the rows were ported from, that was its `crate::game`
    /// builders and `client::messages` reply builders.
    Emitted = 3,
    /// This server acts on the client's message. In ASURA, where the rows were ported from, that
    /// was `asura-world`'s `answer` and `asura-auth`'s dispatch.
    Handled = 4,
    /// The name or the function was inferred rather than read: the evidence column must say from
    /// what.
    Inferred = 5,
}

impl Status {
    /// A short word a progress line or a log can print.
    pub fn label(self) -> &'static str {
        match self {
            Status::Unnamed => "unnamed",
            Status::Known => "known",
            Status::Observed => "observed",
            Status::Emitted => "emitted",
            Status::Handled => "handled",
            Status::Inferred => "inferred",
        }
    }
}

/// A field kind, as OpenTyria's `msgdefs.c` names it (`FieldType` in `msgdefs.h`). One enum for both
/// families: OpenTyria uses one field vocabulary for both, and the counted kinds are the same
/// on the wire -- a `u16` count and that many units, with no padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    /// `TYPE_MSG_HEADER`: the u16 header, two bytes on the wire.
    MsgHeader,
    /// `TYPE_MSG_HEADER_GAME`: the header plus a zero u16 -- four bytes on the wire. Defined,
    /// packed and unpacked in the reference. Four rows of the game table use it
    /// (`game/src/Server/Protocol/Tables.rs`); no row of the login table does.
    MsgHeaderGame,
    /// `TYPE_AGENT_ID`.
    AgentId,
    /// `TYPE_FLOAT`.
    Float,
    /// `TYPE_VECT2`: two floats.
    Vect2,
    /// `TYPE_VECT3`: three floats.
    Vect3,
    /// `TYPE_BYTE`.
    Byte,
    /// `TYPE_WORD`.
    Word,
    /// `TYPE_DWORD`.
    Dword,
    /// `TYPE_BLOB`: `length` opaque bytes, no count prefix.
    Blob,
    /// `TYPE_STRING_16`: on the wire a `u16` count then that many u16 units; `length` is the
    /// capacity, which bounds the count and never pads the message.
    String16,
    /// `TYPE_ARRAY_8`: a `u16` count then that many u8 units.
    Array8,
    /// `TYPE_ARRAY_16`: a `u16` count then that many u16 units.
    Array16,
    /// `TYPE_ARRAY_32`: a `u16` count then that many u32 units.
    Array32,
    /// `TYPE_NESTED_STRUCT`: a `u8` count then that many repeats of the fields that follow it.
    NestedStruct,
}

/// One field: its kind and, for the kinds that carry a count or a length, that number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    /// What the field holds and how it is counted.
    pub kind: FieldKind,
    /// `field.length` in OpenTyria (`msgdefs.h`): the unit capacity, the blob size, or 0 when unused.
    pub length: u16,
}

impl Field {
    /// Bytes this field occupies when a counted field is written at full capacity, or `None` for
    /// [`FieldKind::NestedStruct`], whose size depends on the repeats it carries.
    ///
    /// This is the arithmetic a declared size comes from (the reference's `max_size`): the count
    /// field is four bytes wide in the reference's struct, and the units fill the capacity. It is
    /// *not* the wire size -- see [`Field::wire_size_for`].
    pub fn wire_size(&self) -> Option<usize> {
        let n = self.length as usize;
        match self.kind {
            FieldKind::MsgHeader => Some(2),
            FieldKind::MsgHeaderGame => Some(4),
            FieldKind::AgentId | FieldKind::Float | FieldKind::Dword => Some(4),
            FieldKind::Vect2 => Some(8),
            FieldKind::Vect3 => Some(12),
            FieldKind::Byte => Some(1),
            FieldKind::Word => Some(2),
            FieldKind::Blob => Some(n),
            FieldKind::String16 | FieldKind::Array16 => Some(4 + n * 2),
            FieldKind::Array8 => Some(4 + n),
            FieldKind::Array32 => Some(4 + n * 4),
            FieldKind::NestedStruct => None,
        }
    }

    /// Bytes this field occupies on the wire when a counted field carries `count` units, or `None`
    /// for [`FieldKind::NestedStruct`].
    ///
    /// A counted field is a two-byte count then `count` units and nothing else, so even at capacity
    /// it is two bytes smaller than the field's share of a declared size: the declared maximum
    /// bounds the wire, it is not the wire.
    pub fn wire_size_for(&self, count: u16) -> Option<usize> {
        let n = self.length as usize;
        let count = count as usize;
        match self.kind {
            FieldKind::MsgHeader => Some(2),
            FieldKind::MsgHeaderGame => Some(4),
            FieldKind::AgentId | FieldKind::Float | FieldKind::Dword => Some(4),
            FieldKind::Vect2 => Some(8),
            FieldKind::Vect3 => Some(12),
            FieldKind::Byte => Some(1),
            FieldKind::Word => Some(2),
            FieldKind::Blob => Some(n),
            // The capacity is only a maximum; the count the body carries decides the wire, and the
            // OpenTyria rejects a count above the capacity (`pack_helper`, `msgpack.c`).
            FieldKind::String16 | FieldKind::Array16 => Some(2 + count * 2),
            FieldKind::Array8 => Some(2 + count),
            FieldKind::Array32 => Some(2 + count * 4),
            FieldKind::NestedStruct => None,
        }
    }
}

/// One packet: a header of one family, one direction. The single source for what a message is.
#[derive(Debug, Clone, Copy)]
pub struct Packet {
    /// The family this header belongs to.
    pub conn: Conn,
    /// The wire value, mask included: `0x8000 | index` or `0x0000 | index`.
    pub header: u16,
    /// The index GWPS uses to address its format array (`header & 0x7FFF`).
    pub index: u16,
    /// The auth family's fixed on-wire size, or the game family's declared maximum. Never the
    /// length of a game message that carries a counted field.
    pub size: usize,
    /// The name `opcodes.h` gives this index; `None` for the indices it does not name.
    pub name: Option<&'static str>,
    /// Whether the header is client-to-server (`0x8000` set).
    pub from_client: bool,
    /// How far this packet is known. A session edits this column.
    pub status: Status,
    /// Where the status, name or function comes from: a capture, a protocol document section, a
    /// `file:fn`. A session edits this column, and a guess says it is a guess.
    pub evidence: &'static [&'static str],
    /// The fields the message is made of, in wire order.
    pub fields: &'static [Field],
}

impl Packet {
    /// The name for a log line: the reference's name, or the header when it has none.
    pub fn label(&self) -> String {
        match self.name {
            Some(name) => String::from(name),
            None => format!("{:#06X} {}", self.header, self.conn.label()),
        }
    }
}

/// `size` can be checked against its own layout instead of trusted.
pub fn declared_max_size(fields: &[Field]) -> usize {
    let mut total = 0;
    for (position, field) in fields.iter().enumerate() {
        let n = field.length as usize;
        total += match field.kind {
            FieldKind::MsgHeader => 2,
            FieldKind::MsgHeaderGame => 4,
            FieldKind::AgentId | FieldKind::Float | FieldKind::Dword => 4,
            FieldKind::Vect2 => 8,
            FieldKind::Vect3 => 12,
            FieldKind::Byte => 1,
            FieldKind::Word => 2,
            FieldKind::Blob => n,
            FieldKind::String16 | FieldKind::Array16 => 4 + n * 2,
            FieldKind::Array8 => 4 + n,
            FieldKind::Array32 => 4 + n * 4,
            FieldKind::NestedStruct => 4 + n * declared_max_size(&fields[position + 1..]),
        };
        if field.kind == FieldKind::NestedStruct {
            break;
        }
    }
    total
}
