//! Sealing a [`Record`] so that only the devices holding the key can read it.
//!
//! XChaCha20-Poly1305 with a random nonce. The 24-byte nonce is the reason for
//! the X: at that width a random nonce per envelope is safe without a counter,
//! and a counter is exactly the kind of state two devices syncing the same
//! store would get wrong.
//!
//! The layout is `[version:1][nonce:24][ciphertext+tag:..]`. The identifier the
//! envelope will be stored under is authenticated as associated data, so a
//! server cannot move a sealed body to a different identifier without the
//! client noticing.

use crate::record::Record;
use anyhow::{Context, Result, anyhow};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

/// Envelope layout version. Bump only for a change to the bytes themselves;
/// [`crate::wire::PROTOCOL`] covers the request and response shapes.
pub const VERSION: u8 = 1;

const NONCE: usize = 24;
const TAG: usize = 16;

/// The content key. Never derived from anything the server sees, and never
/// sent to it.
///
/// No `Debug`, `Display`, or `Serialize` on purpose: the compiler is a better
/// guard against a key reaching a log line than a review is.
#[derive(Clone)]
pub struct Key([u8; 32]);

impl Key {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// For handing the key to the platform keystore, and nothing else.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Seal a record for storage under `uid`.
pub fn seal(key: &Key, uid: &str, record: &Record) -> Result<Vec<u8>> {
    let plain = serde_json::to_vec(record).context("could not encode the record to seal")?;
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &plain,
                aad: uid.as_bytes(),
            },
        )
        .map_err(|_| anyhow!("could not seal the record"))?;

    let mut out = Vec::with_capacity(1 + NONCE + sealed.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Open an envelope stored under `uid`.
pub fn open(key: &Key, uid: &str, sealed: &[u8]) -> Result<Record> {
    let (&version, rest) = sealed
        .split_first()
        .ok_or_else(|| anyhow!("the envelope is empty"))?;
    anyhow::ensure!(
        version == VERSION,
        "this envelope is version {version}, and this build understands {VERSION}"
    );
    anyhow::ensure!(
        rest.len() > NONCE + TAG,
        "the envelope is too short to open"
    );

    let (nonce, body) = rest.split_at(NONCE);
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let plain = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: body,
                aad: uid.as_bytes(),
            },
        )
        // Either the key is wrong or the bytes were altered, and the caller
        // cannot act differently on the difference.
        .map_err(|_| anyhow!("could not open the envelope with this key"))?;

    serde_json::from_slice(&plain).context("the envelope opened but did not hold a record")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Scope;

    fn key(byte: u8) -> Key {
        Key::new([byte; 32])
    }

    fn record() -> Record {
        Record::Put {
            body: "the mesh stays off until it is switched on".into(),
            source: "session".into(),
            scope: Scope::Global,
            project: String::new(),
            created: 1_700_000_000,
        }
    }

    #[test]
    fn a_sealed_record_opens_again_with_the_same_key() {
        let sealed = seal(&key(7), "abc", &record()).unwrap();
        assert_eq!(open(&key(7), "abc", &sealed).unwrap(), record());
    }

    #[test]
    fn the_plaintext_is_not_recoverable_from_the_envelope() {
        let sealed = seal(&key(7), "abc", &record()).unwrap();
        let haystack = String::from_utf8_lossy(&sealed).to_string();
        assert!(!haystack.contains("the mesh stays off"));
    }

    #[test]
    fn another_key_cannot_open_it() {
        let sealed = seal(&key(7), "abc", &record()).unwrap();
        assert!(open(&key(8), "abc", &sealed).is_err());
    }

    #[test]
    fn an_envelope_moved_to_another_identifier_stops_opening() {
        let sealed = seal(&key(7), "abc", &record()).unwrap();
        assert!(open(&key(7), "xyz", &sealed).is_err());
    }

    #[test]
    fn an_altered_envelope_stops_opening() {
        let mut sealed = seal(&key(7), "abc", &record()).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(open(&key(7), "abc", &sealed).is_err());
    }

    #[test]
    fn two_seals_of_one_record_differ() {
        let first = seal(&key(7), "abc", &record()).unwrap();
        let second = seal(&key(7), "abc", &record()).unwrap();
        assert_ne!(first, second, "a repeated nonce would leak equality");
    }

    #[test]
    fn a_truncated_or_empty_envelope_is_refused_rather_than_panicking() {
        assert!(open(&key(7), "abc", &[]).is_err());
        assert!(open(&key(7), "abc", &[VERSION]).is_err());
        assert!(open(&key(7), "abc", &[VERSION; 8]).is_err());
    }

    #[test]
    fn an_envelope_from_a_later_layout_is_refused_by_version() {
        let mut sealed = seal(&key(7), "abc", &record()).unwrap();
        sealed[0] = VERSION + 1;
        let message = open(&key(7), "abc", &sealed).unwrap_err().to_string();
        assert!(message.contains("version"), "got: {message}");
    }
}
