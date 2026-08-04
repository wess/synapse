mod atomic;
mod index;
mod rollback;
mod validate;

pub(crate) use atomic::copy as atomiccopy;
pub use index::{data, database, home, read, reveal, soul, write};
// Public because the desktop crate rolls back a multi-file settings edit the
// same way every command does. The rest of this module stays crate-private:
// nothing outside should be choosing its own atomic-write primitive.
pub use rollback::Snapshot;

/// `SYNAPSE_DATA` decides where the data directory is, and it is process-global
/// while tests run in parallel threads. Any test that points it at a temporary
/// directory holds this for as long as it needs the redirection, so two tests
/// cannot aim the same global at two different folders.
#[cfg(test)]
pub(crate) fn scopeddata(path: &std::path::Path) -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    let guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
    unsafe { std::env::set_var("SYNAPSE_DATA", path) };
    guard
}
