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
//! | 5 | server | [`Message::Maps`] | every map file the server hosts for its clients, each with its size and checksum |
//! | 6 | client | [`Message::MapWanted`] | one of those files, which the client does not hold |
//! | 7 | server | [`Message::MapPart`] | that file, in parts of at most [`MAP_PART`] bytes |
//!
//! The server answers a message it will not serve with [`Message::Refused`] and closes.
//!
//! # Map files
//!
//! The server holds the maps and the client fetches what the server lists
//! (`docs/port/24-custom-maps.md`, "The delivery plan"). The list is the whole truth for a client
//! of that server. A file is named by its checksum, the SHA-256 of its bytes as lower-case
//! hexadecimal ([`sha256_hex`]): a map the server has changed has another checksum, and so is
//! another file to fetch. A part's bytes travel as hexadecimal text, because a frame is JSON.

use serde_json::{Value, json};

/// The version of this conversation. A server refuses a client that names another.
pub const PROTOCOL: u32 = 5;

/// The fields of the client's own row for one map, in the order the row holds them, four bytes
/// each: 124 bytes. The names are Py4GW Reforged Native's (`include/GW/context/map.h:87-118`,
/// `AreaInfo`, at `82e4746`); the row's size is the `imul eax, esi, 0x7c` of the client's
/// accessor, measured in build 39167 at `0x5A6793` (`docs/port/24-custom-maps.md`).
pub const MAP_ROW_FIELDS: [&str; 31] = [
    "campaign",
    "continent",
    "region",
    "type",
    "flags",
    "thumbnail_id",
    "min_party_size",
    "max_party_size",
    "min_player_size",
    "max_player_size",
    "controlled_outpost_id",
    "fraction_mission",
    "min_level",
    "max_level",
    "needed_pq",
    "mission_maps_to",
    "x",
    "y",
    "icon_start_x",
    "icon_start_y",
    "icon_end_x",
    "icon_end_y",
    "icon_start_x_dupe",
    "icon_start_y_dupe",
    "icon_end_x_dupe",
    "icon_end_y_dupe",
    "file_id",
    "mission_chronology",
    "ha_map_chronology",
    "name_id",
    "description_id",
];

/// What the server states of one map's row in the client: the fields it names, each with its
/// value. A field it does not name keeps the value the client's own row has.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapRow {
    /// The map.
    pub map_id: u32,
    /// The fields stated, each as its place in [`MAP_ROW_FIELDS`] and its value.
    pub fields: Vec<(usize, u32)>,
    /// The map's name as text of the server's own, in place of a `name_id` of the client's text
    /// files. The client is given it where it would show the text of that id.
    pub name: Option<String>,
    /// The map's description as text of the server's own, in place of a `description_id`.
    pub description: Option<String>,
}

/// The place of `name_id` in [`MAP_ROW_FIELDS`].
pub const MAP_ROW_NAME_ID: usize = 29;
/// The place of `description_id` in [`MAP_ROW_FIELDS`].
pub const MAP_ROW_DESCRIPTION_ID: usize = 30;

impl MapRow {
    /// The row a client answers with: `own`, the client's row for the map, with the stated fields
    /// written over it.
    pub fn over(&self, mut own: [u32; 31]) -> [u32; 31] {
        for (field, value) in &self.fields {
            if let Some(slot) = own.get_mut(*field) {
                *slot = *value;
            }
        }
        own
    }
}

/// The most bytes of a map file one [`Message::MapPart`] carries. As hexadecimal text it is twice
/// that, which with the rest of the message stays under [`MAX_FRAME`].
pub const MAP_PART: usize = 256 * 1024;

/// One map file the server hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapFile {
    /// The file id the client asks its archive for: the map record's `map_file_id`.
    pub file_id: u32,
    /// How many bytes the file is.
    pub size: u32,
    /// Its checksum, [`sha256_hex`] of its bytes.
    pub sha256: String,
}

/// The checksum of a map file: the SHA-256 of its bytes, as lower-case hexadecimal.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    to_hex(&sha2::Sha256::digest(bytes))
}

/// Whether `text` can name a server's flavor: 1 to 64 ASCII letters, digits, `_` and `-`. A flavor
/// is used as a folder name by the client, so nothing else is taken as one.
pub fn is_flavor_name(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Whether `text` has the shape of a checksum: 64 lower-case hexadecimal digits. A checksum is used
/// as a file name by the client, so nothing else is taken as one.
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 0x0F)] as char);
    }
    text
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs.iter().map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?)).collect()
}

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
    /// Every map file the server hosts for its clients. Sent after the titles, and again whenever
    /// the list changes. A file that is not in the list is not served by this server.
    Maps {
        /// The server's flavor: the name of the build and setup it runs, which is its world
        /// pack's name ("Vanilla"). The client keeps one folder of maps for each flavor, and that
        /// folder holds exactly what this list names. None when the server runs no world pack, and
        /// then it lists no file.
        flavor: Option<String>,
        /// The files.
        files: Vec<MapFile>,
    },
    /// What the server states of its maps' rows in the client: the name, kind, region and
    /// world-map place the client itself holds for a map id. Sent after the map list. A map that
    /// is not here is answered from the client's own table.
    MapRows {
        /// One entry for each map whose record states a row.
        rows: Vec<MapRow>,
    },
    /// The client does not hold this file of the list, and asks for it.
    MapWanted {
        /// The file id.
        file_id: u32,
        /// The checksum the list gave it.
        sha256: String,
    },
    /// Some of a file the client asked for. The parts of one file are sent in order, and the last
    /// one ends at `total`.
    MapPart {
        /// The file id.
        file_id: u32,
        /// The checksum of the whole file.
        sha256: String,
        /// Where in the file these bytes begin.
        offset: u32,
        /// How many bytes the whole file is.
        total: u32,
        /// The bytes, at most [`MAP_PART`] of them.
        data: Vec<u8>,
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
            Self::Maps { flavor, files } => json!({
                "type": "maps",
                "flavor": flavor,
                "files": files
                    .iter()
                    .map(|file| json!({
                        "file_id": file.file_id,
                        "size": file.size,
                        "sha256": file.sha256,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Self::MapRows { rows } => json!({
                "type": "map_rows",
                "rows": rows
                    .iter()
                    .map(|row| {
                        let fields: serde_json::Map<String, Value> = row
                            .fields
                            .iter()
                            .filter_map(|(field, value)| {
                                Some((MAP_ROW_FIELDS.get(*field)?.to_string(), json!(value)))
                            })
                            .collect();
                        let mut stated = json!({ "map_id": row.map_id, "fields": fields });
                        if let Some(name) = &row.name {
                            stated["name"] = json!(name);
                        }
                        if let Some(description) = &row.description {
                            stated["description"] = json!(description);
                        }
                        stated
                    })
                    .collect::<Vec<_>>(),
            }),
            Self::MapWanted { file_id, sha256 } => {
                json!({ "type": "map_wanted", "file_id": file_id, "sha256": sha256 })
            }
            Self::MapPart { file_id, sha256, offset, total, data } => json!({
                "type": "map_part",
                "file_id": file_id,
                "sha256": sha256,
                "offset": offset,
                "total": total,
                "data": to_hex(data),
            }),
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
            "maps" => {
                let flavor = match value.get("flavor") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(name)) if is_flavor_name(name) => Some(name.clone()),
                    Some(_) => return Err(malformed("'flavor' is not a flavor name")),
                };
                let files: Vec<MapFile> = array(value, "files")?
                    .iter()
                    .map(|file| {
                        Ok(MapFile {
                            file_id: number(file, "file_id")?,
                            size: number(file, "size")?,
                            sha256: checksum(file)?,
                        })
                    })
                    .collect::<Result<_, LinkError>>()?;
                if flavor.is_none() && !files.is_empty() {
                    return Err(malformed("map files are listed without a flavor"));
                }
                Ok(Self::Maps { flavor, files })
            }
            "map_rows" => {
                let rows = array(value, "rows")?
                    .iter()
                    .map(|row| {
                        let stated = row
                            .get("fields")
                            .and_then(Value::as_object)
                            .ok_or_else(|| malformed("'fields' is missing or is not an object"))?;
                        // In the row's own order, whatever order the object came in.
                        let mut fields = Vec::with_capacity(stated.len());
                        for (name, value) in stated {
                            let field = MAP_ROW_FIELDS
                                .iter()
                                .position(|known| known == name)
                                .ok_or_else(|| malformed(format!("'{name}' is not a field of a map's row")))?;
                            let value = value
                                .as_u64()
                                .and_then(|number| u32::try_from(number).ok())
                                .ok_or_else(|| malformed(format!("'{name}' is not a 32-bit number")))?;
                            fields.push((field, value));
                        }
                        fields.sort_unstable();
                        // A text of the server's own stands in place of the id of one of the
                        // client's, so a row states one or the other.
                        let own_text = |key: &str, id: usize| match row.get(key) {
                            None | Some(Value::Null) => Ok(None),
                            Some(Value::String(text)) if text.is_empty() || text.contains('\0') => {
                                Err(malformed(format!("'{key}' is empty or holds a zero")))
                            }
                            Some(Value::String(_)) if fields.iter().any(|(field, _)| *field == id) => {
                                Err(malformed(format!("'{key}' is stated beside '{}'", MAP_ROW_FIELDS[id])))
                            }
                            Some(Value::String(text)) => Ok(Some(text.clone())),
                            Some(_) => Err(malformed(format!("'{key}' is not text"))),
                        };
                        let name = own_text("name", MAP_ROW_NAME_ID)?;
                        let description = own_text("description", MAP_ROW_DESCRIPTION_ID)?;
                        Ok(MapRow { map_id: number(row, "map_id")?, fields, name, description })
                    })
                    .collect::<Result<_, LinkError>>()?;
                Ok(Self::MapRows { rows })
            }
            "map_wanted" => Ok(Self::MapWanted {
                file_id: number(value, "file_id")?,
                sha256: checksum(value)?,
            }),
            "map_part" => {
                let data = from_hex(&text(value, "data")?)
                    .ok_or_else(|| malformed("'data' is not hexadecimal"))?;
                if data.len() > MAP_PART {
                    return Err(malformed(format!("a part of {} bytes is larger than {MAP_PART}", data.len())));
                }
                Ok(Self::MapPart {
                    file_id: number(value, "file_id")?,
                    sha256: checksum(value)?,
                    offset: number(value, "offset")?,
                    total: number(value, "total")?,
                    data,
                })
            }
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

/// A message's `sha256`, which must have a checksum's shape: the client names a file by it.
fn checksum(value: &Value) -> Result<String, LinkError> {
    let sha256 = text(value, "sha256")?;
    if is_sha256_hex(&sha256) {
        Ok(sha256)
    } else {
        Err(malformed("'sha256' is not 64 lower-case hexadecimal digits"))
    }
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
            Message::Maps { flavor: None, files: Vec::new() },
            Message::Maps {
                flavor: Some("fantasy_modded".to_string()),
                files: vec![MapFile {
                    file_id: 0x5F100,
                    size: 3,
                    sha256: sha256_hex(b"abc"),
                }],
            },
            Message::MapRows { rows: Vec::new() },
            Message::MapRows {
                rows: vec![
                    MapRow { map_id: 148, fields: vec![(2, 7), (16, 0x426), (29, 0x28EE)], ..MapRow::default() },
                    MapRow {
                        map_id: 10_000,
                        fields: vec![(3, 10)],
                        name: Some("Kryta Test Vale".to_string()),
                        description: Some("A vale [made] outside the game.".to_string()),
                    },
                ],
            },
            Message::MapWanted {
                file_id: 0x5F100,
                sha256: sha256_hex(b"abc"),
            },
            Message::MapPart {
                file_id: 0x5F100,
                sha256: sha256_hex(b"abc"),
                offset: 0,
                total: 3,
                data: b"abc".to_vec(),
            },
            Message::Refused {
                reason: "protocol 2 is not served".to_string(),
            },
        ]
    }

    /// The checksum is SHA-256 as lower-case hexadecimal: the value for `abc` is the standard's own
    /// example (FIPS 180-2, appendix B.1).
    #[test]
    fn the_checksum_is_sha256_in_lower_case_hexadecimal() {
        let sum = sha256_hex(b"abc");
        assert_eq!(sum, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(is_sha256_hex(&sum));
        assert!(!is_sha256_hex(&sum.to_uppercase()), "one spelling only");
        assert!(!is_sha256_hex("..\\..\\evil"), "a checksum is used as a file name");
        assert!(!is_sha256_hex(&sum[1..]));
    }

    /// A flavor names a folder on the client's machine, so only a plain name is one, and a list
    /// that names files without a flavor is refused.
    #[test]
    fn a_flavor_is_a_plain_name() {
        assert!(is_flavor_name("Vanilla"));
        assert!(is_flavor_name("fantasy_modded-2"));
        for bad in ["", "..", "a/b", "a\\b", "two words", "C:", &"x".repeat(65)] {
            assert!(!is_flavor_name(bad), "{bad}");
        }
        let escaping = json!({ "type": "maps", "flavor": "..\\..", "files": [] });
        assert!(Message::from_json(&escaping).is_err());
        let nameless = json!({
            "type": "maps",
            "files": [{ "file_id": 1, "size": 3, "sha256": sha256_hex(b"abc") }],
        });
        assert!(Message::from_json(&nameless).is_err());
    }

    /// A map's row names its fields, a field the row does not have is refused, and the stated
    /// fields are written over the client's own row and nothing else is.
    #[test]
    fn a_map_row_states_named_fields_over_the_clients_own() {
        assert_eq!(MAP_ROW_FIELDS.len() * 4, 124, "the row is 124 bytes");
        assert_eq!(MAP_ROW_FIELDS[16], "x");
        assert_eq!(MAP_ROW_FIELDS[29], "name_id");

        let stated = json!({
            "type": "map_rows",
            "rows": [{ "map_id": 148, "fields": { "y": 5, "region": 7, "x": 9 } }],
        });
        let Message::MapRows { rows } = Message::from_json(&stated).unwrap() else {
            panic!("not map rows");
        };
        assert_eq!(
            rows,
            [MapRow { map_id: 148, fields: vec![(2, 7), (16, 9), (17, 5)], ..MapRow::default() }]
        );

        let mut own = [0u32; 31];
        own[2] = 1;
        own[3] = 10;
        let answered = rows[0].over(own);
        assert_eq!((answered[2], answered[3], answered[16], answered[17]), (7, 10, 9, 5));

        for bad in [
            json!({ "type": "map_rows", "rows": [{ "map_id": 1, "fields": { "colour": 1 } }] }),
            json!({ "type": "map_rows", "rows": [{ "map_id": 1, "fields": { "x": -1 } }] }),
            json!({ "type": "map_rows", "rows": [{ "map_id": 1 }] }),
            // A name of the server's own stands in place of a name id, never beside one.
            json!({ "type": "map_rows", "rows": [{ "map_id": 1, "fields": { "name_id": 7 }, "name": "a" }] }),
            json!({ "type": "map_rows", "rows": [{ "map_id": 1, "fields": {}, "name": "" }] }),
            json!({ "type": "map_rows", "rows": [{ "map_id": 1, "fields": {}, "description": 3 }] }),
        ] {
            assert!(Message::from_json(&bad).is_err(), "{bad}");
        }
    }

    /// A part as large as a part may be still fits a frame, and a checksum that is not one is
    /// refused where the message is read.
    #[test]
    fn a_full_part_fits_a_frame_and_a_bad_checksum_is_malformed() {
        let part = Message::MapPart {
            file_id: 1,
            sha256: sha256_hex(b""),
            offset: 0,
            total: MAP_PART as u32,
            data: vec![0xA5; MAP_PART],
        };
        let frame = part.encode();
        assert!(frame.len() - 4 <= MAX_FRAME, "{} bytes", frame.len());
        assert_eq!(Message::decode(&frame[4..]).unwrap(), part);

        let body = br#"{"type":"map_wanted","file_id":1,"sha256":"../../somewhere"}"#;
        assert!(matches!(Message::decode(body), Err(LinkError::Malformed(_))));
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
