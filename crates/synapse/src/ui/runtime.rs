//! One async runtime for the window, made once.
//!
//! Every read the app makes is an `async fn` in `synapsecore`, and the window
//! is not async — so each one has to be blocked on. The obvious way to write
//! that is `Runtime::new()?.block_on(…)` at each call site, and that is what
//! this replaces: a fresh multi-threaded runtime spawns a worker per core,
//! `block_on` uses one of them, and dropping it joins them all again. Launching
//! did that eight times before the window existed — for reads that add up to a
//! few milliseconds of actual work.
//!
//! So: one runtime, built on first use and kept for the life of the process.
//! One worker thread, because nothing here fans out — the work is a handful of
//! SQLite queries, and sqlx keeps its own thread for the connection anyway.
//! Multi-threaded rather than current-thread despite that, because a
//! current-thread runtime cannot be blocked on from two threads at once and
//! some of these reads now happen off the main one.

use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};

fn shared() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("synapse")
            .build()
            .expect("start the Synapse runtime")
    })
}

/// Run one async read to completion on the shared runtime.
pub fn block<T>(future: impl std::future::Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    shared().block_on(future)
}
