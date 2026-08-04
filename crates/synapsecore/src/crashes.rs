//! Where a crash goes when nobody is watching the terminal.
//!
//! A panic in the desktop application has nowhere to be seen: the window
//! disappears and the message goes to a console the user never opened. Synapse
//! reports nothing home, so the only way a crash can become a bug report is if
//! it is written down where the user can find it and `synapse doctor` can read
//! it back.
//!
//! Only the panic itself is recorded — its message, where in Synapse it came
//! from, and when. Nothing here reads the user's memory, their environment, or
//! anything they have stored, so a crash log is safe to paste into an issue.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// How much of the log to keep. Long enough to hold a crash loop's worth of
/// history, short enough to read and to paste.
const MAXBYTES: u64 = 256 * 1024;

pub fn path() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("crash.log"))
}

/// Send panics to the crash log as well as to the terminal.
///
/// Installed for the desktop application, which is the one that has no terminal
/// to print to. The default hook still runs, so nothing is lost for anyone
/// watching one.
pub fn capture() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = record(&format!(
            "{info}\n{}",
            std::backtrace::Backtrace::force_capture()
        ));
        previous(info);
    }));
}

fn record(report: &str) -> Result<()> {
    let path = path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    trim(&path);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))?;
    writeln!(
        file,
        "\n--- synapse {} · {} ---\n{}",
        env!("CARGO_PKG_VERSION"),
        stamp(),
        report.trim_end()
    )?;
    crate::database::securefile(&path)
}

/// The most recent entries, newest last, for `synapse doctor` to show without
/// asking anyone to find a file.
pub fn recent(limit: usize) -> Vec<String> {
    let Ok(raw) = path().and_then(|path| Ok(std::fs::read_to_string(path)?)) else {
        return Vec::new();
    };
    let mut entries: Vec<String> = raw
        .split("\n--- synapse ")
        .skip(1)
        .map(|entry| format!("synapse {}", entry.trim_end()))
        .collect();
    if entries.len() > limit {
        entries.drain(..entries.len() - limit);
    }
    entries
}

/// Drop the oldest half once the log grows past [`MAXBYTES`], so a crash loop
/// cannot fill a disk and the newest crash is always the one still there.
fn trim(path: &Path) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() <= MAXBYTES {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let keep = content.len() - (MAXBYTES / 2) as usize;
    let from = content[keep..]
        .find("\n--- synapse ")
        .map_or(keep, |at| keep + at + 1);
    let _ = std::fs::write(path, &content[from..]);
}

fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_is_written_down_and_read_back() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());

        record("panicked at 'the first one'").unwrap();
        record("panicked at 'the second one'").unwrap();

        let entries = recent(10);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].contains("the first one"));
        assert!(entries[1].contains("the second one"));
        assert!(entries[1].contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn only_the_most_recent_crashes_are_reported() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());
        for i in 0..8 {
            record(&format!("panicked at 'number {i}'")).unwrap();
        }

        let entries = recent(3);

        assert_eq!(entries.len(), 3);
        assert!(entries[2].contains("number 7"), "the newest must be last");
        assert!(entries[0].contains("number 5"));
    }

    #[test]
    fn no_log_is_not_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());
        assert!(recent(5).is_empty());
    }

    #[test]
    fn a_crash_loop_cannot_grow_the_log_without_end() {
        let directory = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(directory.path());
        let long = "x".repeat(4096);
        for i in 0..200 {
            record(&format!("panicked at 'number {i}' {long}")).unwrap();
        }

        let size = std::fs::metadata(path().unwrap()).unwrap().len();
        assert!(size <= MAXBYTES, "grew to {size}");
        let entries = recent(1);
        assert!(entries[0].contains("number 199"), "the newest must survive");
    }
}
