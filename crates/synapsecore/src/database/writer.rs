use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::Mutex as Queue;

static WRITERS: Mutex<Vec<(PathBuf, Weak<Queue<()>>)>> = Mutex::new(Vec::new());

/// SQLite still arbitrates between processes. Within one process, queue long
/// writes fairly before taking a connection so competing busy handlers cannot
/// starve a writer or occupy the entire read pool.
pub(crate) fn writequeue(path: &Path) -> Result<Arc<Queue<()>>> {
    let path = std::fs::canonicalize(path)?;
    let mut writers = WRITERS.lock().unwrap_or_else(|error| error.into_inner());
    writers.retain(|(_, queue)| queue.strong_count() > 0);
    if let Some(queue) = writers
        .iter()
        .find_map(|(known, queue)| (known == &path).then(|| queue.upgrade()).flatten())
    {
        return Ok(queue);
    }
    let queue = Arc::new(Queue::new(()));
    writers.push((path, Arc::downgrade(&queue)));
    Ok(queue)
}
