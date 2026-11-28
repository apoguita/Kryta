//! The map-row feature inside the client: one detour on the function that answers a map's row.
//!
//! The function is `const Row* __cdecl (u32 map_id)` (`maps.row_lookup`,
//! `config/flavors/Vanilla/Client/offsets/maps.json`). In build 39167 it is, whole:
//!
//! ```text
//! 5A6770  55             push ebp
//! 5A6771  8B EC          mov  ebp, esp
//! 5A6773  56             push esi
//! 5A6774  8B 75 08       mov  esi, [ebp+8]
//! 5A6777  81 FE 82030000 cmp  esi, 0x382        ; 898 rows
//! 5A677D  72 14          jb   +0x14
//! ...                    the assertion of ConstMission.cpp, line 3035
//! 5A6793  6B C6 7C       imul eax, esi, 0x7c
//! 5A6796  5E             pop  esi
//! 5A6797  05 48CF9600    add  eax, 0x96CF48
//! 5A679C  5D C3          pop  ebp; ret
//! ```
//!
//! (measured from the client's file by `measure_the_map_table`, `tools/client-dll/src/lib.rs`).
//! The detour answers a map the server states from [`MapRows`] and hands every other map to the
//! client's own function, which is the shape of the title table's detour
//! (`title_hooks.rs`, `metadata_detour`).

use std::ffi::c_void;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::hooks;
use crate::map_rows::{FIELDS, MapRows};
use crate::patterns::Memory;

/// The rows the server stated last.
static ROWS: LazyLock<MapRows> = LazyLock::new(MapRows::default);
/// The client's own lookup, as MinHook left it callable.
static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
/// How many rows the client's own table has: the number its lookup compares a map id with.
static OWN_ROWS: AtomicU32 = AtomicU32::new(0);

/// The resolver this feature needs, by qualified name.
pub const RESOLVER: &str = "maps.row_lookup";

/// The client's own row for a map id, or `None` for an id past its table or before the hook is in.
fn own(map_id: u32) -> Option<[u32; FIELDS]> {
    let original = ORIGINAL.load(Ordering::Acquire);
    if original == 0 || map_id >= OWN_ROWS.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: `install` stored the client's `cdecl (map id) -> row` function, the id is one the
    // function does not assert on, and its answer is a row of 31 values in the client's image.
    unsafe {
        let original: unsafe extern "C" fn(u32) -> *const u32 = std::mem::transmute(original);
        let row = original(map_id);
        if row.is_null() {
            return None;
        }
        Some(std::array::from_fn(|field| row.add(field).read_unaligned()))
    }
}

/// The place among the server's own texts each map was given, by map id. A map keeps its place
/// for as long as the process runs, so a row the client still holds keeps naming the same text.
static TEXT_PLACES: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

/// The rows with each name and description of the server's own turned into an id of its own
/// (`map_names.rs`), and the texts handed to the text hook. Without that hook the rows are left
/// as they came: the client's decoder asserts on an id its text files do not have.
fn with_own_texts(rows: &[client_link::MapRow]) -> Vec<client_link::MapRow> {
    let mut rows = rows.to_vec();
    if !rows.iter().any(|row| row.name.is_some() || row.description.is_some()) {
        if super::text_hooks::installed() {
            super::text_hooks::set_texts(crate::map_names::OwnTexts::default());
        }
        return rows;
    }
    if !super::text_hooks::installed() {
        super::log::line(
            "map names: the server states names of its own, and this client shows its own text for them: the text hook is not in",
        );
        return rows;
    }
    let Ok(mut places) = TEXT_PLACES.lock() else {
        return rows;
    };
    let mut texts = crate::map_names::OwnTexts::default();
    for row in &mut rows {
        let place = match places.iter().position(|map| *map == row.map_id) {
            Some(place) => place,
            None => {
                places.push(row.map_id);
                places.len() - 1
            }
        } as u32;
        for (text, field, id) in [
            (&row.name, client_link::MAP_ROW_NAME_ID, crate::map_names::FIRST_OWN_TEXT + place * 2),
            (&row.description, client_link::MAP_ROW_DESCRIPTION_ID, crate::map_names::FIRST_OWN_TEXT + place * 2 + 1),
        ] {
            let Some(text) = text else { continue };
            match texts.insert(id, text) {
                Ok(()) => {
                    row.fields.retain(|(stated, _)| *stated != field);
                    row.fields.push((field, id));
                    super::log::line(format!(
                        "map names: map {} is given '{text}' as text {id:#x}",
                        row.map_id
                    ));
                }
                Err(error) => super::log::line(format!("map names: map {}: {error}", row.map_id)),
            }
        }
        row.fields.sort_unstable();
    }
    super::text_hooks::set_texts(texts);
    rows
}

/// Take the rows the server sent. Returns how many maps are answered from them.
pub fn set_rows(rows: &[client_link::MapRow]) -> usize {
    let rows = &with_own_texts(rows)[..];
    let count = ROWS.set(rows, &own);
    let table = OWN_ROWS.load(Ordering::Acquire);
    for row in rows {
        let names: Vec<&str> = row
            .fields
            .iter()
            .filter_map(|(field, _)| client_link::MAP_ROW_FIELDS.get(*field).copied())
            .collect();
        let base = if row.map_id < table { "the client's own row" } else { "an empty row: the client has none for this id" };
        super::log::line(format!(
            "map rows: map {} states {} over {base}",
            row.map_id,
            names.join(", ")
        ));
    }
    count
}

/// The detour: a map the server states is answered with its row, and any other is the client's.
unsafe extern "C" fn row_detour(map_id: u32) -> *const u32 {
    if let Some(row) = ROWS.answer(map_id) {
        return row;
    }
    let original = ORIGINAL.load(Ordering::Acquire);
    // SAFETY: `install` stores the original before it enables the detour, and the original is the
    // client's `cdecl (map id) -> row` function.
    unsafe {
        let original: unsafe extern "C" fn(u32) -> *const u32 = std::mem::transmute(original);
        original(map_id)
    }
}

/// The resolver of the client's record of fetched maps, by qualified name.
pub const STORE_RESOLVER: &str = "maps.fetched_store";

/// The client's own record function, as MinHook left it callable.
static STORE_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// The detour on the client's record of the content value a map was loaded with
/// (`maps.fetched_store`). In build 39167 the function is, in outline:
///
/// ```text
/// 850E00  55 8B EC 51 53 56 57   push ebp; mov ebp, esp; push ecx; push ebx; push esi; push edi
/// 850E07  8B 7D 08               mov  edi, [ebp+8]        ; the map id
/// 850E0A  8B F1                  mov  esi, ecx            ; the record
/// 850E0C  81 FF 82030000         cmp  edi, 0x382
/// 850E12  7C 14                  jl   +0x14
/// ...                            the assertion "mission < MISSIONS", MsCliMan.cpp line 368
/// ...                            bit (map id) of the bytes at [esi+0x18] is set, and
/// ...                            [[esi] + map id * 4] = the value
/// 850EBD  C2 0800                ret  8
/// ```
///
/// The record holds one bit and one value for each of the client's own maps and nothing more, so
/// a map id past them cannot be recorded; the client's answer is its assertion (the owner's crash
/// report of 2026-11-27, entering map 10000). Such a map is one the server serves, and the client
/// has nothing of its own to remember about it, so the detour leaves it out.
unsafe extern "thiscall" fn store_detour(record: *mut c_void, map_id: u32, value: u32) {
    if map_id >= OWN_ROWS.load(Ordering::Acquire) {
        super::log::line(format!(
            "map rows: map {map_id} is past the client's own maps and is left out of its record of fetched maps"
        ));
        return;
    }
    let original = STORE_ORIGINAL.load(Ordering::Acquire);
    // SAFETY: `install_store` stores the original before it enables the detour, and the original
    // is the client's `thiscall (map id, value)` function.
    unsafe {
        let original: unsafe extern "thiscall" fn(*mut c_void, u32, u32) = std::mem::transmute(original);
        original(record, map_id, value);
    }
}

/// Install the detour on the record of fetched maps. It is checked first: it must begin as the
/// function above does, and compare with the same number of rows the lookup does.
///
/// # Safety
///
/// `address` must be the place `maps.fetched_store` describes, in this process, and [`install`]
/// must have succeeded.
pub unsafe fn install_store(address: u32) -> Result<(), String> {
    let memory = hooks::ProcessMemory;
    let (Some(start), Some(compare), Some(rows)) = (
        memory.read_u32(address),
        memory.read_u32(address + 10),
        memory.read_u32(address + 14),
    ) else {
        return Err(format!("{address:#x} cannot be read"));
    };
    // 55 8B EC 51: push ebp; mov ebp, esp; push ecx. Then, from +10, 8B F1 81 FF: mov esi, ecx
    // and the start of cmp edi, imm32, whose number follows at +14.
    if start != 0x51EC_8B55 || compare != 0xFF81_F18B || rows != OWN_ROWS.load(Ordering::Acquire) {
        return Err(format!(
            "{address:#x} does not begin as the record of fetched maps does ({start:#010x} {compare:#010x} {rows:#x})"
        ));
    }
    unsafe {
        let original = hooks::create(address, store_detour as *const c_void)?;
        STORE_ORIGINAL.store(original as usize, Ordering::Release);
        hooks::enable(address)?;
    }
    Ok(())
}

/// The resolver of the help guide's list of places, by qualified name.
pub const GUIDE_RESOLVER: &str = "maps.guide_place";

/// The client's own function, as MinHook left it callable.
static GUIDE_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// The detour on the help guide's list of places entered (`maps.guide_place`). In build 39167
/// the function is `void __cdecl (u32 map_id, u32 kind)` at `0x4FC0F0`: it asserts
/// `missionId < MISSIONS` (`GmHelpGuide.cpp` line 2025; the owner's crash report of 2026-11-28,
/// loading into map 10000), looks for the pair in a list of at most five nodes, and when it is
/// not there drops the oldest node, adds `{map id, kind}` and tells the interface the list
/// changed. A map id past the client's own maps is left out of the list: the client refuses it,
/// and what reads the list expects one of its own maps.
unsafe extern "C" fn guide_detour(map_id: u32, kind: u32) {
    if map_id >= OWN_ROWS.load(Ordering::Acquire) {
        super::log::line(format!(
            "map rows: map {map_id} is past the client's own maps and is left out of the help guide's places"
        ));
        return;
    }
    let original = GUIDE_ORIGINAL.load(Ordering::Acquire);
    // SAFETY: `install_guide` stores the original before it enables the detour, and the original
    // is the client's `cdecl (map id, kind)` function.
    unsafe {
        let original: unsafe extern "C" fn(u32, u32) = std::mem::transmute(original);
        original(map_id, kind);
    }
}

/// Install the detour on the help guide's list of places, after checking that the function
/// begins as the one described above and compares with the lookup's number of rows.
///
/// # Safety
///
/// `address` must be the place `maps.guide_place` describes, in this process, and [`install`]
/// must have succeeded.
pub unsafe fn install_guide(address: u32) -> Result<(), String> {
    let memory = hooks::ProcessMemory;
    let (Some(start), Some(compare), Some(rows)) = (
        memory.read_u32(address),
        memory.read_u32(address + 0x23),
        memory.read_u32(address + 0x27),
    ) else {
        return Err(format!("{address:#x} cannot be read"));
    };
    // 55 8B EC 83: push ebp; mov ebp, esp; cmp dword [..], 0. Then, from +0x23, 7D 08 81 FF: the
    // end of mov edi, [ebp+8] and the start of cmp edi, imm32, whose number follows at +0x27.
    if start != 0x83EC_8B55 || compare != 0xFF81_087D || rows != OWN_ROWS.load(Ordering::Acquire) {
        return Err(format!(
            "{address:#x} does not begin as the help guide's place function does ({start:#010x} {compare:#010x} {rows:#x})"
        ));
    }
    unsafe {
        let original = hooks::create(address, guide_detour as *const c_void)?;
        GUIDE_ORIGINAL.store(original as usize, Ordering::Release);
        hooks::enable(address)?;
    }
    Ok(())
}

/// Install the detour at the address the offsets resolved to. Returns how many rows the client's
/// own table has.
///
/// The function is checked before it is replaced: it must begin as the one above does, with the
/// comparison whose number is the table's size. A build where it does not is left alone.
///
/// # Safety
///
/// `address` must be the place `maps.row_lookup` describes, in this process.
pub unsafe fn install(address: u32) -> Result<u32, String> {
    let memory = hooks::ProcessMemory;
    let (Some(start), Some(compare), Some(rows)) = (
        memory.read_u32(address),
        memory.read_u32(address + 5),
        memory.read_u32(address + 9),
    ) else {
        return Err(format!("{address:#x} cannot be read"));
    };
    // 55 8B EC 56: push ebp; mov ebp, esp; push esi. Then, from +5, 75 08 81 FE: the end of
    // mov esi, [ebp+8] and the start of cmp esi, imm32, whose number follows at +9.
    if start != 0x56EC_8B55 || compare != 0xFE81_0875 || rows == 0 || rows > 0xFFFF {
        return Err(format!(
            "{address:#x} does not begin as the map-row lookup does ({start:#010x} {compare:#010x} {rows:#x})"
        ));
    }
    OWN_ROWS.store(rows, Ordering::Release);
    unsafe {
        let original = hooks::create(address, row_detour as *const c_void)?;
        ORIGINAL.store(original as usize, Ordering::Release);
        hooks::enable(address)?;
    }
    Ok(rows)
}
