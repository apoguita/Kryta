//! The project's configuration format: JSON, read by this one module and by nothing else.
//!
//! The reference's format is `Key = value` in a `.conf` file, and its reader is the sibling of this
//! module. This project's format is JSON instead, for one reason: a configuration here is something
//! an operator edits and *shares* - a world list, a map, the numbers a client's behaviour was
//! measured against - and a JSON file is a thing another program can read, a web page can show, and
//! two people can diff. `docs/decisions/0005-json-configuration.md` records that as a deviation from
//! the reference, taken at the owner's request.
//!
//! What survives from the reference is the *shape of the reader*: a flat bag of `dotted.key -> text`
//! that typed getters read with a default, a severity policy, and an environment variable that wins
//! over the file. So a JSON file is not a new configuration system; it is the same one with a
//! different file format, and every key in it is still spelled the reference's way
//! (`RealmServerPort`, `Logger.network.trace`, `LoginDatabaseInfo`).
//!
//! # How a document becomes that bag
//!
//! An object is a **section** and its children are keys; `{"Log": {"EffectiveConfig": true}}` is
//! the key `Log.EffectiveConfig`. A leaf is a scalar, an array of scalars, or an object with a
//! `value` or a `hex`.
//!
//! | JSON | the key's text |
//! |---|---|
//! | `"BindIP": "0.0.0.0"` | `0.0.0.0` |
//! | `"RealmServerPort": 6112` | `6112` |
//! | `"Network": {"Trace": true}` | `1` for `Network.Trace` - the reference's own spelling of yes |
//! | `"Appenders": ["Console", "Auth"]` | `Console,Auth` |
//! | `"data": {"hex": "08 00 66 21"}` | `08 00 66 21`, and [`super::Config::bytes`] decodes it |
//! | `"PidFile": null` | the key is absent, which is how a file says "not set" |
//!
//! Four names are **not** keys, because JSON has no comments and a shared file has to explain
//! itself: `doc` says where a value came from, `note` says what a whole file is for, `immutable`
//! marks a number the client decides rather than the server - changing it does not change the
//! client - and `information` is a container for anything else the file's author wants to write
//! down. An `immutable` number without a `doc` is a complaint, because a label with no reason is a
//! rumour.
//!
//! # `information`, the commentary that has no shape
//!
//! `doc` and `note` are one line each. When the explanation is a measurement, a list of captures, a
//! worked example or a paragraph of history, it goes in `information`, and its contents are the
//! author's business: an object, an array, a nested tree, or a string, at the root of a file, inside
//! a section, or beside a `value`. The reader walks past it and never treats a name inside it as a
//! configuration key, so this costs nothing and cannot break a program:
//!
//! ```json
//! {
//!   "information": {
//!     "what": "the file check is this client's, not the reference's",
//!     "measured": "captured from a live client on 2026-10-26, build 39112",
//!     "replacing it": "capture the reply a client accepts; never arithmetic on these ids"
//!   },
//!   "request": { "hex": "01 00 ...", "doc": "what the client sends first" }
//! }
//! ```
//!
//! The four names are reserved at every level, which is why no configuration key in this tree is
//! called `doc`, `note`, `immutable` or `information`.
//!
//! # And what is not a bag
//!
//! A list of *records* - accounts, worlds - is not a set of keys and is never flattened. It is read
//! as what it is, through [`Document::records`], so one file can hold both settings and rows without
//! either being squeezed into the other's shape.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{Complaint, ConfigError, Severity};

/// The key that describes a value: where it came from, and why it is that number.
pub const DOC: &str = "doc";
/// The key that describes a whole file.
pub const NOTE: &str = "note";
/// The key that marks a number as the client's rather than the server's.
pub const IMMUTABLE: &str = "immutable";
/// The key that holds commentary of any shape: the place for an explanation that is not one line.
///
/// Whatever a file puts behind this name is walked past - objects, arrays, numbers, prose - so an
/// author can write down a measurement, a list of captures or a paragraph of history without the
/// reader mistaking any of it for a setting. It is reserved at every level of a file.
pub const INFORMATION: &str = "information";
/// A leaf's scalar, in the envelope form.
const VALUE: &str = "value";
/// A leaf's bytes, as hexadecimal text.
const HEX: &str = "hex";

/// The names that describe a file rather than configure something.
///
/// Reserved at every level of a file, which is why no configuration key here is called one of them.
const METADATA: [&str; 4] = [DOC, NOTE, IMMUTABLE, INFORMATION];

/// One JSON configuration file, parsed and kept as it was written.
#[derive(Debug, Clone)]
pub struct Document {
    filename: PathBuf,
    root: Value,
}

impl Document {
    /// Read and parse a file.
    pub fn read(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| ConfigError::Malformed(format!("{}: {error}", path.display())))?;
        Self::parse(&text, path)
    }

    /// Parse a file's text, saying in every complaint what it said.
    pub fn parse(text: &str, filename: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let filename = filename.as_ref().to_path_buf();
        let root: Value = serde_json::from_str(text).map_err(|error| {
            ConfigError::Malformed(format!(
                "> Config: {} is not valid JSON: {error}",
                filename.display()
            ))
        })?;

        if !root.is_object() {
            return Err(ConfigError::Malformed(format!(
                "> Config: {} is a JSON {}, but a configuration file is an object of keys",
                filename.display(),
                kind(&root)
            )));
        }

        Ok(Self { filename, root })
    }

    /// The file this document came from.
    pub fn filename(&self) -> &Path {
        &self.filename
    }

    /// The settings in the file, as the reference's key/value bag, with what the reader objected to.
    ///
    /// The bag is `dotted.key -> text`; nothing here is typed. A key the file states twice - which
    /// only happens when two spellings collide, such as a section `A` holding `B` beside a key
    /// named `A.B` - is reported and the first one wins, as the reference's reader does with a
    /// duplicated line.
    pub fn settings(&self) -> (BTreeMap<String, String>, Vec<Complaint>) {
        let mut options = BTreeMap::new();
        let mut complaints = Vec::new();
        flatten(
            &self.root,
            "",
            &self.filename,
            &mut options,
            &mut complaints,
        );
        (options, complaints)
    }

    /// The records in an array at a dotted path, or at the document's root when `path` is empty.
    ///
    /// This is the other shape a file can have: rows rather than settings. Each row must be an
    /// object, because a row is a thing with fields and not a number with a name.
    pub fn records(&self, path: &str) -> Result<Vec<Record<'_>>, String> {
        let node = if path.is_empty() {
            &self.root
        } else {
            follow(&self.root, path).ok_or_else(|| {
                format!("> Config: {} holds no '{}'", self.filename.display(), path)
            })?
        };

        let Some(array) = node.as_array() else {
            return Err(format!(
                "> Config: '{}' in {} is a {}, but a record list is an array",
                path,
                self.filename.display(),
                kind(node)
            ));
        };

        let mut records = Vec::with_capacity(array.len());
        for (index, row) in array.iter().enumerate() {
            if !row.is_object() {
                return Err(format!(
                    "> Config: '{}' row {} in {} is a {}, but a row is an object of fields",
                    path,
                    index + 1,
                    self.filename.display(),
                    kind(row)
                ));
            }
            records.push(Record {
                at: format!("{path}[{}]", index + 1),
                filename: &self.filename,
                value: row,
            });
        }
        Ok(records)
    }

    /// One object as a record: the shape a file with a single thing in it has.
    ///
    /// `""` is the document's root, and a dotted path reaches into it. This is the third shape a file
    /// can have: settings, a record list, and one thing - a map's own description, a card of a pack.
    pub fn record(&self, path: &str) -> Result<Record<'_>, String> {
        let node = if path.is_empty() {
            &self.root
        } else {
            follow(&self.root, path).ok_or_else(|| {
                format!("> Config: {} holds no '{}'", self.filename.display(), path)
            })?
        };
        if !node.is_object() {
            return Err(format!(
                "> Config: '{}' in {} is a {}, but a record is an object of fields",
                path,
                self.filename.display(),
                kind(node)
            ));
        }
        Ok(Record {
            at: if path.is_empty() {
                "root".to_string()
            } else {
                path.to_string()
            },
            filename: &self.filename,
            value: node,
        })
    }
}

/// The documents this process has read, each one parsed and validated once.
///
/// A file is read at load and held here, so a reader asks for it as often as it likes and the file is
/// parsed once - and a file that does not parse fails **at load**, where the program can stop and say
/// so, rather than on the first request that happens to need it. A file that fails is not cached: the
/// next reader tries again and gets the same error, which is how a bad edit stays loud instead of
/// half-remembered.
#[derive(Debug, Default)]
pub struct Store {
    documents: Mutex<HashMap<PathBuf, Arc<Document>>>,
}

impl Store {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The document at `path`: read and validated on the first call, handed back from memory after.
    pub fn document(&self, path: impl AsRef<Path>) -> Result<Arc<Document>, ConfigError> {
        let path = path.as_ref();
        if let Ok(documents) = self.documents.lock()
            && let Some(document) = documents.get(path)
        {
            return Ok(Arc::clone(document));
        }
        let document = Arc::new(Document::read(path)?);
        if let Ok(mut documents) = self.documents.lock() {
            documents.insert(path.to_path_buf(), Arc::clone(&document));
        }
        Ok(document)
    }

    /// How many documents are held: what a start-up line reports.
    pub fn held(&self) -> usize {
        self.documents
            .lock()
            .map(|documents| documents.len())
            .unwrap_or_default()
    }
}
/// One row of a record list.
#[derive(Debug, Clone)]
pub struct Record<'a> {
    at: String,
    filename: &'a Path,
    value: &'a Value,
}

impl Record<'_> {
    /// Where this row is, for a complaint: `accounts[2]`.
    pub fn at(&self) -> &str {
        &self.at
    }

    /// A field's text, or `None` when the row does not state it.
    ///
    /// A reserved name is not a field: `doc`, `note`, `immutable` and `information` are commentary
    /// wherever they appear, so a row that carries them answers `None` for them as a set of settings
    /// would refuse to make them keys.
    pub fn text(&self, field: &str) -> Result<Option<String>, String> {
        if METADATA.contains(&field) {
            return Ok(None);
        }
        let Some(value) = self.value.get(field) else {
            return Ok(None);
        };
        let value = unwrap_envelope(value);
        match value {
            Value::Null => Ok(None),
            Value::String(text) => Ok(Some(text.clone())),
            Value::Number(number) => Ok(Some(number.to_string())),
            Value::Bool(flag) => Ok(Some((if *flag { "1" } else { "0" }).to_string())),
            other => Err(format!(
                "> Config: {}.{field} in {} is a {}, but a field is text, a number or a flag",
                self.at,
                self.filename.display(),
                kind(other)
            )),
        }
    }

    /// A field a row cannot be without.
    pub fn required(&self, field: &str) -> Result<String, String> {
        self.text(field)?.ok_or_else(|| {
            format!(
                "> Config: {} in {} states no '{}'",
                self.at,
                self.filename.display(),
                field
            )
        })
    }

    /// A field's numbers, from `<field>: [1, 2, 3]`.
    ///
    /// A record list of a protocol's own data needs arrays: an item's name is a list of UTF-16 code
    /// units and its modifiers are a list of dwords, and neither is a scalar a key could hold.
    pub fn numbers(&self, field: &str) -> Result<Option<Vec<u32>>, String> {
        if METADATA.contains(&field) {
            return Ok(None);
        }
        let Some(value) = self.value.get(field) else {
            return Ok(None);
        };
        let Value::Array(items) = unwrap_envelope(value) else {
            return Err(format!(
                "> Config: {}.{field} in {} is not a list of numbers",
                self.at,
                self.filename.display()
            ));
        };

        let mut numbers = Vec::with_capacity(items.len());
        for item in items {
            let Some(number) = item.as_u64() else {
                return Err(format!(
                    "> Config: {}.{field} in {} holds a value that is not a number",
                    self.at,
                    self.filename.display()
                ));
            };
            numbers.push(number as u32);
        }
        Ok(Some(numbers))
    }

    /// A field's rows, when the field holds a record list of its own.
    pub fn rows(&self, field: &str) -> Result<Vec<Record<'_>>, String> {
        if METADATA.contains(&field) {
            return Ok(Vec::new());
        }
        let Some(value) = self.value.get(field) else {
            return Ok(Vec::new());
        };
        let Value::Array(items) = value else {
            return Err(format!(
                "> Config: {}.{field} in {} is not a record list",
                self.at,
                self.filename.display()
            ));
        };

        let mut records = Vec::with_capacity(items.len());
        for (index, row) in items.iter().enumerate() {
            if !row.is_object() {
                return Err(format!(
                    "> Config: {}.{field}[{}] in {} is a {}, but a row is an object of fields",
                    self.at,
                    index + 1,
                    self.filename.display(),
                    kind(row)
                ));
            }
            records.push(Record {
                at: format!("{}.{field}[{}]", self.at, index + 1),
                filename: self.filename,
                value: row,
            });
        }
        Ok(records)
    }

    /// A field's strings, from `<field>: ["-c", "a.json"]` - a command line is a list of words.
    ///
    /// A record list of a machine's own processes needs strings: what a service is started with is
    /// words, not a scalar a key could hold, and the same file that says which programs run says
    /// how (the supervisor's `stack.json`).
    pub fn strings(&self, field: &str) -> Result<Option<Vec<String>>, String> {
        if METADATA.contains(&field) {
            return Ok(None);
        }
        let Some(value) = self.value.get(field) else {
            return Ok(None);
        };
        let Value::Array(items) = unwrap_envelope(value) else {
            return Err(format!(
                "> Config: {}.{field} in {} is not a list of strings",
                self.at,
                self.filename.display()
            ));
        };

        let mut strings = Vec::with_capacity(items.len());
        for item in items {
            let Some(text) = item.as_str() else {
                return Err(format!(
                    "> Config: {}.{field} in {} holds a value that is not a string",
                    self.at,
                    self.filename.display()
                ));
            };
            strings.push(text.to_string());
        }
        Ok(Some(strings))
    }

    /// A field's bytes, from `<field>: {"hex": "..."}`.
    pub fn bytes(&self, field: &str) -> Result<Option<Vec<u8>>, String> {
        if METADATA.contains(&field) {
            return Ok(None);
        }
        let Some(value) = self.value.get(field) else {
            return Ok(None);
        };
        let Some(text) = value.get(HEX).and_then(Value::as_str) else {
            return Err(format!(
                "> Config: {}.{field} in {} is not '{{\"hex\": \"...\"}}'",
                self.at,
                self.filename.display()
            ));
        };
        hex_bytes(text).map(Some)
    }
}

/// Decode hexadecimal text: the project's one decoder, which lives in `common::utilities`.
///
/// A configuration file states a byte string this way - `{"hex": "f1 02 20 00"}` - because that is
/// what an operator can compare against a trace. The decoding itself is not this module's business:
/// code that holds a protocol constant uses the same decoder without going through a file, and a
/// payload is not a configuration option however it is written.
pub fn hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    crate::utilities::hex_bytes(text).map_err(|reason| format!("> Config: {reason}"))
}

/// Walk one level of a document into the bag, and complain about what cannot go in it.
fn flatten(
    node: &Value,
    at: &str,
    filename: &Path,
    options: &mut BTreeMap<String, String>,
    complaints: &mut Vec<Complaint>,
) {
    let Value::Object(section) = node else {
        // A leaf a key names directly: `"RealmServerPort": 6112`.
        match node {
            // A null is the file saying "not set", which is how a user comments a key out.
            Value::Null => {}
            other => match scalar(other) {
                Some(text) if !at.is_empty() => insert(at, text, filename, options, complaints),
                Some(_) => {}
                None => complaints.push(Complaint {
                    severity: Severity::Error,
                    message: format!(
                        "> Config: '{}' in {} is a {}, but a value is text, a number, a flag or bytes",
                        at,
                        filename.display(),
                        kind(other)
                    ),
                }),
            },
        }
        return;
    };

    // A leaf stated in the envelope form: `{"value": ...}` or `{"hex": "..."}`. It is a leaf whether
    // or not its value is usable - recursing into it instead would turn a bad leaf into keys nobody
    // wrote, such as `bad.value.nested`.
    if section.contains_key(VALUE) || section.contains_key(HEX) {
        if let Some(text) = leaf(section, at, filename, complaints)
            && !at.is_empty()
        {
            insert(at, text, filename, options, complaints);
        }
        return;
    }

    for (key, child) in section {
        // The names that describe a file rather than configure something. `doc` and `note` are
        // comments, `information` is a container for commentary of any shape, and `immutable`
        // belongs inside a leaf and means nothing on a section.
        if METADATA.contains(&key.as_str()) {
            if key == IMMUTABLE {
                complaints.push(Complaint {
                    severity: Severity::Warn,
                    message: format!(
                        "> Config: {} in {} marks '{}' immutable, but immutable belongs beside a value",
                        key,
                        filename.display(),
                        if at.is_empty() { "the file" } else { at }
                    ),
                });
            }
            continue;
        }
        let path = if at.is_empty() {
            key.clone()
        } else {
            format!("{at}.{key}")
        };
        flatten(child, &path, filename, options, complaints);
    }
}

/// A leaf's text, when this object is a leaf rather than a section.
fn leaf(
    section: &serde_json::Map<String, Value>,
    at: &str,
    filename: &Path,
    complaints: &mut Vec<Complaint>,
) -> Option<String> {
    let value = section.get(VALUE).or_else(|| section.get(HEX))?;

    // An immutable number has to say why it is immutable: the label alone is a rumour, and the file
    // is the only place the reason can live.
    if section.get(IMMUTABLE).is_some() && !section.contains_key(DOC) {
        complaints.push(Complaint {
            severity: Severity::Warn,
            message: format!(
                "> Config: '{}' in {} is marked immutable and carries no 'doc' saying why",
                at,
                filename.display()
            ),
        });
    }

    if section.contains_key(HEX) {
        let text = value.as_str().unwrap_or_default();
        return match hex_bytes(text) {
            Ok(bytes) => Some(
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            Err(reason) => {
                complaints.push(Complaint {
                    severity: Severity::Error,
                    message: format!(
                        "> Config: '{}' in {} is not hexadecimal: {}",
                        at,
                        filename.display(),
                        reason.trim_start_matches("> Config: ")
                    ),
                });
                None
            }
        };
    }

    match scalar(value) {
        Some(text) => Some(text),
        None => {
            complaints.push(Complaint {
                severity: Severity::Error,
                message: format!(
                    "> Config: '{}' in {} is a {}, but a value is text, a number, a flag or bytes",
                    at,
                    filename.display(),
                    kind(value)
                ),
            });
            None
        }
    }
}

/// A scalar's text, in the reference's own spelling.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        // The reference reads a flag as text and compares it with "1", so yes is 1 here.
        Value::Bool(flag) => Some(if *flag { "1" } else { "0" }.to_string()),
        Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(scalar(item)?);
            }
            Some(parts.join(","))
        }
        Value::Null | Value::Object(_) => None,
    }
}

/// Put one key in the bag, or complain that it is already there.
fn insert(
    at: &str,
    text: String,
    filename: &Path,
    options: &mut BTreeMap<String, String>,
    complaints: &mut Vec<Complaint>,
) {
    if options.contains_key(at) {
        complaints.push(Complaint {
            severity: Severity::Warn,
            message: format!(
                "> Config: '{}' is stated twice in {}, and the first one is the one used",
                at,
                filename.display()
            ),
        });
        return;
    }
    options.insert(at.to_string(), text);
}

/// An envelope's value, for a record's field: `{"value": 3}` is 3.
fn unwrap_envelope(value: &Value) -> &Value {
    value.get(VALUE).unwrap_or(value)
}

/// Follow a dotted path through a document.
fn follow<'a>(node: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = node;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current)
}

/// What a JSON value is, for a complaint that has to name it.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "flag",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        Document::parse(text, PathBuf::from("test.json")).unwrap()
    }

    #[test]
    fn sections_become_the_references_dotted_keys() {
        let (settings, complaints) = document(
            r#"{
                "RealmServerPort": 6112,
                "BindIP": "0.0.0.0",
                "Log": {"EffectiveConfig": true, "root": "4,Console Auth"},
                "Network": {"Trace": false}
            }"#,
        )
        .settings();

        assert!(complaints.is_empty(), "{complaints:?}");
        assert_eq!(settings["RealmServerPort"], "6112");
        assert_eq!(settings["BindIP"], "0.0.0.0");
        assert_eq!(
            settings["Log.EffectiveConfig"], "1",
            "a flag is the reference's 1"
        );
        assert_eq!(settings["Log.root"], "4,Console Auth");
        assert_eq!(settings["Network.Trace"], "0");
    }

    #[test]
    fn a_key_may_hold_dots_without_becoming_a_section() {
        // The loggers are named this way: `Logger.network.trace`, where the name carries dots.
        let (settings, _) =
            document(r#"{"Logger": {"network.trace": "6,Console Auth"}}"#).settings();
        assert_eq!(settings["Logger.network.trace"], "6,Console Auth");
    }

    #[test]
    fn a_hex_leaf_is_kept_as_text_and_decodes_to_its_bytes() {
        let (settings, complaints) = document(
            r#"{"data": {"hex": "08 00 66 21", "doc": "the block a client is handed on every login"}}"#,
        )
        .settings();
        assert!(complaints.is_empty(), "{complaints:?}");
        assert_eq!(settings["data"], "08 00 66 21");
        assert_eq!(
            hex_bytes(&settings["data"]).unwrap(),
            vec![0x08, 0x00, 0x66, 0x21]
        );
    }

    #[test]
    fn hex_text_may_be_written_the_way_a_capture_prints_it_and_without_separators() {
        assert_eq!(
            hex_bytes("f1 02 20 00").unwrap(),
            vec![0xF1, 0x02, 0x20, 0x00]
        );
        assert_eq!(
            hex_bytes("F102 2000").unwrap(),
            vec![0xF1, 0x02, 0x20, 0x00]
        );
        assert_eq!(hex_bytes("0xf1,0x02").unwrap(), vec![0xF1, 0x02]);
        assert!(hex_bytes("f1 0").unwrap_err().contains("odd number"));
        assert!(
            hex_bytes("zz")
                .unwrap_err()
                .contains("not a hexadecimal byte")
        );
    }

    #[test]
    fn a_null_is_the_file_saying_not_set() {
        let (settings, complaints) =
            document(r#"{"PidFile": null, "BindIP": "0.0.0.0"}"#).settings();
        assert!(complaints.is_empty(), "{complaints:?}");
        assert!(!settings.contains_key("PidFile"));
        assert_eq!(settings.len(), 1);
    }

    #[test]
    fn an_array_of_scalars_is_a_comma_separated_list() {
        let (settings, _) = document(r#"{"Appenders": ["Console", "Auth"]}"#).settings();
        assert_eq!(settings["Appenders"], "Console,Auth");
    }

    #[test]
    fn the_value_envelope_is_a_leaf_and_its_doc_is_not_a_key() {
        let (settings, complaints) = document(
            r#"{"creation_map": {"value": 0, "immutable": true, "doc": "the map a client asks for while creating a character"}}"#,
        )
        .settings();
        assert!(complaints.is_empty(), "{complaints:?}");
        assert_eq!(settings["creation_map"], "0");
        assert!(!settings.keys().any(|key| key.starts_with("creation_map.")));
    }

    #[test]
    fn an_immutable_number_without_a_reason_is_a_complaint() {
        let (_, complaints) =
            document(r#"{"creation_map": {"value": 0, "immutable": true}}"#).settings();
        assert_eq!(complaints.len(), 1);
        assert!(
            complaints[0].message.contains("carries no 'doc'"),
            "{:?}",
            complaints[0]
        );
    }

    #[test]
    fn a_note_describes_the_file_and_is_not_a_key() {
        let (settings, _) =
            document(r#"{"note": "the intervals this server runs at", "tick_interval_ms": 100}"#)
                .settings();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings["tick_interval_ms"], "100");
    }

    #[test]
    fn information_holds_commentary_of_any_shape_and_none_of_it_is_a_key() {
        let (settings, complaints) = document(
            r#"{
                "information": {
                    "what": "the file check is this client's, not the reference's",
                    "measured": "a live client, 2026-10-26, build 39112",
                    "captures": ["gwps/world/load-stream.log", "client/feature/travel"],
                    "how_many": 2,
                    "nested": {"deeper": {"still": "not a key"}}
                },
                "request": {"hex": "01 00"}
            }"#,
        )
        .settings();

        assert!(complaints.is_empty(), "{complaints:?}");
        assert_eq!(settings.len(), 1, "only 'request' is a key");
        assert_eq!(settings["request"], "01 00");
        assert!(!settings.keys().any(|key| key.starts_with("information")));
    }

    #[test]
    fn information_sits_inside_a_section_and_beside_a_value_too() {
        let (settings, complaints) = document(
            r#"{
                "Portal": {
                    "information": "the client speaks plain HTTP to localhost:6601",
                    "Enable": true
                },
                "RealmServerPort": {
                    "value": 6112,
                    "information": {"not": ["a", "key"]}
                }
            }"#,
        )
        .settings();

        assert!(complaints.is_empty(), "{complaints:?}");
        assert_eq!(settings["Portal.Enable"], "1");
        assert_eq!(settings["RealmServerPort"], "6112");
        assert_eq!(settings.len(), 2);
    }

    #[test]
    fn a_record_may_carry_commentary_and_it_is_not_a_field() {
        let document = document(
            r#"{"accounts": [
                 {"user": "one@example.com", "user_id": "U", "token": "T",
                  "information": {"who": "the name this machine types"}}
               ]}"#,
        );
        let records = document.records("accounts").unwrap();
        assert_eq!(records[0].required("user").unwrap(), "one@example.com");
        assert_eq!(records[0].text("information").unwrap(), None, "not a field");
    }

    #[test]
    fn two_spellings_of_one_key_are_reported_and_the_first_wins() {
        let (settings, complaints) = document(r#"{"A": {"B": 1}, "A.B": 2}"#).settings();
        assert_eq!(settings["A.B"], "1");
        assert_eq!(complaints.len(), 1);
        assert!(
            complaints[0].message.contains("stated twice"),
            "{:?}",
            complaints[0]
        );
    }

    #[test]
    fn a_value_that_cannot_be_a_key_is_reported_and_skipped() {
        let (settings, complaints) = document(r#"{"bad": {"value": {"nested": 1}}}"#).settings();
        assert!(settings.is_empty());
        assert_eq!(complaints[0].severity, Severity::Error);
        assert!(complaints[0].message.contains("bad"), "{:?}", complaints[0]);
    }

    #[test]
    fn a_file_that_is_not_json_names_itself_and_the_parser_error() {
        let error = Document::parse("{ not json", PathBuf::from("bad.json")).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("bad.json"), "{text}");
        assert!(text.contains("not valid JSON"), "{text}");
    }

    #[test]
    fn a_file_that_is_not_an_object_is_refused() {
        let error = Document::parse("[1, 2]", PathBuf::from("list.json")).unwrap_err();
        assert!(error.to_string().contains("is a JSON array"), "{error}");
    }

    #[test]
    fn a_record_list_is_read_as_rows_and_not_as_settings() {
        let document = document(
            r#"{
                "note": "who the portal says an account is",
                "accounts": [
                    {"user": "player@example.com", "user_id": "U-1", "token": "T-1"},
                    {"user": "second@example.com", "user_id": "U-2", "token": "T-2"}
                ]
            }"#,
        );

        let records = document.records("accounts").unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].required("user").unwrap(), "player@example.com");
        assert_eq!(records[1].required("token").unwrap(), "T-2");
        assert_eq!(records[1].at(), "accounts[2]");
        assert_eq!(records[0].text("missing").unwrap(), None);
    }

    #[test]
    fn a_row_without_a_field_names_the_row() {
        let document = document(r#"{"rows": [{"only": "this"}]}"#);
        let records = document.records("rows").unwrap();
        let error = records[0].required("user").unwrap_err();
        assert!(error.contains("rows[1]"), "{error}");
    }

    #[test]
    fn a_record_list_that_is_not_a_list_says_what_it_is() {
        let document = document(r#"{"accounts": {"user": "one"}}"#);
        let error = document.records("accounts").unwrap_err();
        assert!(error.contains("is a object"), "{error}");
        assert!(
            document
                .records("nothing")
                .unwrap_err()
                .contains("holds no")
        );
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    /// A file is parsed once: the second reader gets the same document, not a second parse.
    #[test]
    fn a_document_is_read_once() {
        let path = std::env::temp_dir().join("kryta-store-once.json");
        std::fs::write(&path, "{\"RealmServerPort\": 6112}").expect("the file");
        let store = Store::new();
        let first = store.document(&path).expect("the document reads");
        let second = store.document(&path).expect("the second read is cached");
        assert!(Arc::ptr_eq(&first, &second), "one parsed document, shared");
        assert_eq!(store.held(), 1);
        std::fs::remove_file(&path).ok();
    }

    /// A file that does not parse fails here, at load, and is not remembered as if it had.
    #[test]
    fn a_broken_file_fails_at_load_and_is_not_cached() {
        let path = std::env::temp_dir().join("kryta-store-broken.json");
        std::fs::write(&path, "{ this is not json").expect("the file");
        let store = Store::new();
        assert!(
            store.document(&path).is_err(),
            "a broken file is an error now"
        );
        assert_eq!(store.held(), 0, "and nothing half-parsed is remembered");
        std::fs::remove_file(&path).ok();
    }
}
