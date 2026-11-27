//! The server's maps inside the client: one detour on the client's archive open.
//!
//! The client opens every archive file through one function, `files.open_file_by_file_id_func`
//! (`config/Client/offsets/files.json`); in build 39167 it is at `0x470DB0`, and its one caller of
//! the archive's own lookup (`0x471590`) is itself. Its second function, the open by name
//! (`files.file_hash_to_rec_obj_func`, `0x470FB0`), sends a name that is an encoded file id to the
//! first, and opens any other name as a real file on disk; both give back the same kind of handle,
//! which the map loader reads, frees and closes the same way whichever opened it (`0x705880`:
//! the open by name at `0x705938`, the archive open at `0x705955`, then one read at `0x70596C`).
//!
//! So for a file id the server lists ([`Served`]), the detour asks the client's own open by name
//! for the cached file and hands its handle back. The client reads a real file with its own code;
//! nothing is fabricated, and the archive is not touched. Every other open goes to the client's
//! function unchanged. The cached file is one this DLL fetched from the server; nobody places it
//! (`docs/port/24-custom-maps.md`, "The delivery plan"). A file the cache lacks is fetched on the
//! load that opens it, and the open returns when the file is whole; the other listed files are
//! fetched in the background by the same means (`runtime/mod.rs`).
//!
//! Both are `cdecl`: `handle (archive, file_id, stream, flags, error*)` and
//! `handle (name, flags, error*)` (Py4GW Reforged Native `include/GW/textures/gw_dat_reader.h:42-43`;
//! the calls above).

use std::ffi::c_void;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use client_link::{MapFile, Message};

use super::hooks;
use crate::map_cache::{Answer, Served};

/// The server's list, shared with the link's thread.
static SERVED: OnceLock<&'static Served> = OnceLock::new();

/// The client's archive open, as MinHook left it callable.
static OPEN_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
/// The client's open by name.
static OPEN_BY_NAME: AtomicU32 = AtomicU32::new(0);

/// What the client's archive open writes to its error argument when the archive has no such file
/// (build 39167, `0x470F77`).
const ERROR_NOT_FOUND: u32 = 2;

/// How long a map load waits for a file with no part of it arriving, before it gives up. Each part
/// that arrives starts the wait again, so a large file on a slow link is waited for to its end.
/// This DLL's own value.
pub const PATIENCE: Duration = Duration::from_secs(120);

/// The link, for writing: a load asks the server for its file from the client's own thread, while
/// the link's thread reads. None while the link is down.
static LINK: Mutex<Option<TcpStream>> = Mutex::new(None);

/// Give the hook the link to ask on, or take it away when the link dropped.
pub fn set_link(stream: Option<TcpStream>) {
    *LINK.lock().unwrap_or_else(|error| error.into_inner()) = stream;
}

/// Ask the server for a listed file. False when the link is down or the request was not sent.
pub fn ask(file: &MapFile) -> bool {
    let mut link = LINK.lock().unwrap_or_else(|error| error.into_inner());
    let Some(stream) = link.as_mut() else {
        return false;
    };
    let wanted = Message::MapWanted { file_id: file.file_id, sha256: file.sha256.clone() };
    client_link::write(stream, &wanted).is_ok()
}

/// How many answers are written to the log. A map load opens its file a few times.
static SAID: AtomicU32 = AtomicU32::new(0);
const SAID_AT_MOST: u32 = 64;

fn said(line: String) {
    if SAID.fetch_add(1, Ordering::Relaxed) < SAID_AT_MOST {
        super::log::line(line);
    }
}

/// The resolvers this feature needs, by qualified name.
pub const RESOLVERS: [&str; 2] =
    ["files.open_file_by_file_id_func", "files.file_hash_to_rec_obj_func"];

type OpenArchive = unsafe extern "C" fn(u32, u32, u32, u32, *mut u32) -> *mut c_void;
type OpenByName = unsafe extern "C" fn(*const u16, u32, *mut u32) -> *mut c_void;

/// The client's archive open, answered for the file ids the server lists.
unsafe extern "C" fn open_detour(
    archive: u32,
    file_id: u32,
    stream: u32,
    flags: u32,
    error: *mut u32,
) -> *mut c_void {
    // A listed file the cache lacks is asked of the server here, on the load that needs it, and the
    // load is held until it is whole (`map_cache::Served::answer`).
    let ask_for_load = |file: &MapFile| {
        said(format!(
            "maps: file {file_id:#x} is not in the cache; asked of the server, and the load waits for it"
        ));
        ask(file)
    };
    let answer = SERVED.get().map_or(Answer::Archive, |served| {
        served.answer(file_id, stream, flags, PATIENCE, &ask_for_load)
    });
    let not_found = |why: &str| {
        if !error.is_null() {
            // SAFETY: the caller's error argument, which the client's own function writes too.
            unsafe { error.write(ERROR_NOT_FOUND) };
        }
        said(format!(
            "maps: file {file_id:#x} stream {stream} flags {flags:#x}: {why}; the client was told it is not found"
        ));
        std::ptr::null_mut()
    };
    match answer {
        Answer::Archive => {
            let original = OPEN_ORIGINAL.load(Ordering::Acquire);
            // SAFETY: `install` stores the original before it enables the detour, and the original
            // is the client's function with these arguments.
            unsafe {
                let original: OpenArchive = std::mem::transmute(original);
                original(archive, file_id, stream, flags, error)
            }
        }
        Answer::Cached(path) => {
            let Some(name) = crate::map_cache::name_of(&path) else {
                return not_found("the cached file's path is longer than the client takes");
            };
            let open = OPEN_BY_NAME.load(Ordering::Acquire);
            // SAFETY: the client's own open by name, given a zero-terminated path it copies
            // (`0x471051`), and the caller's own flags and error pointer.
            let handle = unsafe {
                let open: OpenByName = std::mem::transmute(open as usize);
                open(name.as_ptr(), flags, error)
            };
            said(format!(
                "maps: file {file_id:#x} was {} {}",
                if handle.is_null() { "not opened from" } else { "opened from" },
                path.display()
            ));
            handle
        }
        Answer::NotFound => not_found("the server lists it and this open is not served, or its file did not arrive"),
    }
}

/// Install the detour at the addresses the offsets resolved to, given in the order of
/// [`RESOLVERS`]. It answers from `served`, which the link's thread fills.
///
/// # Safety
///
/// The addresses must be the functions `config/Client/offsets/files.json` describes, in this
/// process.
pub unsafe fn install(addresses: [u32; 2], served: &'static Served) -> Result<(), String> {
    let [open_archive, open_by_name] = addresses;
    SERVED
        .set(served)
        .map_err(|_| "the map hook was already installed".to_string())?;
    OPEN_BY_NAME.store(open_by_name, Ordering::Release);
    unsafe {
        let original = hooks::create(open_archive, open_detour as *const c_void)
            .map_err(|error| format!("{}: {error}", RESOLVERS[0]))?;
        OPEN_ORIGINAL.store(original as usize, Ordering::Release);
        hooks::enable(open_archive).map_err(|error| format!("{}: {error}", RESOLVERS[0]))?;
    }
    Ok(())
}
