//! The key the encrypted store is sealed with, and the bargain it makes.
//!
//! It sits in a file beside the store, owner-only. That protects a vault that
//! has been *copied* — a backup, a synced folder, a disk image, a laptop
//! somebody else now has — and it does not protect a process already running as
//! you, which can read the key exactly the way Synapse reads it. `sync.rs`
//! makes the same argument about the sync key, and the honest version of it
//! here is that this is encryption at rest, not an access control. Keychain is
//! the stronger bargain on macOS and is one setting away.
//!
//! The file names its version so that a passphrase-wrapped key can be written
//! in the same place later without every existing vault becoming unreadable.

use anyhow::{Context, Result};
use chacha20poly1305::aead::OsRng;
use chacha20poly1305::aead::rand_core::RngCore;
use std::path::PathBuf;

const TAG: &str = "synapsevaultkey";
const VERSION: &str = "v1";

/// The content key.
///
/// No `Debug`, `Display`, or `Serialize` on purpose: the compiler is a better
/// guard against a key reaching a log line than a review is.
#[derive(Clone)]
pub struct Key([u8; 32]);

impl Key {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub fn path() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("vault.key"))
}

/// Read the key, creating one on first use.
///
/// A second key is never written over the first: every value already sealed
/// would stop opening, and the only thing left to say so would be a decryption
/// failure that reads exactly like a corrupted file.
pub fn load() -> Result<Key> {
    let path = path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            parse(&content).with_context(|| format!("could not read {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create(),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

fn create() -> Result<Key> {
    let path = path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
        crate::database::securedirectory(parent)?;
    }

    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let content = format!("{TAG} {VERSION} {}\n", encode(&bytes));

    // Created owner-only and refusing to clobber, in one syscall each: a file
    // that is briefly world-readable is a key that briefly leaked, and two
    // processes reaching first use together must not both mint a key.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(content.as_bytes())
                .with_context(|| format!("could not write {}", path.display()))?;
            Ok(Key::new(bytes))
        }
        // Somebody else won the race. Theirs is the key the values are under.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => load(),
        Err(error) => Err(error).with_context(|| format!("could not create {}", path.display())),
    }
}

fn parse(content: &str) -> Result<Key> {
    let mut fields = content.split_whitespace();
    anyhow::ensure!(
        fields.next() == Some(TAG),
        "this does not look like a Synapse vault key"
    );
    let version = fields.next().unwrap_or_default();
    anyhow::ensure!(
        version == VERSION,
        "this vault key is {version}, and this build understands {VERSION}"
    );
    let bytes = decode(fields.next().unwrap_or_default())
        .context("the vault key is not 32 bytes of hex")?;
    Ok(Key::new(bytes))
}

fn encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode(value: &str) -> Result<[u8; 32]> {
    anyhow::ensure!(value.len() == 64, "expected 64 hex characters");
    let mut bytes = [0u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("expected hex characters")?;
    }
    Ok(bytes)
}

#[cfg(test)]
#[allow(
    clippy::await_holding_lock,
    reason = "the guard serialises tests over one process-wide SYNAPSE_DATA; \
              holding it across the await is the point"
)]
mod tests {
    use super::*;

    #[test]
    fn a_key_round_trips_through_its_file_format() {
        let key = Key::new([9u8; 32]);
        let written = format!("{TAG} {VERSION} {}\n", encode(key.bytes()));
        assert_eq!(parse(&written).unwrap().bytes(), key.bytes());
    }

    #[test]
    fn anything_that_is_not_a_key_file_is_refused_by_name_and_by_version() {
        assert!(parse("").is_err());
        assert!(parse("hunter2").is_err());
        assert!(parse(&format!("{TAG} v2 {}", encode(&[0u8; 32]))).is_err());
        assert!(parse(&format!("{TAG} {VERSION} abcd")).is_err());
    }

    #[test]
    fn the_key_is_created_once_and_then_read_back() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());
        let first = load().unwrap();
        let second = load().unwrap();
        assert_eq!(
            first.bytes(),
            second.bytes(),
            "a second key would orphan every value"
        );
        assert!(path().unwrap().is_file());
    }

    #[cfg(unix)]
    #[test]
    fn the_key_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());
        load().unwrap();
        let mode = std::fs::metadata(path().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
