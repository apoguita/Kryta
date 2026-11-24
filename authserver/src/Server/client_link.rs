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
        })
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

            // The connection stays open for as long as the client runs. Nothing the client sends
            // after its report is acted on yet; a message is logged and the link goes on.
            loop {
                match next_message(&mut io, &mut decoder).await {
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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/Client")
    }

    #[test]
    fn the_shipped_offsets_load() {
        let link = ClientLink::load(&shipped(), Vec::new()).unwrap();
        assert!(link.offset_documents() >= 1);
        assert_eq!(link.custom_titles(), 0);
    }

    /// A listener serving the shipped offsets and one custom title, and a client connected to it.
    fn served() -> (shared::network::socket_mgr::SocketMgr, std::net::TcpStream) {
        let track = serde_json::json!({ "id": 48, "name": "Hog Jumper" });
        let link = Arc::new(ClientLink::load(&shipped(), vec![track]).unwrap());
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
        assert_eq!(documents[0]["namespace"], "titles");

        let report = Message::Resolved {
            found: vec!["titles.rank_call".to_string()],
            missing: Vec::new(),
        };
        client_link::write(&mut stream, &report).unwrap();
        let Message::Titles { custom_tracks } = client_link::read(&mut stream).unwrap() else {
            panic!("the report was not answered with the titles");
        };
        assert_eq!(custom_tracks[0]["name"], "Hog Jumper");

        drop(stream);
        sockets.stop_network();
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
