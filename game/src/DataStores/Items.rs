//! One item's definition: what it is, once, however many of it exist.
//!
//! The record lists a creation screen starts from (`config/World/Vanilla/creation/inventory.json`, `config/World/Vanilla/creation/equipment.json`) carry
//! the whole item, because that is what the client is told about it; a template is the same fields
//! without the per-instance ones - the runtime id, the bag and the slot - and the world's start-up
//! seeds one template per distinct definition into `kryta_world.item_template`
//! (`data/sql/base/db_world/0004-item-templates.sql`). It is game data: the world's content, so a row
//! (`docs/port/04-configuration.md`, "Three kinds of data").
//!
//! The reference keeps its item templates in the world database and reads them once at
//! `SetInitialWorldSettings`; this is that pass for the items this tree has so far.
//!
//! Every method goes through the pool's worker queue: a caller here is a session running inside the
//! network's runtime, and the synchronous connection's own `block_on` is not allowed there
//! (`database/src/Database/Pool.rs`).

use std::sync::{Arc, Mutex};

use common::logging::{Log, LogLevel};
use database::database::{DatabaseWorkerPool, Param};

use crate::creation::Item;

/// One item's definition, as the pack states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDefinition {
    /// The table's row id.
    pub id: i64,
    /// The `Gw.dat` file id the client draws.
    pub file_id: u32,
    /// The item's kind code.
    pub item_type: u8,
    /// Which dye slot mask applies.
    pub dye_tint: u8,
    /// The four packed dye colours.
    pub dye_colors: u16,
    /// Crafting materials, as the reference packs them.
    pub materials: u16,
    /// The reference's own unverified byte.
    pub unk1: u8,
    /// Item flags.
    pub flags: u32,
    /// The item's value in gold.
    pub value: u32,
    /// The model id the client draws.
    pub model_id: u32,
    /// How many are in the stack.
    pub quantity: u32,
    /// The profession the item was made for, or zero for none.
    pub profession: u8,
    /// The name, as UTF-16 code units.
    pub name: Vec<u16>,
    /// The modifier list, as dwords.
    pub modifiers: Vec<u32>,
}

const UPSERT_TEMPLATE: &str = "INSERT INTO \"item_template\" (\"file_id\", \"item_type\", \
\"dye_tint\", \"dye_colors\", \"materials\", \"unk1\", \"flags\", \"value\", \"model_id\", \
\"quantity\", \"profession\", \"name\", \"modifiers\") \
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
ON CONFLICT (\"file_id\", \"item_type\", \"dye_tint\", \"dye_colors\", \"materials\", \"unk1\", \
\"flags\", \"value\", \"model_id\", \"quantity\", \"profession\", \"name\", \"modifiers\") \
DO NOTHING RETURNING 1";

const SELECT_TEMPLATES: &str = "SELECT \"id\"::bigint, \"file_id\"::bigint, \"item_type\"::bigint, \
\"dye_tint\"::bigint, \"dye_colors\"::bigint, \"materials\"::bigint, \"unk1\"::bigint, \
\"flags\"::bigint, \"value\"::bigint, \"model_id\"::bigint, \"quantity\"::bigint, \
\"profession\"::bigint, \"name\", \"modifiers\" FROM \"item_template\" ORDER BY \"file_id\"";

/// The item templates in the world database.
pub struct DatabaseItems {
    pool: Arc<Mutex<DatabaseWorkerPool>>,
    log: Log,
}

/// The definitions a world runs with, built straight from the creation content's items.
///
/// The pack says what an item is; nothing copies it into a table first. The id is the definition's
/// position, which is only an identity within one start - the client's key for an item is the item's
/// own id, and the row a character holds carries its fields.
pub fn definitions_as_templates(items: Vec<Item>) -> Vec<ItemDefinition> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| ItemDefinition {
            id: index as i64 + 1,
            file_id: item.file_id,
            item_type: item.item_type,
            dye_tint: item.dye_tint,
            dye_colors: item.dye_colors,
            materials: item.materials,
            unk1: item.unk1,
            flags: item.flags,
            value: item.value,
            model_id: item.model_id,
            quantity: item.quantity,
            profession: item.profession,
            name: item.name.clone(),
            modifiers: item.modifiers.clone(),
        })
        .collect()
}
impl DatabaseItems {
    /// A source that asks this pool.
    pub fn new(pool: Arc<Mutex<DatabaseWorkerPool>>, log: Log) -> Self {
        Self { pool, log }
    }

    /// Seed one template per distinct definition, and answer how many rows were written.
    ///
    /// Idempotent: a definition the table already holds (the same file id and dyes) writes nothing,
    /// so a start-up seeds the definitions it has and leaves the rest alone.
    pub fn seed(&self, definitions: &[Item]) -> Result<usize, String> {
        let mut written = 0usize;
        for item in definitions {
            let reply = match self.pool.lock() {
                Ok(pool) => pool.query_reply(
                    UPSERT_TEMPLATE,
                    vec![
                        Param::Integer(i64::from(item.file_id)),
                        Param::Integer(i64::from(item.item_type)),
                        Param::Integer(i64::from(item.dye_tint)),
                        Param::Integer(i64::from(item.dye_colors)),
                        Param::Integer(i64::from(item.materials)),
                        Param::Integer(i64::from(item.unk1)),
                        Param::Integer(i64::from(item.flags)),
                        Param::Integer(i64::from(item.value)),
                        Param::Integer(i64::from(item.model_id)),
                        Param::Integer(i64::from(item.quantity)),
                        Param::Integer(i64::from(item.profession)),
                        Param::Text(units_text(&item.name)),
                        Param::Text(dwords_text(&item.modifiers)),
                    ],
                ),
                Err(_) => {
                    return Err(
                        "the world database pool is poisoned; no template was written".to_string(),
                    );
                }
            };
            match reply.recv() {
                Ok(Ok(rows)) => written += rows.len(),
                Ok(Err(error)) => {
                    return Err(format!(
                        "the item template {} cannot be written: {error}",
                        item.file_id
                    ));
                }
                Err(_) => {
                    return Err(
                        "the world database answered no query: it is shutting down".to_string()
                    );
                }
            }
        }
        Ok(written)
    }

    /// Every template the table holds.
    pub fn list(&self) -> Result<Vec<ItemDefinition>, String> {
        let rows = self.query(SELECT_TEMPLATES, Vec::new())?;
        rows.iter()
            .map(|row| {
                Ok(ItemDefinition {
                    id: number(row, 0)?,
                    file_id: number(row, 1)? as u32,
                    item_type: number(row, 2)? as u8,
                    dye_tint: number(row, 3)? as u8,
                    dye_colors: number(row, 4)? as u16,
                    materials: number(row, 5)? as u16,
                    unk1: number(row, 6)? as u8,
                    flags: number(row, 7)? as u32,
                    value: number(row, 8)? as u32,
                    model_id: number(row, 9)? as u32,
                    quantity: number(row, 10)? as u32,
                    profession: number(row, 11)? as u8,
                    name: parse_units(&row.get::<usize, String>(12))?,
                    modifiers: parse_dwords(&row.get::<usize, String>(13))?,
                })
            })
            .collect()
    }

    /// How many templates the table holds.
    pub fn count(&self) -> Result<i64, String> {
        let rows = self.query("SELECT count(*)::bigint FROM \"item_template\"", Vec::new())?;
        match rows.first() {
            Some(row) => number(row, 0),
            None => Err("the count answered no row".to_string()),
        }
    }

    fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<postgres::Row>, String> {
        let reply = match self.pool.lock() {
            Ok(pool) => pool.query_reply(sql.to_string(), params),
            Err(_) => return Err("the world database pool is poisoned".to_string()),
        };
        match reply.recv() {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => {
                self.log.out(
                    LogLevel::Error,
                    "server.items",
                    format!("the item template table cannot be read: {error}"),
                );
                Err(error)
            }
            Err(_) => Err("the world database answered no query: it is shutting down".to_string()),
        }
    }
}

/// A list of code units as comma-separated decimal text.
fn units_text(units: &[u16]) -> String {
    units
        .iter()
        .map(|unit| unit.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// A list of dwords as comma-separated decimal text.
fn dwords_text(values: &[u32]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_units(text: &str) -> Result<Vec<u16>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|part| {
            part.trim()
                .parse::<u16>()
                .map_err(|error| format!("'{text}' is not a list of code units: {error}"))
        })
        .collect()
}

fn parse_dwords(text: &str) -> Result<Vec<u32>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|part| {
            part.trim()
                .parse::<u32>()
                .map_err(|error| format!("'{text}' is not a list of dwords: {error}"))
        })
        .collect()
}

fn number(row: &postgres::Row, at: usize) -> Result<i64, String> {
    row.try_get::<usize, i64>(at)
        .map_err(|error| format!("column {at}: {error}"))
}
