//! World-pack title records and their gameplay packets.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::configuration::json::{Document, Record};

use crate::server::packets::title_packets;

/// A title row for the operator sheet, using the same values the server sends.
pub struct SheetRow<'a> {
    /// The name configured for this title.
    pub name: &'a str,
    /// The metric the title represents.
    pub metric: &'a str,
    /// The sample points on the gameplay load.
    pub points: u32,
    /// The visible rank number.
    pub current_rank: u32,
    /// The next goal.
    pub next_minimum: u32,
}

const TEST_TITLE_POINTS: u32 = 1;
// Build 39167's metadata accessor at 0x005A7540 covers IDs 0 through 47.
pub(crate) const FIRST_CUSTOM_TITLE_ROW: u32 = 48;
const RETIRED_VANILLA_IDS: [u32; 3] = [8, 11, 12];
// Preforged packets-6468-5728843.jsonl:110587 supplies the count track shape.
// Gw.dat records 2729 and 2442 are "%str1%" and "%str1%: %str2%".
const COUNT_TRACK: u32 = 2;
const CUSTOM_PROGRESS: &[u16] = &[0x0BA9];
const CUSTOM_HINT: &[u16] = &[0x0A8A];

/// Title records for the test display, read with the world pack.
#[derive(Default)]
pub struct VanillaTitles {
    ranks: BTreeMap<u32, CapturedRank>,
    tracks: BTreeMap<u32, CapturedTrack>,
    initial_rank: u32,
}

struct CapturedRank {
    flags: u32,
    number: u32,
    name: Vec<u16>,
}

struct CapturedTrack {
    name: String,
    metric: String,
    description: Option<String>,
    flags: u32,
    points: u32,
    current_rank: u32,
    current_minimum: u32,
    mode: u32,
    next_rank: u32,
    next_minimum: u32,
    rank_count: u32,
    max_rank_id: u32,
    progress: Vec<u16>,
    hint: Vec<u16>,
}

fn number(row: &Record<'_>, key: &str) -> Result<u32, String> {
    row.required(key)?
        .parse()
        .map_err(|_| format!("{}.{key}: expected an unsigned 32-bit integer", row.at()))
}

fn custom_text(row: &Record<'_>, field: &str) -> Result<String, String> {
    let text = row.required(field)?;
    if text.trim().is_empty()
        || !text
            .bytes()
            .all(|b| b == b' ' || (b.is_ascii_graphic() && !b"[]\\".contains(&b)))
    {
        return Err(format!(
            "{}.{field}: expected printable literal ASCII without markup",
            row.at()
        ));
    }
    Ok(text)
}

fn sample_points(row: &Record<'_>) -> Result<(u32, u32), String> {
    match (row.text("points")?, row.text("current_minimum")?) {
        (None, None) => Ok((TEST_TITLE_POINTS, 0)),
        (Some(_), Some(_)) => Ok((number(row, "points")?, number(row, "current_minimum")?)),
        _ => Err(format!(
            "{}: points and current_minimum must be set together",
            row.at()
        )),
    }
}

fn coded_units(row: &Record<'_>, key: &str) -> Result<Vec<u16>, String> {
    let error = || {
        format!(
            "{}.{key}: expected one to seven nonzero UTF-16 units",
            row.at()
        )
    };
    let units = row.numbers(key)?.ok_or_else(error)?;
    if units.first().is_none_or(|unit| *unit < 0x100) || units.len() >= 8 || units.contains(&0) {
        return Err(error());
    }
    units
        .into_iter()
        .map(|unit| u16::try_from(unit).map_err(|_| error()))
        .collect()
}

impl VanillaTitles {
    /// Read the captured rank and track data and check their references before serving a client.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        Self::read(path).map_err(|error| format!("Title catalog {}: {error}", path.display()))
    }

    fn read(path: &Path) -> Result<Self, String> {
        let catalog = Document::read(path).map_err(|error| error.to_string())?;
        let wire = Document::read(path.with_file_name("vanilla-title-wire.json"))
            .map_err(|error| error.to_string())?;
        Self::from_documents(&catalog, &wire)
    }

    fn from_documents(catalog: &Document, wire: &Document) -> Result<Self, String> {
        let ids = catalog
            .record("")?
            .numbers("vanilla_ids")?
            .ok_or("missing vanilla_ids")?;
        let selected: BTreeSet<_> = ids.iter().copied().collect();
        if selected.len() != ids.len() {
            return Err("vanilla_ids contains a duplicate".into());
        }

        let mut ranks = BTreeMap::new();
        for row in wire.records("ranks")? {
            let id = number(&row, "id")?;
            let rank = CapturedRank {
                flags: number(&row, "flags")?,
                number: number(&row, "number")?,
                name: coded_units(&row, "name")?,
            };
            if ranks.insert(id, rank).is_some() {
                return Err(format!("{}: rank ID {id} is already in use", row.at()));
            }
        }

        let mut tracks = BTreeMap::new();
        for row in wire.records("tracks")? {
            let id = number(&row, "id")?;
            let (points, current_minimum) = sample_points(&row)?;
            let track = CapturedTrack {
                name: row.required("name")?,
                metric: format!("title.{id}"),
                description: None,
                flags: number(&row, "flags")?,
                points,
                current_rank: number(&row, "current_rank")?,
                current_minimum,
                mode: number(&row, "mode")?,
                next_rank: number(&row, "next_rank")?,
                next_minimum: number(&row, "next_minimum")?,
                rank_count: number(&row, "rank_count")?,
                max_rank_id: number(&row, "max_rank_id")?,
                progress: coded_units(&row, "progress")?,
                hint: coded_units(&row, "hint")?,
            };
            if id >= 48
                || RETIRED_VANILLA_IDS.contains(&id)
                || ranks.get(&track.current_rank).is_none_or(|rank| {
                    rank.number == u32::MAX || (rank.number == 0) != (track.current_minimum == 0)
                })
                || ranks.get(&track.next_rank).is_none_or(|rank| {
                    rank.number == 0
                        || (rank.number == u32::MAX) != (track.next_minimum == u32::MAX)
                })
                || !ranks.contains_key(&track.max_rank_id)
                || track.current_rank == track.next_rank
                || track.points < TEST_TITLE_POINTS
                || track.points < track.current_minimum
                || track.points >= track.next_minimum
            {
                return Err(format!(
                    "{}: invalid vanilla title ID {id} or rank references",
                    row.at()
                ));
            }
            if tracks.insert(id, track).is_some() {
                return Err(format!("{}: duplicate title ID {id}", row.at()));
            }
        }
        if let Some(id) = selected.iter().find(|id| !tracks.contains_key(id)) {
            return Err(format!(
                "vanilla_ids contains {id}, which has no record in {}",
                wire.filename().display()
            ));
        }
        tracks.retain(|id, _| selected.contains(id));
        let mut initial_rank = 0;
        let mut custom = BTreeMap::new();
        for row in catalog.records("custom_tracks")? {
            let id = number(&row, "id")?;
            if id < FIRST_CUSTOM_TITLE_ROW {
                return Err(format!(
                    "{}: custom title IDs start at {FIRST_CUSTOM_TITLE_ROW}",
                    row.at()
                ));
            }
            if custom.contains_key(&id) {
                return Err(format!("{}: duplicate custom title ID {id}", row.at()));
            }
            custom.insert(id, row);
        }
        // Build 39167's track worker at 0x00813320 grows its array by row number.
        // Keep client rows dense even when catalog IDs have gaps.
        for (client_row, (_, row)) in (FIRST_CUSTOM_TITLE_ROW..u32::MAX).zip(custom) {
            let current_rank = number(&row, "current_rank")?;
            let next_rank = number(&row, "next_rank")?;
            let points = number(&row, "points")?;
            let current_minimum = number(&row, "current_minimum")?;
            let next_minimum = number(&row, "next_minimum")?;
            let selected = number(&row, "selected")?;
            let name = custom_text(&row, "name")?;
            let description = custom_text(&row, "description")?;
            let metric = custom_text(&row, "metric")?;
            if let Some(rank) = [current_rank, next_rank]
                .into_iter()
                .find(|rank| ranks.contains_key(rank))
            {
                return Err(format!("{}: rank ID {rank} is already in use", row.at()));
            }
            if current_rank == 0 || current_rank >= next_rank || next_rank == u32::MAX {
                return Err(format!(
                    "{}: expected 0 < current_rank < next_rank < u32::MAX",
                    row.at()
                ));
            }
            if current_minimum == 0
                || points < current_minimum
                || points >= next_minimum
                || next_minimum == u32::MAX
            {
                return Err(format!(
                    "{}: expected 0 < current_minimum <= points < next_minimum < u32::MAX",
                    row.at()
                ));
            }
            if selected > 1 || (selected == 1 && initial_rank != 0) {
                return Err(format!(
                    "{}: selected must be 0 or 1, with at most one selected title",
                    row.at()
                ));
            }
            for (id, number, field) in [
                (current_rank, 1, "short_name"),
                (next_rank, 2, "next_short_name"),
            ] {
                let text = custom_text(&row, field)?;
                if text.len() > 4 {
                    return Err(format!(
                        "{}.{field}: at most four ASCII characters",
                        row.at()
                    ));
                }
                ranks.insert(
                    id,
                    CapturedRank {
                        flags: 0,
                        number,
                        name: title_packets::coded_title_name(&text),
                    },
                );
            }
            if selected == 1 {
                initial_rank = current_rank;
            }
            tracks.insert(
                client_row,
                CapturedTrack {
                    name,
                    metric,
                    flags: COUNT_TRACK,
                    points,
                    current_rank,
                    current_minimum,
                    mode: 0,
                    next_rank,
                    next_minimum,
                    rank_count: 2,
                    max_rank_id: next_rank,
                    progress: CUSTOM_PROGRESS.to_vec(),
                    hint: CUSTOM_HINT.to_vec(),
                    // The test title already has its first rank and can be displayed.
                    description: Some(description),
                },
            );
        }
        Ok(Self {
            ranks,
            tracks,
            initial_rank,
        })
    }

    /// Number of configured tracks.
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// The rank initially displayed on an instance load.
    pub(crate) fn initial_display_rank(&self) -> u32 {
        self.initial_rank
    }

    /// The earned tier the current character may display.
    pub(crate) fn selectable_rank(&self, id: u32) -> Option<u32> {
        let track = self.tracks.get(&id)?;
        // Eden titles.rs:309-316 compares the current tier ID to the display threshold.
        (track.current_minimum > 0
            && (track.description.is_some() || track.current_rank >= track.max_rank_id))
            .then_some(track.current_rank)
    }

    /// Public text definitions for a patched client, sent to its DLL over the client link
    /// (`authserver/src/Server/client_link.rs`). No player data is included.
    pub fn custom_catalog(&self) -> Vec<u8> {
        let titles: Vec<_> = self
            .tracks
            .iter()
            .filter_map(|(&client_row, track)| {
                Some(serde_json::json!({
                    "id": client_row,
                    "name": track.name,
                    "description": track.description.as_ref()?,
                    "current_rank": track.current_rank,
                    "next_rank": track.next_rank,
                    "next_minimum": track.next_minimum,
                    "short_units": self.ranks[&track.current_rank].name,
                }))
            })
            .collect();
        serde_json::json!({ "custom_tracks": titles })
            .to_string()
            .into_bytes()
    }

    /// The operator sheet uses the same rank state as the gameplay load.
    pub fn sheet(&self) -> impl Iterator<Item = SheetRow<'_>> {
        self.tracks.values().map(|track| SheetRow {
            name: &track.name,
            metric: &track.metric,
            points: track.points,
            current_rank: self.ranks[&track.current_rank].number,
            next_minimum: track.next_minimum,
        })
    }
}

/// Copy the configured tracks and their referenced ranks onto the normal load.
pub fn load_frames(titles: &VanillaTitles) -> Vec<Vec<u8>> {
    let mut sent_ranks = BTreeSet::new();
    let mut frames = Vec::with_capacity(titles.ranks.len() + titles.tracks.len());
    // ASURA crates/asura-protocol/src/packets.rs:771,774 defines the packet shapes.
    // Retail Northlands seq 537-575 sends the current and next ranks before a track.
    for (&id, track) in &titles.tracks {
        for id in [track.current_rank, track.next_rank, track.max_rank_id] {
            if sent_ranks.insert(id) {
                let rank = &titles.ranks[&id];
                frames.push(title_packets::rank_data(
                    id,
                    rank.flags,
                    rank.number,
                    &rank.name,
                ));
            }
        }
        frames.push(title_packets::track_info(
            id,
            track.flags,
            track.points,
            track.current_rank,
            track.current_minimum,
            track.mode,
            track.next_rank,
            track.next_minimum,
            track.rank_count,
            track.max_rank_id,
            &track.progress,
            &track.hint,
        ));
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_title_metadata_reaches_the_stock_packets_without_importing_points() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/World/Vanilla/titles.json");
        let vanilla = VanillaTitles::load(path).unwrap();
        let frames = load_frames(&vanilla);
        let track_at = |id: u32| {
            frames
                .iter()
                .position(|frame| {
                    frame.starts_with(&title_packets::TITLE_TRACK_INFO.to_le_bytes())
                        && frame[2..6] == id.to_le_bytes()
                })
                .unwrap()
        };
        // Northlands seq 981: Defender of Ascalon at zero rank. The pack carries the
        // two encoded templates, rank IDs and first threshold, but leaves points local.
        let row = &vanilla.tracks[&21];
        let rank = &vanilla.ranks[&177];
        let defender_at = track_at(21);
        assert_eq!(
            frames[defender_at - 1],
            title_packets::rank_data(177, rank.flags, rank.number, &rank.name)
        );
        assert_eq!(
            frames[defender_at],
            title_packets::track_info(
                21,
                2,
                1,
                176,
                0,
                0,
                177,
                20,
                1,
                177,
                &row.progress,
                &row.hint,
            ),
        );
        // Preforged's ranger capture, line 1384, gives Wisdom's first tier.
        let wisdom = &vanilla.tracks[&46];
        assert_eq!(
            (wisdom.current_rank, wisdom.next_rank, wisdom.next_minimum),
            (360, 361, 100)
        );
        let wisdom_frame = &frames[track_at(46)];
        assert_eq!(wisdom_frame[30..34], 100u32.to_le_bytes());
        // Northlands seq 972 and the Preforged ranger capture lines 1381 and 1384.
        for (id, points, goal) in [(10, 3u32, 5u32), (45, 3, 100), (46, 53, 100)] {
            let frame = &frames[track_at(id)];
            assert_eq!(frame[10..14], points.to_le_bytes());
            assert_eq!(frame[18..22], 0u32.to_le_bytes());
            assert_eq!(frame[30..34], goal.to_le_bytes());
        }
        let jump = &frames[track_at(48)];
        assert_eq!(jump[6..10], COUNT_TRACK.to_le_bytes());
        assert_eq!(jump[10..14], 1u32.to_le_bytes());
        assert_eq!(jump[30..34], 10_000u32.to_le_bytes());
        assert_eq!(jump[42..], [1, 0, 0xA9, 0x0B, 1, 0, 0x8A, 0x0A]);
        assert!(
            wisdom_frame.ends_with(
                &wisdom
                    .hint
                    .iter()
                    .flat_map(|unit| unit.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
        );
        // Preforged packets-1360-15821281.jsonl:2237-2239 captures the completed
        // Legendary Skill Hunter row. Send that state without awarding title progress.
        let legendary_at = track_at(32);
        let legendary = &frames[legendary_at];
        assert_eq!(legendary[10..14], 3u32.to_le_bytes());
        assert_eq!(legendary[14..22], [246, 0, 0, 0, 3, 0, 0, 0]);
        assert_eq!(legendary[26..34], [247, 0, 0, 0, 255, 255, 255, 255]);
        assert_eq!(frames[legendary_at - 2][2..6], 246u32.to_le_bytes());
        assert_eq!(frames[legendary_at - 1][2..6], 247u32.to_le_bytes());
    }

    #[test]
    fn custom_title_configuration_sets_the_sheet_and_displayed_rank() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/World/Vanilla/titles.json");
        let titles = VanillaTitles::load(path).unwrap();
        let sample = titles.sheet().find(|row| row.metric == "jumps").unwrap();
        assert_eq!(sample.name, "Hog Jumper");
        assert_eq!(sample.next_minimum, 10_000);
        assert_eq!(titles.initial_display_rank(), 10);
        assert_eq!(titles.selectable_rank(48), Some(10));
        assert_eq!(titles.selectable_rank(46), None);
    }

    #[test]
    fn the_catalog_can_add_another_custom_title_without_server_code() {
        let mut input: serde_json::Value =
            serde_json::from_str(include_str!("../../../config/World/Vanilla/titles.json"))
                .unwrap();
        let custom = input["custom_tracks"].as_array_mut().unwrap();
        custom.retain(|row| row["id"] == 48);
        custom.push(serde_json::json!({
            "id": 1_000, "name": "Frog Chaser", "description": "Chase 200 frogs.",
            "metric": "frogs", "short_name": "Frog", "next_short_name": "Ace",
            "current_rank": 369, "next_rank": 370, "points": 1,
            "current_minimum": 1, "next_minimum": 200, "selected": 0,
        }));
        let doc = Document::parse(&input.to_string(), "second-custom-title.json").unwrap();
        let wire = Document::parse(
            include_str!("../../../config/World/Vanilla/vanilla-title-wire.json"),
            "vanilla-title-wire.json",
        )
        .unwrap();
        let titles = VanillaTitles::from_documents(&doc, &wire).unwrap();
        assert_eq!(titles.track_count(), 47);
        assert_eq!(titles.selectable_rank(49), Some(369));
        assert_eq!(titles.initial_display_rank(), 10);
        let frames = load_frames(&titles);
        assert!(frames.iter().any(|packet| {
            packet[..2] == title_packets::TITLE_TRACK_INFO.to_le_bytes()
                && packet[2..6] == 49u32.to_le_bytes()
                && packet[30..34] == 200u32.to_le_bytes()
        }));

        input["custom_tracks"][1]["id"] = serde_json::json!(48);
        let duplicate = Document::parse(&input.to_string(), "duplicate-title.json").unwrap();
        assert!(
            VanillaTitles::from_documents(&duplicate, &wire)
                .err()
                .unwrap()
                .contains("duplicate custom title ID 48")
        );
        input["custom_tracks"][1]["id"] = serde_json::json!(1_000);
        input["custom_tracks"][1]["short_name"] = serde_json::json!("Frogs");
        let too_long = Document::parse(&input.to_string(), "long-fallback.json").unwrap();
        assert!(
            VanillaTitles::from_documents(&too_long, &wire)
                .err()
                .unwrap()
                .contains(".short_name: at most four ASCII characters")
        );
    }

    /// What a patched client's DLL is sent over the client link: custom text and the numbers the
    /// client's own title code is matched by, and nothing of a player's.
    #[test]
    fn the_custom_catalog_holds_only_custom_text_and_lookup_ids() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../config/World/Vanilla/titles.json");
        let titles = VanillaTitles::load(path).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&titles.custom_catalog()).unwrap();
        let custom = body["custom_tracks"].as_array().unwrap();
        let hog = custom.iter().find(|row| row["id"] == 48).unwrap();
        assert_eq!(hog["name"], "Hog Jumper");
        assert_eq!(hog["description"], "Jump over a hog 10,000 times.");
        for row in custom {
            assert!(row["id"].as_u64().unwrap() >= 48);
            assert!(row.get("points").is_none());
            assert!(row.get("metric").is_none());
        }
    }

    #[test]
    fn a_hundred_custom_titles_fit_after_the_vanilla_table() {
        let mut input: serde_json::Value =
            serde_json::from_str(include_str!("../../../config/World/Vanilla/titles.json"))
                .unwrap();
        let template = input["custom_tracks"][0].clone();
        let custom = input["custom_tracks"].as_array_mut().unwrap();
        custom.clear();
        for index in (0..100).rev() {
            let mut row = template.clone();
            row["id"] = serde_json::json!(48 + index * 1000);
            row["current_rank"] = serde_json::json!(369 + index * 2);
            row["next_rank"] = serde_json::json!(370 + index * 2);
            row["selected"] = serde_json::json!(0);
            custom.push(row);
        }
        let doc = Document::parse(&input.to_string(), "hundred-custom-titles.json").unwrap();
        let wire = Document::parse(
            include_str!("../../../config/World/Vanilla/vanilla-title-wire.json"),
            "vanilla-title-wire.json",
        )
        .unwrap();
        let titles = VanillaTitles::from_documents(&doc, &wire).unwrap();
        assert_eq!(titles.track_count(), 145);
        assert_eq!(titles.selectable_rank(48), Some(369));
        assert_eq!(titles.selectable_rank(147), Some(567));
        assert!(titles.tracks.keys().filter(|&&id| id >= 48).copied().eq(48..148));
        let body: serde_json::Value = serde_json::from_slice(&titles.custom_catalog()).unwrap();
        let rows = body["custom_tracks"].as_array().unwrap();
        assert_eq!(rows.len(), 100);
        assert_eq!(rows[99]["id"], 147);
        assert_eq!(rows[99]["current_rank"], 567);
    }
}
