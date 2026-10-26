use super::*;
use std::fs;

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join("source.rs"), "fn main() {}\n").unwrap();
    dir
}

fn request() -> WriteRequest {
    WriteRequest {
        project: None,
        name: "overview".into(),
        description: "Example".into(),
        content: "# Example\n".into(),
        sources: vec!["source.rs".into()],
        revision: None,
    }
}

#[test]
fn overview_redacts_complete_tokens_before_truncating() {
    let dir = setup();
    write(dir.path(), &request()).unwrap();
    let path = dir.path().join(".synapse/knowledge/overview.md");
    let original = fs::read_to_string(&path).unwrap();
    // The start of the opening tag falls across the old 1,800-byte boundary.
    let body = format!(
        "{}<private>{}</private>",
        " ".repeat(1795),
        "sensitive".repeat(300)
    );
    let (header, _) = original.rsplit_once("+++\n").unwrap();
    fs::write(&path, format!("{header}+++\n{body}")).unwrap();
    let index = catalog(dir.path()).unwrap();
    assert!(!index.overview.unwrap().contains("<priv"));
    let body = format!("{}Bearer {}", " ".repeat(1775), "sensitive".repeat(100));
    fs::write(&path, format!("{header}+++\n{body}")).unwrap();
    assert!(
        !catalog(dir.path())
            .unwrap()
            .overview
            .unwrap()
            .contains("sensitive")
    );
}

#[test]
fn invalid_metadata_errors_never_echo_its_contents() {
    let dir = setup();
    write(dir.path(), &request()).unwrap();
    fs::write(
        dir.path().join(".synapse/knowledge/broken.md"),
        "+++\nsecret = 'sensitive' invalid\n+++\nbody",
    )
    .unwrap();
    assert!(
        !read(dir.path(), "broken", 0)
            .unwrap_err()
            .to_string()
            .contains("sensitive")
    );
    let index = catalog(dir.path()).unwrap();
    assert!(!serde_json::to_string(&index).unwrap().contains("sensitive"));
}

#[test]
fn directory_work_and_warning_output_are_bounded_even_for_invalid_guides() {
    let dir = setup();
    write(dir.path(), &request()).unwrap();
    for number in 0..300 {
        fs::write(
            dir.path()
                .join(format!(".synapse/knowledge/invalid{number}.md")),
            "invalid",
        )
        .unwrap();
    }
    let index = catalog(dir.path()).unwrap();
    assert!(index.warnings.len() <= 10);
    assert!(index.warnings.iter().any(|warning| warning.contains("128")));
    assert!(serde_json::to_vec(&index).unwrap().len() <= 32768);
}

#[test]
fn shared_sources_are_hashed_once_per_call_and_never_cached_across_calls() {
    let dir = setup();
    let content = vec![b'x'; 1024 * 1024];
    fs::write(dir.path().join("source.rs"), &content).unwrap();
    for number in 0..32 {
        let mut request = request();
        request.name = format!("guide{number}");
        write(dir.path(), &request).unwrap();
    }
    let index = catalog(dir.path()).unwrap();
    assert_eq!(index.guides.len(), 32);
    assert_eq!(index.sourcebytes, content.len());
    fs::write(dir.path().join("source.rs"), vec![b'y'; content.len()]).unwrap();
    assert!(
        catalog(dir.path())
            .unwrap()
            .guides
            .iter()
            .all(|guide| guide.status == "needsreview")
    );
}

#[test]
fn the_source_budget_is_enforced_and_unchecked_sources_are_explicit() {
    use sha2::{Digest, Sha256};
    let dir = setup();
    write(dir.path(), &request()).unwrap();
    let content = vec![b'x'; 2 * 1024 * 1024];
    let hash = format!("{:x}", Sha256::digest(&content));
    let mut sources = std::collections::BTreeMap::new();
    for number in 0..9 {
        let name = format!("source{number}.rs");
        fs::write(dir.path().join(&name), &content).unwrap();
        sources.insert(name, hash.clone());
    }
    let header = super::model::Header {
        description: "Example".into(),
        sources,
    };
    fs::write(
        dir.path().join(".synapse/knowledge/overview.md"),
        format!("+++\n{}+++\nbody", toml::to_string(&header).unwrap()),
    )
    .unwrap();
    let index = catalog(dir.path()).unwrap();
    assert_eq!(index.sourcebytes, 16 * 1024 * 1024);
    assert_eq!(index.guides[0].status, "unchecked");
    assert_eq!(index.guides[0].unchecked, 1);
    let mut request = request();
    request.sources = header.sources.into_keys().collect();
    request.name = "toolarge".into();
    assert!(write(dir.path(), &request).is_err());
    assert!(!dir.path().join(".synapse/knowledge/toolarge.md").exists());
}

#[test]
fn a_busy_writer_fails_without_waiting_for_the_other_process() {
    use fs2::FileExt;
    let dir = setup();
    let stored = write(dir.path(), &request()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join(".synapse/knowledge/write.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let root = dir.path().to_owned();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut request = request();
        request.revision = Some(stored.revision);
        send.send(write(&root, &request).map_err(|error| error.to_string()))
            .unwrap();
    });
    let answer = receive.recv_timeout(std::time::Duration::from_secs(2));
    FileExt::unlock(&lock).unwrap();
    worker.join().unwrap();
    assert!(answer.unwrap().unwrap_err().contains("retry"));
}

#[cfg(unix)]
#[test]
fn pinned_directory_handles_cannot_be_redirected_after_validation() {
    use std::os::unix::fs::symlink;
    let dir = setup();
    write(dir.path(), &request()).unwrap();
    let root = super::io::Folder::open(dir.path()).unwrap();
    let directory = super::io::knowledge(&root, false).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.md"), "sensitive").unwrap();
    fs::rename(dir.path().join(".synapse"), dir.path().join("saved")).unwrap();
    symlink(outside.path(), dir.path().join(".synapse")).unwrap();
    directory.replace("overview.md", b"updated").unwrap();
    assert_eq!(
        fs::read(dir.path().join("saved/knowledge/overview.md")).unwrap(),
        b"updated"
    );
    assert!(!outside.path().join("overview.md").exists());
    let (entries, _) = directory.entries(128).unwrap();
    assert!(!entries.iter().any(|name| name == "outside.md"));
    assert!(root.source(".synapse/outside.md").is_err());
}

#[cfg(unix)]
#[test]
fn special_files_and_symlinked_locks_cannot_block_or_redirect_io() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    let dir = setup();
    write(dir.path(), &request()).unwrap();
    let path = dir.path().join(".synapse/knowledge/pipe.md");
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    assert!(read(dir.path(), "pipe", 0).is_err());
    let lock = dir.path().join(".synapse/knowledge/write.lock");
    fs::remove_file(&lock).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let untouched = outside.path().join("untouched");
    symlink(&untouched, lock).unwrap();
    assert!(write(dir.path(), &request()).is_err());
    assert!(!untouched.exists());
}
