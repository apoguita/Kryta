//! The DLL inside the client: start-up, the server link, and the features' installation.
//!
//! Windows restricts what a DLL may do while it is being loaded, so `DllMain` starts one thread and
//! returns, as Reforged's does (`src/dllmain.cpp`, `DLL_PROCESS_ATTACH`). The thread then, in order:
//!
//! 1. reads the client's own file, to search it (`scanner`);
//! 2. connects to the server and says which process and which build this is;
//! 3. takes the offsets the server sends and resolves them (`patterns`);
//! 4. tells the server what resolved, and installs the features' hooks;
//! 5. stays on the link and takes what the server sends (the custom title text).
//!
//! The client needs the server (owner, 2026-11-23): when start-up cannot finish - the server is not
//! reached, refuses, or its offsets do not fit this build - the player is told why and the client
//! ends. Once started, a link that drops is opened again.

mod crash;
mod file_hooks;
mod hooks;
mod log;
mod map_row_hooks;
mod text_hooks;
mod title_hooks;

use std::ffi::c_void;
use std::net::TcpStream;
use std::time::Duration;

use client_link::{Message, PROTOCOL};
use windows_sys::Win32::Foundation::{CloseHandle, HMODULE, TRUE};
use windows_sys::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetModuleHandleW};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::{CreateThread, ExitProcess};
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

use crate::patterns::Patterns;
use crate::scanner::Scanner;

/// The port the link is served on when nothing says otherwise: `ClientLink.Bind` in
/// `config/machine.json.dist`, section `authserver`.
const DEFAULT_LINK_PORT: u16 = 6650;

/// How many times the first connection is tried, and how long between tries. The stack may still be
/// starting when the client is launched; these are this DLL's own values.
const CONNECT_ATTEMPTS: u32 = 10;
const CONNECT_PAUSE: Duration = Duration::from_secs(1);

/// How long between tries to open a link that dropped after start-up.
const RECONNECT_PAUSE: Duration = Duration::from_secs(3);

/// This DLL's own module handle, for the crash recorder.
static OWN_MODULE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The entry Windows calls when the client loads this DLL.
///
/// # Safety
///
/// Called by the Windows loader only.
#[unsafe(no_mangle)]
unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // SAFETY: the loader's own module handle; a thread that takes no argument.
        unsafe {
            DisableThreadLibraryCalls(module);
            OWN_MODULE.store(module as usize, std::sync::atomic::Ordering::Release);
            let thread = CreateThread(
                std::ptr::null(),
                0,
                Some(thread_main),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            );
            if !thread.is_null() {
                CloseHandle(thread);
            }
        }
    }
    TRUE
}

unsafe extern "system" fn thread_main(_: *mut c_void) -> u32 {
    let outcome = std::panic::catch_unwind(run);
    let failure = match outcome {
        Ok(Ok(())) => return 0,
        Ok(Err(reason)) => reason,
        Err(_) => "Kryta.dll stopped unexpectedly".to_string(),
    };
    log::line(format!("start-up failed: {failure}"));
    fatal(&failure)
}

/// Tell the player why the client cannot run, and end it.
fn fatal(reason: &str) -> ! {
    let text: Vec<u16> = format!("Kryta cannot start the client.\n\n{reason}\n\nSee the Kryta log beside the client.")
        .encode_utf16()
        .chain([0])
        .collect();
    let title: Vec<u16> = "Kryta".encode_utf16().chain([0]).collect();
    // SAFETY: two zero-terminated strings that outlive the call.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
        ExitProcess(1)
    }
}

/// Where the link is served: the host the client was pointed at, on the link's port.
///
/// The client is launched as `Gw.custom.exe -authsrv <host> -portal <host>`
/// (`docs/port/05-tooling.md`), and the link is served by the same program as the portal, so the
/// `-portal` host is the one dialled, then `-authsrv`. A file `Kryta.json` beside the client,
/// `{ "link": "host:port" }`, names another address.
fn link_address(directory: &std::path::Path) -> String {
    if let Ok(text) = std::fs::read_to_string(directory.join("Kryta.json"))
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(address) = value.get("link").and_then(serde_json::Value::as_str)
    {
        return address.to_string();
    }
    let arguments: Vec<String> = std::env::args().collect();
    let host_after = |flag: &str| {
        arguments
            .windows(2)
            .find(|pair| pair[0].eq_ignore_ascii_case(flag))
            .map(|pair| pair[1].clone())
    };
    let host = host_after("-portal")
        .or_else(|| host_after("-authsrv"))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    format!("{host}:{DEFAULT_LINK_PORT}")
}

/// Open the link and say hello.
fn open_link(address: &str, hello: &Message) -> Result<TcpStream, String> {
    let mut stream =
        TcpStream::connect(address).map_err(|error| format!("{address}: {error}"))?;
    let _ = stream.set_nodelay(true);
    client_link::write(&mut stream, hello).map_err(|error| format!("{address}: {error}"))?;
    Ok(stream)
}

fn run() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("The client's own file cannot be named: {error}"))?;
    let directory = executable
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    log::open(&directory);
    crash::install(OWN_MODULE.load(std::sync::atomic::Ordering::Acquire));
    log::line(format!(
        "Kryta.dll {} in process {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        executable.display()
    ));

    // 1. The client's own file.
    let file = std::fs::read(&executable)
        .map_err(|error| format!("The client's own file cannot be read: {error}"))?;
    // SAFETY: a null name asks for the handle of the process's own executable.
    let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize as u32;
    let scanner = Scanner::from_file(&file, base)
        .map_err(|error| format!("The client's own file is not a client: {error}"))?;
    drop(file);
    log::line(format!(
        "client build stamp {:#010x}, loaded at {base:#010x}",
        scanner.time_date_stamp()
    ));

    // 2. The server.
    let address = link_address(&directory);
    let hello = Message::Hello {
        protocol: PROTOCOL,
        pid: std::process::id(),
        time_date_stamp: scanner.time_date_stamp(),
    };
    let mut attempt = 0;
    let mut stream = loop {
        attempt += 1;
        match open_link(&address, &hello) {
            Ok(stream) => break stream,
            Err(error) if attempt < CONNECT_ATTEMPTS => {
                log::line(format!("link: {error} (try {attempt} of {CONNECT_ATTEMPTS})"));
                std::thread::sleep(CONNECT_PAUSE);
            }
            Err(error) => {
                return Err(format!("The Kryta server could not be reached at {error}."));
            }
        }
    };
    log::line(format!("link: connected to {address}"));

    // 3. The offsets.
    let documents = match client_link::read(&mut stream) {
        Ok(Message::Offsets { documents }) => documents,
        Ok(Message::Refused { reason }) => {
            return Err(format!("The Kryta server refused this client: {reason}"));
        }
        Ok(other) => return Err(format!("The Kryta server answered out of turn: {other:?}")),
        Err(error) => return Err(format!("The Kryta server closed the link: {error}")),
    };
    let mut patterns = Patterns::new();
    for document in &documents {
        patterns
            .load(document)
            .map_err(|error| format!("The server's offsets cannot be read: {error}"))?;
    }

    let memory = hooks::ProcessMemory;
    let mut found = Vec::new();
    let mut missing = Vec::new();
    let mut addresses = std::collections::HashMap::new();
    for name in patterns.resolver_names() {
        let result = patterns.resolve(&name, &scanner, &memory);
        match result.value {
            Some(value) => {
                log::line(format!("offset {name} = {value:#010x}"));
                addresses.insert(name.clone(), value);
                found.push(name);
            }
            None => {
                log::line(format!("offset {name} not found: {} {:?}", result.message, result.trace));
                missing.push((name, result.message));
            }
        }
    }

    // 4. What resolved, and the hooks.
    let report = Message::Resolved { found, missing: missing.clone() };
    client_link::write(&mut stream, &report)
        .map_err(|error| format!("The Kryta server closed the link: {error}"))?;

    let mut title_addresses = [0u32; 6];
    for (slot, name) in title_addresses.iter_mut().zip(title_hooks::RESOLVERS) {
        *slot = *addresses.get(name).ok_or_else(|| {
            format!(
                "This client build is not one the server's offsets fit: '{name}' was not found."
            )
        })?;
    }
    // SAFETY: the six addresses were resolved, in this process, from the offsets that describe them.
    unsafe { title_hooks::install(title_addresses) }
        .map_err(|error| format!("The title hooks could not be installed: {error}"))?;
    log::line("titles: hooks installed");

    // The server's maps (`docs/port/24-custom-maps.md`, "The delivery plan"): the hook answers from
    // the list the server sends, and the files are fetched into one cache for this Windows user.
    // A build the file offsets do not fit, or a user with no local application data folder, runs
    // without them, and the log says so.
    let served: &'static crate::map_cache::Served = Box::leak(Box::default());
    let cache = std::env::var_os("LOCALAPPDATA")
        .map(|local| crate::map_cache::Cache::new(crate::map_cache::folder(std::path::Path::new(&local))));
    let maps_hooked = match (
        addresses.get(file_hooks::RESOLVERS[0]),
        addresses.get(file_hooks::RESOLVERS[1]),
        &cache,
    ) {
        (Some(&open_archive), Some(&open_by_name), Some(cache)) => {
            // SAFETY: the two addresses were resolved, in this process, from the offsets that
            // describe them.
            match unsafe { file_hooks::install([open_archive, open_by_name], served) } {
                Ok(()) => {
                    log::line(format!("maps: hook installed; the cache is {}", cache.folder().display()));
                    true
                }
                Err(error) => {
                    log::line(format!("maps: not installed: {error}"));
                    false
                }
            }
        }
        (_, _, None) => {
            log::line("maps: not served: this user has no LOCALAPPDATA folder for the cache");
            false
        }
        _ => {
            log::line("maps: not served: the file offsets did not resolve in this build");
            false
        }
    };
    // The rows the server states for its maps (`docs/port/24-custom-maps.md`, "The map's row in
    // the client"). A build the offset does not fit runs without them: every map is then the
    // client's own row, and the log says so.
    let rows_hooked = match addresses.get(map_row_hooks::RESOLVER) {
        // SAFETY: the address was resolved, in this process, from the offset that describes it,
        // and `install` checks the function before it replaces it.
        Some(&lookup) => match unsafe { map_row_hooks::install(lookup) } {
            Ok(rows) => {
                log::line(format!("map rows: hook installed; the client's own table has {rows} rows"));
                true
            }
            Err(error) => {
                log::line(format!("map rows: not installed: {error}"));
                false
            }
        },
        None => {
            log::line("map rows: not answered: the row offset did not resolve in this build");
            false
        }
    };
    // The texts the server states for its maps (`docs/port/24-custom-maps.md`, step R4). A build
    // the offset does not fit shows the client's own text, and the log says so.
    if rows_hooked {
        match addresses.get(text_hooks::RESOLVER) {
            // SAFETY: the address was resolved, in this process, from the offset that describes
            // it, and `install` checks the function before it replaces it.
            Some(&decode) => match unsafe { text_hooks::install(decode) } {
                Ok(()) => log::line("map names: hook installed"),
                Err(error) => log::line(format!("map names: not installed: {error}")),
            },
            None => log::line("map names: not installed: the offset did not resolve in this build"),
        }
    }
    // A map id past the client's own maps is kept out of the client's record of fetched maps,
    // which has no slot for it. Without this a map of the server's own cannot be entered under
    // such an id, and the log says so.
    if rows_hooked {
        match addresses.get(map_row_hooks::STORE_RESOLVER) {
            // SAFETY: the address was resolved, in this process, from the offset that describes
            // it, the row hook is in, and `install_store` checks the function first.
            Some(&store) => match unsafe { map_row_hooks::install_store(store) } {
                Ok(()) => log::line("map rows: the record of fetched maps is hooked"),
                Err(error) => log::line(format!("map rows: the record of fetched maps is not hooked: {error}")),
            },
            None => log::line(
                "map rows: the record of fetched maps is not hooked: its offset did not resolve in this build",
            ),
        }
        // The same for the help guide's list of places entered.
        match addresses.get(map_row_hooks::GUIDE_RESOLVER) {
            // SAFETY: as above, for `install_guide`.
            Some(&guide) => match unsafe { map_row_hooks::install_guide(guide) } {
                Ok(()) => log::line("map rows: the help guide's places are hooked"),
                Err(error) => log::line(format!("map rows: the help guide's places are not hooked: {error}")),
            },
            None => log::line(
                "map rows: the help guide's places are not hooked: the offset did not resolve in this build",
            ),
        }
    }
    drop(scanner);
    // The files being fetched, by file id.
    let mut fetching: std::collections::HashMap<u32, crate::map_cache::Fetch> =
        std::collections::HashMap::new();
    // The folder of the flavor the connected server named, under the cache's root.
    let mut flavor_cache: Option<crate::map_cache::Cache> = None;
    // A load asks for its file on the link from the client's own thread.
    file_hooks::set_link(stream.try_clone().ok());

    // 5. What the server sends, for as long as the client runs.
    loop {
        match client_link::read(&mut stream) {
            Ok(Message::Titles { custom_tracks }) => {
                match title_hooks::set_catalog(&custom_tracks) {
                    Ok(count) => log::line(format!("titles: {count} custom title(s) received")),
                    Err(error) => log::line(format!("titles: the list was refused: {error}")),
                }
            }
            Ok(Message::MapRows { rows }) => {
                if rows_hooked {
                    let count = map_row_hooks::set_rows(&rows);
                    log::line(format!("map rows: {count} map(s) are answered with the server's row"));
                } else if !rows.is_empty() {
                    log::line(format!(
                        "map rows: the server states {} map(s), and this client shows its own rows for them",
                        rows.len()
                    ));
                }
            }
            Ok(Message::Maps { flavor, files }) => {
                fetching.clear();
                let Some(root) = cache.as_ref().filter(|_| maps_hooked) else {
                    continue;
                };
                // The server's flavor names its own folder of the cache. A server that runs no
                // world pack names none and lists nothing: nothing is served and no folder is
                // touched.
                let Some(flavor_folder) = flavor
                    .as_deref()
                    .and_then(|flavor| crate::map_cache::Cache::of_flavor(root.folder(), flavor))
                else {
                    served.set_list(&[], root);
                    flavor_cache = None;
                    log::line("maps: the server names no flavor and lists no file");
                    continue;
                };
                served.set_list(&files, &flavor_folder);
                flavor_cache = Some(flavor_folder.clone());
                let listed = files.len();
                log::line(format!(
                    "maps: the server lists {listed} file(s); its folder is {}",
                    flavor_folder.folder().display()
                ));
                // Every listed map is copied here, silently, one after another, so it is in the
                // cache before it is needed; a map a load needs before it has arrived is fetched on
                // that load, which waits for it (`file_hooks`). First the folder is made equal to
                // the list, so no stale map stays in it (owner, 2026-11-27).
                std::thread::spawn(move || {
                    let removed = flavor_folder.sweep(&files);
                    if removed > 0 {
                        log::line(format!(
                            "maps: {removed} file(s) the server no longer lists were removed"
                        ));
                    }
                    if listed > 0 {
                        let ready = served.fetch_all(file_hooks::PATIENCE, &file_hooks::ask);
                        log::line(format!(
                            "maps: {ready} of the {listed} listed file(s) are in the cache"
                        ));
                    }
                });
            }
            Ok(Message::MapPart { file_id, sha256, offset, total, data }) => {
                // A fetch begins with the first part of a file that was asked for.
                let fetch = match fetching.entry(file_id) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let Some(file) = served.fetching_file(file_id) else {
                            continue;
                        };
                        entry.insert(crate::map_cache::Fetch::new(file))
                    }
                };
                match fetch.part(&sha256, offset, total, &data) {
                    Ok(false) => served.part_arrived(),
                    Ok(true) => {
                        let Some(fetch) = fetching.remove(&file_id) else {
                            continue;
                        };
                        let stored = flavor_cache
                            .as_ref()
                            .ok_or_else(|| "no cache".to_string())
                            .and_then(|cache| cache.store(fetch.file(), fetch.bytes(), std::process::id()));
                        match stored {
                            Ok(path) => {
                                log::line(format!(
                                    "maps: file {file_id:#x} fetched, {total} byte(s), into {}",
                                    path.display()
                                ));
                                served.ready(fetch.file(), path);
                            }
                            Err(error) => {
                                log::line(format!("maps: file {file_id:#x} was not stored: {error}"));
                                served.failed(fetch.file());
                            }
                        }
                    }
                    Err(error) => {
                        log::line(format!("maps: {error}"));
                        if let Some(fetch) = fetching.remove(&file_id) {
                            served.failed(fetch.file());
                        }
                    }
                }
            }
            Ok(Message::Refused { reason }) => {
                log::line(format!("link: refused by the server: {reason}"));
            }
            Ok(other) => log::line(format!("link: a message this DLL does not take: {other:?}")),
            Err(error) => {
                log::line(format!("link: lost ({error}); opening it again"));
                // What was being fetched will not arrive on the lost link; the list the server
                // sends on the new one asks again.
                fetching.clear();
                file_hooks::set_link(None);
                served.link_lost();
                stream = loop {
                    std::thread::sleep(RECONNECT_PAUSE);
                    let Ok(mut stream) = open_link(&address, &hello) else {
                        continue;
                    };
                    // The server answers a hello with the offsets, which are already in force, then
                    // takes the report again and carries on.
                    if matches!(client_link::read(&mut stream), Ok(Message::Offsets { .. }))
                        && client_link::write(&mut stream, &report).is_ok()
                    {
                        break stream;
                    }
                };
                file_hooks::set_link(stream.try_clone().ok());
                log::line("link: open again");
            }
        }
    }
}
