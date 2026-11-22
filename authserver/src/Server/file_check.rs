//! The file check: a fixed 21-byte question and a fixed 32-byte answer, before anything else.
//!
//! Reference for the shape: the reference has no equivalent - this is this client's own gate, and it
//! is the reason `game/FileCheck/` exists in this tree's naming. The bytes are a **protocol payload**
//! rather than a configuration option: nobody chooses them, and the client compares them exactly. They
//! are constants here for that reason (`docs/port/04-configuration.md`), refreshed by capturing what a
//! live client accepts.
//!
//! What it is: on the file-server address the client asks which game files it should have, compares
//! the whole reply, and treats a wrong or missing answer as fatal. The question is 21 bytes and the
//! answer 32.
//!
//! Why it belongs on the login path: a socket that classifies the first four bytes as an unknown
//! connection and closes leaves the client sitting at "Connecting" with nothing to say. Before this
//! existed, that is exactly what this server did to the file check.

use std::fmt;

/// The question, as a capture prints it.
///
/// Source: `docs/evidence/client-protocol.md` section 7, measured from this client.
const REQUEST_HEX: &str = "01 00 00 00 00 f1 00 10 00 01 00 00 00 00 00 00 00 00 00 00 00";

/// The answer, as a capture prints it.
///
/// Header `0x02F1`, length `0x0020`, then six `Gw.dat` file ids in tight consecutive order with a zero
/// among them. The order is what a capture shows, not a rule; the ids are a client-build fact,
/// refreshed by capturing the reply a live client accepts and never by arithmetic on these numbers.
const ANSWER_HEX: &str = "f1 02 20 00 ab f7 05 00 b4 f7 05 00 9c 43 05 00 b6 f7 05 00 b7 f7 05 00 00 00 00 00 b5 f7 05 00";

/// Why the file check could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileCheckError {
    /// A payload that is missing, or is not an even number of hexadecimal digits.
    BadValue {
        /// Which of the two.
        key: String,
        /// What was wrong with it.
        reason: String,
    },
}

impl fmt::Display for FileCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadValue { key, reason } => write!(f, "'{key}' in the file check: {reason}"),
        }
    }
}

impl std::error::Error for FileCheckError {}

/// The question and the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCheck {
    request: Vec<u8>,
    answer: Vec<u8>,
}

impl FileCheck {
    /// The file check this client demands.
    ///
    /// Built from the two constants above: a file check with no question cannot recognise the client,
    /// and one with no answer would close the socket the client is waiting on - which is the failure
    /// this exists to stop.
    pub fn shipped() -> Result<Self, FileCheckError> {
        Ok(Self {
            request: payload("request", REQUEST_HEX)?,
            answer: payload("answer", ANSWER_HEX)?,
        })
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

/// One of the two payloads, decoded, with its own name in a complaint.
fn payload(key: &str, value: &str) -> Result<Vec<u8>, FileCheckError> {
    common::utilities::hex_bytes(value).map_err(|reason| FileCheckError::BadValue {
        key: key.to_string(),
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> FileCheck {
        FileCheck::shipped().expect("the shipped file check decodes")
    }

    #[test]
    fn the_shipped_payloads_are_twenty_one_bytes_of_question_and_thirty_two_of_answer() {
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
        assert!(
            !check.could_be_request(&[0x00, 0x04, 0x0C, 0x00]),
            "the auth header is not the file check"
        );
        assert!(
            !check.could_be_request(
                &check.request()[..4]
                    .iter()
                    .copied()
                    .chain([0x99])
                    .collect::<Vec<_>>()
            )
        );
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
    fn an_odd_number_of_digits_is_refused() {
        let error = payload("request", "01 0").unwrap_err();
        assert!(error.to_string().contains("odd number"), "{error}");
    }
}
