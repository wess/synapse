//! Who may talk to this log.
//!
//! One shared bearer token, which is the whole of it for now. Per-device
//! keypairs and pairing codes are the design this wants to grow into; until
//! that exists, a token that the server refuses to start without is the
//! difference between "not finished" and "anyone can read your log".
//!
//! The token is only as private as the transport. The envelopes are sealed
//! and stay unreadable over plain HTTP, but the token does not — run this
//! behind TLS.

use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Short enough to type, long enough that guessing is not a strategy.
const MINIMUM: usize = 32;

/// No `Debug` or `Display`: a token in a log line is a token in a bug report.
pub struct Token(String);

impl Token {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        ensure!(
            value.len() >= MINIMUM,
            "the access token must be at least {MINIMUM} characters"
        );
        Ok(Self(value))
    }

    /// Compare in constant time, through a digest so the comparison does not
    /// take a different amount of time for a token of a different length.
    pub fn matches(&self, presented: &str) -> bool {
        let expected = Sha256::digest(self.0.as_bytes());
        let actual = Sha256::digest(presented.as_bytes());
        expected.ct_eq(&actual).into()
    }
}

/// Pull the credential out of an `Authorization: Bearer …` header.
pub fn bearer(header: Option<&str>) -> Option<&str> {
    let value = header?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_token_shorter_than_the_minimum_is_refused() {
        assert!(Token::new("short").is_err());
        assert!(Token::new(GOOD).is_ok());
    }

    #[test]
    fn a_token_matches_only_itself() {
        let token = Token::new(GOOD).unwrap();
        assert!(token.matches(GOOD));
        assert!(!token.matches("0123456789abcdef0123456789abcdeg"));
        assert!(!token.matches(""));
        // A prefix of the real token must not pass.
        assert!(!token.matches("0123456789abcdef"));
    }

    #[test]
    fn the_bearer_scheme_is_read_but_nothing_else_is() {
        assert_eq!(bearer(Some("Bearer abc")), Some("abc"));
        assert_eq!(bearer(Some("bearer abc")), Some("abc"));
        assert_eq!(bearer(Some("Basic abc")), None);
        assert_eq!(bearer(Some("abc")), None);
        assert_eq!(bearer(Some("Bearer ")), None);
        assert_eq!(bearer(None), None);
    }
}
