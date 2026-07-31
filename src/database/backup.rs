use crate::database::permission;
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many snapshots to keep.
///
/// Each is a complete copy of the store, and one is written before every
/// migration and before every restore, so without a bound a large store quietly
/// costs a multiple of itself on disk and nothing ever says so. Five is enough
/// to undo a bad restore and step back through a couple of upgrades. It is not
/// a version history, and it is not a substitute for the user's own backups.
const KEEP: usize = 5;

pub async fn create(pool: &SqlitePool, database: &Path, label: &str) -> Result<PathBuf> {
    let folder = folder(database)?;
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("could not create {}", folder.display()))?;
    permission::securedirectory(&folder)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_millis();
    let path = folder.join(format!("brain.{stamp}.{label}.db"));
    vacuum(pool, &path).await?;
    permission::securefile(&path)?;
    // Losing an old snapshot is not worth failing the migration that just
    // succeeded over, so trimming is allowed to quietly not happen.
    let _ = rotate(&folder, KEEP);
    Ok(path)
}

pub fn folder(database: &Path) -> Result<PathBuf> {
    Ok(database
        .parent()
        .context("database path has no parent")?
        .join("backups"))
}

/// Snapshots in the folder, newest first.
pub fn snapshots(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(u128, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            Some((stamp(&path)?, path))
        })
        .collect();
    found.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
    found.into_iter().map(|(_, path)| path).collect()
}

/// The millisecond stamp in `brain.<stamp>.<label>.db`, or nothing when the file
/// is not one Synapse wrote — anything else in the folder is left alone.
fn stamp(path: &Path) -> Option<u128> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("brain.")?;
    if !name.ends_with(".db") {
        return None;
    }
    rest.split('.').next()?.parse().ok()
}

fn rotate(folder: &Path, keep: usize) -> Result<()> {
    for stale in snapshots(folder).into_iter().skip(keep) {
        std::fs::remove_file(&stale)
            .with_context(|| format!("could not remove {}", stale.display()))?;
    }
    Ok(())
}

pub async fn vacuum(pool: &SqlitePool, target: &Path) -> Result<()> {
    anyhow::ensure!(!target.exists(), "{} already exists", target.display());
    sqlx::query("VACUUM INTO ?")
        .bind(target.display().to_string())
        .execute(pool)
        .await
        .with_context(|| format!("could not write {}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(folder: &Path, name: &str) -> PathBuf {
        let path = folder.join(name);
        std::fs::write(&path, b"snapshot").unwrap();
        path
    }

    #[test]
    fn only_the_most_recent_snapshots_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path();
        for stamp in [1000, 3000, 2000, 5000, 4000, 6000] {
            snapshot(folder, &format!("brain.{stamp}.v4.db"));
        }

        rotate(folder, 3).unwrap();

        let left: Vec<String> = snapshots(folder)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            left,
            ["brain.6000.v4.db", "brain.5000.v4.db", "brain.4000.v4.db"],
            "the newest survive, whatever order they were written in"
        );
    }

    /// The folder is the user's to put things in. Anything Synapse did not name
    /// is not Synapse's to delete.
    #[test]
    fn files_synapse_did_not_write_are_never_removed() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path();
        for stamp in [1000, 2000, 3000] {
            snapshot(folder, &format!("brain.{stamp}.v4.db"));
        }
        let keepers = [
            snapshot(folder, "my-own-copy.db"),
            snapshot(folder, "notes.txt"),
            snapshot(folder, "brain.db"),
        ];

        rotate(folder, 1).unwrap();

        for keeper in keepers {
            assert!(keeper.is_file(), "{} was removed", keeper.display());
        }
        assert_eq!(snapshots(folder).len(), 1);
    }
}
