//! Sealing one secret value.
//!
//! XChaCha20-Poly1305 with a random 24-byte nonce, and the account name
//! authenticated as associated data: a row copied to another name in the same
//! file stops opening, so the envelope and the label cannot be recombined.
//!
//! The layout is `[version:1][nonce:24][ciphertext+tag:..]`, which is the shape
//! `synapsesync` uses for a memory. Two copies of it rather than one shared
//! one, deliberately: `synapsesync` is what a client and a server must agree
//! on, and a vault that never leaves this machine is not part of that
//! agreement — a change made there for the wire must not be able to make every
//! local secret unreadable.

use crate::vault::key::Key;
use anyhow::{Result, anyhow};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

/// Bump only for a change to the bytes themselves.
pub const VERSION: u8 = 1;

const NONCE: usize = 24;
const TAG: usize = 16;

pub fn seal(key: &Key, account: &str, value: &str) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: value.as_bytes(),
                aad: account.as_bytes(),
            },
        )
        .map_err(|_| anyhow!("could not seal the secret"))?;

    let mut out = Vec::with_capacity(1 + NONCE + sealed.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

pub fn open(key: &Key, account: &str, sealed: &[u8]) -> Result<String> {
    let (&version, rest) = sealed
        .split_first()
        .ok_or_else(|| anyhow!("the stored secret is empty"))?;
    anyhow::ensure!(
        version == VERSION,
        "this secret is version {version}, and this build understands {VERSION}"
    );
    anyhow::ensure!(
        rest.len() > NONCE + TAG,
        "the stored secret is too short to open"
    );

    let (nonce, body) = rest.split_at(NONCE);
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let plain = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: body,
                aad: account.as_bytes(),
            },
        )
        // Either the key is wrong or the bytes were altered, and there is
        // nothing the caller could do differently with the difference.
        .map_err(|_| anyhow!("could not open this secret with the vault key"))?;

    String::from_utf8(plain).map_err(|_| anyhow!("the stored secret is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> Key {
        Key::new([byte; 32])
    }

    #[test]
    fn a_sealed_value_opens_again_with_the_same_key() {
        let sealed = seal(&key(7), "work.token", "hunter2").unwrap();
        assert_eq!(open(&key(7), "work.token", &sealed).unwrap(), "hunter2");
    }

    #[test]
    fn the_value_is_not_recoverable_from_the_envelope() {
        let sealed = seal(&key(7), "work.token", "hunter2").unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("hunter2"));
    }

    #[test]
    fn another_key_cannot_open_it() {
        let sealed = seal(&key(7), "work.token", "hunter2").unwrap();
        assert!(open(&key(8), "work.token", &sealed).is_err());
    }

    #[test]
    fn a_value_moved_to_another_account_stops_opening() {
        let sealed = seal(&key(7), "work.token", "hunter2").unwrap();
        assert!(open(&key(7), "home.token", &sealed).is_err());
    }

    #[test]
    fn an_altered_envelope_stops_opening() {
        let mut sealed = seal(&key(7), "work.token", "hunter2").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(open(&key(7), "work.token", &sealed).is_err());
    }

    #[test]
    fn two_seals_of_one_value_differ() {
        let first = seal(&key(7), "work.token", "hunter2").unwrap();
        let second = seal(&key(7), "work.token", "hunter2").unwrap();
        assert_ne!(first, second, "a repeated nonce would leak equality");
    }

    #[test]
    fn a_truncated_or_empty_envelope_is_refused_rather_than_panicking() {
        assert!(open(&key(7), "work.token", &[]).is_err());
        assert!(open(&key(7), "work.token", &[VERSION]).is_err());
        assert!(open(&key(7), "work.token", &[VERSION; 8]).is_err());
    }
}
