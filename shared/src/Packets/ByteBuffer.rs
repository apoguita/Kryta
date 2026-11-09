//! The byte buffer every message is written into and read out of.
//!
//! Reference: `src/server/shared/Packets/ByteBuffer.{h,cpp}`. The behaviour ported here is the
//! reference's, including the parts that look like accidents:
//!
//! - Numbers are **little endian** on the wire, and the buffer writes them that way.
//! - A **string is its bytes followed by a NUL**, not a length and its bytes. Reading a string reads
//!   to the NUL, which is why a message with a string in the middle is not a fixed size.
//! - A **bool is one byte**, 1 or 0.
//! - There are two cursors: where writing ended, and where reading has reached. Reading past the
//!   end is an error, never a silent zero.
//!
//! What is not here yet: `readPackGUID` and `ReadPackedTime`, whose shapes belong to Azeroth's
//! protocol rather than to this game's, and which arrive with whatever needs them.

use std::fmt;

/// Why a read or a write failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteBufferError {
    /// A read wanted more bytes than the buffer holds.
    ///
    /// Reference: `ByteBufferPositionException`.
    Position {
        /// Where the read started.
        position: usize,
        /// How many bytes it wanted.
        wanted: usize,
        /// How many the buffer holds.
        size: usize,
    },
    /// A string was not valid UTF-8.
    ///
    /// Reference: `ByteBufferSourceException`, which `ReadCString` raises for the same reason.
    Source {
        /// The bytes that were not UTF-8.
        bytes: Vec<u8>,
    },
}

impl fmt::Display for ByteBufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position {
                position,
                wanted,
                size,
            } => write!(
                f,
                "attempted to read {wanted} byte(s) at position {position}, but the buffer holds {size}"
            ),
            Self::Source { bytes } => write!(f, "not valid UTF-8: {} byte(s)", bytes.len()),
        }
    }
}

impl std::error::Error for ByteBufferError {}

/// A buffer of bytes with a write end and a read cursor.
///
/// Reference: `class ByteBuffer`, which is a `std::string` with two positions. This is a `Vec<u8>`
/// with the same two positions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteBuffer {
    data: Vec<u8>,
    rpos: usize,
}

impl ByteBuffer {
    /// An empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty buffer with room for `capacity` bytes.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            rpos: 0,
        }
    }

    /// A buffer over bytes that are already written, with the read cursor at the start.
    ///
    /// Reference: the `ByteBuffer(uint8 const* bytes, std::size_t size)` constructor.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: bytes.to_vec(),
            rpos: 0,
        }
    }

    /// How many bytes are written.
    ///
    /// Reference: `size()`, which is the write position.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Where reading has reached.
    pub fn rpos(&self) -> usize {
        self.rpos
    }

    /// Move the read cursor.
    pub fn set_rpos(&mut self, position: usize) {
        self.rpos = position;
    }

    /// Move the read cursor to the end, so the buffer can be read no further.
    ///
    /// Reference: `rfinish`.
    pub fn rfinish(&mut self) {
        self.rpos = self.data.len();
    }

    /// Where writing would continue, which is the buffer's size.
    pub fn wpos(&self) -> usize {
        self.data.len()
    }

    /// The bytes written, whatever the read cursor is doing.
    pub fn contents(&self) -> &[u8] {
        &self.data
    }

    /// The bytes not yet read.
    pub fn remaining(&self) -> &[u8] {
        &self.data[self.rpos.min(self.data.len())..]
    }

    /// How many bytes are left to read.
    pub fn remaining_len(&self) -> usize {
        self.data.len().saturating_sub(self.rpos)
    }

    /// Empty the buffer and put both cursors back to the start.
    ///
    /// Reference: `clear`.
    pub fn clear(&mut self) {
        self.data.clear();
        self.rpos = 0;
    }

    // ---- writing ----

    /// Append raw bytes.
    ///
    /// Reference: `append(uint8 const* bytes, std::size_t count)`.
    pub fn append_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    /// Append one byte.
    pub fn append_u8(&mut self, value: u8) {
        self.data.push(value);
    }

    /// Append a 16-bit number, little endian.
    pub fn append_u16(&mut self, value: u16) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a 32-bit number, little endian.
    pub fn append_u32(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a 64-bit number, little endian.
    pub fn append_u64(&mut self, value: u64) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a signed 8-bit number.
    pub fn append_i8(&mut self, value: i8) {
        self.data.push(value as u8);
    }

    /// Append a signed 16-bit number, little endian.
    pub fn append_i16(&mut self, value: i16) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a signed 32-bit number, little endian.
    pub fn append_i32(&mut self, value: i32) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a 32-bit float, little endian.
    pub fn append_f32(&mut self, value: f32) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a 64-bit float, little endian.
    pub fn append_f64(&mut self, value: f64) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a bool as one byte, 1 or 0.
    ///
    /// Reference: `operator<<(bool)`, which appends a `uint8`.
    pub fn append_bool(&mut self, value: bool) {
        self.data.push(u8::from(value));
    }

    /// Append a string: its bytes, then a NUL.
    ///
    /// Reference: `operator<<(std::string_view)`. The length is **not** written, which is why a
    /// message carrying a string is variable in size.
    pub fn append_str(&mut self, value: &str) {
        self.data.extend_from_slice(value.as_bytes());
        self.data.push(0);
    }

    /// Overwrite bytes at a position.
    ///
    /// Reference: `put`, which the reference uses to fill in a length once the body after it is
    /// written.
    pub fn put_bytes(&mut self, position: usize, bytes: &[u8]) -> Result<(), ByteBufferError> {
        if position + bytes.len() > self.data.len() {
            return Err(ByteBufferError::Position {
                position,
                wanted: bytes.len(),
                size: self.data.len(),
            });
        }
        self.data[position..position + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    // ---- reading ----

    fn take<const N: usize>(&mut self) -> Result<[u8; N], ByteBufferError> {
        if self.rpos + N > self.data.len() {
            return Err(ByteBufferError::Position {
                position: self.rpos,
                wanted: N,
                size: self.data.len(),
            });
        }
        let mut out = [0u8; N];
        out.copy_from_slice(&self.data[self.rpos..self.rpos + N]);
        self.rpos += N;
        Ok(out)
    }

    /// Read one byte.
    pub fn read_u8(&mut self) -> Result<u8, ByteBufferError> {
        Ok(self.take::<1>()?[0])
    }

    /// Read a 16-bit number, little endian.
    pub fn read_u16(&mut self) -> Result<u16, ByteBufferError> {
        Ok(u16::from_le_bytes(self.take::<2>()?))
    }

    /// Read a 32-bit number, little endian.
    pub fn read_u32(&mut self) -> Result<u32, ByteBufferError> {
        Ok(u32::from_le_bytes(self.take::<4>()?))
    }

    /// Read a 64-bit number, little endian.
    pub fn read_u64(&mut self) -> Result<u64, ByteBufferError> {
        Ok(u64::from_le_bytes(self.take::<8>()?))
    }

    /// Read a signed 8-bit number.
    pub fn read_i8(&mut self) -> Result<i8, ByteBufferError> {
        Ok(self.read_u8()? as i8)
    }

    /// Read a signed 16-bit number, little endian.
    pub fn read_i16(&mut self) -> Result<i16, ByteBufferError> {
        Ok(i16::from_le_bytes(self.take::<2>()?))
    }

    /// Read a signed 32-bit number, little endian.
    pub fn read_i32(&mut self) -> Result<i32, ByteBufferError> {
        Ok(i32::from_le_bytes(self.take::<4>()?))
    }

    /// Read a 32-bit float, little endian.
    pub fn read_f32(&mut self) -> Result<f32, ByteBufferError> {
        Ok(f32::from_le_bytes(self.take::<4>()?))
    }

    /// Read a 64-bit float, little endian.
    pub fn read_f64(&mut self) -> Result<f64, ByteBufferError> {
        Ok(f64::from_le_bytes(self.take::<8>()?))
    }

    /// Read a bool: one byte, and anything but zero is true.
    ///
    /// Reference: `operator>>(bool&)`, which reads a `uint8` and compares it with 0.
    pub fn read_bool(&mut self) -> Result<bool, ByteBufferError> {
        Ok(self.read_u8()? != 0)
    }

    /// Read `count` bytes.
    ///
    /// Reference: `read(uint8* dest, std::size_t len)`.
    pub fn read_bytes(&mut self, count: usize) -> Result<Vec<u8>, ByteBufferError> {
        if self.rpos + count > self.data.len() {
            return Err(ByteBufferError::Position {
                position: self.rpos,
                wanted: count,
                size: self.data.len(),
            });
        }
        let out = self.data[self.rpos..self.rpos + count].to_vec();
        self.rpos += count;
        Ok(out)
    }

    /// Skip `count` bytes.
    ///
    /// Reference: `read_skip`.
    pub fn read_skip(&mut self, count: usize) -> Result<(), ByteBufferError> {
        if self.rpos + count > self.data.len() {
            return Err(ByteBufferError::Position {
                position: self.rpos,
                wanted: count,
                size: self.data.len(),
            });
        }
        self.rpos += count;
        Ok(())
    }

    /// Read a NUL-terminated string.
    ///
    /// Reference: `ReadCString(true)`, which reads to the NUL, refuses a string that runs past the
    /// end, and refuses bytes that are not UTF-8.
    pub fn read_str(&mut self) -> Result<String, ByteBufferError> {
        let start = self.rpos;
        let Some(offset) = self.data[start..].iter().position(|byte| *byte == 0) else {
            return Err(ByteBufferError::Position {
                position: start,
                wanted: 1,
                size: self.data.len(),
            });
        };

        let bytes = self.data[start..start + offset].to_vec();
        self.rpos = start + offset + 1;

        String::from_utf8(bytes.clone()).map_err(|_| ByteBufferError::Source { bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_little_endian() {
        let mut buffer = ByteBuffer::new();
        buffer.append_u16(0x0102);
        buffer.append_u32(0x01020304);
        buffer.append_u64(0x0102030405060708);

        assert_eq!(
            buffer.contents(),
            &[0x02, 0x01, 0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
    }

    #[test]
    fn numbers_round_trip() {
        let mut buffer = ByteBuffer::new();
        buffer.append_u8(1);
        buffer.append_u16(0xF00D);
        buffer.append_u32(0xDEADBEEF);
        buffer.append_u64(u64::MAX);
        buffer.append_i8(-1);
        buffer.append_i16(-2);
        buffer.append_i32(-3);
        buffer.append_f32(1.5);
        buffer.append_f64(-2.25);
        buffer.append_bool(true);

        assert_eq!(buffer.read_u8().unwrap(), 1);
        assert_eq!(buffer.read_u16().unwrap(), 0xF00D);
        assert_eq!(buffer.read_u32().unwrap(), 0xDEADBEEF);
        assert_eq!(buffer.read_u64().unwrap(), u64::MAX);
        assert_eq!(buffer.read_i8().unwrap(), -1);
        assert_eq!(buffer.read_i16().unwrap(), -2);
        assert_eq!(buffer.read_i32().unwrap(), -3);
        assert_eq!(buffer.read_f32().unwrap(), 1.5);
        assert_eq!(buffer.read_f64().unwrap(), -2.25);
        assert!(buffer.read_bool().unwrap());
        assert_eq!(buffer.remaining_len(), 0);
    }

    #[test]
    fn a_string_is_its_bytes_and_a_nul_with_no_length() {
        let mut buffer = ByteBuffer::new();
        buffer.append_str("abc");
        assert_eq!(buffer.contents(), b"abc\0");
        assert_eq!(buffer.len(), 4);

        assert_eq!(buffer.read_str().unwrap(), "abc");
        assert_eq!(buffer.remaining_len(), 0);
    }

    #[test]
    fn an_empty_string_is_a_lone_nul() {
        let mut buffer = ByteBuffer::new();
        buffer.append_str("");
        assert_eq!(buffer.contents(), &[0]);
        assert_eq!(buffer.read_str().unwrap(), "");
    }

    #[test]
    fn a_string_without_its_nul_runs_past_the_end() {
        let mut buffer = ByteBuffer::from_bytes(b"abc");
        assert!(matches!(
            buffer.read_str(),
            Err(ByteBufferError::Position { .. })
        ));
    }

    #[test]
    fn bytes_that_are_not_utf8_are_refused() {
        let mut buffer = ByteBuffer::from_bytes(&[0xFF, 0xFE, 0x00]);
        assert!(matches!(
            buffer.read_str(),
            Err(ByteBufferError::Source { .. })
        ));
    }

    #[test]
    fn reading_past_the_end_is_an_error_not_a_zero() {
        let mut buffer = ByteBuffer::from_bytes(&[1, 2]);
        assert!(matches!(
            buffer.read_u32(),
            Err(ByteBufferError::Position {
                position: 0,
                wanted: 4,
                size: 2
            })
        ));
    }

    #[test]
    fn the_read_cursor_moves_and_can_be_moved() {
        let mut buffer = ByteBuffer::from_bytes(&[10, 20, 30, 40]);
        assert_eq!(buffer.read_u8().unwrap(), 10);
        assert_eq!(buffer.rpos(), 1);
        buffer.set_rpos(3);
        assert_eq!(buffer.read_u8().unwrap(), 40);
        buffer.rfinish();
        assert_eq!(buffer.rpos(), 4);
        assert_eq!(buffer.remaining_len(), 0);
    }

    #[test]
    fn skipping_advances_without_reading() {
        let mut buffer = ByteBuffer::from_bytes(&[1, 2, 3, 4]);
        buffer.read_skip(2).unwrap();
        assert_eq!(buffer.read_u16().unwrap(), 0x0403);
    }

    #[test]
    fn a_value_can_be_overwritten_where_it_was_written() {
        let mut buffer = ByteBuffer::new();
        buffer.append_u32(0);
        buffer.append_str("body");
        buffer.put_bytes(0, &7u32.to_le_bytes()).unwrap();
        assert_eq!(buffer.read_u32().unwrap(), 7);
        assert_eq!(buffer.read_str().unwrap(), "body");
    }

    #[test]
    fn overwriting_past_the_end_is_refused() {
        let mut buffer = ByteBuffer::from_bytes(&[1, 2]);
        assert!(buffer.put_bytes(1, &[3, 4, 5]).is_err());
    }

    #[test]
    fn clearing_empties_the_buffer_and_both_cursors() {
        let mut buffer = ByteBuffer::new();
        buffer.append_u32(1);
        buffer.read_u32().unwrap();
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.rpos(), 0);
    }
}
