//! The text feature inside the client: one detour on the function every coded string is handed
//! to for decoding.
//!
//! The function is Py4GW Reforged Native's `ValidateAsyncDecodeStr`,
//! `void __cdecl (const wchar_t* coded, DecodeStr_Callback callback, void* param)`
//! (`src/GW/ui/ui_methods.cpp:29` at `82e4746`), found by its pattern
//! (`text.validate_async_decode_str_func`, `config/flavors/Vanilla/Client/offsets/text.json`). In
//! build 39167 it is at `0x7C7CA0`: it asserts that the string and the callback are there and
//! that the string starts with a number (`TextApi.cpp` lines 580, 581 and 585), checks the string
//! and hands the three arguments to the decoder of the text context (measured by
//! `find_an_assertion_of_the_client`, `tools/client-dll/src/lib.rs`).
//!
//! Reforged calls this function. The detour here stands in front of it and replaces the ids of
//! the server's own texts by the texts ([`OwnTexts`]), which is this tree's own
//! (`docs/decisions/0017-a-maps-client-row-is-stated-in-the-pack.md`).

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

use super::hooks;
use crate::map_names::OwnTexts;
use crate::patterns::Memory;

/// The resolver this feature needs, by qualified name.
pub const RESOLVER: &str = "text.validate_async_decode_str_func";

/// The texts the server stated last.
static TEXTS: RwLock<Option<OwnTexts>> = RwLock::new(None);
/// The client's own function, as MinHook left it callable.
static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
/// Whether the detour is in.
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// The strings handed to the client in place of its own, by the string they stand for. One is
/// made once and never freed or moved: what the decoder does with the pointer after the call
/// returns is not known, and the strings that name a map of the server's are few.
static ANSWERS: Mutex<Option<HashMap<Vec<u16>, &'static [u16]>>> = Mutex::new(None);
/// How many answers the log has told of. The client asks for a map's name often.
static SAID: AtomicU32 = AtomicU32::new(0);

/// The longest coded string the detour reads. A string that has not ended by here is the
/// client's to decode as it is.
const LONGEST: usize = 4096;

/// Whether the detour is in: only then may a map's row hold an id of the server's own, because
/// the client's decoder asserts on an id its text files do not have.
pub fn installed() -> bool {
    INSTALLED.load(Ordering::Acquire)
}

/// Take the texts the server stated.
pub fn set_texts(texts: OwnTexts) {
    if let Ok(mut answers) = ANSWERS.lock() {
        // The strings already handed out stay where they are; they are only no longer looked up.
        *answers = None;
    }
    if let Ok(mut held) = TEXTS.write() {
        *held = Some(texts);
    }
}

/// The string to decode in place of `coded`, when it names a text of the server's own.
fn answer(coded: &[u16]) -> Option<*const u16> {
    let replaced = TEXTS.read().ok()?.as_ref()?.replaced(coded)?;
    let mut answers = ANSWERS.lock().ok()?;
    let answers = answers.get_or_insert_with(HashMap::new);
    let held = answers
        .entry(coded.to_vec())
        .or_insert_with(|| Box::leak(replaced.into_boxed_slice()));
    if SAID.fetch_add(1, Ordering::Relaxed) < 8 {
        super::log::line(format!(
            "map names: a text of {} unit(s) was given the server's own text in place of an id",
            coded.len()
        ));
    }
    Some(held.as_ptr())
}

/// The detour: the client's function, with the string replaced when it names a text of the
/// server's own.
unsafe extern "C" fn decode_detour(coded: *const u16, callback: *const c_void, param: *mut c_void) {
    let replaced = std::panic::catch_unwind(|| {
        if coded.is_null() {
            return None;
        }
        // SAFETY: the client passes a zero-terminated string; the read stops at the zero or at
        // LONGEST units, whichever comes first.
        let length = (0..LONGEST).find(|&index| unsafe { coded.add(index).read() } == 0)?;
        let units = unsafe { std::slice::from_raw_parts(coded, length) };
        answer(units)
    })
    .ok()
    .flatten();
    let original = ORIGINAL.load(Ordering::Acquire);
    // SAFETY: `install` stores the original before it enables the detour, and the original is the
    // client's `cdecl (coded, callback, param)` function.
    unsafe {
        let original: unsafe extern "C" fn(*const u16, *const c_void, *mut c_void) =
            std::mem::transmute(original);
        original(replaced.unwrap_or(coded), callback, param);
    }
}

/// Install the detour at the address the offset resolved to, after checking that the function
/// begins as the one described above does.
///
/// # Safety
///
/// `address` must be the place `text.validate_async_decode_str_func` describes, in this process.
pub unsafe fn install(address: u32) -> Result<(), String> {
    let memory = hooks::ProcessMemory;
    let (Some(start), Some(next)) = (memory.read_u32(address), memory.read_u32(address + 4)) else {
        return Err(format!("{address:#x} cannot be read"));
    };
    // 55 8B EC 57 8B 7D 08 85: push ebp; mov ebp, esp; push edi; mov edi, [ebp+8]; test edi, edi.
    if start != 0x57EC_8B55 || next != 0x8508_7D8B {
        return Err(format!(
            "{address:#x} does not begin as the decode function does ({start:#010x} {next:#010x})"
        ));
    }
    unsafe {
        let original = hooks::create(address, decode_detour as *const c_void)?;
        ORIGINAL.store(original as usize, Ordering::Release);
        hooks::enable(address)?;
    }
    INSTALLED.store(true, Ordering::Release);
    Ok(())
}
