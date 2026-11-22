//! Experience and levels: the curve a total experience falls on, and the skill points a level grants.
//!
//! Ported 1:1 from two ASURA files: the rule is `ASURA/crates/asura-rules/src/experience.rs` (124 lines,
//! all carried: `Curve` and its four methods `:9-57`, the three tests `:59-123`), and the reader is
//! `ASURA/crates/asura-world/src/experience.rs` (89 lines: `Rules` `:9-23`, `load` `:25-50`; its
//! once-per-process `curve()` `:52-89` is this tree's `World::load_experience`, read at start-up like
//! every other pack file). What the client is told when the total changes is
//! `Handlers/CharacterHandler.rs`, `answer_experience`.
//!
//! The numbers are the pack's (`config/World/Vanilla/experience.json`, copied from ASURA's file of the
//! same name); this module holds the rule and no value of its own. The curve is preforged's
//! (`gw_experience_level.h`, cited by ASURA): from level L, the next level costs
//! `first + step * (L - 1)`, which puts level 20 at 140,600 total experience.

/// The file inside a pack that holds the experience curve (`ASURA/crates/asura-world/src/experience.rs:7`).
pub const EXPERIENCE_FILE: &str = "experience.json";

/// The experience curve and the skill points a level grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Curve {
    /// Total experience level 1 to 2 costs.
    pub first_level_xp: u32,
    /// How much more each later level costs than the one before it.
    pub xp_per_level_step: u32,
    /// The highest level the curve reaches.
    pub max_level: u32,
    /// Skill points a level grants, on top of the previous level's total.
    pub skill_points_per_level: u32,
}

impl Curve {
    /// The total experience a character at `level` has at least.
    ///
    /// Level 1 is zero. Level 2 is `first_level_xp`. Each step after that adds `xp_per_level_step` more
    /// than the previous step, so the cost is a triangle number, not a flat multiple.
    pub fn threshold(&self, level: u32) -> u32 {
        let level = level.clamp(1, self.max_level.max(1));
        let steps = level - 1;
        steps * self.first_level_xp + self.xp_per_level_step * steps * steps.saturating_sub(1) / 2
    }

    /// The level a total experience falls on.
    ///
    /// The client's own rule is "at least the threshold", not "just below the next one": a character
    /// with exactly 2,000 experience is level 2.
    pub fn level_for(&self, total: u32) -> u32 {
        let mut level = 1;
        while level < self.max_level && total >= self.threshold(level + 1) {
            level += 1;
        }
        level
    }

    /// The skill points a level has earned in total. A level 1 character has none.
    pub fn skill_points_for(&self, level: u32) -> u32 {
        let level = level.clamp(1, self.max_level.max(1));
        self.skill_points_per_level * (level - 1)
    }

    /// The most experience the curve gives a name to: the level cap's own threshold.
    pub fn max_total(&self) -> u32 {
        self.threshold(self.max_level)
    }
}

/// One `{"value": n, "doc": ...}` entry of the file, as ASURA's `character_defaults::Value<u32>` reads it.
fn value(document: &serde_json::Value, key: &str) -> Result<u32, String> {
    let entry = document
        .get(key)
        .ok_or_else(|| format!("{key} is missing"))?;
    let number = entry
        .get("value")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{key}.value is not a whole number"))?;
    u32::try_from(number).map_err(|_| format!("{key}.value {number} does not fit a dword"))
}

/// Read the text of a pack's `experience.json` into the rule the world calls
/// (`ASURA/crates/asura-world/src/experience.rs:25-50`, `load`): the same four keys, and the same two
/// refusals.
pub fn parse(text: &str) -> Result<Curve, String> {
    let document: serde_json::Value =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    let curve = Curve {
        first_level_xp: value(&document, "first_level_xp")?,
        xp_per_level_step: value(&document, "xp_per_level_step")?,
        max_level: value(&document, "max_level")?,
        skill_points_per_level: value(&document, "skill_points_per_level")?,
    };
    if curve.first_level_xp == 0 {
        return Err("first_level_xp must be greater than zero".to_string());
    }
    if curve.max_level == 0 {
        return Err("max_level must be greater than zero".to_string());
    }
    Ok(curve)
}

/// Read `<pack>/experience.json`.
pub fn load(path: &std::path::Path) -> Result<Curve, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("{} is not readable: {error}", path.display()))?;
    parse(&text).map_err(|error| format!("{} is not an experience file: {error}", path.display()))
}

/// A character's progress as the progress record (`0x00E9`) carries it and the `characters` row stores
/// it: total experience, level, and the skill points held and earned
/// (`ASURA/crates/asura-db/src/game.rs:141-147`, migration `0036_character_experience.sql`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// Total experience: the first of the fifteen dwords.
    pub experience: u32,
    /// The level: the tenth.
    pub level: u8,
    /// Skill points held now: the fourteenth.
    pub skill_points: u32,
    /// Skill points earned in total: the fifteenth.
    pub skill_points_total: u32,
}

/// What a change of total experience does to a character's progress
/// (`ASURA/crates/asura-world/src/player.rs:477-503`, `set_experience`, its arithmetic half): the total is
/// capped at the curve's top, the level is what the total says, a level gained adds its skill points,
/// and a lower level gives back the points its levels granted.
pub fn apply(curve: &Curve, before: Progress, total: u32) -> Progress {
    let total = total.min(curve.max_total());
    let level = curve.level_for(total);
    let previous_level = u32::from(before.level.max(1));
    let gained = curve
        .skill_points_for(level)
        .saturating_sub(curve.skill_points_for(previous_level));
    let mut after = before;
    after.experience = total;
    after.level = u8::try_from(level).unwrap_or(u8::MAX);
    after.skill_points = after.skill_points.saturating_add(gained);
    after.skill_points_total = after.skill_points_total.saturating_add(gained);
    // A lower level gives back the points its levels granted. Quests do not grant points yet, so the
    // level's own grant is the whole ledger (`player.rs:493-503`).
    if level < previous_level {
        let granted = curve.skill_points_for(level);
        after.skill_points = after.skill_points.min(granted);
        after.skill_points_total = after
            .skill_points_total
            .min(granted)
            .max(after.skill_points);
    }
    after
}

/// The two shapes a change is told to the client in (`ASURA/crates/asura-world/src/player.rs:528-559`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Told {
    /// One `0x00EE {0, delta}`: a rise of one level or less. The client applies `+=`, and this is what
    /// moves the bar.
    Award(u32),
    /// The whole `0x00E9` block: a decrease, or a rise of more than one level.
    Block,
    /// Nothing changed.
    Nothing,
}

/// Which shape a change from `before` to `after` is told in (`player.rs:540-559`).
pub fn told(before: Progress, after: Progress) -> Told {
    let previous_level = u32::from(before.level.max(1));
    if after.experience > before.experience
        && u32::from(after.level) <= previous_level.saturating_add(1)
    {
        Told::Award(after.experience - before.experience)
    } else if after.experience != before.experience {
        Told::Block
    } else {
        Told::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The values the pack ships, so the test reads as the game's own curve.
    fn curve() -> Curve {
        Curve {
            first_level_xp: 2000,
            xp_per_level_step: 600,
            max_level: 20,
            skill_points_per_level: 1,
        }
    }

    #[test]
    fn the_curve_is_the_reference_curve() {
        let curve = curve();
        assert_eq!(curve.threshold(1), 0);
        assert_eq!(curve.threshold(2), 2000);
        assert_eq!(curve.threshold(3), 4600);
        assert_eq!(curve.threshold(20), 140_600, "the level 20 threshold");
        assert_eq!(curve.level_for(0), 1);
        assert_eq!(curve.level_for(1999), 1);
        assert_eq!(curve.level_for(2000), 2);
        assert_eq!(curve.level_for(4599), 2);
        assert_eq!(curve.level_for(4600), 3);
        assert_eq!(curve.level_for(140_599), 19);
        assert_eq!(curve.level_for(140_600), 20);
        assert_eq!(curve.level_for(u32::MAX), 20);
    }

    #[test]
    fn a_level_grants_one_skill_point_and_level_one_grants_none() {
        let curve = curve();
        assert_eq!(curve.skill_points_for(1), 0);
        assert_eq!(curve.skill_points_for(2), 1);
        assert_eq!(curve.skill_points_for(20), 19);
    }

    #[test]
    fn a_curve_with_a_cap_of_one_stays_at_level_one() {
        let curve = Curve {
            first_level_xp: 2000,
            xp_per_level_step: 600,
            max_level: 1,
            skill_points_per_level: 1,
        };
        assert_eq!(curve.threshold(1), 0);
        assert_eq!(curve.threshold(9), 0, "a level past the cap is the cap");
        assert_eq!(curve.level_for(u32::MAX), 1);
        assert_eq!(curve.skill_points_for(9), 0);
    }

    #[test]
    fn the_shipped_file_is_the_reference_curve() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../config/World/Vanilla")
            .join(EXPERIENCE_FILE);
        assert_eq!(load(&path), Ok(curve()));
        assert!(parse(r#"{"first_level_xp":{"value":0},"xp_per_level_step":{"value":1},"max_level":{"value":20},"skill_points_per_level":{"value":1}}"#).is_err());
        assert!(parse(r#"{"first_level_xp":{"value":1}}"#).is_err());
    }

    #[test]
    fn a_rise_of_one_level_is_an_award_and_grants_its_point() {
        let fresh = Progress {
            level: 1,
            ..Progress::default()
        };
        let after = apply(&curve(), fresh, 2000);
        assert_eq!(after.level, 2);
        assert_eq!((after.skill_points, after.skill_points_total), (1, 1));
        assert_eq!(told(fresh, after), Told::Award(2000));
    }

    #[test]
    fn a_jump_or_a_fall_restates_the_block_and_a_fall_gives_points_back() {
        let fresh = Progress {
            level: 1,
            ..Progress::default()
        };
        let high = apply(&curve(), fresh, 10_000_000);
        assert_eq!(high.experience, 140_600, "capped at the curve's top");
        assert_eq!(high.level, 20);
        assert_eq!(high.skill_points, 19);
        assert_eq!(told(fresh, high), Told::Block);
        let low = apply(&curve(), high, 0);
        assert_eq!(low.level, 1);
        assert_eq!((low.skill_points, low.skill_points_total), (0, 0));
        assert_eq!(told(high, low), Told::Block);
        assert_eq!(told(low, low), Told::Nothing);
    }
}
