//! Framing a connection: where one message ends, decided by the row that header has.
//!
//! A message carries **no length**: the header is two bytes and the layout the row gives that header
//! decides where the body stops. A counted field (`String16`, `Array8`, `Array16`, `Array32`)
//! carries its own `u16` count and consumes exactly that many units, and a nested struct is the same
//! idea one level down: a `u8` count and that many repeats of the fields after it. So the table's
//! `size` column is an upper bound, never a length.
//!
//! Ported from `ASURA/crates/asura-protocol/src/client/frame.rs`: same walker, same shapes, same
//! error cases. The Rust here is this tree's, the rule is not.
//!
//! The rows are not here. Each program owns the rows of the connection it serves
//! (`game/src/Server/Protocol/Tables.rs`, `authserver/src/Protocol/Tables.rs`) and calls this with
//! the row it found; the vocabulary the row is written in is `Types.rs` beside this file.

use crate::packets::types::{Field, FieldKind};

/// The header is two bytes. Everything after it is the body, whose length the table fixes.
pub const HEADER_LEN: usize = 2;

/// One message, already framed against the table: its bytes, header first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    bytes: Vec<u8>,
}

/// Why a byte stream is not (yet) a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Fewer bytes than the frame needs. Not a protocol violation: read more.
    Incomplete {
        /// The whole frame size once the header has been read, or the header size while it is not.
        needed: usize,
    },
    /// A header that is in no table row.
    UnknownHeader {
        /// The header that is in no row.
        header: u16,
    },
    /// A counted field claims more units than the table allows for it.
    SizeMismatch {
        /// The header whose field disagreed.
        header: u16,
        /// The count the message carried.
        got: usize,
        /// The capacity the table gives the field.
        expected: usize,
    },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete { needed } => write!(f, "{needed} more byte(s) are needed"),
            Self::UnknownHeader { header } => write!(f, "0x{header:04X} is in no table row"),
            Self::SizeMismatch {
                header,
                got,
                expected,
            } => write!(
                f,
                "0x{header:04X} counts {got} units where the table allows {expected}"
            ),
        }
    }
}

impl std::error::Error for FrameError {}

impl Frame {
    /// The whole message, as it goes on the wire.
    pub fn encode(&self) -> &[u8] {
        &self.bytes
    }

    /// How many bytes it is.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether it is empty, which a frame never is.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The header, read from the message.
    pub fn header(&self) -> u16 {
        u16::from_le_bytes([self.bytes[0], self.bytes[1]])
    }

    /// The fields, read from the message.
    pub fn body(&self) -> &[u8] {
        &self.bytes[HEADER_LEN..]
    }

    /// The header, for a log line when no row names it.
    pub fn header_label(&self) -> String {
        format!("{:#06X}", self.header())
    }

    /// Frame one message of either connection, from the row the caller's own table gives it.
    ///
    /// The walk is the rule; the row is the vocabulary, and each program owns the rows of the
    /// connection it serves. `game` and `authserver` each wrap this with their own lookup.
    pub fn decode_with(
        header: u16,
        fields: &'static [Field],
        bytes: &[u8],
    ) -> Result<(Frame, usize), FrameError> {
        if bytes.len() < HEADER_LEN {
            return Err(FrameError::Incomplete { needed: HEADER_LEN });
        }
        let shapes: Vec<Shape> = fields.iter().map(shape_of).collect();
        let position = walk(bytes, header, &shapes)?;
        Ok((
            Frame {
                bytes: bytes[..position].to_vec(),
            },
            position,
        ))
    }

    /// The table's name for a header, from the row the caller found.
    ///
    /// A log line names what arrived; the name lives on the row, not on the message.
    pub fn label_of(row: &crate::packets::types::Packet) -> String {
        row.label()
    }
}

/// How one field occupies the wire, for the walk.
#[derive(Clone, Copy)]
enum Shape {
    /// The message header: already consumed before the walk starts.
    Header,
    /// A fixed number of bytes.
    Fixed(usize),
    /// A `u16` count and that many units, up to the field's capacity.
    Counted { unit: usize, capacity: u16 },
    /// A nested struct: a `u8` count and that many repeats of the fields that follow it.
    Nested { capacity: u8 },
}

fn shape_of(field: &Field) -> Shape {
    match field.kind {
        FieldKind::MsgHeader => Shape::Header,
        // A four-byte game header: the walker has already consumed the first two bytes, so the field
        // covers the two that remain.
        FieldKind::MsgHeaderGame => Shape::Fixed(HEADER_LEN),
        FieldKind::AgentId | FieldKind::Float | FieldKind::Dword => Shape::Fixed(4),
        FieldKind::Vect2 => Shape::Fixed(8),
        FieldKind::Vect3 => Shape::Fixed(12),
        FieldKind::Byte => Shape::Fixed(1),
        FieldKind::Word => Shape::Fixed(2),
        FieldKind::Blob => Shape::Fixed(field.length as usize),
        FieldKind::String16 | FieldKind::Array16 => Shape::Counted {
            unit: 2,
            capacity: field.length,
        },
        FieldKind::Array8 => Shape::Counted {
            unit: 1,
            capacity: field.length,
        },
        FieldKind::Array32 => Shape::Counted {
            unit: 4,
            capacity: field.length,
        },
        FieldKind::NestedStruct => Shape::Nested {
            capacity: field.length.min(u8::MAX as u16) as u8,
        },
    }
}

/// Walk a message's field layout and return where its body ends.
fn walk(bytes: &[u8], header: u16, shapes: &[Shape]) -> Result<usize, FrameError> {
    walk_from(bytes, header, HEADER_LEN, shapes)
}

fn walk_from(
    bytes: &[u8],
    header: u16,
    mut position: usize,
    shapes: &[Shape],
) -> Result<usize, FrameError> {
    let mut index = 0;
    while index < shapes.len() {
        match shapes[index] {
            Shape::Header => {}
            Shape::Fixed(width) => {
                position += width;
                if position > bytes.len() {
                    return Err(FrameError::Incomplete { needed: position });
                }
            }
            Shape::Counted { unit, capacity } => {
                if position + 2 > bytes.len() {
                    return Err(FrameError::Incomplete {
                        needed: position + 2,
                    });
                }
                let count = u16::from_le_bytes([bytes[position], bytes[position + 1]]) as usize;
                if count > capacity as usize {
                    return Err(FrameError::SizeMismatch {
                        header,
                        got: count,
                        expected: capacity as usize,
                    });
                }
                position += 2 + count * unit;
                if position > bytes.len() {
                    return Err(FrameError::Incomplete { needed: position });
                }
            }
            Shape::Nested { capacity } => {
                let Some(&count) = bytes.get(position) else {
                    return Err(FrameError::Incomplete {
                        needed: position + 1,
                    });
                };
                if count > capacity {
                    return Err(FrameError::SizeMismatch {
                        header,
                        got: count as usize,
                        expected: capacity as usize,
                    });
                }
                position += 1;
                // Every repeat is the tail of the layout, so the tail is consumed here and the loop
                // ends: there is nothing left to walk after the repeats.
                let tail = &shapes[index + 1..];
                for _ in 0..count {
                    position = walk_from(bytes, header, position, tail)?;
                }
                return Ok(position);
            }
        }
        index += 1;
    }
    Ok(position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::types::Field;

    /// A row with no fields: the header alone.
    const BARE: &[Field] = &[Field {
        kind: FieldKind::MsgHeader,
        length: 0,
    }];

    /// A row with one counted string of up to twenty units.
    const COUNTED: &[Field] = &[
        Field {
            kind: FieldKind::MsgHeader,
            length: 0,
        },
        Field {
            kind: FieldKind::String16,
            length: 20,
        },
    ];

    #[test]
    fn the_header_alone_is_two_bytes() {
        let (frame, used) = Frame::decode_with(0x0186, BARE, &[0x86, 0x01]).expect("frames");
        assert_eq!(used, 2);
        assert_eq!(frame.encode(), &[0x86, 0x01]);
        assert_eq!(frame.body().len(), 0);
    }

    #[test]
    fn a_short_stream_asks_for_more_rather_than_guessing() {
        assert_eq!(
            Frame::decode_with(0x0186, BARE, &[0x86]),
            Err(FrameError::Incomplete { needed: 2 })
        );
        assert_eq!(
            Frame::decode_with(0x800A, COUNTED, &[0x0A, 0x80, 0x02, 0x00, 0x41]),
            Err(FrameError::Incomplete { needed: 8 })
        );
    }

    #[test]
    fn a_counted_field_decides_the_length() {
        // A count of one and one unit: header, count, unit.
        let (frame, used) =
            Frame::decode_with(0x800A, COUNTED, &[0x0A, 0x80, 0x01, 0x00, 0x41, 0x00, 0xFF])
                .expect("frames");
        assert_eq!(
            used, 6,
            "header, count and one unit - the stray byte is the next message"
        );
        assert_eq!(frame.len(), 6);
    }

    #[test]
    fn a_count_above_the_fields_capacity_is_refused() {
        let mut bytes = vec![0x0A, 0x80, 21, 0x00];
        bytes.extend(std::iter::repeat_n(0u8, 42));
        assert_eq!(
            Frame::decode_with(0x800A, COUNTED, &bytes),
            Err(FrameError::SizeMismatch {
                header: 0x800A,
                got: 21,
                expected: 20,
            })
        );
    }

    #[test]
    fn a_nested_struct_is_a_byte_count_and_that_many_repeats() {
        const NESTED: &[Field] = &[
            Field {
                kind: FieldKind::MsgHeader,
                length: 0,
            },
            Field {
                kind: FieldKind::NestedStruct,
                length: 4,
            },
            Field {
                kind: FieldKind::Dword,
                length: 0,
            },
        ];
        // Two repeats of one dword each.
        let mut bytes = vec![0x61, 0x01, 0x02];
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        let (_, used) = Frame::decode_with(0x0161, NESTED, &bytes).expect("frames");
        assert_eq!(used, 3 + 8);
    }
}
