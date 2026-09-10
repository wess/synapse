//! What a value has to get past before it is stored.
//!
//! A secret is the one thing here nobody can read back, so a mistake made
//! entering one is not found where it was made — it is found later, by whatever
//! the credential was for, as an authentication failure that looks exactly like
//! a wrong password. Both of the ones seen in the wild were entry slips: a
//! nineteen-character app password stored as twenty because the paste carried a
//! trailing space, and, before that, the environment variable's *name* typed
//! where its value belonged.
//!
//! So: surrounding whitespace comes off and is reported, a value that is one of
//! the names standing beside it on the command line is refused, and what was
//! stored is echoed back as a shape. None of that reveals the secret, and all
//! of it moves the discovery to the moment the mistake is made.

use anyhow::Result;

/// A value as it will be stored, and what had to be corrected on the way in.
pub struct Entry {
    pub value: String,
    /// Whitespace was taken off one end or both. Worth saying out loud: it is
    /// invisible in the terminal and it is almost always a paste.
    pub trimmed: bool,
}

/// Written by hand, and it prints the shape. A derived one would put the
/// credential in whatever formatted it — a panic message, a crash log, a
/// `dbg!` somebody left in — which is the one place a secret must never reach.
impl std::fmt::Debug for Entry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Entry")
            .field("value", &shape(&self.value))
            .field("trimmed", &self.trimmed)
            .finish()
    }
}

/// Clean a value up and refuse the ones that are plainly not one.
///
/// `names` are the arguments that stand beside the value when it is entered —
/// the vault, the secret's name, the environment variable. Every one of them is
/// a name, so a value equal to any of them is the argument that got typed one
/// slot late, and storing it only postpones the error.
pub fn entered(raw: &str, names: &[&str]) -> Result<Entry> {
    let value = raw.trim();
    anyhow::ensure!(!value.is_empty(), "secret value cannot be empty");
    if let Some(name) = names
        .iter()
        .find(|name| !name.trim().is_empty() && name.trim().eq_ignore_ascii_case(value))
    {
        anyhow::bail!(
            "that is `{name}`, which is a name and not a value — nothing was stored. \
             The value is asked for last, after the vault, the secret name, and the \
             environment variable."
        );
    }
    Ok(Entry {
        value: value.to_owned(),
        trimmed: value.len() != raw.len(),
    })
}

/// How long a stored value is and how it is punctuated, with every character
/// that carries information replaced.
///
/// Enough to recognise `xxxx-xxxx-xxxx-xxxx` as the right shape, or to see that
/// what went in was one character longer than what was pasted, and not enough
/// to be worth reading over a shoulder.
pub fn shape(value: &str) -> String {
    let masked: String = value
        .chars()
        .take(LIMIT)
        .map(|character| match SEPARATORS.contains(character) {
            true => character,
            false => '•',
        })
        .collect();
    let elision = match value.chars().count() > LIMIT {
        true => "…",
        false => "",
    };
    format!("{} chars · {masked}{elision}", value.chars().count())
}

/// Punctuation kept as itself, because the shape of a credential is most of
/// what makes it recognisable and none of what makes it secret.
const SEPARATORS: &str = "-_.:/@";

/// How much of a long value to draw. A forty-character token is a row of dots
/// either way; the count above it is the part that says anything.
const LIMIT: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_space_comes_off_and_is_reported() {
        let entry = entered("abcd-efgh-ijkl-mnop ", &[]).unwrap();
        assert_eq!(entry.value, "abcd-efgh-ijkl-mnop");
        assert!(entry.trimmed);
        assert!(!entered("abcd-efgh-ijkl-mnop", &[]).unwrap().trimmed);
    }

    #[test]
    fn a_value_that_is_only_whitespace_is_empty() {
        assert!(entered("   \n", &[]).is_err());
        assert!(entered("", &[]).is_err());
    }

    /// The signature puts three name-shaped arguments before the one thing that
    /// is not a name, and this is what happens when the value lands one slot
    /// late. It failed against the service weeks later, indistinguishable from
    /// a wrong password.
    #[test]
    fn the_variable_name_typed_where_the_value_goes_is_refused() {
        let names = ["general", "SolarAppPass", "SOLAR_APP_PASSWORD"];
        for name in names {
            let error = entered(name, &names).unwrap_err().to_string();
            assert!(error.contains("is a name and not a value"), "{error}");
            assert!(error.contains("nothing was stored"), "{error}");
        }
        assert!(entered("hunter2", &names).is_ok());
    }

    /// A real value that happens to match nothing still goes in, and the guard
    /// never sees a name it was not given.
    #[test]
    fn an_empty_name_never_matches() {
        assert!(entered("hunter2", &["", "  "]).is_ok());
    }

    /// Because the one place a secret must never reach is a panic message.
    #[test]
    fn debugging_an_entry_prints_its_shape_and_not_its_value() {
        let printed = format!("{:?}", entered("hunter2", &[]).unwrap());
        assert!(printed.contains("7 chars"), "{printed}");
        assert!(!printed.contains("hunter2"), "{printed}");
    }

    #[test]
    fn a_shape_shows_the_length_and_the_punctuation_and_nothing_else() {
        assert_eq!(
            shape("abcd-efgh-ijkl-mnop"),
            "19 chars · ••••-••••-••••-••••"
        );
        assert_eq!(shape("hunter2"), "7 chars · •••••••");
    }

    /// The one character a paste adds is the whole point of showing a count.
    #[test]
    fn a_trailing_space_changes_the_shape_it_would_have_been_stored_with() {
        assert_ne!(shape("abcd-efgh-ijkl-mnop "), shape("abcd-efgh-ijkl-mnop"));
    }

    #[test]
    fn a_long_token_is_counted_in_full_and_drawn_in_part() {
        let shape = shape(&"x".repeat(64));
        assert!(shape.starts_with("64 chars · "));
        assert!(shape.ends_with('…'));
        assert_eq!(shape.matches('•').count(), LIMIT);
    }
}
