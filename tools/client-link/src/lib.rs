//! The link between the client DLL and the server: its messages and its framing.
//!
//! The link is the DLL's own connection to the server, apart from the game connection
//! (`docs/port/22-client-platform.md`, "The server link"). These messages are Kryta's own: no
//! research project and no recording has them, because no other server adds a DLL to the client.
//! They belong to the custom-client feature the owner added (`docs/port/09-parity-audit.md`, the
//! custom title text row), and the game protocol does not carry them.
//!
//! Both ends use this crate, so the two cannot drift apart: the server (`authserver`) and the DLL
//! (`tools/client-dll`).
//!
//! # The frame
//!
//! A frame is a four-byte length, little endian, followed by that many bytes of UTF-8 JSON: one
//! object with a `"type"` member. A frame larger than [`MAX_FRAME`] is refused.
//!
//! # The conversation
//!
//! | # | from | message | says |
//! |---|---|---|---|
//! | 1 | client | [`Message::Hello`] | which process this is and which client build it runs in |
//! | 2 | server | [`Message::Offsets`] | the offset documents, in Reforged's format |
//! | 3 | client | [`Message::Resolved`] | which offsets were found and which were not |
//! | 4 | server | [`Message::Titles`] | the custom title text; sent again whenever it changes |
//!
//! The server answers a message it will not serve with [`Message::Refused`] and closes.

use serde_json::{Value, json};

/// The version of this conversation. A server refuses a client that names another.
pub const PROTOCOL: u32 = 1;

/// The largest frame either end reads. The offset documents and the title text are far smaller; the
/// limit exists so a connection that sends a wild length cannot grow the process.
pub const MAX_FRAME: usize = 1024 * 1024;

/// One message of the link.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// The client's first message.
    Hello {
        /// The conversation version the client speaks, [`PROTOCOL`].
        protocol: u32,
        /// The client's process id: the key that names this client on its machine, from start-up,
        /// before any account logs in (`docs/port/22-client-platform.md`, "Which client is which").
        pid: u32,
        /// The `TimeDateStamp` of the client executable's PE header: which build this is. It is
        /// readable before any offset is known, and the patch does not change it.
        time_date_stamp: u32,
    },
    /// The server's offset documents, each one a document in Reforged's format
    /// (`namespace`, `patterns`, `resolvers`).
    Offsets {
        /// The documents, in the order the server read them.
        documents: Vec<Value>,
    },
    /// What the client made of the offsets.
    Resolved {
        /// The resolvers that produced an address, by qualified name.
        found: Vec<String>,
        /// The resolvers that did not, each with the reason.
        missing: Vec<(String, String)>,
    },
    /// The custom title text, as the `custom_tracks` array the world pack produces.
    Titles {
        /// One object per custom track.
        custom_tracks: Vec<Value>,
    },
    /// The server will not serve this client.
    Refused {
        /// Why, for the client's log and its message to the player.
        reason: String,
    },
}

/// Why bytes were not a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// The frame's length is larger than [`MAX_FRAME`].
    TooLarge(usize),
    /// The frame is not UTF-8 JSON, or not an object this crate knows.
    Malformed(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(length) => {
                write!(f, "a frame of {length} bytes is larger than {MAX_FRAME}")
            }
            Self::Malformed(reason) => write!(f, "malformed frame: {reason}"),
        }
    }
}

impl std::error::Error for LinkError {}

impl Message {
    /// The message as a JSON object.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Hello {
                protocol,
                pid,
                time_date_stamp,
            } => json!({
                "type": "hello",
                "protocol": protocol,
                "pid": pid,
                "time_date_stamp": time_date_stamp,
            }),
            Self::Offsets { documents } => json!({ "type": "offsets", "documents": documents }),
            Self::Resolved { found, missing } => json!({
                "type": "resolved",
                "found": found,
                "missing": missing
                    .iter()
                    .map(|(name, reason)| json!({ "name": name, "reason": reason }))
                    .collect::<Vec<_>>(),
            }),
            Self::Titles { custom_tracks } => {
                json!({ "type": "titles", "custom_tracks": custom_tracks })
            }
            Self::Refused { reason } => json!({ "type": "refused", "reason": reason }),
        }
    }

    /// Read a message from a JSON object.
    pub fn from_json(value: &Value) -> Result<Self, LinkError> {
        let kind = text(value, "type")?;
        match kind.as_str() {
            "hello" => Ok(Self::Hello {
                protocol: number(value, "protocol")?,
                pid: number(value, "pid")?,
                time_date_stamp: number(value, "time_date_stamp")?,
            }),
            "offsets" => Ok(Self::Offsets {
                documents: array(value, "documents")?.clone(),
            }),
            "resolved" => {
                let found = array(value, "found")?
                    .iter()
                    .map(|name| {
                        name.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| malformed("found holds something that is not a name"))
                    })
                    .collect::<Result<_, _>>()?;
                let missing = array(value, "missing")?
                    .iter()
                    .map(|row| Ok((text(row, "name")?, text(row, "reason")?)))
                    .collect::<Result<_, LinkError>>()?;
                Ok(Self::Resolved { found, missing })
            }
            "titles" => Ok(Self::Titles {
                custom_tracks: array(value, "custom_tracks")?.clone(),
            }),
            "refused" => Ok(Self::Refused {
                reason: text(value, "reason")?,
            }),
            other => Err(malformed(format!("unknown message type '{other}'"))),
        }
    }

    /// The message as one frame: the length, then the JSON.
    pub fn encode(&self) -> Vec<u8> {
        let body = self.to_json().to_string().into_bytes();
        let mut frame = Vec::with_capacity(4 + body.len());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    /// Read a message from a frame's body (the bytes after the length).
    pub fn decode(body: &[u8]) -> Result<Self, LinkError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|error| malformed(error.to_string()))?;
        Self::from_json(&value)
    }
}

fn malformed(reason: impl Into<String>) -> LinkError {
    LinkError::Malformed(reason.into())
}

fn text(value: &Value, key: &str) -> Result<String, LinkError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| malformed(format!("'{key}' is missing or is not text")))
}

fn number(value: &Value, key: &str) -> Result<u32, LinkError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| malformed(format!("'{key}' is missing or is not a 32-bit number")))
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, LinkError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| malformed(format!("'{key}' is missing or is not a list")))
}

/// Frames out of a stream of bytes, however the stream was cut into reads.
#[derive(Debug, Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    /// A decoder that has read nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add bytes that arrived.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete message, if the bytes so far hold one.
    pub fn next_message(&mut self) -> Result<Option<Message>, LinkError> {
        let Some(header) = self.buffer.first_chunk::<4>() else {
            return Ok(None);
        };
        let length = u32::from_le_bytes(*header) as usize;
        if length > MAX_FRAME {
            return Err(LinkError::TooLarge(length));
        }
        if self.buffer.len() < 4 + length {
            return Ok(None);
        }
        let message = Message::decode(&self.buffer[4..4 + length]);
        self.buffer.drain(..4 + length);
        message.map(Some)
    }
}

/// Write one message to a blocking stream.
pub fn write(stream: &mut impl std::io::Write, message: &Message) -> std::io::Result<()> {
    stream.write_all(&message.encode())?;
    stream.flush()
}

/// Read one message from a blocking stream.
pub fn read(stream: &mut impl std::io::Read) -> std::io::Result<Message> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            LinkError::TooLarge(length),
        ));
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Message::decode(&body)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_message() -> Vec<Message> {
        vec![
            Message::Hello {
                protocol: PROTOCOL,
                pid: 4242,
                time_date_stamp: 0x6AE9_51C4,
            },
            Message::Offsets {
                documents: vec![json!({ "namespace": "titles", "patterns": {} })],
            },
            Message::Resolved {
                found: vec!["titles.metadata_lookup".to_string()],
                missing: vec![(
                    "titles.hover_call".to_string(),
                    "scan returned null".to_string(),
                )],
            },
            Message::Titles {
                custom_tracks: vec![json!({ "id": 48, "name": "Hog Jumper" })],
            },
            Message::Refused {
                reason: "protocol 2 is not served".to_string(),
            },
        ]
    }

    #[test]
    fn every_message_survives_its_frame() {
        for message in every_message() {
            let frame = message.encode();
            let length = u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize;
            assert_eq!(length, frame.len() - 4);
            assert_eq!(Message::decode(&frame[4..]).unwrap(), message);
        }
    }

    #[test]
    fn the_decoder_reads_frames_cut_anywhere() {
        let stream: Vec<u8> = every_message().iter().flat_map(Message::encode).collect();
        let mut decoder = Decoder::new();
        let mut read = Vec::new();
        for byte in &stream {
            decoder.push(std::slice::from_ref(byte));
            while let Some(message) = decoder.next_message().unwrap() {
                read.push(message);
            }
        }
        assert_eq!(read, every_message());
    }

    #[test]
    fn a_wild_length_is_refused_before_it_is_read() {
        let mut decoder = Decoder::new();
        decoder.push(&u32::MAX.to_le_bytes());
        assert_eq!(
            decoder.next_message(),
            Err(LinkError::TooLarge(u32::MAX as usize))
        );
    }

    #[test]
    fn an_unknown_type_is_malformed() {
        let body = br#"{"type":"teleport"}"#;
        assert!(matches!(
            Message::decode(body),
            Err(LinkError::Malformed(_))
        ));
    }

    #[test]
    fn the_blocking_reader_and_writer_agree() {
        let mut wire = Vec::new();
        for message in every_message() {
            write(&mut wire, &message).unwrap();
        }
        let mut cursor = std::io::Cursor::new(wire);
        for message in every_message() {
            assert_eq!(read(&mut cursor).unwrap(), message);
        }
    }
}
