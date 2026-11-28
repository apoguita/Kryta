//! The names the server states for its maps as text of its own, and how the client is given them.
//!
//! The client's row for a map holds a `name_id` and a `description_id`: ids of texts in the
//! client's own text files. Every place of the interface that shows a map's name codes that id
//! into a string of 16-bit units and has it decoded. A map of the server's own has no text in
//! those files, so its record states the text (`client_row.name`, `client_row.description`,
//! `docs/port/24-custom-maps.md`, step R4), the row is given an id no text file has, and where the
//! client hands a coded string to its decoder (`runtime/text_hooks.rs`) the units of that id are
//! replaced by the text in the coded form for literal text.
//!
//! Ported from Py4GW Reforged Native at `82e4746`, the owner's own project:
//!
//! - the coding of a number in a coded string (`src/GW/ui/ui_methods.cpp:152-159`, the constants,
//!   and `:2648-2656`, `EncStrToUInt32`): a number is written in base `0x7F00`, most significant
//!   digit first, each digit as the unit `0x0100 + digit`, and every unit but the last has
//!   `0x8000` set;
//! - the literal form (`docs/RE/native_ui_title_and_encoded_string_reference.md`, "Literal
//!   plain-text payload structure", `BuildStandaloneLiteralEncodedTextPayload`): a text id that
//!   prints its first text argument, the unit `0x0107`, the text with `[`, `]` and `\` escaped by
//!   `\`, and the unit `0x0001`.
//!
//! Reforged's writer of a number, `UInt32ToEncStr` (`:2631-2646`), sizes its answer by
//! `(value + 0x7EFF) / 0x7F00` and so refuses a large number; the client's own writer does not
//! (`research/Eden/docs/re/encoded-string.md:85-113`, and `:899-905` for the difference). What is
//! matched here is what the client writes, so [`coded_number`] is the inverse of the ported
//! reader, and a test holds the two together.
//!
//! Replacing the units is this tree's own (`docs/decisions/0017-a-maps-client-row-is-stated-in-
//! the-pack.md`): Reforged calls the decoder and does not stand in front of it.

/// The first unit a digit is written as (`WORD_VALUE_BASE`).
const WORD_VALUE_BASE: u16 = 0x0100;
/// Set on every unit of a number but its last (`WORD_BIT_MORE`).
const WORD_BIT_MORE: u16 = 0x8000;
/// The base a number is written in (`WORD_VALUE_RANGE`).
const WORD_VALUE_RANGE: u32 = (WORD_BIT_MORE - WORD_VALUE_BASE) as u32;
/// Ends a literal text inside a coded string (`TERM_INTERMEDIATE`).
const TERM_INTERMEDIATE: u16 = 0x0001;
/// The argument tag for literal text as the first text argument.
const PARAM_LITERAL_1: u16 = 0x0107;
/// The text id that prints its first text argument, in build 39167: the one this tree's custom
/// titles are shown with (`titles.rs`, `CODED_PREFIX`).
const PRINTS_ITS_ARGUMENT: u16 = 0x0BA9;

/// The first id given to a text of the server's own. The client's text files hold 1,024 texts
/// each and about a hundred files for a language (`research/Rurik/toolkit/clientscan/
/// textrec.py:13`), so no text of the client's has an id near this one; and from here up a
/// number is three units, which no shorter number's units can be mistaken for.
pub const FIRST_OWN_TEXT: u32 = 0x7F00_0000;

/// A number as the client writes it in a coded string.
pub fn coded_number(mut value: u32) -> Vec<u16> {
    let mut digits = vec![(value % WORD_VALUE_RANGE) as u16];
    value /= WORD_VALUE_RANGE;
    while value != 0 {
        digits.push((value % WORD_VALUE_RANGE) as u16);
        value /= WORD_VALUE_RANGE;
    }
    digits.reverse();
    let last = digits.len() - 1;
    digits
        .iter()
        .enumerate()
        .map(|(place, digit)| WORD_VALUE_BASE + digit + if place == last { 0 } else { WORD_BIT_MORE })
        .collect()
}

/// The number a coded string starts with, or `None` when it does not start with one.
///
/// Port of `EncStrToUInt32` (`ui_methods.cpp:2648-2656`), with its assertion as the refusal.
pub fn number_of(coded: &[u16]) -> Option<u32> {
    let mut value: u32 = 0;
    for unit in coded {
        let digit = (unit & !WORD_BIT_MORE).checked_sub(WORD_VALUE_BASE)?;
        value = value.checked_mul(WORD_VALUE_RANGE)?.checked_add(u32::from(digit))?;
        if unit & WORD_BIT_MORE == 0 {
            return Some(value);
        }
    }
    None
}

/// Text in the coded form for literal text, with no zero after it.
fn literal(text: &str) -> Vec<u16> {
    let mut coded = vec![PRINTS_ITS_ARGUMENT, PARAM_LITERAL_1];
    for unit in text.encode_utf16() {
        if unit == u16::from(b'[') || unit == u16::from(b']') || unit == u16::from(b'\\') {
            coded.push(u16::from(b'\\'));
        }
        coded.push(unit);
    }
    coded.push(TERM_INTERMEDIATE);
    coded
}

/// The texts of the server's own, each under the id its map's row was given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnTexts {
    /// The units of the id, and the units that stand in their place.
    texts: Vec<(Vec<u16>, Vec<u16>)>,
}

impl OwnTexts {
    /// Hold `text` under `id`. Empty text, or text holding a zero or a unit the decoder reads as
    /// a control, is refused.
    pub fn insert(&mut self, id: u32, text: &str) -> Result<(), String> {
        if text.is_empty() || text.encode_utf16().any(|unit| unit < 0x20) {
            return Err(format!("the text for id {id:#x} is empty or holds a control character"));
        }
        self.texts.push((coded_number(id), literal(text)));
        Ok(())
    }

    /// Whether no text is held.
    pub fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }

    /// A coded string, given without its final zero, with every id held here replaced by its
    /// text; the answer ends with a zero. `None` when the string names none of them, which is
    /// nearly every string the client decodes.
    pub fn replaced(&self, coded: &[u16]) -> Option<Vec<u16>> {
        let mut answer: Option<Vec<u16>> = None;
        let mut at = 0;
        while at < coded.len() {
            // Every id held is three units, the first two with the high bit set.
            let found = (coded[at] & WORD_BIT_MORE != 0)
                .then(|| self.texts.iter().find(|(id, _)| coded[at..].starts_with(id)))
                .flatten();
            match found {
                Some((id, text)) => {
                    answer.get_or_insert_with(|| coded[..at].to_vec()).extend_from_slice(text);
                    at += id.len();
                }
                None => {
                    if let Some(answer) = answer.as_mut() {
                        answer.push(coded[at]);
                    }
                    at += 1;
                }
            }
        }
        answer.map(|mut answer| {
            answer.push(0);
            answer
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The writer is the inverse of the ported reader, and it writes what the sources state: one
    /// unit below `0x7F00`, `[0x8101, 0x0100]` for `0x7F00`
    /// (`research/Eden/crates/eden-text/src/value.rs:310`), three units for an id of the server's.
    #[test]
    fn a_number_is_written_as_the_client_writes_it() {
        assert_eq!(coded_number(8), [0x0108]);
        assert_eq!(coded_number(0x28EE), [0x29EE], "map 148's name id is one unit");
        assert_eq!(coded_number(0x7F00), [0x8101, 0x0100]);
        assert_eq!(coded_number(FIRST_OWN_TEXT).len(), 3);
        for value in [0, 1, 0x7EFF, 0x7F00, 0x7F01, 0x3F00_FFFF, FIRST_OWN_TEXT, FIRST_OWN_TEXT + 7, u32::MAX] {
            let coded = coded_number(value);
            assert_eq!(number_of(&coded), Some(value), "{value:#x}");
            assert!(coded[..coded.len() - 1].iter().all(|unit| unit & WORD_BIT_MORE != 0));
            assert_eq!(coded[coded.len() - 1] & WORD_BIT_MORE, 0);
        }
        assert_eq!(number_of(&[0x0003]), None, "a control unit is not a number");
        assert_eq!(number_of(&[0x8101]), None, "a number that does not end");
    }

    #[test]
    fn an_id_of_the_servers_is_replaced_by_its_text_wherever_it_stands() {
        let mut texts = OwnTexts::default();
        texts.insert(FIRST_OWN_TEXT, "Kryta [Test] Vale").unwrap();
        texts.insert(FIRST_OWN_TEXT + 1, "A vale.").unwrap();
        assert!(texts.insert(FIRST_OWN_TEXT + 2, "").is_err());
        assert!(texts.insert(FIRST_OWN_TEXT + 2, "two\nlines").is_err());

        let name = coded_number(FIRST_OWN_TEXT);
        let mut expected: Vec<u16> = vec![0x0BA9, 0x0107];
        expected.extend("Kryta \\[Test\\] Vale".encode_utf16());
        expected.push(0x0001);

        // Alone, as the loading screen codes a map's name.
        let mut whole = expected.clone();
        whole.push(0);
        assert_eq!(texts.replaced(&name), Some(whole));

        // As a nested argument of another text, as a window title codes it: text 0x2B0 with the
        // name as its first text argument, a segment that ends with 0x0001.
        let mut title = vec![0x03B0, 0x010A];
        title.extend(&name);
        title.push(0x0001);
        let mut answer = vec![0x03B0, 0x010A];
        answer.extend(&expected);
        answer.extend([0x0001, 0]);
        assert_eq!(texts.replaced(&title), Some(answer));

        // A string that names no text of the server's is left alone.
        assert_eq!(texts.replaced(&[0x29EE]), None);
        assert_eq!(texts.replaced(&coded_number(FIRST_OWN_TEXT + 9)), None);
        assert_eq!(OwnTexts::default().replaced(&name), None);
    }
}
