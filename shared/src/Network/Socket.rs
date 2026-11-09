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
}

impl SessionIo {
    /// Wrap an accepted stream.
    pub fn new(stream: TcpStream) -> Self {
        let peer = stream.peer_addr().ok();
        Self { stream, peer }
    }

    /// Who is on the other end, when the system knows.
    ///
    /// Reference: `Socket::GetRemoteAddress`, which the reference logs when it cannot read it.
    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Read into a buffer, answering how many bytes arrived. Zero means the peer closed.
    pub async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(buffer).await
    }

    /// Write everything in a buffer.
    pub async fn write_all(&mut self, buffer: &[u8]) -> std::io::Result<()> {
        self.stream.write_all(buffer).await
    }

    /// Close the connection.
    pub async fn close(&mut self) -> std::io::Result<()> {
        self.stream.shutdown().await
    }
}

/// What the manager calls to build a session for a new connection.
pub type SessionFactory = Arc<dyn Fn() -> Box<dyn Session> + Send + Sync>;
