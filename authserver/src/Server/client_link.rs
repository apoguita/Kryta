//! The server end of the client link: the DLL's own connection to the server.
//!
//! The patched client loads `Kryta.dll`, and the DLL opens this connection when the client starts,
//! before any account logs in (`docs/port/22-client-platform.md`). Over it the server provides the
//! offsets the DLL finds the client's functions by, and pushes the data the DLL's features show -
//! the custom title text first.
//!
//! The reference has no equivalent: no AzerothCore service talks to code inside the client. The
//! **arrangement** is the portal's (`game/src/Portal/serve.rs`): a session the socket layer runs,
//! served by this program because the client knows this program's address from its command line
//! before it knows any other. The **messages** are Kryta's own and are defined once, for both ends,
//! in `tools/client-link`.
//!
//! What a connection is, in order: the client's hello (its process id and its build), the offset
//! documents, the client's report of what it found, the custom titles, and then an open connection
//! the server may send on again.

use std::path::Path;
use std::sync::Arc;

use client_link::{Decoder, Message, PROTOCOL};
use common::logging::{Log, LogLevel};
use serde_json::Value;
use shared::network::socket::{Session, SessionFuture, SessionIo};

/// What every connection is served from, read once at start-up.
pub struct ClientLink {
    /// The offset documents, in file-name order.
    offsets: Vec<Value>,
    /// The custom title text, as the world pack's `custom_tracks`.
    custom_tracks: Vec<Value>,
    /// The map files this server hosts for its clients (`docs/port/24-custom-maps.md`, "The
    /// delivery plan").
    maps: Vec<HostedMap>,
    /// What the world pack states of its maps' rows in the client
    /// (`docs/port/24-custom-maps.md`, "The map's row in the client").
    map_rows: Vec<client_link::MapRow>,
    /// The server's flavor, its world pack's name; none when it runs no pack.
    flavor: Option<String>,
    /// How long the server waits before each part of a map file it sends
    /// (`ClientLink.MapPartPauseMs`). Zero in use; a test of a slow link sets it.
    map_part_pause: std::time::Duration,
}

/// One map file the server hosts: what the list says of it, and its bytes.
struct HostedMap {
    file: client_link::MapFile,
    bytes: Arc<Vec<u8>>,
}

impl ClientLink {
    /// Read the offset documents under `<directory>/offsets` and take the custom titles.
    ///
    /// Every `*.json` file there is one document in Reforged's offset format. A directory that is
    /// missing or empty, and a file that is not JSON or breaks the format, stops the program: the
    /// documents are checked here with the DLL's own reader, so a file a client could not use is
    /// found when the server starts and not when a client does (rule 7).
    pub fn load(directory: &Path, custom_tracks: Vec<Value>) -> Result<Self, String> {
        let offsets_directory = directory.join("offsets");
        let mut files: Vec<_> = std::fs::read_dir(&offsets_directory)
            .map_err(|error| format!("{}: {error}", offsets_directory.display()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(format!(
                "{}: no offset file; a client's DLL would have nothing to find",
                offsets_directory.display()
            ));
        }

        let mut checked = kryta_client::patterns::Patterns::new();
        let mut offsets = Vec::with_capacity(files.len());
        for file in &files {
            let text = std::fs::read_to_string(file)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            let document: Value = serde_json::from_str(&text)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            checked
                .load(&document)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            offsets.push(document);
        }
        Ok(Self {
            offsets,
            custom_tracks,
            maps: Vec::new(),
            map_rows: Vec::new(),
            flavor: None,
            map_part_pause: std::time::Duration::ZERO,
        })
    }

    /// Take what a world pack states of its maps' rows in the client.
    ///
    /// The client holds a row for each map id - its name, kind, region and world-map place - in a
    /// table compiled into it, which cannot be edited; a map's record states its row in
    /// `client_row`, and `Kryta.dll` answers the client with it (owner, 2026-11-27: "that is why
    /// we have the json config files"). A field name the row does not have stops the program, as
    /// a broken file does (rule 7). Returns how many maps state a row.
    pub fn host_map_rows(&mut self, rows: &[game::maps::pack::ClientRow]) -> Result<usize, String> {
        self.map_rows.clear();
        for row in rows {
            let mut fields = Vec::with_capacity(row.fields.len());
            for (name, value) in &row.fields {
                let field = client_link::MAP_ROW_FIELDS
                    .iter()
                    .position(|known| known == name)
                    .ok_or_else(|| {
                        format!(
                            "map {}: client_row.{name} is not a field of the client's map row; the fields are {}",
                            row.map_id,
                            client_link::MAP_ROW_FIELDS.join(", ")
                        )
                    })?;
                fields.push((field, *value));
            }
            fields.sort_unstable();
            // A name or a description of the pack's own stands in place of the id of one of the
            // client's texts, so a row that states both is a broken file.
            for (key, text, id) in [
                ("name", &row.name, client_link::MAP_ROW_NAME_ID),
                ("description", &row.description, client_link::MAP_ROW_DESCRIPTION_ID),
            ] {
                let Some(text) = text else { continue };
                if text.is_empty() || text.contains('\0') {
                    return Err(format!("map {}: client_row.{key} is empty", row.map_id));
                }
                if fields.iter().any(|(field, _)| *field == id) {
                    return Err(format!(
                        "map {}: client_row states both {key} and {}; a map has one or the other",
                        row.map_id,
                        client_link::MAP_ROW_FIELDS[id]
                    ));
                }
            }
            self.map_rows.push(client_link::MapRow {
                map_id: row.map_id,
                fields,
                name: row.name.clone(),
                description: row.description.clone(),
            });
        }
        Ok(self.map_rows.len())
    }

    /// How many maps' rows are stated.
    pub fn map_rows(&self) -> usize {
        self.map_rows.len()
    }

    /// Send map files slowly: wait this long before each part. It stands in for a slow link or a
    /// very large map, neither of which a test machine has, so that a map load that has to wait
    /// for its file can be tried with a real client (owner, 2026-11-27).
    pub fn set_map_part_pause(&mut self, pause: std::time::Duration) {
        self.map_part_pause = pause;
    }

    /// Take the finished map files of a world pack as the maps this server hosts.
    ///
    /// The server holds the maps and serves them (owner, 2026-11-27): each finished file
    /// (`<file id>.ffna` in a map's folder, `game/src/Maps/Pack.rs`) is read here, once, checked to
    /// be a map, and given its checksum. A file that is not a map stops the program, as it stops the
    /// world program (`worldserver/src/main.rs`, `load_map_files`). A map with a stripped file and
    /// no finished one is not served: a client is only ever given a finished map. Returns how many
    /// files are hosted.
    ///
    /// `flavor` names the build and setup this server runs: its world pack's name. A client keeps
    /// one folder of maps for each flavor and keeps it equal to this server's list, so no stale
    /// map stays in it (owner, 2026-11-27). The name becomes a folder on the client's machine, so a
    /// pack whose name is not a plain one stops the program.
    pub fn host_maps(
        &mut self,
        flavor: &str,
        files: &[game::maps::pack::MapFiles],
    ) -> Result<usize, String> {
        if !client_link::is_flavor_name(flavor) {
            return Err(format!(
                "the world pack's name '{flavor}' cannot name its clients' map folder: use 1 to 64 letters, digits, '_' and '-'"
            ));
        }
        self.flavor = Some(flavor.to_string());
        for files in files {
            let Some(path) = &files.finished else {
                continue;
            };
            let bytes = game::maps::pack::read_map_file(path, files.map_id)?;
            let size = u32::try_from(bytes.len())
                .map_err(|_| format!("{} is too large to serve", path.display()))?;
            // Two maps may name one file id, as two of the game's maps share a file; it is one file.
            if self.maps.iter().any(|held| held.file.file_id == files.file_id) {
                continue;
            }
            self.maps.push(HostedMap {
                file: client_link::MapFile {
                    file_id: files.file_id,
                    size,
                    sha256: client_link::sha256_hex(&bytes),
                },
                bytes: Arc::new(bytes),
            });
        }
        Ok(self.maps.len())
    }

    /// How many map files are hosted.
    pub fn hosted_maps(&self) -> usize {
        self.maps.len()
    }

    /// The list a client is sent.
    fn map_list(&self) -> Message {
        Message::Maps {
            flavor: self.flavor.clone(),
            files: self.maps.iter().map(|held| held.file.clone()).collect(),
        }
    }

    /// How many offset documents are served.
    pub fn offset_documents(&self) -> usize {
        self.offsets.len()
    }

    /// How many custom titles are served.
    pub fn custom_titles(&self) -> usize {
        self.custom_tracks.len()
    }
}

/// One client's connection.
pub struct ClientLinkSession {
    link: Arc<ClientLink>,
    log: Option<Log>,
}

impl ClientLinkSession {
    /// A session served from this link.
    pub fn new(link: Arc<ClientLink>, log: Option<Log>) -> Self {
        Self { link, log }
    }

    fn say(&self, level: LogLevel, text: impl Into<String>) {
        if let Some(log) = self.log.as_ref() {
            log.out(level, "clientlink", text);
        }
    }
}

/// The next message of a connection, or why there is none: `Ok(None)` is the peer closing.
async fn next_message(io: &mut SessionIo, decoder: &mut Decoder) -> Result<Option<Message>, String> {
    let mut window = [0u8; 4096];
    loop {
        if let Some(message) = decoder.next_message().map_err(|error| error.to_string())? {
            return Ok(Some(message));
        }
        match io.read(&mut window).await {
            Ok(0) => return Ok(None),
            Ok(count) => decoder.push(&window[..count]),
            Err(error) => return Err(error.to_string()),
        }
    }
}

impl Session for ClientLinkSession {
    fn run(self: Box<Self>, mut io: SessionIo) -> SessionFuture {
        Box::pin(async move {
            let peer = io
                .peer()
                .map(|address| address.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let mut decoder = Decoder::new();

            // 1. The hello. The process id, with the address, names this client from here on.
            let (pid, stamp) = match next_message(&mut io, &mut decoder).await {
                Ok(Some(Message::Hello {
                    protocol,
                    pid,
                    time_date_stamp,
                })) if protocol == PROTOCOL => (pid, time_date_stamp),
                Ok(Some(Message::Hello { protocol, .. })) => {
                    self.say(
                        LogLevel::Warn,
                        format!("{peer}: speaks link protocol {protocol}, this server {PROTOCOL}"),
                    );
                    let refusal = Message::Refused {
                        reason: format!(
                            "this client's Kryta.dll speaks link protocol {protocol} and the server speaks {PROTOCOL}"
                        ),
                    };
                    let _ = io.write_all(&refusal.encode()).await;
                    let _ = io.close().await;
                    return;
                }
                Ok(Some(other)) => {
                    self.say(LogLevel::Warn, format!("{peer}: began with {other:?}, not a hello"));
                    let _ = io.close().await;
                    return;
                }
                Ok(None) => return,
                Err(error) => {
                    self.say(LogLevel::Warn, format!("{peer}: {error}"));
                    let _ = io.close().await;
                    return;
                }
            };
            let client = format!("{peer} pid {pid}");
            self.say(
                LogLevel::Info,
                format!("{client}: connected, client build stamp {stamp:#010x}."),
            );

            // 2. The offsets.
            let offsets = Message::Offsets {
                documents: self.link.offsets.clone(),
            };
            if io.write_all(&offsets.encode()).await.is_err() {
                return;
            }

            // 3. What the client found.
            match next_message(&mut io, &mut decoder).await {
                Ok(Some(Message::Resolved { found, missing })) => {
                    if missing.is_empty() {
                        self.say(
                            LogLevel::Info,
                            format!("{client}: resolved all {} offset(s).", found.len()),
                        );
                    } else {
                        for (name, reason) in &missing {
                            self.say(
                                LogLevel::Error,
                                format!(
                                    "{client}: offset {name} was not found in build {stamp:#010x}: {reason}"
                                ),
                            );
                        }
                    }
                }
                Ok(Some(other)) => {
                    self.say(LogLevel::Warn, format!("{client}: sent {other:?}, not its report"));
                    let _ = io.close().await;
                    return;
                }
                Ok(None) => {
                    self.say(LogLevel::Info, format!("{client}: closed before its report."));
                    return;
                }
                Err(error) => {
                    self.say(LogLevel::Warn, format!("{client}: {error}"));
                    let _ = io.close().await;
                    return;
                }
            }

            // 4. The custom titles.
            let titles = Message::Titles {
                custom_tracks: self.link.custom_tracks.clone(),
            };
            if io.write_all(&titles.encode()).await.is_err() {
                return;
            }
            self.say(
                LogLevel::Info,
                format!(
                    "{client}: sent {} custom title(s).",
                    self.link.custom_tracks.len()
                ),
            );

            // 5. The maps this server hosts. The list is sent even when it is empty: it is the
            // whole truth for this client, and an empty one says no map file is served.
            if io.write_all(&self.link.map_list().encode()).await.is_err() {
                return;
            }
            self.say(
                LogLevel::Info,
                format!("{client}: listed {} map file(s).", self.link.maps.len()),
            );

            // 6. The rows the pack states for its maps, also sent when there are none: a client
            // answers a map that is not here from its own table.
            let rows = Message::MapRows { rows: self.link.map_rows.clone() };
            if io.write_all(&rows.encode()).await.is_err() {
                return;
            }
            self.say(
                LogLevel::Info,
                format!("{client}: stated the row of {} map(s).", self.link.map_rows.len()),
            );

            // The connection stays open for as long as the client runs. A client asks for the map
            // files it does not hold; any other message is logged and the link goes on.
            loop {
                match next_message(&mut io, &mut decoder).await {
                    Ok(Some(Message::MapWanted { file_id, sha256 })) => {
                        let Some(held) = self
                            .link
                            .maps
                            .iter()
                            .find(|held| held.file.file_id == file_id && held.file.sha256 == sha256)
                        else {
                            // The list is sent again: what the client asked for is not on it.
                            self.say(
                                LogLevel::Warn,
                                format!("{client}: asked for map file {file_id:#x} with a checksum this server does not host."),
                            );
                            if io.write_all(&self.link.map_list().encode()).await.is_err() {
                                return;
                            }
                            continue;
                        };
                        let bytes = Arc::clone(&held.bytes);
                        let mut offset = 0usize;
                        // A file with no bytes is still one part, so the client knows it is whole.
                        loop {
                            if !self.link.map_part_pause.is_zero() {
                                io.pause(self.link.map_part_pause).await;
                            }
                            let end = (offset + client_link::MAP_PART).min(bytes.len());
                            let part = Message::MapPart {
                                file_id,
                                sha256: sha256.clone(),
                                offset: offset as u32,
                                total: held.file.size,
                                data: bytes[offset..end].to_vec(),
                            };
                            if io.write_all(&part.encode()).await.is_err() {
                                return;
                            }
                            offset = end;
                            if offset >= bytes.len() {
                                break;
                            }
                        }
                        self.say(
                            LogLevel::Info,
                            format!("{client}: sent map file {file_id:#x}, {} byte(s).", bytes.len()),
                        );
                    }
                    Ok(Some(message)) => {
                        self.say(LogLevel::Debug, format!("{client}: sent {message:?}"));
                    }
                    Ok(None) => {
                        self.say(LogLevel::Info, format!("{client}: closed."));
                        return;
                    }
                    Err(error) => {
                        self.say(LogLevel::Warn, format!("{client}: {error}"));
                        let _ = io.close().await;
                        return;
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/flavors/Vanilla/Client")
    }

    #[test]
    fn the_shipped_offsets_load() {
        let link = ClientLink::load(&shipped(), Vec::new()).unwrap();
        assert!(link.offset_documents() >= 1);
        assert_eq!(link.custom_titles(), 0);
    }

    /// A map container with one record of `length` bytes: a file the server will host.
    fn a_map(length: usize) -> Vec<u8> {
        let mut map = Vec::from(*b"ffna\x03");
        map.extend_from_slice(&0x2000_0000u32.to_le_bytes());
        map.extend_from_slice(&(length as u32).to_le_bytes());
        map.extend((0..length).map(|at| at as u8));
        map
    }

    /// A folder holding one finished map file, as a pack's map folder does.
    fn a_pack_file(name: &str, bytes: &[u8]) -> game::maps::pack::MapFiles {
        let folder = std::env::temp_dir().join(format!("kryta-link-maps-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("0005F100.ffna");
        std::fs::write(&path, bytes).unwrap();
        game::maps::pack::MapFiles {
            map_id: 148,
            file_id: 0x5F100,
            stripped: None,
            finished: Some(path),
        }
    }

    /// A listener serving the shipped offsets and one custom title, and a client connected to it.
    fn served() -> (shared::network::socket_mgr::SocketMgr, std::net::TcpStream) {
        served_with(&[])
    }

    fn served_with(
        maps: &[game::maps::pack::MapFiles],
    ) -> (shared::network::socket_mgr::SocketMgr, std::net::TcpStream) {
        served_slowly(maps, std::time::Duration::ZERO)
    }

    fn served_slowly(
        maps: &[game::maps::pack::MapFiles],
        pause: std::time::Duration,
    ) -> (shared::network::socket_mgr::SocketMgr, std::net::TcpStream) {
        let track = serde_json::json!({ "id": 48, "name": "Hog Jumper" });
        let mut link = ClientLink::load(&shipped(), vec![track]).unwrap();
        link.host_maps("Vanilla", maps).unwrap();
        let row = game::maps::pack::ClientRow {
            map_id: 148,
            fields: vec![("y".to_string(), 600), ("type".to_string(), 2), ("x".to_string(), 500)],
            name: Some("A Name Of Its Own".to_string()),
            description: None,
        };
        assert_eq!(link.host_map_rows(&[row]).unwrap(), 1);
        link.set_map_part_pause(pause);
        let link = Arc::new(link);
        let factory: shared::network::socket::SessionFactory = Arc::new(move || {
            Box::new(ClientLinkSession::new(Arc::clone(&link), None)) as Box<dyn Session>
        });
        let mut sockets = shared::network::socket_mgr::SocketMgr::new(factory);
        let port = sockets.start_network("127.0.0.1", 0, 1).unwrap();
        let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        (sockets, stream)
    }

    #[test]
    fn a_client_is_given_the_offsets_and_then_the_titles() {
        let (mut sockets, mut stream) = served();
        let hello = Message::Hello {
            protocol: PROTOCOL,
            pid: 4242,
            time_date_stamp: 0x6AE9_51C4,
        };
        client_link::write(&mut stream, &hello).unwrap();

        let Message::Offsets { documents } = client_link::read(&mut stream).unwrap() else {
            panic!("the hello was not answered with the offsets");
        };
        // Every document in the folder is sent, in the order of their file names.
        let namespaces: Vec<&str> =
            documents.iter().filter_map(|document| document["namespace"].as_str()).collect();
        assert_eq!(namespaces, ["files", "maps", "text", "titles"]);

        let report = Message::Resolved {
            found: vec!["titles.rank_call".to_string()],
            missing: Vec::new(),
        };
        client_link::write(&mut stream, &report).unwrap();
        let Message::Titles { custom_tracks } = client_link::read(&mut stream).unwrap() else {
            panic!("the report was not answered with the titles");
        };
        assert_eq!(custom_tracks[0]["name"], "Hog Jumper");
        // A server that hosts no map file says so: the list is the whole truth.
        assert_eq!(
            client_link::read(&mut stream).unwrap(),
            Message::Maps { flavor: Some("Vanilla".to_string()), files: Vec::new() }
        );
        // And the rows its pack states, here the kind and world-map place of map 148.
        assert_eq!(
            client_link::read(&mut stream).unwrap(),
            Message::MapRows {
                rows: vec![client_link::MapRow {
                    map_id: 148,
                    fields: vec![(3, 2), (16, 500), (17, 600)],
                    name: Some("A Name Of Its Own".to_string()),
                    description: None,
                }],
            }
        );

        drop(stream);
        sockets.stop_network();
    }

    /// The hello, the offsets, the report and the titles, as a client goes through them.
    fn up_to_the_map_list(stream: &mut std::net::TcpStream) -> Vec<client_link::MapFile> {
        let hello = Message::Hello { protocol: PROTOCOL, pid: 7, time_date_stamp: 0 };
        client_link::write(stream, &hello).unwrap();
        assert!(matches!(client_link::read(stream).unwrap(), Message::Offsets { .. }));
        let report = Message::Resolved { found: Vec::new(), missing: Vec::new() };
        client_link::write(stream, &report).unwrap();
        assert!(matches!(client_link::read(stream).unwrap(), Message::Titles { .. }));
        let Message::Maps { flavor, files } = client_link::read(stream).unwrap() else {
            panic!("the titles were not followed by the map list");
        };
        assert_eq!(flavor.as_deref(), Some("Vanilla"), "the server names its flavor");
        assert!(matches!(client_link::read(stream).unwrap(), Message::MapRows { .. }));
        files
    }

    /// The server lists the finished map file of its pack with its size and checksum, and sends it
    /// in parts to a client that asks: a file larger than one part arrives whole.
    #[test]
    fn a_hosted_map_is_listed_and_sent_in_parts() {
        let map = a_map(client_link::MAP_PART * 2 + 1000);
        let (mut sockets, mut stream) = served_with(&[a_pack_file("parts", &map)]);
        let files = up_to_the_map_list(&mut stream);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_id, 0x5F100);
        assert_eq!(files[0].size as usize, map.len());
        assert_eq!(files[0].sha256, client_link::sha256_hex(&map));

        let wanted = Message::MapWanted { file_id: 0x5F100, sha256: files[0].sha256.clone() };
        client_link::write(&mut stream, &wanted).unwrap();
        let mut got = Vec::new();
        let mut parts = 0;
        while got.len() < map.len() {
            let Message::MapPart { file_id, sha256, offset, total, data } =
                client_link::read(&mut stream).unwrap()
            else {
                panic!("the request was not answered with a part");
            };
            assert_eq!((file_id, sha256.as_str()), (0x5F100, files[0].sha256.as_str()));
            assert_eq!((offset as usize, total as usize), (got.len(), map.len()));
            got.extend_from_slice(&data);
            parts += 1;
        }
        assert_eq!(parts, 3, "two full parts and the rest");
        assert_eq!(got, map);

        // A checksum the server does not host is answered with the list again.
        let stale = Message::MapWanted { file_id: 0x5F100, sha256: client_link::sha256_hex(b"old") };
        client_link::write(&mut stream, &stale).unwrap();
        assert!(matches!(client_link::read(&mut stream).unwrap(), Message::Maps { .. }));

        drop(stream);
        sockets.stop_network();
    }

    /// A server told to send slowly waits before each part, and the file still arrives whole.
    #[test]
    fn a_map_sent_slowly_takes_the_pauses_and_arrives_whole() {
        let map = a_map(client_link::MAP_PART * 2 + 10);
        let pause = std::time::Duration::from_millis(100);
        let (mut sockets, mut stream) = served_slowly(&[a_pack_file("slow", &map)], pause);
        let files = up_to_the_map_list(&mut stream);
        let wanted = Message::MapWanted { file_id: 0x5F100, sha256: files[0].sha256.clone() };
        let began = std::time::Instant::now();
        client_link::write(&mut stream, &wanted).unwrap();
        let mut got = Vec::new();
        while got.len() < map.len() {
            let Message::MapPart { data, .. } = client_link::read(&mut stream).unwrap() else {
                panic!("the request was not answered with a part");
            };
            got.extend_from_slice(&data);
        }
        assert!(began.elapsed() >= pause * 3, "three parts, a pause before each");
        assert_eq!(got, map);
        drop(stream);
        sockets.stop_network();
    }

    /// Both ends together, as a client's DLL goes through them (`tools/client-dll/src/runtime/mod.rs`,
    /// the link's loop) with everything but the hook itself: the list arrives and fetches nothing;
    /// the client's load of that file id finds the cache without it, asks the server on the link
    /// and is held; the parts are taken and the file stored under its checksum; and the load is
    /// then answered with the cached file. A second client of the same cache asks for nothing.
    #[test]
    fn a_load_fetches_a_hosted_map_into_the_cache_and_a_second_client_does_not() {
        use kryta_client::map_cache::{Answer, Cache, Fetch, OPEN_READ, Served};
        let map = a_map(client_link::MAP_PART + 77);
        let hosted = a_pack_file("fetch", &map);
        let folder = std::env::temp_dir().join(format!("kryta-link-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let cache = Cache::new(folder.clone());
        let patience = std::time::Duration::from_secs(10);

        let (mut sockets, mut stream) = served_with(std::slice::from_ref(&hosted));
        let files = up_to_the_map_list(&mut stream);
        let served = Arc::new(Served::new());
        served.set_list(&files, &cache);
        assert_eq!(served.fetching_file(0x5F100), None, "the list alone asks for nothing");

        // The client's load, on its own thread: it asks on the link and is held.
        let load = {
            let served = Arc::clone(&served);
            let writer = std::sync::Mutex::new(stream.try_clone().unwrap());
            std::thread::spawn(move || {
                served.answer(0x5F100, 1, OPEN_READ, patience, &|file| {
                    let wanted =
                        Message::MapWanted { file_id: file.file_id, sha256: file.sha256.clone() };
                    client_link::write(&mut *writer.lock().unwrap(), &wanted).is_ok()
                })
            })
        };
        // The link's thread: the parts of the file the load asked for.
        let mut fetch = None;
        loop {
            let Message::MapPart { file_id, sha256, offset, total, data } =
                client_link::read(&mut stream).unwrap()
            else {
                panic!("the request was not answered with a part");
            };
            let fetch = fetch.get_or_insert_with(|| {
                Fetch::new(served.fetching_file(file_id).expect("the load asked for it"))
            });
            if fetch.part(&sha256, offset, total, &data).unwrap() {
                break;
            }
            served.part_arrived();
            assert!(!load.is_finished(), "the load is held while the file arrives");
        }
        let fetch = fetch.expect("a part arrived");
        let path = cache.store(fetch.file(), fetch.bytes(), 1).unwrap();
        served.ready(fetch.file(), path.clone());
        assert_eq!(load.join().unwrap(), Answer::Cached(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), map, "the cached file is the server's file");
        drop(stream);
        sockets.stop_network();

        // A second client, the same cache: its load is answered without asking the server.
        let never = |_: &client_link::MapFile| -> bool { panic!("a cached file was asked for") };
        let (mut sockets, mut stream) = served_with(std::slice::from_ref(&hosted));
        let second = Served::new();
        second.set_list(&up_to_the_map_list(&mut stream), &cache);
        assert_eq!(second.answer(0x5F100, 1, OPEN_READ, patience, &never), Answer::Cached(path));
        drop(stream);
        sockets.stop_network();

        // The server changes the map: another checksum, and a load asks for it.
        let changed = a_pack_file("fetch", &a_map(client_link::MAP_PART + 78));
        let (mut sockets, mut stream) = served_with(&[changed]);
        let third = Served::new();
        let list = up_to_the_map_list(&mut stream);
        third.set_list(&list, &cache);
        let asked = std::sync::Mutex::new(Vec::new());
        let refuse = |file: &client_link::MapFile| {
            asked.lock().unwrap().push(file.sha256.clone());
            false
        };
        assert_eq!(third.answer(0x5F100, 1, OPEN_READ, patience, &refuse), Answer::NotFound);
        assert_eq!(*asked.lock().unwrap(), [list[0].sha256.clone()]);
        drop(stream);
        sockets.stop_network();
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A finished file that is not a map stops the load, and a map with no finished file is not
    /// hosted.
    #[test]
    fn a_file_that_is_not_a_map_is_not_hosted() {
        let mut link = ClientLink::load(&shipped(), Vec::new()).unwrap();
        let error = link.host_maps("Vanilla", &[a_pack_file("bad", b"not a map")]).err().unwrap();
        assert!(error.contains("is not a map file"), "{error}");
        // A pack whose name cannot be a folder on a client's machine stops the load too.
        let error = link.host_maps("two words", &[]).err().unwrap();
        assert!(error.contains("cannot name"), "{error}");

        let stripped_only = game::maps::pack::MapFiles {
            map_id: 148,
            file_id: 0x5F100,
            stripped: Some("nowhere".into()),
            finished: None,
        };
        assert_eq!(link.host_maps("Vanilla", &[stripped_only]).unwrap(), 0);

        // A row that names a field the client's row does not have stops the load as well.
        let misnamed = game::maps::pack::ClientRow {
            map_id: 148,
            fields: vec![("colour".to_string(), 1)],
            name: None,
            description: None,
        };
        // And so does a row that states a name of its own beside a name id.
        let twice = game::maps::pack::ClientRow {
            map_id: 148,
            fields: vec![("name_id".to_string(), 7)],
            name: Some("Twice".to_string()),
            description: None,
        };
        assert!(link.host_map_rows(&[twice]).err().unwrap().contains("both name and name_id"));
        let error = link.host_map_rows(&[misnamed]).err().unwrap();
        assert!(error.contains("client_row.colour"), "{error}");
    }

    #[test]
    fn a_client_speaking_another_protocol_is_refused() {
        let (mut sockets, mut stream) = served();
        let hello = Message::Hello {
            protocol: PROTOCOL + 1,
            pid: 1,
            time_date_stamp: 0,
        };
        client_link::write(&mut stream, &hello).unwrap();
        assert!(matches!(
            client_link::read(&mut stream).unwrap(),
            Message::Refused { .. }
        ));
        drop(stream);
        sockets.stop_network();
    }

    #[test]
    fn a_missing_or_broken_offset_directory_stops_the_load() {
        let directory = std::env::temp_dir().join(format!("kryta-link-{}", std::process::id()));
        let offsets = directory.join("offsets");
        let _ = std::fs::remove_dir_all(&directory);
        assert!(ClientLink::load(&directory, Vec::new()).is_err(), "no directory");

        std::fs::create_dir_all(&offsets).unwrap();
        let error = ClientLink::load(&directory, Vec::new()).err().unwrap();
        assert!(error.contains("no offset file"), "{error}");

        std::fs::write(offsets.join("bad.json"), "{ not json").unwrap();
        assert!(ClientLink::load(&directory, Vec::new()).is_err(), "not JSON");

        std::fs::write(
            offsets.join("bad.json"),
            r#"{ "resolvers": { "a": { "steps": [ { "op": "teleport" } ] } } }"#,
        )
        .unwrap();
        let error = ClientLink::load(&directory, Vec::new()).err().unwrap();
        assert!(error.contains("Invalid resolver step op"), "{error}");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
