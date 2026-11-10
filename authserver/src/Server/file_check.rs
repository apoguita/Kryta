//! The file check: a fixed 21-byte question and a fixed 32-byte answer, before anything else.
//!
//! Reference for the shape: the reference has no equivalent - this is this client's own gate, and it
//! is the reason `game/FileCheck/` exists in this tree's naming. The bytes are a value file,
//! `data/file_check.conf`, because they change when the client's files change.
//!
//! What it is: on the file-server address the client asks which game files it should have, compares
//! the whole reply, and treats a wrong or missing answer as fatal. The question is 21 bytes and the
//! answer 32.
//!
//! Why it belongs on the login path: a socket that classifies the first four bytes as an unknown
//! connection and closes leaves the client sitting at "Connecting" with nothing to say. Before this
//! existed, that is exactly what this server did to the file check.

use std::fmt;

/// Why the file check could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileCheckError {
    /// The file could not be read.
    Unreadable(String),
    /// A value is missing, or is not an even number of hexadecimal digits.
    BadValue {
        /// Which key.
        key: String,
        /// What was wrong with it.
        reason: String,
    },
}

impl fmt::Display for FileCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(reason) => f.write_str(reason),
            Self::BadValue { key, reason } => write!(f, "'{key}' in the file check: {reason}"),
        }
    }
}

impl std::error::Error for FileCheckError {}

/// The question and the answer, as the value file states them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCheck {
    request: Vec<u8>,
    answer: Vec<u8>,
}

impl FileCheck {
    /// Read the value file.
    ///
    /// One `key = hex` per line, `#` starts a comment. Both keys are required: a file check with no
    /// question cannot recognise the client, and one with no answer would close the socket the
    /// client is waiting on - which is the failure this exists to stop.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, FileCheckError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| FileCheckError::Unreadable(format!("{}: {error}", path.display())))?;

        let mut request = None;
        let mut answer = None;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "request" => request = Some(parse_hex(key.trim(), value)?),
                "answer" => answer = Some(parse_hex(key.trim(), value)?),
                _ => {}
            }
        }

        let request = request.ok_or_else(|| FileCheckError::BadValue {
            key: "request".to_string(),
            reason: "missing".to_string(),
        })?;
        let answer = answer.ok_or_else(|| FileCheckError::BadValue {
            key: "answer".to_string(),
            reason: "missing".to_string(),
        })?;

        Ok(Self { request, answer })
    }

    /// A file check built from bytes, for a test or a caller that holds them already.
    pub fn from_bytes(request: Vec<u8>, answer: Vec<u8>) -> Self {
        Self { request, answer }
    }

    /// The question.
    pub fn request(&self) -> &[u8] {
        &self.request
    }

    /// The answer.
    pub fn answer(&self) -> &[u8] {
        &self.answer
    }

    /// How many bytes a complete question is.
    pub fn request_len(&self) -> usize {
        self.request.len()
    }

    /// Whether what has arrived so far could still become the question.
    ///
    /// A partial question is **not** a refusal: the caller reads more and asks again, which is what
    /// the reference does rather than guessing at a short read.
    pub fn could_be_request(&self, bytes: &[u8]) -> bool {
        bytes.len() <= self.request.len() && self.request.starts_with(bytes)
    }

    /// Whether these are the whole question.
    pub fn is_request(&self, bytes: &[u8]) -> bool {
        bytes == self.request.as_slice()
    }
}

/// Read space- or comma-separated hexadecimal pairs.
fn parse_hex(key: &str, value: &str) -> Result<Vec<u8>, FileCheckError> {
    let mut bytes = Vec::new();
    for word in value.split(|character: char| character.is_whitespace() || character == ',') {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        let word = word.strip_prefix("0x").unwrap_or(word);
        if word.len() % 2 != 0 {
            return Err(FileCheckError::BadValue {
                key: key.to_string(),
                reason: format!("'{word}' is an odd number of hexadecimal digits"),
            });
        }
        // Two digits at a time, so a long string needs no separators at all.
        for pair in word.as_bytes().chunks(2) {
            let pair = std::str::from_utf8(pair).map_err(|_| FileCheckError::BadValue {
                key: key.to_string(),
                reason: "not hexadecimal text".to_string(),
            })?;
            let byte = u8::from_str_radix(pair, 16).map_err(|_| FileCheckError::BadValue {
                key: key.to_string(),
                reason: format!("'{pair}' is not a hexadecimal byte"),
            })?;
            bytes.push(byte);
        }
    }

    if bytes.is_empty() {
        return Err(FileCheckError::BadValue {
            key: key.to_string(),
            reason: "no bytes".to_string(),
        });
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> FileCheck {
        FileCheck::load("../data/file_check.conf").expect("the shipped file check is readable")
    }

    #[test]
    fn the_shipped_file_holds_twenty_one_bytes_of_question_and_thirty_two_of_answer() {
        let check = shipped();
        assert_eq!(check.request_len(), 21);
        assert_eq!(check.answer().len(), 32);
    }

    #[test]
    fn the_answer_starts_with_the_header_a_capture_shows() {
        let check = shipped();
        assert_eq!(&check.answer()[..2], &[0xF1, 0x02], "header 0x02F1");
        assert_eq!(&check.answer()[2..4], &[0x20, 0x00], "length 0x0020");
    }

    #[test]
    fn a_prefix_could_still_become_the_question() {
        let check = shipped();
        assert!(check.could_be_request(&[]));
        assert!(check.could_be_request(&check.request()[..4]));
        assert!(check.could_be_request(&check.request()[..20]));
        assert!(!check.could_be_request(&[0x00, 0x04, 0x0C, 0x00]), "the auth header is not the file check");
        assert!(!check.could_be_request(&check.request()[..4].iter().copied().chain([0x99]).collect::<Vec<_>>()));
    }

    #[test]
    fn only_the_whole_question_is_the_question() {
        let check = shipped();
        assert!(check.is_request(check.request()));
        assert!(!check.is_request(&check.request()[..20]));

        let mut wrong = check.request().to_vec();
        wrong[20] = 0x01;
        assert!(!check.is_request(&wrong));
    }

    #[test]
    fn a_file_without_a_key_is_refused() {
        let dir = std::env::temp_dir().join(format!("kryta-filecheck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("only_request.conf");
        std::fs::write(&path, "request = 01 02 03 04\n").unwrap();

        let error = FileCheck::load(&path).unwrap_err();
        assert!(error.to_string().contains("'answer'"), "{error}");
    }

    #[test]
    fn an_odd_number_of_digits_is_refused() {
        let error = parse_hex("request", "01 0").unwrap_err();
        assert!(error.to_string().contains("odd number"), "{error}");
    }
}
