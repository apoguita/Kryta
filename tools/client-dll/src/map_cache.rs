//! The maps a server hosts, as the client holds them: the list the server sent, and the cache.
//!
//! The server holds the maps and serves them; the client fetches what the server lists, and nobody
//! places a file by hand (owner, 2026-11-27; `docs/port/24-custom-maps.md`, "The delivery plan").
//! The list ([`client_link::Message::Maps`]) is the whole truth for a client of that server: a file
//! id on it is answered from the cache, in the archive's place, and a file id that is not on it goes
//! to the client's archive as it always did.
//!
//! # The cache
//!
//! One folder for every client a Windows user runs: `%LOCALAPPDATA%\Kryta\maps\`. It is not a
//! client's own folder, which would hold one copy for each client, and not a temporary folder,
//! which would be fetched again each time. Under it is one folder for each server flavor - the
//! build and setup a server runs, named by the server (`maps\Vanilla\`, `maps\fantasy_modded\`) -
//! and a flavor's folder is kept equal to its server's list: a map stays for as long as that
//! server lists it and is removed when it does not, so nothing in it is stale and nothing expires
//! by age (owner, 2026-11-27). A file is stored under its checksum, `<sha256>.ffna`, so:
//!
//! - a map two clients need is one file;
//! - a map the server has changed has another checksum, is another file, and is fetched;
//! - a file whose bytes do not give its name's checksum is not used, and is fetched again.
//!
//! A fetched file is written under a name of this process's own and renamed when it is whole, so
//! two clients that fetch one file at once cannot spoil it for each other.
//!
//! # What the client is answered
//!
//! Only the finished map is served: stream 1, opened for reading (`OPEN_READ`), which is how the map
//! loader asks for it (build 39167, `0x705955`). Any other open of a listed file id is answered "not
//! found", so the client compiles nothing and writes nothing into its archive for it.
//!
//! # When a file is fetched
//!
//! On the map load that needs it, as the game streams its own content (owner, 2026-11-27). The list
//! alone fetches nothing. When the client opens a listed file the cache does not hold, the server
//! is asked for it and the load is held until the file is whole, checked and in the cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use client_link::MapFile;

/// The stream that holds the finished map: the one the map loader reads.
pub const FINISHED_STREAM: u32 = 1;

/// The flags the map loader opens a map with: read. An open with any other flags is not a load.
pub const OPEN_READ: u32 = 1;

/// The longest path the client's open by name takes, in UTF-16 units with the terminator (build
/// 39167, `0x471051`).
pub const NAME_UNITS: usize = 0x104;

/// The cache folder under a user's local application data.
pub fn folder(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Kryta").join("maps")
}

/// The folder every client of one Windows user shares.
#[derive(Debug, Clone)]
pub struct Cache {
    folder: PathBuf,
}

impl Cache {
    /// The cache in `folder`. The folder is made when the first file is stored.
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// The folder of one server flavor under the cache's root: `maps\Vanilla\`. `None` when the
    /// name cannot be a folder's.
    pub fn of_flavor(root: &Path, flavor: &str) -> Option<Self> {
        client_link::is_flavor_name(flavor).then(|| Self::new(root.join(flavor)))
    }

    /// Where the cache is.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Keep the folder equal to the server's list: remove every map file in it that the list does
    /// not name, so no stale map stays (owner, 2026-11-27). A flavor's folder belongs to that
    /// flavor's server, which is why the list of the server connected to decides. Only files this
    /// cache names are touched: `<sha256>.ffna`, and the `<sha256>.<pid>.part` of a fetch that
    /// never finished. A file another client has open is left for the next time. Returns how many
    /// were removed.
    pub fn sweep(&self, list: &[MapFile]) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.folder) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut pieces = name.split('.');
            let sha256 = pieces.next().unwrap_or_default();
            let rest: Vec<&str> = pieces.collect();
            let ours = client_link::is_sha256_hex(sha256)
                && match rest.as_slice() {
                    ["ffna"] => true,
                    [pid, "part"] => !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()),
                    _ => false,
                };
            if ours
                && !list.iter().any(|file| file.sha256 == sha256)
                && std::fs::remove_file(entry.path()).is_ok()
            {
                removed += 1;
            }
        }
        removed
    }

    /// Where a file of this checksum is, or would be.
    pub fn path_of(&self, sha256: &str) -> PathBuf {
        self.folder.join(format!("{sha256}.ffna"))
    }

    /// The cached file, when the cache holds exactly the bytes the list names: the size and the
    /// checksum are both taken from the file itself.
    pub fn held(&self, file: &MapFile) -> Option<PathBuf> {
        if !client_link::is_sha256_hex(&file.sha256) {
            return None;
        }
        let path = self.path_of(&file.sha256);
        let bytes = std::fs::read(&path).ok()?;
        (bytes.len() == file.size as usize && client_link::sha256_hex(&bytes) == file.sha256)
            .then_some(path)
    }

    /// Store a file that arrived whole. `own` makes the temporary name this process's own (its
    /// process id). The bytes must give the checksum the list named.
    pub fn store(&self, file: &MapFile, bytes: &[u8], own: u32) -> Result<PathBuf, String> {
        if !client_link::is_sha256_hex(&file.sha256) {
            return Err(format!("'{}' is not a checksum", file.sha256));
        }
        if bytes.len() != file.size as usize || client_link::sha256_hex(bytes) != file.sha256 {
            return Err(format!(
                "the {} byte(s) that arrived for file {:#x} are not the file the server listed",
                bytes.len(),
                file.file_id
            ));
        }
        std::fs::create_dir_all(&self.folder)
            .map_err(|error| format!("{}: {error}", self.folder.display()))?;
        let path = self.path_of(&file.sha256);
        let temporary = self.folder.join(format!("{}.{own}.part", file.sha256));
        std::fs::write(&temporary, bytes)
            .map_err(|error| format!("{}: {error}", temporary.display()))?;
        if let Err(error) = std::fs::rename(&temporary, &path) {
            // Another client stored the same file first, or has it open: its copy is the same
            // bytes if it checks, and this one is not needed.
            let _ = std::fs::remove_file(&temporary);
            if self.held(file).is_none() {
                return Err(format!("{}: {error}", path.display()));
            }
        }
        Ok(path)
    }
}

/// A file that is arriving in parts.
#[derive(Debug)]
pub struct Fetch {
    file: MapFile,
    bytes: Vec<u8>,
}

impl Fetch {
    /// Begin a file the list names.
    pub fn new(file: MapFile) -> Self {
        Self { file, bytes: Vec::new() }
    }

    /// The file this is.
    pub fn file(&self) -> &MapFile {
        &self.file
    }

    /// Take one part. `Ok(true)` when the file is whole. A part that is not the next one of this
    /// file is an error, and the fetch is then of no use.
    pub fn part(&mut self, sha256: &str, offset: u32, total: u32, data: &[u8]) -> Result<bool, String> {
        if sha256 != self.file.sha256 || total != self.file.size {
            return Err(format!("a part of another file arrived for {:#x}", self.file.file_id));
        }
        if offset as usize != self.bytes.len() || self.bytes.len() + data.len() > total as usize {
            return Err(format!(
                "a part of file {:#x} arrived out of order: offset {offset}, {} byte(s) held",
                self.file.file_id,
                self.bytes.len()
            ));
        }
        self.bytes.extend_from_slice(data);
        Ok(self.bytes.len() == total as usize)
    }

    /// The bytes that arrived.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Where one listed file stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// On the server's list; no load has needed it yet, so nothing was looked up or asked.
    Listed,
    /// Asked of the server and not whole yet.
    Fetching,
    /// In the cache.
    Ready(PathBuf),
    /// It could not be fetched or stored.
    Failed,
}

/// How the client's open of an archive file is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The server lists no such file: the client's own archive answers.
    Archive,
    /// The cached file is opened in the archive file's place.
    Cached(PathBuf),
    /// The file is listed and this open is not served: the client is told it is not found.
    NotFound,
}

/// The server's list, and where each of its files stands. Shared by the link's thread, which
/// fetches, and the client's threads, which open.
#[derive(Debug, Default)]
pub struct Served {
    held: Mutex<Held>,
    changed: Condvar,
}

/// What [`Served`] holds under its lock.
#[derive(Debug, Default)]
struct Held {
    files: HashMap<u32, (MapFile, State)>,
    /// The cache the listed files are looked up in; none until a list arrives.
    cache: Option<Cache>,
    /// Counts the parts that arrived, so a load that waits can tell a fetch that moves from one
    /// that is stuck.
    parts: u64,
    /// Counts the lists taken.
    lists: u64,
}

impl Served {
    /// A client that has been sent no list: every open goes to the archive.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Take the server's list in place of the one held. Nothing is fetched and nothing is looked
    /// up here: a file is fetched when a map load needs it ([`Served::answer`]). A file id that is
    /// no longer listed is no longer served.
    pub fn set_list(&self, list: &[MapFile], cache: &Cache) {
        let mut held = self.lock();
        held.files = list
            .iter()
            .map(|file| (file.file_id, (file.clone(), State::Listed)))
            .collect();
        held.cache = Some(cache.clone());
        held.lists += 1;
        drop(held);
        self.changed.notify_all();
    }

    /// How many lists have been taken. The background fetch of one list stops when another came.
    pub fn lists(&self) -> u64 {
        self.lock().lists
    }

    /// Fetch every listed file the cache lacks, one after another, as a load would: the maps no
    /// load has needed yet arrive silently in the background (owner, 2026-11-27). A load that
    /// comes meanwhile asks for its own file at once and waits only for that one. Returns how many
    /// files are in the cache at the end. Stops when the server sends another list.
    pub fn fetch_all(&self, patience: Duration, ask: &dyn Fn(&MapFile) -> bool) -> usize {
        let lists = self.lists();
        let mut ready = 0;
        for file_id in self.file_ids() {
            if self.lists() != lists {
                break;
            }
            if matches!(
                self.answer(file_id, FINISHED_STREAM, OPEN_READ, patience, ask),
                Answer::Cached(_)
            ) {
                ready += 1;
            }
        }
        ready
    }

    /// The listed file a load asked the server for, while it is still arriving: the link's thread
    /// begins its fetch from this when the first part comes.
    pub fn fetching_file(&self, file_id: u32) -> Option<MapFile> {
        match self.lock().files.get(&file_id) {
            Some((file, State::Fetching)) => Some(file.clone()),
            _ => None,
        }
    }

    /// A part of a file arrived: a load that waits goes on waiting.
    pub fn part_arrived(&self) {
        self.lock().parts += 1;
        self.changed.notify_all();
    }

    /// A fetched file is in the cache.
    pub fn ready(&self, file: &MapFile, path: PathBuf) {
        self.settle(file, State::Ready(path));
    }

    /// A file could not be fetched or stored.
    pub fn failed(&self, file: &MapFile) {
        self.settle(file, State::Failed);
    }

    fn settle(&self, file: &MapFile, state: State) {
        let mut held = self.lock();
        // Only the file the list still names: a list that came since may name another checksum.
        if let Some(listed) = held.files.get_mut(&file.file_id)
            && listed.0.sha256 == file.sha256
        {
            listed.1 = state;
        }
        drop(held);
        self.changed.notify_all();
    }

    /// The link dropped: what was being fetched will not arrive on it, and the loads that wait for
    /// it are told so. The list the server sends on the new link starts again.
    pub fn link_lost(&self) {
        let mut held = self.lock();
        for listed in held.files.values_mut() {
            if listed.1 == State::Fetching {
                listed.1 = State::Failed;
            }
        }
        drop(held);
        self.changed.notify_all();
    }

    /// The file ids the list names, for a log line.
    pub fn file_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.lock().files.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// The answer to the client opening `stream` of `file_id` with `flags`.
    ///
    /// This is where a map is fetched: on the load that needs it (owner, 2026-11-27). A listed file
    /// the cache holds is answered at once. One it does not hold is asked of the server with `ask`,
    /// and the load is held here until the file is whole and in the cache. `ask` returns whether
    /// the request was sent. The wait ends when the file is ready, when it fails, or when no part
    /// has arrived for `patience`.
    pub fn answer(
        &self,
        file_id: u32,
        stream: u32,
        flags: u32,
        patience: Duration,
        ask: &dyn Fn(&MapFile) -> bool,
    ) -> Answer {
        let mut held = self.lock();
        if !held.files.contains_key(&file_id) {
            return Answer::Archive;
        }
        if stream != FINISHED_STREAM || flags != OPEN_READ {
            return Answer::NotFound;
        }
        // A file that failed before is tried again by a new load.
        if let Some(listed) = held.files.get_mut(&file_id)
            && listed.1 == State::Failed
        {
            listed.1 = State::Listed;
        }
        let mut until = Instant::now() + patience;
        let mut parts = held.parts;
        loop {
            let to_look_up = match held.files.get(&file_id) {
                // The list changed while this open waited, and no longer names the file.
                None => return Answer::Archive,
                Some((_, State::Ready(path))) => return Answer::Cached(path.clone()),
                Some((_, State::Failed)) => return Answer::NotFound,
                Some((file, State::Listed)) => Some(file.clone()),
                Some((_, State::Fetching)) => None,
            };
            if let Some(file) = to_look_up {
                // The cached file is read and checked without the lock, so the client's other
                // opens are not held up by it.
                let cache = held.cache.clone();
                drop(held);
                let found = cache.and_then(|cache| cache.held(&file));
                held = self.lock();
                let Some((now, state)) = held.files.get_mut(&file_id) else {
                    return Answer::Archive;
                };
                if *state != State::Listed || now.sha256 != file.sha256 {
                    // Another load, or another list, got here first: look again.
                    continue;
                }
                if let Some(path) = found {
                    *state = State::Ready(path.clone());
                    return Answer::Cached(path);
                }
                // Marked first, so no other load asks for it too; asked without the lock, as the
                // request goes out on the link.
                *state = State::Fetching;
                drop(held);
                let sent = ask(&file);
                held = self.lock();
                if !sent {
                    if let Some((now, state)) = held.files.get_mut(&file_id)
                        && *state == State::Fetching
                        && now.sha256 == file.sha256
                    {
                        *state = State::Failed;
                    }
                    drop(held);
                    self.changed.notify_all();
                    return Answer::NotFound;
                }
                // The file may have arrived, or the list changed, meanwhile: look again.
                continue;
            }
            if held.parts != parts {
                parts = held.parts;
                until = Instant::now() + patience;
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Answer::NotFound;
            }
            held = self
                .changed
                .wait_timeout(held, left)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }
}

/// A cached file's path as the client's open by name takes it: UTF-16, zero-terminated, and no
/// longer than the client copies.
pub fn name_of(path: &Path) -> Option<Vec<u16>> {
    let name: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
    (name.len() <= NAME_UNITS).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> Cache {
        let folder =
            std::env::temp_dir().join(format!("kryta-map-cache-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        Cache::new(folder)
    }

    fn listed(file_id: u32, bytes: &[u8]) -> MapFile {
        MapFile {
            file_id,
            size: bytes.len() as u32,
            sha256: client_link::sha256_hex(bytes),
        }
    }

    const NOW: Duration = Duration::ZERO;

    #[test]
    fn the_cache_is_one_folder_under_the_users_local_application_data() {
        assert_eq!(
            folder(Path::new(r"C:\Users\A\AppData\Local")),
            Path::new(r"C:\Users\A\AppData\Local").join("Kryta").join("maps")
        );
    }

    /// A file is stored under its checksum, and held only while its bytes are the ones the list
    /// names: a changed file, a cut file and another size are all "not held".
    #[test]
    fn a_file_is_held_only_while_its_bytes_give_its_checksum() {
        let cache = scratch("held");
        let file = listed(0x5F100, b"a finished map");
        assert_eq!(cache.held(&file), None, "nothing is cached yet");

        let path = cache.store(&file, b"a finished map", 1).expect("stored");
        assert_eq!(path, cache.folder().join(format!("{}.ffna", file.sha256)));
        assert_eq!(cache.held(&file), Some(path.clone()));
        assert_eq!(
            std::fs::read_dir(cache.folder()).unwrap().count(),
            1,
            "the temporary file is gone"
        );

        // A second client stores the same file: still one file, still held.
        assert_eq!(cache.store(&file, b"a finished map", 2).expect("stored again"), path);
        assert_eq!(std::fs::read_dir(cache.folder()).unwrap().count(), 1);

        std::fs::write(&path, b"a finished mAp").unwrap();
        assert_eq!(cache.held(&file), None, "bytes that do not give the checksum");
        std::fs::write(&path, b"a finished").unwrap();
        assert_eq!(cache.held(&file), None, "a cut file");

        assert!(cache.store(&file, b"other bytes!!!", 1).is_err(), "not the file the list named");
        let _ = std::fs::remove_dir_all(cache.folder());
    }

    /// A flavor's folder is kept equal to its server's list: a map the server no longer lists, an
    /// old version of a changed map and a fetch that never finished are removed, and nothing that
    /// is not a file of this cache is touched.
    #[test]
    fn a_flavors_folder_holds_only_what_its_server_lists() {
        let root = scratch("flavors");
        let vanilla = Cache::of_flavor(root.folder(), "Vanilla").expect("a plain name");
        let modded = Cache::of_flavor(root.folder(), "fantasy_modded").expect("a plain name");
        assert_eq!(vanilla.folder(), root.folder().join("Vanilla"));
        assert!(Cache::of_flavor(root.folder(), "..").is_none(), "not a folder's name");

        let kept = listed(1, b"kept");
        let old = listed(2, b"the old version");
        let dropped = listed(3, b"dropped");
        for (file, bytes) in [(&kept, &b"kept"[..]), (&old, b"the old version"), (&dropped, b"dropped")] {
            vanilla.store(file, bytes, 1).unwrap();
        }
        let elsewhere = modded.store(&dropped, b"dropped", 1).unwrap();
        let unfinished = vanilla.folder().join(format!("{}.77.part", old.sha256));
        std::fs::write(&unfinished, b"the old").unwrap();
        let foreign = vanilla.folder().join("notes.txt");
        std::fs::write(&foreign, b"not ours").unwrap();

        let new = listed(2, b"the new version");
        assert_eq!(vanilla.sweep(&[kept.clone(), new]), 3, "the old version, its part, the dropped map");
        assert!(vanilla.held(&kept).is_some());
        assert!(vanilla.held(&old).is_none() && vanilla.held(&dropped).is_none());
        assert!(!unfinished.exists());
        assert!(foreign.exists(), "a file this cache did not name is not touched");
        assert!(elsewhere.exists(), "another flavor's folder is its own server's");
        let _ = std::fs::remove_dir_all(root.folder());
    }

    #[test]
    fn a_fetch_takes_its_parts_in_order_and_knows_when_it_is_whole() {
        let file = listed(7, b"abcdef");
        let mut fetch = Fetch::new(file.clone());
        assert_eq!(fetch.part(&file.sha256, 0, 6, b"abc"), Ok(false));
        assert!(fetch.part(&file.sha256, 0, 6, b"abc").is_err(), "the same part again");
        let mut fetch = Fetch::new(file.clone());
        assert_eq!(fetch.part(&file.sha256, 0, 6, b"abc"), Ok(false));
        assert_eq!(fetch.part(&file.sha256, 3, 6, b"def"), Ok(true));
        assert_eq!(fetch.bytes(), b"abcdef");

        let mut fetch = Fetch::new(file.clone());
        assert!(fetch.part(&client_link::sha256_hex(b"x"), 0, 6, b"abc").is_err(), "another file");
        assert!(fetch.part(&file.sha256, 0, 6, b"abcdefg").is_err(), "more than the file");

        let empty = listed(8, b"");
        assert_eq!(Fetch::new(empty.clone()).part(&empty.sha256, 0, 0, b""), Ok(true));
    }

    /// The list decides: a file id on it is served from the cache, one that is not goes to the
    /// archive, and only the map loader's read of the finished stream is served.
    #[test]
    fn the_servers_list_decides_what_is_answered() {
        let cache = scratch("list");
        let held = listed(0x5F100, b"held");
        let path = cache.store(&held, b"held", 1).unwrap();
        let missing = listed(0x5F101, b"missing");

        // What was asked of the server, in order.
        let asked = Mutex::new(Vec::new());
        let ask = |file: &MapFile| {
            asked.lock().unwrap().push(file.clone());
            true
        };
        let asked_now = || std::mem::take(&mut *asked.lock().unwrap());

        let served = Served::new();
        assert_eq!(served.answer(0x5F100, 1, OPEN_READ, NOW, &ask), Answer::Archive, "no list yet");

        served.set_list(&[held.clone(), missing.clone()], &cache);
        assert!(asked_now().is_empty(), "the list alone fetches nothing");
        assert_eq!(served.file_ids(), [0x5F100, 0x5F101]);
        assert_eq!(served.answer(0x5F100, 1, OPEN_READ, NOW, &ask), Answer::Cached(path));
        assert_eq!(served.answer(0x1B97D, 1, OPEN_READ, NOW, &ask), Answer::Archive, "a map of the game's");
        assert_eq!(served.answer(0x5F100, 0, OPEN_READ, NOW, &ask), Answer::NotFound, "no compile");
        assert_eq!(served.answer(0x5F100, 1, 2, NOW, &ask), Answer::NotFound, "no write");
        assert!(asked_now().is_empty(), "nothing the cache holds, or no load reads, is asked for");

        // The load of a file the cache lacks asks for it, and waits no longer than its patience.
        assert_eq!(served.answer(0x5F101, 1, OPEN_READ, NOW, &ask), Answer::NotFound, "not arrived in time");
        assert_eq!(asked_now(), std::slice::from_ref(&missing));
        assert_eq!(served.fetching_file(0x5F101), Some(missing.clone()));
        assert_eq!(served.answer(0x5F101, 1, OPEN_READ, NOW, &ask), Answer::NotFound);
        assert!(asked_now().is_empty(), "a file that is arriving is not asked for twice");

        // The server changes a map: another checksum, so the load asks for it.
        let changed = listed(0x5F100, b"held, and changed");
        served.set_list(std::slice::from_ref(&changed), &cache);
        assert_eq!(served.answer(0x5F101, 1, OPEN_READ, NOW, &ask), Answer::Archive, "no longer listed");
        assert_eq!(served.answer(0x5F100, 1, OPEN_READ, NOW, &ask), Answer::NotFound);
        assert_eq!(asked_now(), std::slice::from_ref(&changed));

        // A link that is down cannot be asked: the load is told at once.
        served.set_list(std::slice::from_ref(&missing), &cache);
        let down = |_: &MapFile| false;
        assert_eq!(
            served.answer(0x5F101, 1, OPEN_READ, Duration::from_secs(30), &down),
            Answer::NotFound
        );

        // A server that hosts nothing: the empty list serves nothing.
        served.set_list(&[], &cache);
        assert_eq!(served.answer(0x5F100, 1, OPEN_READ, NOW, &ask), Answer::Archive);
        let _ = std::fs::remove_dir_all(cache.folder());
    }

    /// The load of a map the cache lacks asks the server for it and is held until the file is
    /// whole and in the cache.
    #[test]
    fn a_load_asks_for_its_file_and_waits_until_it_is_whole() {
        let cache = scratch("wait");
        let file = listed(0x5F100, b"arriving");
        let served = std::sync::Arc::new(Served::new());
        served.set_list(std::slice::from_ref(&file), &cache);
        assert_eq!(served.fetching_file(0x5F100), None, "nothing is asked for before a load");

        let opener = {
            let served = std::sync::Arc::clone(&served);
            std::thread::spawn(move || {
                served.answer(0x5F100, 1, OPEN_READ, Duration::from_secs(30), &|_| true)
            })
        };
        // The link's thread sees the request's first part, and the parts keep the load waiting.
        while served.fetching_file(0x5F100).is_none() {
            std::thread::sleep(Duration::from_millis(5));
        }
        served.part_arrived();
        assert!(!opener.is_finished(), "the load is held while the file arrives");
        let path = cache.store(&file, b"arriving", 1).unwrap();
        served.ready(&file, path.clone());
        assert_eq!(opener.join().unwrap(), Answer::Cached(path));

        // A file settled for a checksum the list no longer names changes nothing.
        let newer = listed(0x5F100, b"newer");
        served.set_list(std::slice::from_ref(&newer), &cache);
        served.ready(&file, cache.path_of(&file.sha256));
        assert_eq!(served.answer(0x5F100, 1, OPEN_READ, NOW, &|_| true), Answer::NotFound, "still fetching");

        // A dropped link ends the wait of a load that is held.
        let opener = {
            let served = std::sync::Arc::clone(&served);
            std::thread::spawn(move || {
                served.answer(0x5F100, 1, OPEN_READ, Duration::from_secs(30), &|_| true)
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        served.link_lost();
        assert_eq!(opener.join().unwrap(), Answer::NotFound, "a dropped link ends the wait");
        let _ = std::fs::remove_dir_all(cache.folder());
    }

    /// Every listed map the cache lacks is fetched in the background, one after another, and one
    /// the cache holds is not asked for.
    #[test]
    fn the_other_listed_maps_are_fetched_in_the_background() {
        let cache = scratch("background");
        let held = listed(1, b"held");
        cache.store(&held, b"held", 1).unwrap();
        let contents: [(u32, &[u8]); 2] = [(2, b"second"), (3, b"third")];
        let lacking: Vec<MapFile> = contents.iter().map(|(id, bytes)| listed(*id, bytes)).collect();
        let served = Served::new();
        served.set_list(&[held, lacking[0].clone(), lacking[1].clone()], &cache);

        // The server's part: each file asked for is stored and reported, as the link's thread does.
        let asked = Mutex::new(Vec::new());
        let ask = |file: &MapFile| {
            asked.lock().unwrap().push(file.file_id);
            let bytes = contents.iter().find(|(id, _)| *id == file.file_id).unwrap().1;
            served.ready(file, cache.store(file, bytes, 1).unwrap());
            true
        };
        assert_eq!(served.fetch_all(Duration::from_secs(30), &ask), 3);
        assert_eq!(*asked.lock().unwrap(), [2, 3]);
        assert_eq!(std::fs::read_dir(cache.folder()).unwrap().count(), 3);
        let _ = std::fs::remove_dir_all(cache.folder());
    }

    #[test]
    fn a_cached_path_fits_the_clients_open_by_name() {
        let cache = Cache::new(folder(Path::new(r"C:\Users\Somebody\AppData\Local")));
        let name = name_of(&cache.path_of(&client_link::sha256_hex(b"x"))).expect("it fits");
        assert_eq!(name.last(), Some(&0));
        assert!(name_of(Path::new(&"x".repeat(NAME_UNITS))).is_none(), "longer than the client copies");
    }
}
