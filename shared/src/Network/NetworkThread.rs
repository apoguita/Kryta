//! One network thread: a runtime, the connections it owns, and a count of them.
//!
//! Reference: `src/server/shared/Network/NetworkThread.h`. The reference gives each thread its own
//! `io_context` and a 1 ms timer that adds newly accepted sockets and calls their `Update()`. Here
//! the thread owns a tokio runtime instead: accepted connections are sent to it, each becomes a
//! task, and the count is what the manager reads to pick the least busy thread.
//!
//! The stop signal is the queue closing. Every sender for a thread's queue lives in a
//! [`NetworkThreadHandle`], so dropping the last handle ends the runtime's own loop and the thread
//! exits without a flag to poll.

use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use super::socket::{Session, SessionIo};

/// One accepted connection on its way to a thread.
type Job = (TcpStream, Box<dyn Session>);

/// The part of a thread another thread may touch.
#[derive(Clone)]
pub struct NetworkThreadHandle {
    sender: UnboundedSender<Job>,
    connections: Arc<AtomicUsize>,
}

impl NetworkThreadHandle {
    /// How many connections this thread is serving right now.
    ///
    /// Reference: `NetworkThread::GetConnectionCount`, which the manager reads to choose a thread.
    pub fn connection_count(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    /// Hand a connection to this thread.
    ///
    /// Answers false when the thread has stopped, which is the reference's `AddSocket` on a
    /// thread that is no longer running.
    pub fn submit(&self, stream: TcpStream, session: Box<dyn Session>) -> bool {
        self.connections.fetch_add(1, Ordering::Relaxed);
        if self.sender.send((stream, session)).is_err() {
            self.connections.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        true
    }
}

/// A network thread, its handle, and the thread itself.
pub struct NetworkThread {
    handle: Option<NetworkThreadHandle>,
    worker: Option<JoinHandle<()>>,
}

impl NetworkThread {
    /// Start a thread with its own runtime.
    ///
    /// Reference: `NetworkThread::Start`, which spawns the thread that runs the io context.
    pub fn start(index: usize, trace: Option<common::logging::Log>) -> std::io::Result<Self> {
        let (sender, mut receiver) = unbounded_channel::<Job>();
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connections);

        let worker = std::thread::Builder::new()
            .name(format!("network-{index}"))
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };

                runtime.block_on(async move {
                    // The queue ends when the manager drops its last handle.
                    while let Some((stream, session)) = receiver.recv().await {
                        let Ok(stream) = tokio::net::TcpStream::from_std(stream) else {
                            counter.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        };

                        let counter = Arc::clone(&counter);
                        let trace = trace.clone();
                        tokio::spawn(async move {
                            let mut io = SessionIo::new(stream);
                            if let Some(log) = trace {
                                io.enable_trace(log);
                            }
                            session.run(io).await;
                            counter.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                });
            })?;

        Ok(Self {
            handle: Some(NetworkThreadHandle {
                sender,
                connections,
            }),
            worker: Some(worker),
        })
    }

    /// The handle another thread may submit to.
    pub fn handle(&self) -> Option<NetworkThreadHandle> {
        self.handle.clone()
    }

    /// How many connections this thread is serving.
    pub fn connection_count(&self) -> usize {
        self.handle
            .as_ref()
            .map(NetworkThreadHandle::connection_count)
            .unwrap_or(0)
    }

    /// Drop this thread's handle and wait for the thread to finish.
    ///
    /// Reference: `NetworkThread::Stop` then `Wait`. The queue closing is what stops the runtime's
    /// loop; the join is what proves it did.
    pub fn stop(&mut self) {
        self.handle = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for NetworkThread {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.stop();
        }
    }
}
