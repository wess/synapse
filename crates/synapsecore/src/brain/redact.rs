//! Keeping credentials out of memory.
//!
//! Guidance tells every agent not to store secrets, and an agent writing down
//! "the deploy needs `STRIPE_KEY=sk_live_…`" is following the spirit of a
//! durable fact while breaking that rule. A memory is recalled into every later
//! session, printed in the desktop app, and sealed onto every machine the store
//! syncs with, so a credential that gets in is one that has leaked to all of
//! them. The vault is where values go; this makes sure memory does not become a
//! second, unguarded one.
//!
//! What is removed is replaced with a marker rather than dropped, so the
//! memory still reads as the sentence it was — "set the token from the vault"
//! with the value gone, not a sentence with a hole in it.

/// What redaction left, and how many things it took out.
#[derive(Debug, PartialEq, Eq)]
pub struct Redacted {
    pub body: String,
    pub count: usize,
}

const MARKER: &str = "[redacted]";

/// Takes credentials and `<private>` sections out of `body`.
pub fn redact(body: &str) -> Redacted {
    let mut count = 0;
    let body = private(body, &mut count);
    let body = pem(&body, &mut count);
    let body = words(&body, &mut count);
    Redacted { body, count }
}

/// `<private>…</private>`, removed with what it holds. An unclosed tag runs to
/// the end: a person who opened one meant everything after it.
fn private(body: &str, count: &mut usize) -> String {
    const OPEN: &str = "<private>";
    const CLOSE: &str = "</private>";
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = find(rest, OPEN) {
        out.push_str(&rest[..start]);
        *count += 1;
        let after = &rest[start + OPEN.len()..];
        rest = match find(after, CLOSE) {
            Some(end) => &after[end + CLOSE.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// A PEM private key block, whole.
fn pem(body: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("-----BEGIN") {
        let header = &rest[start..];
        let Some(close) = header["-----BEGIN".len()..].find("-----") else {
            break;
        };
        let label = &header[.."-----BEGIN".len() + close];
        if !label.contains("PRIVATE KEY") {
            out.push_str(&rest[..start + "-----BEGIN".len()]);
            rest = &rest[start + "-----BEGIN".len()..];
            continue;
        }
        out.push_str(&rest[..start]);
        out.push_str("[redacted private key]");
        *count += 1;
        rest = match header.find("-----END") {
            Some(end) => {
                let tail = &header[end + "-----END".len()..];
                match tail.find("-----") {
                    Some(close) => &tail[close + "-----".len()..],
                    None => "",
                }
            }
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Every whitespace-separated word, checked for the shapes a credential takes.
/// Whitespace is kept exactly, so the memory's layout survives.
fn words(body: &str, count: &mut usize) -> String {
    let mut out = String::with_capacity(body.len());
    let mut bearer = false;
    for piece in body.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end_matches(char::is_whitespace);
        let space = &piece[word.len()..];
        let lead = word.len() - word.trim_start_matches(QUOTES).len();
        let value = token(&word[lead..]);
        let cleaned = if bearer && value.len() >= 20 {
            *count += 1;
            format!("{}{MARKER}{}", &word[..lead], &word[lead + value.len()..])
        } else {
            word_redacted(word, count)
        };
        bearer = word.eq_ignore_ascii_case("bearer");
        out.push_str(&cleaned);
        out.push_str(space);
    }
    out
}

const QUOTES: &[char] = &[
    '"', '\'', '`', ',', ';', '(', ')', '[', ']', '{', '}', '<', '>',
];

/// One word: an assignment to a secret-sounding name, a URL carrying a
/// password, or a token with a provider's well-known prefix.
fn word_redacted(word: &str, count: &mut usize) -> String {
    if let Some(cleaned) = assignment(word) {
        *count += 1;
        return cleaned;
    }
    if let Some(cleaned) = userinfo(word) {
        *count += 1;
        return cleaned;
    }
    // A known token can sit inside punctuation or a longer string, so look at
    // each run of token characters rather than the word as a whole.
    let mut out = String::with_capacity(word.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String, count: &mut usize| {
        if known(run) {
            *count += 1;
            out.push_str(MARKER);
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for character in word.chars() {
        if tokenchar(character) {
            run.push(character);
        } else {
            flush(&mut run, &mut out, count);
            out.push(character);
        }
    }
    flush(&mut run, &mut out, count);
    out
}

/// `NAME=value` where the name says it holds a secret. The name stays, so the
/// memory still says which variable matters; only the value goes.
fn assignment(word: &str) -> Option<String> {
    let (name, value) = word.split_once('=')?;
    let bare = name
        .trim_start_matches(['-', '"', '\'', '`'])
        .trim_matches(['"', '\'']);
    if bare.is_empty()
        || !bare
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return None;
    }
    if !sensitive(bare) || placeholder(value) {
        return None;
    }
    let quote: String = value.chars().take_while(|c| QUOTES.contains(c)).collect();
    let close: String = value
        .chars()
        .rev()
        .take_while(|c| QUOTES.contains(c))
        .collect();
    Some(format!("{name}={quote}{MARKER}{close}"))
}

/// Whether a variable name says it holds a credential.
fn sensitive(name: &str) -> bool {
    let upper = name.to_ascii_uppercase().replace(['-', '.'], "_");
    [
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "PRIVATE_KEY",
        "APIKEY",
    ]
    .iter()
    .any(|part| upper.contains(part))
        || upper.ends_with("_KEY")
        || upper == "KEY"
        || upper.ends_with("_PWD")
        || upper == "PWD"
}

/// A value that only names where a secret comes from: a variable, a command,
/// a template slot, an ellipsis. Those are exactly what a memory should say.
fn placeholder(value: &str) -> bool {
    let value = value.trim_matches(QUOTES);
    value.chars().count() < 6
        || value.starts_with(['$', '<', '{', '%'])
        || value.contains("...")
        || value.contains('…')
        || value.chars().all(|c| c == '*' || c == 'x' || c == 'X')
        || [
            "true", "false", "null", "none", "redacted", "changeme", "example",
        ]
        .iter()
        .any(|word| value.eq_ignore_ascii_case(word))
}

/// `scheme://user:password@host`, with the password removed.
fn userinfo(word: &str) -> Option<String> {
    let scheme = word.find("://")? + 3;
    let rest = &word[scheme..];
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let at = authority.rfind('@')?;
    let colon = authority[..at].find(':')?;
    let password = &authority[colon + 1..at];
    if password.is_empty() || placeholder(password) {
        return None;
    }
    Some(format!(
        "{}{}{MARKER}{}",
        &word[..scheme],
        &authority[..=colon],
        &word[scheme + at..]
    ))
}

fn tokenchar(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

/// The longest leading run of token characters.
fn token(word: &str) -> &str {
    let end = word.find(|c: char| !tokenchar(c)).unwrap_or(word.len());
    &word[..end]
}

/// A token in one of the formats providers issue, recognised by its prefix
/// and length. A prefix alone would catch `sk-learn` and `ghp_` in prose.
fn known(run: &str) -> bool {
    let after = |prefix: &str, least: usize| {
        run.strip_prefix(prefix)
            .is_some_and(|rest| rest.len() >= least)
    };
    after("sk-", 20)
        || after("sk_live_", 16)
        || after("rk_live_", 16)
        || ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"]
            .iter()
            .any(|prefix| after(prefix, 30))
        || after("github_pat_", 30)
        || after("glpat-", 20)
        || ["xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-"]
            .iter()
            .any(|prefix| after(prefix, 10))
        || after("npm_", 30)
        || (run.len() == 20
            && run.starts_with("AKIA")
            && run
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        || (run.len() == 39 && run.starts_with("AIza"))
}

/// Case-insensitive search for an ASCII tag.
fn find(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(body: &str) -> String {
        redact(body).body
    }

    #[test]
    fn an_ordinary_memory_is_left_exactly_as_it_was() {
        let body = "Use bun, not npm.\n\n  Deploys run from main; tokens live in the vault.";
        assert_eq!(
            redact(body),
            Redacted {
                body: body.to_owned(),
                count: 0
            }
        );
    }

    #[test]
    fn a_secret_assignment_keeps_its_name_and_loses_its_value() {
        assert_eq!(
            clean("Set STRIPE_SECRET_KEY=sk_test_51Habcdefghijkl before running."),
            "Set STRIPE_SECRET_KEY=[redacted] before running."
        );
        assert_eq!(
            clean("export GITHUB_TOKEN=\"abc123def456\""),
            "export GITHUB_TOKEN=\"[redacted]\""
        );
        assert_eq!(clean("--password=hunter2hunter2"), "--password=[redacted]");
    }

    /// Saying where a value comes from is the memory worth keeping.
    #[test]
    fn a_reference_to_a_secret_is_not_a_secret() {
        for body in [
            "GITHUB_TOKEN=$(gh auth token)",
            "API_KEY=<your key>",
            "TOKEN=${{ secrets.TOKEN }}",
            "DB_PASSWORD=...",
            "DEBUG_TOKEN=false",
            "PRIMARY=postgres",
        ] {
            assert_eq!(clean(body), body, "{body}");
        }
    }

    #[test]
    fn provider_tokens_are_found_wherever_they_sit() {
        let body = "Key was sk-ant-api03-abcdefghijklmnopqrstuvwx, and (ghp_abcdefghijklmnopqrstuvwxyz0123456789).";
        assert_eq!(
            redact(body),
            Redacted {
                body: "Key was [redacted], and ([redacted]).".to_owned(),
                count: 2
            }
        );
        assert_eq!(clean("AKIAIOSFODNN7EXAMPLE"), "[redacted]");
        assert_eq!(
            clean("We use sk-learn and ghp_ prefixes"),
            "We use sk-learn and ghp_ prefixes"
        );
    }

    #[test]
    fn a_password_in_a_url_is_removed_and_the_rest_kept() {
        assert_eq!(
            clean("DATABASE=postgres://app:s3cretpassw0rd@db.internal:5432/app"),
            "DATABASE=postgres://app:[redacted]@db.internal:5432/app"
        );
        assert_eq!(
            clean("https://user@github.com/x"),
            "https://user@github.com/x"
        );
    }

    #[test]
    fn a_bearer_token_is_removed() {
        assert_eq!(
            clean("curl -H 'Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345'"),
            "curl -H 'Authorization: Bearer [redacted]'"
        );
    }

    #[test]
    fn private_sections_go_with_their_content() {
        assert_eq!(
            redact("Ship Friday. <private>my salary</private> Done.\n<PRIVATE>rest"),
            Redacted {
                body: "Ship Friday.  Done.\n".to_owned(),
                count: 2
            }
        );
    }

    #[test]
    fn a_private_key_block_is_removed_whole() {
        let body = "Key:\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----\nend";
        assert_eq!(clean(body), "Key:\n[redacted private key]\nend");
        let public = "-----BEGIN PUBLIC KEY-----\nMIIB\n-----END PUBLIC KEY-----";
        assert_eq!(clean(public), public);
    }
}
