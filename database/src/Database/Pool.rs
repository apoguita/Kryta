//! The pool: N worker threads, one connection each, and a queue for the callers.
//!
//! Reference: `src/server/database/Database/DatabaseWorkerPool.{h,cpp}`. The arrangement is the
//! reference's and is kept deliberately (`docs/decisions/0002-worker-pool-threads.md`):
//!
//! - one connection per worker thread, opened when the pool opens;
//! - callers hand work over a queue and do not wait for it, which is the reference's `Execute`;
//! - a caller that must have the answer uses the pool's own synchronous connection, which is the
//!   reference's `DirectExecute` and `Query`;
//! - closing means closing the queue and then joining the threads, in that order, so nothing
//!   queued is lost.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use common::logging::Log;

use super::{Connection, DatabaseKind, DatabaseSettings};

/// One thing for a worker to do.
enum Operation {
    /// Run a statement and forget the answer. The reference's one-way `Execute`.
    Execute(String),
}

/// The pool for one logical database.
pub struct DatabaseWorkerPool {
    kind: DatabaseKind,
    worker_threads: u8,
    synch: Connection,
    queue: Option<Sender<Operation>>,
    workers: Vec<JoinHandle<()>>,
}

impl DatabaseWorkerPool {
    /// Open the pool: the synchronous connection, then one connection per worker thread.
    ///
    /// Reference: `DatabaseWorkerPool::Open`, which logs the pool's name and its two connection
    /// counts before it opens anything.
    pub fn open(
        kind: DatabaseKind,
        settings: &DatabaseSettings,
        log: &Log,
    ) -> Result<Self, String> {
        log.out(
            common::logging::LogLevel::Info,
            "sql.driver",
            format!(
                "Opening DatabasePool '{}'. Asynchronous connections: {}, synchronous connections: {}.",
                settings.info.database, settings.worker_threads, settings.synch_threads
            ),
        );

        let synch = Connection::open(&settings.info)?;

        let (sender, receiver) = channel::<Operation>();
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(settings.worker_threads as usize);
        for index in 0..settings.worker_threads {
            let info = settings.info.clone();
            let receiver = Arc::clone(&receiver);
            let log = log.clone();
            let database = settings.info.database.clone();

            let handle = std::thread::Builder::new()
                .name(format!("db-{database}-{index}"))
                .spawn(move || {
                    let mut connection = match Connection::open(&info) {
                        Ok(connection) => connection,
                        Err(error) => {
                            log.out(
                                common::logging::LogLevel::Error,
                                "sql.sql",
                                format!("worker {index} of {database} cannot connect: {error}"),
                            );
                            return;
                        }
                    };

                    loop {
                        let operation = {
                            let receiver = match receiver.lock() {
                                Ok(receiver) => receiver,
                                Err(_) => return,
                            };
                            match receiver.recv() {
                                Ok(operation) => operation,
                                // The queue is closed: the pool is shutting down.
                                Err(_) => return,
                            }
                        };

                        match operation {
                            Operation::Execute(sql) => {
                                if let Err(error) = connection.execute(&sql) {
                                    log.out(
                                        common::logging::LogLevel::Error,
                                        "sql.sql",
                                        format!("{error}: {sql}"),
                                    );
                                }
                            }
                        }
                    }
                })
                .map_err(|error| format!("cannot start a database worker: {error}"))?;

            workers.push(handle);
        }

        Ok(Self {
            kind,
            worker_threads: settings.worker_threads,
            synch,
            queue: Some(sender),
            workers,
        })
    }

    /// Which database this pool is for.
    pub fn kind(&self) -> DatabaseKind {
        self.kind
    }

    /// How many worker threads serve it.
    pub fn worker_threads(&self) -> u8 {
        self.worker_threads
    }

    /// Hand a statement to a worker and answer nothing.
    ///
    /// Reference: `DatabaseWorkerPool::Execute` for a string - a one-way operation, used for work
    /// nobody waits on. An error is logged by the worker, not returned here.
    pub fn execute(&self, sql: impl Into<String>) {
        if let Some(queue) = self.queue.as_ref() {
            let _ = queue.send(Operation::Execute(sql.into()));
        }
    }

    /// Run a statement on the pool's own connection and answer how many rows it touched.
    ///
    /// Reference: `DirectExecute`, which the reference uses for the statements a startup cannot
    /// continue without.
    pub fn execute_sync(&mut self, sql: &str) -> Result<u64, String> {
        self.synch.execute(sql)
    }

    /// Query on the pool's own connection.
    ///
    /// Reference: `Query`.
    pub fn query_sync(&mut self, sql: &str) -> Result<Vec<postgres::Row>, String> {
        self.synch.query(sql)
    }

    /// Bring this database up to date with the schema files given, and answer what ran.
    ///
    /// Reference: `DatabaseLoader::Load`, which opens the pool and then runs `DBUpdater` on it -
    /// the same connection, the same order.
    pub fn apply_updates(
        &mut self,
        files: &[crate::updater::UpdateFile],
        log: &Log,
    ) -> Result<crate::updater::apply::UpdateSummary, String> {
        crate::updater::apply::apply(&mut self.synch, files, log)
            .map_err(|error| format!("the {} database could not be updated: {error}", self.kind.name()))
    }

    /// Whether the database still answers.
    ///
    /// Reference: the pool's keep-alive ping.
    pub fn ping(&mut self) -> Result<(), String> {
        self.synch.ping()
    }

    /// Close the queue and wait for the workers.
    ///
    /// Reference: `DatabaseWorkerPool::Close`, which cancels the queue and then waits for the
    /// threads, in that order.
    pub fn close(mut self) {
        self.queue = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Drop for DatabaseWorkerPool {
    fn drop(&mut self) {
        self.queue = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::logging::Log;
    use std::time::{Duration, Instant};

    /// A pool against a real server, for the two tests that cannot be faked.
    ///
    /// Run them with the connection string in `KRYTA_TEST_LOGIN_DB`, which is shaped like the
    /// configuration's own value - `host;port;user;password;database`. They are ignored by default
    /// because a checkout without a database should not fail its test run.
    fn live_pool() -> Option<(DatabaseWorkerPool, Log)> {
        let info = std::env::var("KRYTA_TEST_LOGIN_DB").ok()?;
        let info = super::super::DatabaseInfo::parse(&info).ok()?;

        let settings = DatabaseSettings {
            info,
            worker_threads: 2,
            synch_threads: 1,
        };

        let log = Log::new();
        let pool = DatabaseWorkerPool::open(DatabaseKind::Login, &settings, &log).ok()?;
        Some((pool, log))
    }

    #[test]
    #[ignore = "needs a live database: set KRYTA_TEST_LOGIN_DB"]
    fn a_pool_opens_and_answers() {
        let Some((mut pool, log)) = live_pool() else {
            return;
        };
        assert_eq!(pool.kind(), DatabaseKind::Login);
        assert_eq!(pool.worker_threads(), 2);
        pool.ping().unwrap();
        pool.close();
        log.shutdown();
    }

    #[test]
    #[ignore = "needs a live database: set KRYTA_TEST_LOGIN_DB"]
    fn a_deferred_statement_runs_on_a_worker_and_the_pool_sees_it() {
        let Some((mut pool, log)) = live_pool() else {
            return;
        };

        pool.execute_sync("DROP TABLE IF EXISTS t_probe").unwrap();
        pool.execute("CREATE TABLE t_probe (value int)");

        // The worker runs it on its own connection, so the pool's connection seeing the table is
        // the proof it ran at all.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while Instant::now() < deadline {
            let rows = pool
                .query_sync("SELECT count(*) FROM information_schema.tables WHERE table_name = 't_probe'")
                .unwrap();
            let count: i64 = rows[0].get(0);
            if count == 1 {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        pool.execute_sync("DROP TABLE IF EXISTS t_probe").unwrap();
        pool.close();
        log.shutdown();

        assert!(found, "the deferred statement never reached the database");
    }
}
