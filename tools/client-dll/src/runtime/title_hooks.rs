//! The custom title feature inside the client: three hooks that let the client's own title code
//! ask [`Catalog`] for text.
//!
//! These are the three hooks the first title patch, `tools/client/title_patch.py` (commit `7679ba1`,
//! removed once these were confirmed), wrote into the executable's file at fixed addresses of build
//! 39167 (`rank_hook`, `hover_hook`, `metadata_hook`, `:44-74`), doing the same thing at the same
//! three places. The line numbers cited here are that file's. What changed is how they get there: the places are found by
//! byte pattern from the offsets the server sends (`config/flavors/Vanilla/Client/offsets/titles.json`), and the
//! hooks are installed while the client runs.
//!
//! | place | kind | what the hook does |
//! |---|---|---|
//! | `titles.rank_call` | redirect | the rank's short name is replaced by the full name before the client's worker stores it |
//! | `titles.hover_call` | redirect | the hover text is the description, and the client's own formatter is skipped |
//! | `titles.metadata_lookup` | detour | a row from 48 up answers a fallback record instead of asserting |

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::hooks;
use crate::titles::{Catalog, FIRST_CUSTOM_TITLE_ROW};

/// The custom titles the server sent last.
static CATALOG: Mutex<Option<Catalog>> = Mutex::new(None);

/// The client's rank worker: where the rank hook goes on to.
static RANK_WORKER: AtomicU32 = AtomicU32::new(0);
/// The client's hover formatter: where the hover hook goes when the title is not a custom one.
static HOVER_FORMATTER: AtomicU32 = AtomicU32::new(0);
/// The end of the function that holds the hover call: where the hover hook goes with its own text.
static HOVER_RETURN: AtomicU32 = AtomicU32::new(0);
/// The client's own title table lookup, as MinHook left it callable.
static METADATA_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// The record a custom row answers from the title table lookup: the stock "no text" fallback. The
/// client's row and hover code read only the text id at +8 from it; an earned custom row gets its
/// real name from the rank record (`title_patch.py:63-66`, `:112`).
static CUSTOM_METADATA: [u32; 3] = [0, 0, 1];

/// The longest short name the rank hook reads from the client. A custom short name is four to seven
/// units (`titles.rs`); a string that has not ended by here is not one.
const LONGEST_SHORT_NAME: usize = 16;

thread_local! {
    /// The text last handed to the client on this thread. The client copies it before it asks
    /// again, which is what `title-runtime.cpp` relied on with its `thread_local std::wstring`.
    static ANSWER: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

/// How many times each hook has said in the log that it answered the client. The client asks often -
/// a hover asks on every frame it is drawn - so only the first few answers of each kind are written.
static SAID_NAME: AtomicU32 = AtomicU32::new(0);
static SAID_HOVER: AtomicU32 = AtomicU32::new(0);
const SAID_AT_MOST: u32 = 8;

/// Write a hook's answer to the log, the first few times.
fn said(kind: &str, line: String) {
    let counter = if kind == "name" { &SAID_NAME } else { &SAID_HOVER };
    if counter.fetch_add(1, Ordering::Relaxed) < SAID_AT_MOST {
        super::log::line(line);
    }
}

/// Take the titles the server sent. A list that cannot be read leaves the titles held in place.
pub fn set_catalog(tracks: &[serde_json::Value]) -> Result<usize, String> {
    let catalog = Catalog::from_tracks(tracks)?;
    let count = catalog.len();
    if let Ok(mut held) = CATALOG.lock() {
        *held = Some(catalog);
    }
    Ok(count)
}

/// Hand `text` to the client as a pointer that stays good until this thread is asked again.
fn answer(text: Option<Vec<u16>>) -> *const u16 {
    match text {
        Some(text) => ANSWER.with(|held| {
            *held.borrow_mut() = text;
            held.borrow().as_ptr()
        }),
        None => std::ptr::null(),
    }
}

/// The full name for a rank, or null to keep the client's text. Called by [`rank_stub`].
unsafe extern "C" fn name_lookup(rank: u32, original: *const u16) -> *const u16 {
    let found = std::panic::catch_unwind(|| {
        if original.is_null() {
            return None;
        }
        // SAFETY: the client passes its zero-terminated short name; the read stops at the zero or
        // at LONGEST_SHORT_NAME units, whichever comes first.
        let length = (0..LONGEST_SHORT_NAME).find(|&index| unsafe { original.add(index).read() } == 0)?;
        let short = unsafe { std::slice::from_raw_parts(original, length) };
        let held = CATALOG.lock().ok()?;
        held.as_ref()?.name(rank, short).map(<[u16]>::to_vec)
    });
    let found = found.ok().flatten();
    if found.is_some() {
        said("name", format!("titles: rank {rank} was given its full name"));
    }
    answer(found)
}

/// The hover description for a title in the state the client shows, or null. Called by
/// [`hover_stub`].
unsafe extern "C" fn hover_lookup(id: u32, rank: u32, next_rank: u32, goal: u32) -> *const u16 {
    let found = std::panic::catch_unwind(|| {
        let held = CATALOG.lock().ok()?;
        held.as_ref()?
            .hover(id, rank, next_rank, goal)
            .map(<[u16]>::to_vec)
    });
    let found = found.ok().flatten();
    if found.is_some() {
        said("hover", format!("titles: title {id} at rank {rank} was given its description"));
    }
    answer(found)
}

/// Stands in for the client's rank worker at `titles.rank_call`.
///
/// The worker is `cdecl (rank id, pair, name)`. The hook asks for the full name of `(rank id,
/// name)`, puts it in place of `name` when there is one, and goes on to the worker with the stack
/// as the caller left it. The same instructions as `rank_hook` (`title_patch.py:44-49`).
#[unsafe(naked)]
unsafe extern "C" fn rank_stub() {
    core::arch::naked_asm!(
        "push ebp",
        "mov ebp, esp",
        "push dword ptr [ebp + 0x10]",
        "push dword ptr [ebp + 8]",
        "call {lookup}",
        "add esp, 8",
        "test eax, eax",
        "jz 2f",
        "mov dword ptr [ebp + 0x10], eax",
        "2:",
        "leave",
        "jmp dword ptr [{worker}]",
        lookup = sym name_lookup,
        worker = sym RANK_WORKER,
    )
}

/// Stands in for the client's hover formatter at `titles.hover_call`.
///
/// At that call the client has pushed the formatter's six arguments, `edi` points at the title's
/// progress record (current rank at +8, next rank at +0x14, goal at +0x18) and `[ebp - 4]` at the
/// title, whose id is at +4 - the reads of `hover_hook` (`title_patch.py:52-60`). With a
/// description to show, the hook drops its return address and the six arguments and goes to the
/// end of the function with the text in `eax`; otherwise it goes on to the formatter, which
/// returns to the caller as if it had been called directly.
#[unsafe(naked)]
unsafe extern "C" fn hover_stub() {
    core::arch::naked_asm!(
        "push dword ptr [edi + 0x18]",
        "push dword ptr [edi + 0x14]",
        "push dword ptr [edi + 8]",
        "mov edx, dword ptr [ebp - 4]",
        "push dword ptr [edx + 4]",
        "call {lookup}",
        "add esp, 16",
        "test eax, eax",
        "jz 2f",
        "add esp, 0x1C",
        "jmp dword ptr [{finish}]",
        "2:",
        "jmp dword ptr [{formatter}]",
        lookup = sym hover_lookup,
        finish = sym HOVER_RETURN,
        formatter = sym HOVER_FORMATTER,
    )
}

/// Replaces the client's title table lookup: a custom row answers the fallback record, and every
/// other row is the client's own answer (`metadata_hook`, `title_patch.py:63-74`).
unsafe extern "C" fn metadata_detour(row: u32) -> *const u32 {
    if row >= FIRST_CUSTOM_TITLE_ROW {
        return CUSTOM_METADATA.as_ptr();
    }
    let original = METADATA_ORIGINAL.load(Ordering::Acquire);
    // SAFETY: `install` stores the original before it enables the detour, and the original is the
    // client's `cdecl (row) -> record` function.
    unsafe {
        let original: unsafe extern "C" fn(u32) -> *const u32 = std::mem::transmute(original);
        original(row)
    }
}

/// The resolvers this feature needs, by qualified name.
pub const RESOLVERS: [&str; 6] = [
    "titles.rank_call",
    "titles.rank_worker",
    "titles.hover_call",
    "titles.hover_formatter",
    "titles.hover_return",
    "titles.metadata_lookup",
];

/// Install the three hooks at the addresses the offsets resolved to, given in the order of
/// [`RESOLVERS`].
///
/// # Safety
///
/// The addresses must be the places `config/flavors/Vanilla/Client/offsets/titles.json` describes, in this process.
pub unsafe fn install(addresses: [u32; 6]) -> Result<(), String> {
    let [rank_call, rank_worker, hover_call, hover_formatter, hover_return, metadata] = addresses;
    RANK_WORKER.store(rank_worker, Ordering::Release);
    HOVER_FORMATTER.store(hover_formatter, Ordering::Release);
    HOVER_RETURN.store(hover_return, Ordering::Release);
    unsafe {
        let original = hooks::create(metadata, metadata_detour as *const c_void)
            .map_err(|error| format!("titles.metadata_lookup: {error}"))?;
        METADATA_ORIGINAL.store(original as usize, Ordering::Release);
        hooks::enable(metadata).map_err(|error| format!("titles.metadata_lookup: {error}"))?;
        hooks::redirect(rank_call, rank_worker, rank_stub as *const c_void)
            .map_err(|error| format!("titles.rank_call: {error}"))?;
        hooks::redirect(hover_call, hover_formatter, hover_stub as *const c_void)
            .map_err(|error| format!("titles.hover_call: {error}"))?;
    }
    Ok(())
}
