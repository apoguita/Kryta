//! Custom titles: the text the server sent, and the two questions the client's title code asks.
//!
//! The wire carries a custom title's short rank name; the client's own title code is asked for the
//! full name and for the hover description, and answers from here
//! (`docs/port/19-title-research.md`, "Client builds on this machine"). This is the logic of the
//! first title DLL, `tools/client/title-runtime.cpp` (commit `7679ba1`, removed once this one was
//! confirmed): the same rows, the same matching and the same text encoding. What changed is where
//! the rows come from - the server sends them over the link, where the C++ fetched
//! `/kryta/custom-titles.json` every three seconds. The line numbers cited here are that file's.

use serde_json::Value;

/// The first row of the client's title table that is a custom title. The client's own table ends
/// at 47; `title-runtime.cpp:15` names the same row, measured on build 39167.
pub const FIRST_CUSTOM_TITLE_ROW: u32 = 48;

/// The two units every coded text of a custom title starts with: `%str1%` and the literal tag
/// (build 39167 `TextEncode`, `title-runtime.cpp:94`).
const CODED_PREFIX: [u16; 2] = [0x0BA9, 0x0107];

/// One custom title, as the client is asked about it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Title {
    id: u32,
    rank: u32,
    next_rank: u32,
    goal: u32,
    /// The short rank name the wire carries, coded: what the client holds before the substitution.
    fallback: Vec<u16>,
    /// The full name, coded and terminated.
    name: Vec<u16>,
    /// The hover description, coded and terminated.
    description: Vec<u16>,
}

/// The custom titles the server sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    rows: Vec<Title>,
}

/// Text as the client's text decoder wants it: the prefix, the text with `[`, `]` and `\` escaped,
/// the terminator unit 1, and a zero so the client can read it as a string.
///
/// Port of `coded` (`title-runtime.cpp:84-103`). Empty text, or text holding a zero, is refused.
fn coded(text: &str) -> Result<Vec<u16>, String> {
    if text.is_empty() || text.contains('\0') {
        return Err("invalid title text".to_string());
    }
    let mut result = CODED_PREFIX.to_vec();
    for unit in text.encode_utf16() {
        if unit == u16::from(b'[') || unit == u16::from(b']') || unit == u16::from(b'\\') {
            result.push(u16::from(b'\\'));
        }
        result.push(unit);
    }
    result.push(1);
    result.push(0);
    Ok(result)
}

fn number(row: &Value, key: &str) -> Result<u32, String> {
    row.get(key)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| format!("a custom title has no number '{key}'"))
}

fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str, String> {
    row.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("a custom title has no text '{key}'"))
}

impl Catalog {
    /// Read the server's `custom_tracks`.
    ///
    /// Port of `Catalog::refresh` (`title-runtime.cpp:119-141`): a row below
    /// [`FIRST_CUSTOM_TITLE_ROW`] is skipped, and so is a row whose short name is not four to seven
    /// units starting with the coded prefix. A row that is malformed in any other way refuses the
    /// whole list, so the titles already held stay in use.
    pub fn from_tracks(tracks: &[Value]) -> Result<Self, String> {
        let mut rows = Vec::new();
        for row in tracks {
            let id = number(row, "id")?;
            if id < FIRST_CUSTOM_TITLE_ROW {
                continue;
            }
            let units = row
                .get("short_units")
                .and_then(Value::as_array)
                .ok_or("a custom title has no 'short_units'")?
                .iter()
                .map(|unit| unit.as_u64().and_then(|unit| u16::try_from(unit).ok()))
                .collect::<Option<Vec<u16>>>()
                .ok_or("a custom title's 'short_units' holds something that is not a unit")?;
            if !(4..=7).contains(&units.len()) || units[..2] != CODED_PREFIX {
                continue;
            }
            rows.push(Title {
                id,
                rank: number(row, "current_rank")?,
                next_rank: number(row, "next_rank")?,
                goal: number(row, "next_minimum")?,
                fallback: units,
                name: coded(text(row, "name")?)?,
                description: coded(text(row, "description")?)?,
            });
        }
        Ok(Self { rows })
    }

    /// How many custom titles are held.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no custom title is held.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The full name for a rank whose short name the client holds, as a zero-terminated coded
    /// string, or `None` to leave the client's text alone.
    ///
    /// Port of `kryta_title_name` (`title-runtime.cpp:150-169`): `original` is the client's
    /// zero-terminated short name, and it must be a coded custom text and equal a row's short name
    /// with that row's rank.
    pub fn name(&self, rank: u32, original: &[u16]) -> Option<&[u16]> {
        if original.len() < 2 || original[..2] != CODED_PREFIX {
            return None;
        }
        self.rows
            .iter()
            .find(|row| row.rank == rank && row.fallback == original)
            .map(|row| row.name.as_slice())
    }

    /// The hover description for a custom title in the state the client shows, as a zero-terminated
    /// coded string, or `None` to leave the client's text alone.
    ///
    /// Port of `kryta_title_hover_v2` (`title-runtime.cpp:171-187`).
    pub fn hover(&self, id: u32, rank: u32, next_rank: u32, goal: u32) -> Option<&[u16]> {
        if id < FIRST_CUSTOM_TITLE_ROW {
            return None;
        }
        self.rows
            .iter()
            .find(|row| {
                row.id == id && row.rank == rank && row.next_rank == next_rank && row.goal == goal
            })
            .map(|row| row.description.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tracks() -> Vec<Value> {
        vec![
            json!({ "id": 7, "name": "Vanilla", "description": "x", "current_rank": 1,
                    "next_rank": 2, "next_minimum": 3, "short_units": [0x0BA9, 0x0107, 74, 1] }),
            json!({ "id": 48, "name": "Hog [Jumper]", "description": "Jump over hogs",
                    "current_rank": 369, "next_rank": 370, "next_minimum": 10000,
                    "short_units": [0x0BA9, 0x0107, 74, 117, 109, 112, 1] }),
            json!({ "id": 49, "name": "Too long", "description": "x", "current_rank": 371,
                    "next_rank": 372, "next_minimum": 5,
                    "short_units": [0x0BA9, 0x0107, 1, 2, 3, 4, 5, 6] }),
        ]
    }

    #[test]
    fn only_well_formed_custom_rows_are_kept() {
        let catalog = Catalog::from_tracks(&tracks()).unwrap();
        assert_eq!(catalog.len(), 1);
    }

    #[test]
    fn a_short_name_is_answered_with_the_full_coded_name() {
        let catalog = Catalog::from_tracks(&tracks()).unwrap();
        let short = [0x0BA9, 0x0107, 74, 117, 109, 112, 1];
        let name = catalog.name(369, &short).unwrap();
        let mut expected = vec![0x0BA9u16, 0x0107];
        expected.extend("Hog \\[Jumper\\]".encode_utf16());
        expected.extend([1, 0]);
        assert_eq!(name, expected);
        assert_eq!(catalog.name(370, &short), None, "another rank");
        assert_eq!(catalog.name(369, &[0x0BA9, 0x0107, 74, 1]), None, "another short name");
        assert_eq!(catalog.name(369, &[74, 117, 109, 112, 1]), None, "not a coded custom text");
    }

    #[test]
    fn a_hover_is_answered_only_for_the_exact_state() {
        let catalog = Catalog::from_tracks(&tracks()).unwrap();
        let hover = catalog.hover(48, 369, 370, 10000).unwrap();
        assert_eq!(hover.last(), Some(&0));
        assert_eq!(hover[hover.len() - 2], 1);
        assert_eq!(catalog.hover(48, 369, 370, 9999), None);
        assert_eq!(catalog.hover(7, 1, 2, 3), None);
    }

    #[test]
    fn a_malformed_row_refuses_the_whole_list() {
        let mut rows = tracks();
        rows.push(json!({ "id": 50, "name": "", "description": "x", "current_rank": 1,
                          "next_rank": 2, "next_minimum": 3,
                          "short_units": [0x0BA9, 0x0107, 74, 1] }));
        assert_eq!(Catalog::from_tracks(&rows), Err("invalid title text".to_string()));
        assert!(Catalog::from_tracks(&[json!({ "name": "no id" })]).is_err());
    }
}
