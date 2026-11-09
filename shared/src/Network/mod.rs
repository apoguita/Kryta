//! The socket layer.
//!
//! Reference: `src/server/shared/Network/` - four files, and they are the whole of the
//! reference's transport: `AsyncAcceptor`, `NetworkThread`, `Socket` and `SocketMgr`. They
//! are a library both programs start, not a program of their own, which is why a world process can
//! own one listener and still keep the network as a subsystem.
//!
//! The runtime is tokio where the reference has boost::asio; the arrangement is the reference's:
//! one acceptor, N threads, each thread serving many connections, and a new connection given to
//! the thread serving fewest.

#[path = "AsyncAcceptor.rs"]
pub mod async_acceptor;
#[path = "NetworkThread.rs"]
pub mod network_thread;
#[path = "Socket.rs"]
pub mod socket;
#[path = "SocketMgr.rs"]
pub mod socket_mgr;

pub use async_acceptor::AsyncAcceptor;
pub use network_thread::{NetworkThread, NetworkThreadHandle};
pub use socket::{Session, SessionFactory, SessionFuture, SessionIo};
pub use socket_mgr::{select_thread_with_min_connections, SocketMgr};