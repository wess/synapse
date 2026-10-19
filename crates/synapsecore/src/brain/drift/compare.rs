//! Lexical comparison of two memories. High word overlap plus a flipped
//! negation, an always/never pair, or a changed number is what an unmarked
//! correction usually looks like. Overlap can't prove two facts are the same,
//! so results are flags for review, not merges.

use std::collections::BTreeSet;

/// Shorter memories overlap with anything on their topic, so they're skipped.
pub const MINWORDS: usize = 4;

/// Minimum shared words for any relation. `drift::candidates` depends on this.
pub const MINSHARED: usize = 4;

/// Overlap (as a share of the shorter memory) for "same subject". Enough for
/// an always/never flip to count.
const SAMESUBJECT: f64 = 0.7;

/// Stricter overlap for weaker signals: a bare negation or a changed number.
const SAMESTATEMENT: f64 = 0.8;

/// Jaccard needed for a duplicate, on top of [`SAMESTATEMENT`]. Keeps a short
/// summary of a long memory, or "tracks owners" vs "tracks alerts", from
/// counting as copies.
const REPEATS: f64 = 0.75;

/// So "three times" vs "five times" is a changed value and "three" equals "3".
const NUMBERWORDS: &[(&str, &str)] = &[
    ("zero", "0"),
    ("one", "1"),
    ("once", "1"),
    ("two", "2"),
    ("twice", "2"),
    ("three", "3"),
    ("four", "4"),
    ("five", "5"),
    ("six", "6"),
    ("seven", "7"),
    ("eight", "8"),
    ("nine", "9"),
    ("ten", "10"),
    ("eleven", "11"),
    ("twelve", "12"),
    ("fifteen", "15"),
    ("twenty", "20"),
    ("thirty", "30"),
    ("forty", "40"),
    ("fifty", "50"),
    ("sixty", "60"),
    ("ninety", "90"),
    ("hundred", "100"),
    ("thousand", "1000"),
];

const NEGATIONS: &[&str] = &[
    "not",
    "never",
    "no",
    "nor",
    "don't",
    "dont",
    "doesn't",
    "doesnt",
    "isn't",
    "isnt",
    "aren't",
    "arent",
    "won't",
    "wont",
    "can't",
    "cant",
    "cannot",
    "shouldn't",
    "shouldnt",
    "mustn't",
    "mustnt",
    "avoid",
    "stop",
    "stopped",
    "instead",
    "without",
    "deprecated",
    "removed",
    "forbidden",
];

const OPPOSITES: &[(&str, &str)] = &[
    ("always", "never"),
    ("use", "avoid"),
    ("enable", "disable"),
    ("enabled", "disabled"),
    ("allow", "deny"),
    ("allowed", "forbidden"),
    ("required", "optional"),
    ("include", "exclude"),
    ("before", "after"),
    ("true", "false"),
    ("public", "private"),
    ("increase", "decrease"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// Lowercased, lightly stemmed content words. Excludes stopwords,
    /// negations, opposites, and numbers.
    words: BTreeSet<String>,
    negated: bool,
    poles: BTreeSet<&'static str>,
    numbers: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relation {
    Duplicate,
    Conflict(Disagreement),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disagreement {
    Opposites(&'static str, &'static str),
    /// Numbers only in the first, numbers only in the second.
    Values(Vec<String>, Vec<String>),
    Negation,
}

pub fn profile(body: &str) -> Profile {
    let mut profile = Profile::default();
    let lowered = body.to_lowercase().replace(['’', '‘'], "'");
    let tokens = lowered
        .split(|character: char| {
            !(character.is_alphanumeric() || matches!(character, '\'' | '.' | '_'))
        })
        .map(|token| token.trim_matches(|character| matches!(character, '\'' | '.')))
        .filter(|token| !token.is_empty());
    for token in tokens {
        if NEGATIONS.contains(&token) {
            profile.negated = true;
        }
        if let Some(pole) = OPPOSITES
            .iter()
            .flat_map(|(first, second)| [*first, *second])
            .find(|pole| *pole == token)
        {
            profile.poles.insert(pole);
            continue;
        }
        if let Some((_, digits)) = NUMBERWORDS.iter().find(|(word, _)| *word == token) {
            profile.numbers.insert((*digits).to_owned());
            continue;
        }
        if token.chars().any(|character| character.is_ascii_digit()) {
            if token
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.')
            {
                profile.numbers.insert(token.to_owned());
            }
            continue;
        }
        let bare = token.replace('\'', "");
        if bare.chars().count() < 3
            || NEGATIONS.contains(&token)
            || crate::brain::store::STOPWORDS.contains(&bare.as_str())
        {
            continue;
        }
        profile.words.insert(stem(&bare));
    }
    profile
}

impl Profile {
    pub fn comparable(&self) -> bool {
        self.words.len() >= MINWORDS
    }

    pub fn words(&self) -> impl Iterator<Item = &String> {
        self.words.iter()
    }

    pub fn size(&self) -> usize {
        self.words.len()
    }
}

/// Symmetric, apart from which side each detail is reported on.
pub fn relate(first: &Profile, second: &Profile) -> Option<Relation> {
    if !first.comparable() || !second.comparable() {
        return None;
    }
    let shared = first.words.intersection(&second.words).count();
    if shared < MINSHARED {
        return None;
    }
    let smaller = first.words.len().min(second.words.len());
    let union = first.words.union(&second.words).count();
    let containment = shared as f64 / smaller as f64;
    let jaccard = shared as f64 / union as f64;
    if containment < SAMESUBJECT {
        return None;
    }
    if let Some(opposites) = opposed(first, second) {
        return Some(Relation::Conflict(opposites));
    }
    if containment < SAMESTATEMENT {
        return None;
    }
    if !first.numbers.is_empty() && !second.numbers.is_empty() && first.numbers != second.numbers {
        return Some(Relation::Conflict(Disagreement::Values(
            first.numbers.difference(&second.numbers).cloned().collect(),
            second.numbers.difference(&first.numbers).cloned().collect(),
        )));
    }
    if first.negated != second.negated {
        return Some(Relation::Conflict(Disagreement::Negation));
    }
    (jaccard >= REPEATS && first.numbers == second.numbers).then_some(Relation::Duplicate)
}

/// One side says a word, the other its opposite, and neither says both.
fn opposed(first: &Profile, second: &Profile) -> Option<Disagreement> {
    OPPOSITES.iter().find_map(|(this, that)| {
        let split = |a: &Profile, b: &Profile, x: &'static str, y: &'static str| {
            a.poles.contains(x)
                && !a.poles.contains(y)
                && b.poles.contains(y)
                && !b.poles.contains(x)
        };
        if split(first, second, this, that) {
            Some(Disagreement::Opposites(this, that))
        } else if split(first, second, that, this) {
            Some(Disagreement::Opposites(that, this))
        } else {
            None
        }
    })
}

/// Crude suffix stripping so `deploys`, `deployed`, `deploying` match. Both
/// sides get the same treatment, which is all a comparison needs.
fn stem(word: &str) -> String {
    let length = word.chars().count();
    let cut = |suffix: &str, keep: usize| -> Option<String> {
        (length > keep && word.ends_with(suffix))
            .then(|| word[..word.len() - suffix.len()].to_owned())
    };
    if let Some(stem) = cut("ies", 4) {
        return format!("{stem}y");
    }
    cut("ing", 5)
        .or_else(|| cut("ed", 4))
        .or_else(|| (!word.ends_with("ss")).then(|| cut("s", 3)).flatten())
        .unwrap_or_else(|| word.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relation(first: &str, second: &str) -> Option<Relation> {
        let forward = relate(&profile(first), &profile(second));
        let backward = relate(&profile(second), &profile(first));
        // Swapping the pair can swap the details, never the kind.
        assert_eq!(
            std::mem::discriminant(&forward),
            std::mem::discriminant(&backward),
            "{first} / {second}"
        );
        forward
    }

    #[test]
    fn a_reworded_copy_is_a_duplicate() {
        assert_eq!(
            relation(
                "Use bun for every JavaScript task: install, test, and scripts.",
                "Use bun for every JavaScript task, including install, tests, and scripts."
            ),
            Some(Relation::Duplicate)
        );
    }

    #[test]
    fn always_and_never_about_the_same_thing_conflict() {
        assert_eq!(
            relation(
                "Always squash commits when merging a pull request into main.",
                "Never squash commits when merging a pull request into main."
            ),
            Some(Relation::Conflict(Disagreement::Opposites(
                "always", "never"
            )))
        );
    }

    #[test]
    fn a_changed_number_is_a_conflict() {
        assert_eq!(
            relation(
                "The production database is Postgres 16 with a read replica.",
                "The production database is Postgres 17 with a read replica."
            ),
            Some(Relation::Conflict(Disagreement::Values(
                vec!["16".into()],
                vec!["17".into()]
            )))
        );
    }

    #[test]
    fn a_negated_statement_conflicts_with_the_plain_one() {
        assert_eq!(
            relation(
                "Integration tests run against the shared staging database every night.",
                "Integration tests do not run against the shared staging database every night."
            ),
            Some(Relation::Conflict(Disagreement::Negation))
        );
    }

    #[test]
    fn before_and_after_turn_a_rule_around() {
        assert_eq!(
            relation(
                "Consumers commit offsets after processing each message batch.",
                "Consumers commit offsets before processing each message batch."
            ),
            Some(Relation::Conflict(Disagreement::Opposites(
                "after", "before"
            )))
        );
    }

    #[test]
    fn related_memories_on_one_topic_are_left_alone() {
        for (first, second) in [
            (
                "Deploys run from the release branch after the tag is signed.",
                "Staging deploys happen automatically on every merge to main.",
            ),
            (
                "Database migrations are hand-written reversible SQL files.",
                "Run database migrations against a copy of production before merging.",
            ),
            (
                "Credentials live in the vault, never in the repository.",
                "Rotate database credentials every ninety days through the vault.",
            ),
            (
                "Unit tests must not touch the network; mock the HTTP client.",
                "Integration tests run against the test database and a local queue.",
            ),
            (
                "The queue retries a failed job three times with backoff.",
                "Flaky tests are quarantined with a tracking issue, never deleted.",
            ),
        ] {
            assert_eq!(relation(first, second), None, "{first} / {second}");
        }
    }

    #[test]
    fn a_short_memory_is_not_compared() {
        assert_eq!(relation("Use bun.", "Never use bun."), None);
        assert!(!profile("Use bun.").comparable());
    }

    #[test]
    fn a_summary_inside_a_longer_memory_is_not_a_copy() {
        assert_eq!(
            relation(
                "Release builds sign packages with the hardware key.",
                "Release builds sign packages with the hardware key, then notarize them, \
                 upload them to the mirror, update the appcast feed, announce the version \
                 in the changelog channel, and tag the commit for reproducible rebuilds."
            ),
            None
        );
    }

    #[test]
    fn the_same_number_twice_is_not_a_change() {
        assert_eq!(
            relation(
                "Rate limits allow 600 requests per minute for each API token.",
                "Rate limits allow 600 requests per minute for each token of the API."
            ),
            Some(Relation::Duplicate)
        );
    }

    #[test]
    fn a_number_written_as_a_word_is_a_value() {
        assert_eq!(
            relation(
                "The queue retries a failed job three times with exponential backoff.",
                "The queue retries a failed job five times with exponential backoff."
            ),
            Some(Relation::Conflict(Disagreement::Values(
                vec!["3".into()],
                vec!["5".into()]
            )))
        );
        assert_eq!(
            relation(
                "The queue retries a failed job three times with exponential backoff.",
                "The queue retries a failed job 3 times with exponential backoff."
            ),
            Some(Relation::Duplicate)
        );
    }

    #[test]
    fn one_different_word_in_a_short_memory_is_a_different_fact() {
        assert_eq!(
            relation(
                "The deploy group tracks owners for the platform.",
                "The deploy group tracks alerts for the platform."
            ),
            None
        );
    }

    #[test]
    fn endings_meet_and_function_words_do_not_count() {
        let profile = profile("The deploys were deployed and deploying the libraries.");
        let words: Vec<&String> = profile.words().collect();
        assert_eq!(words, vec!["deploy", "library"]);
        assert!(!profile.negated);
        assert!(super::profile("We don’t deploy on Fridays").negated);
    }
}
