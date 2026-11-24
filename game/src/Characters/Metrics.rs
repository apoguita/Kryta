//! The metric platform: every value a title or the character sheet will read later.
//!
//! This is deliberately not a title list. A metric is a named, typed value with a scope and an
//! eventual source; the catalog below names the values the server knows about, and the registry
//! seeds every one at zero. No event writes a value yet and no row stores one. The point is that
//! the character sheet can show the full inventory now, and a source can fill one value later
//! without changing the row it appears on.
//!
//! The keys follow the families in `docs/port/19-title-research.md`: character progress, faction,
//! missions and vanquishes, cartography, skills, items and activities, PvP, and the one test
//! counter. No title reads a metric yet (`Titles.rs` keeps a title's `metric` as a label only);
//! every value is waiting for its source.

use std::collections::BTreeMap;

/// One metric the platform knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricDef {
    /// The stable key a title or a source names.
    pub key: &'static str,
    /// What the operator sheet calls it.
    pub label: &'static str,
    /// What one unit counts.
    pub unit: &'static str,
}

/// The metric catalog. Every entry starts at zero and has no source yet.
pub const METRICS: &[MetricDef] = &[
    MetricDef {
        key: "experience",
        label: "Experience",
        unit: "points",
    },
    MetricDef {
        key: "level",
        label: "Level",
        unit: "level",
    },
    MetricDef {
        key: "skill_points",
        label: "Skill points",
        unit: "points",
    },
    MetricDef {
        key: "morale",
        label: "Morale",
        unit: "percent",
    },
    MetricDef {
        key: "faction.kurzick",
        label: "Kurzick faction",
        unit: "points",
    },
    MetricDef {
        key: "faction.luxon",
        label: "Luxon faction",
        unit: "points",
    },
    MetricDef {
        key: "faction.balthazar",
        label: "Balthazar faction",
        unit: "points",
    },
    MetricDef {
        key: "faction.imperial",
        label: "Imperial faction",
        unit: "points",
    },
    MetricDef {
        key: "missions.protector.tyria",
        label: "Tyria missions protected",
        unit: "missions",
    },
    MetricDef {
        key: "missions.protector.cantha",
        label: "Cantha missions protected",
        unit: "missions",
    },
    MetricDef {
        key: "missions.protector.elona",
        label: "Elona missions protected",
        unit: "missions",
    },
    MetricDef {
        key: "missions.guardian.tyria",
        label: "Tyria missions guarded",
        unit: "missions",
    },
    MetricDef {
        key: "missions.guardian.cantha",
        label: "Cantha missions guarded",
        unit: "missions",
    },
    MetricDef {
        key: "missions.guardian.elona",
        label: "Elona missions guarded",
        unit: "missions",
    },
    MetricDef {
        key: "missions.vanquisher.tyria",
        label: "Tyria areas vanquished",
        unit: "areas",
    },
    MetricDef {
        key: "missions.vanquisher.cantha",
        label: "Cantha areas vanquished",
        unit: "areas",
    },
    MetricDef {
        key: "missions.vanquisher.elona",
        label: "Elona areas vanquished",
        unit: "areas",
    },
    MetricDef {
        key: "missions.skill_hunter.tyria",
        label: "Tyria elite skills",
        unit: "skills",
    },
    MetricDef {
        key: "missions.skill_hunter.cantha",
        label: "Cantha elite skills",
        unit: "skills",
    },
    MetricDef {
        key: "missions.skill_hunter.elona",
        label: "Elona elite skills",
        unit: "skills",
    },
    MetricDef {
        key: "cartography.tyria",
        label: "Tyria exploration",
        unit: "per-mille",
    },
    MetricDef {
        key: "cartography.cantha",
        label: "Cantha exploration",
        unit: "per-mille",
    },
    MetricDef {
        key: "cartography.elona",
        label: "Elona exploration",
        unit: "per-mille",
    },
    MetricDef {
        key: "skills.elite.tyria",
        label: "Tyria elite skills learned",
        unit: "skills",
    },
    MetricDef {
        key: "skills.elite.cantha",
        label: "Cantha elite skills learned",
        unit: "skills",
    },
    MetricDef {
        key: "skills.elite.elona",
        label: "Elona elite skills learned",
        unit: "skills",
    },
    MetricDef {
        key: "items.consumed",
        label: "Items consumed",
        unit: "items",
    },
    MetricDef {
        key: "items.chests",
        label: "High-end chests opened",
        unit: "chests",
    },
    MetricDef {
        key: "items.rare_identified",
        label: "Rare items identified",
        unit: "items",
    },
    MetricDef {
        key: "deaths",
        label: "Counted deaths",
        unit: "deaths",
    },
    MetricDef {
        key: "jumps",
        label: "Jumps",
        unit: "jumps",
    },
    MetricDef {
        key: "pvp.wins",
        label: "PvP wins",
        unit: "wins",
    },
    MetricDef {
        key: "pvp.consecutive_wins",
        label: "Consecutive wins",
        unit: "wins",
    },
    MetricDef {
        key: "pvp.hero_points",
        label: "Hero Battle points",
        unit: "points",
    },
    MetricDef {
        key: "pvp.codex_points",
        label: "Codex Arena points",
        unit: "points",
    },
    MetricDef {
        key: "pvp.gamer_points",
        label: "Festival game points",
        unit: "points",
    },
    MetricDef {
        key: "pvp.zaishen_points",
        label: "Zaishen points",
        unit: "points",
    },
    MetricDef {
        key: "titles.maxed",
        label: "Maxed titles",
        unit: "titles",
    },
];

/// The registry. Every catalog metric starts at zero; no source writes one yet.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    values: BTreeMap<&'static str, u32>,
}

impl Metrics {
    /// Every metric the catalog names, at zero.
    pub fn zero() -> Self {
        let mut values = BTreeMap::new();
        for metric in METRICS {
            values.insert(metric.key, 0);
        }
        Self { values }
    }

    /// The value a title or the sheet reads. An unknown key is zero rather than a panic.
    pub fn value(&self, key: &str) -> u32 {
        self.values.get(key).copied().unwrap_or(0)
    }

    /// Set a metric's value. The method exists for the source that will own it later; nothing in
    /// this tree calls it yet.
    pub fn set(&mut self, key: &'static str, value: u32) {
        self.values.insert(key, value);
    }

    /// Add to a metric's value, saturating at `u32::MAX`.
    pub fn add(&mut self, key: &'static str, amount: u32) {
        let value = self.value(key).saturating_add(amount);
        self.values.insert(key, value);
    }

    /// Every metric and its value, in catalog order.
    pub fn rows(&self) -> impl Iterator<Item = (&'static MetricDef, u32)> + '_ {
        METRICS
            .iter()
            .map(move |metric| (metric, self.value(metric.key)))
    }
}

/// One metric row as the character sheet reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetRow {
    /// The stable key.
    pub key: &'static str,
    /// The operator label.
    pub label: &'static str,
    /// What one unit counts.
    pub unit: &'static str,
    /// The value, which is zero at present.
    pub value: u32,
}

/// Every metric as a sheet row, all zero.
pub fn sheet() -> Vec<SheetRow> {
    let metrics = Metrics::zero();
    metrics
        .rows()
        .map(|(metric, value)| SheetRow {
            key: metric.key,
            label: metric.label,
            unit: metric.unit,
            value,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_metric_starts_at_zero() {
        let metrics = Metrics::zero();
        for metric in METRICS {
            assert_eq!(metrics.value(metric.key), 0, "{}", metric.key);
        }
        for row in sheet() {
            assert_eq!(row.value, 0, "{}", row.key);
        }
    }

    #[test]
    fn the_catalog_has_unique_keys_and_the_test_counter() {
        let keys: std::collections::BTreeSet<_> = METRICS.iter().map(|m| m.key).collect();
        assert_eq!(keys.len(), METRICS.len(), "metric keys are unique");
        assert!(keys.contains("jumps"), "the test title's counter exists");
        assert_eq!(Metrics::zero().value("jumps"), 0);
    }

    #[test]
    fn a_source_can_fill_one_value_without_touching_the_rest() {
        let mut metrics = Metrics::zero();
        metrics.add("jumps", 7);
        assert_eq!(metrics.value("jumps"), 7);
        assert_eq!(metrics.value("experience"), 0);
    }
}
