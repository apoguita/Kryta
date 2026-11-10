//! What one accepted connection does.
//!
//! Reference: `src/server/shared/Network/Socket.{h,cpp}`, the object a connection becomes. The
//! reference's socket is a class with callbacks (`OnOpen`, `ReadHandler`, `CloseSocket`) driven by
//! a 1 ms update timer on its network thread. Here a session is a future that the connection's
//! thread runs, which is the same work with the timer replaced by the runtime's own scheduling.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use common::logging::{Log, LogLevel};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The future a session runs.
pub type SessionFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// One connection's conversation.
///
/// Reference: a `Socket<Self>` subclass. Implementors write the conversation; the network thread
/// owns the socket, its reads and its lifetime.
pub trait Session: Send + 'static {
    /// Run the conversation to its end.
    fn run(self: Box<Self>, io: SessionIo) -> SessionFuture;
}

/// The stream a session talks through.
pub struct SessionIo {
    stream: TcpStream,
    peer: Option<SocketAddr>,
    /// Where every byte is reported, when tracing is on. Off by default: a trace writes a line per
    /// read and per write, which is what an investigation wants and what a served client does not.
    trace: Option<Log>,
    /// Bytes received and sent, kept whether tracing is on or not.
    received: usize,
    sent: usize,
}

impl SessionIo {
    /// Wrap an accepted stream.
    pub fn new(stream: TcpStream) -> Self {
        let peer = stream.peer_addr().ok();
        Self {
            stream,
            peer,
            trace: None,
            received: 0,
            sent: 0,
        }
    }

    /// Report every read and every write to this log.
    ///
    /// The format is one line per operation, with the direction, the count and the bytes:
    ///
    /// ```text
    /// trace 127.0.0.1:56280 in  4 bytes: 00 04 0C 00
    /// trace 127.0.0.1:56280 out 22 bytes: 01 16 04 3B ...
    /// ```
    ///
    /// This is the instrument that turns "the client said nothing useful" into evidence: whatever a
    /// client sends, including traffic this server does not understand, appears here byte for byte.
    pub fn enable_trace(&mut self, log: Log) {
        self.trace = Some(log);
    }

    /// Who is on the other end, when the system knows.
    ///
    /// Reference: `Socket::GetRemoteAddress`, which the reference logs when it cannot read it.
    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Bytes received on this connection so far.
    pub fn received(&self) -> usize {
        self.received
    }

    /// Bytes sent on this connection so far.
    pub fn sent(&self) -> usize {
        self.sent
    }

    fn peer_text(&self) -> String {
        self.peer
            .map(|address| address.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn report(&self, direction: &str, bytes: &[u8]) {
        if let Some(log) = self.trace.as_ref() {
            log.out(
                LogLevel::Debug,
                "network.trace",
                format!(
                    "trace {} {direction} {} bytes: {}",
                    self.peer_text(),
                    bytes.len(),
                    hex(bytes)
                ),
            );
        }
    }

    /// Read into a buffer, answering how many bytes arrived. Zero means the peer closed.
    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.stream.read(buffer).await;
        if let Ok(count) = &read {
            self.received += count;
            if *count == 0 {
                if let Some(log) = self.trace.as_ref() {
                    log.out(
                        LogLevel::Debug,
                        "network.trace",
                        format!(
                            "trace {} closed the read side after {} received, {} sent",
                            self.peer_text(),
                            self.received,
                            self.sent
                        ),
                    );
                }
            } else {
                self.report("in ", &buffer[..*count]);
            }
        }
        read
    }

    /// Write everything in a buffer.
    pub async fn write_all(&mut self, buffer: &[u8]) -> std::io::Result<()> {
        let written = self.stream.write_all(buffer).await;
        if written.is_ok() {
            self.sent += buffer.len();
            self.report("out", buffer);
        }
        written
    }

    /// Close the connection.
    pub async fn close(&mut self) -> std::io::Result<()> {
        self.stream.shutdown().await
    }
}

/// Bytes as the trace prints them: two lowercase hex digits each, space separated.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_prints_two_upper_digits_per_byte() {
        assert_eq!(hex(&[0x00, 0x04, 0x0C, 0xFF]), "00 04 0C FF");
        assert_eq!(hex(&[]), "");
    }
}

/// What the manager calls to build a session for a new connection.
pub type SessionFactory = Arc<dyn Fn() -> Box<dyn Session> + Send + Sync>;
