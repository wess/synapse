//! Ranking a query against two indexes and fusing the answers.
//!
//! `memory` is tokenized by word and stemmed, so "deploying" finds "deploys".
//! `memorygram` is tokenized by three-character pieces, so "hashpw" finds
//! `scripts/hashpw.sh` and a word the stemmer has never seen still matches
//! its own fragments. Neither is right on its own: words miss fragments, and
//! fragments rank "overwhelm" for "helm".
//!
//! Each index ranks the query on its own and the two lists are merged by
//! reciprocal rank: a memory scores `1 / (K + position)` in each list it
//! appears in. A memory both indexes like rises above one only a single index
//! likes, and the raw scores of two differently built indexes never have to be
//! compared with each other, which is the part that cannot be done honestly.
//!
//! A memory's `source` — the topic it was filed under — is indexed too, at
//! well under the weight of its body, so a topic word helps a memory without
//! letting its label outrank what another memory actually says.
//!
//! Terms are OR'd, so a memory sharing one word with a long question still
//! matches. For recall, [`floor`] drops weak matches before the limit.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

/// Dampens how much the top of either list dominates. Sixty is the value the
/// method was published with, and the one nearly everything since has kept.
const K: f64 = 60.0;

/// `bm25` weights for (body, source). The third column is not indexed.
const WEIGHTS: &str = "1.0, 0.3";

/// How many candidates each index contributes per result asked for. Fusion
/// can only reorder what it is given, so each list has to reach past the
/// final limit for a memory ranked low by one index to be rescued by the
/// other.
const DEPTH: usize = 4;

/// Marks around matched text in an excerpt. Control characters, because they
/// never occur in a memory and cannot be confused with its own punctuation.
pub(super) const OPEN: char = '\u{2}';
pub(super) const CLOSE: char = '\u{3}';

/// Which memories a ranking may return.
#[derive(Clone, Copy)]
pub(super) enum Reach<'a> {
    /// What a session in this project can see: global memory, this project's,
    /// and nothing superseded.
    Visible(&'a str),
    /// Every stored memory, for a person browsing their own store.
    Everything,
}

/// Recall gets the [`floor`]; browsing sees every match.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Purpose {
    Recall,
    Browse,
}

/// One ranked memory.
#[derive(Debug, Clone)]
pub(super) struct Hit {
    pub id: i64,
    /// The fused score. Higher is better.
    pub score: f64,
    /// The part of the body around what matched, with the matched text marked
    /// by `OPEN` and `CLOSE`.
    pub excerpt: Option<String>,
}

/// The two expressions a query becomes, or `None` for a list when it has
/// nothing to search for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Expressions {
    pub words: Option<String>,
    pub fragments: Option<String>,
}

/// Quotes a term as an FTS5 phrase so punctuation isn't parsed as syntax.
fn quoted(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Builds both expressions from the terms a query kept.
///
/// A fragment search needs at least three characters, because that is the
/// smallest piece the trigram index holds; a shorter term would match
/// nothing there and is left to the word index.
pub(super) fn expressions(terms: &[String]) -> Expressions {
    let join = |items: Vec<String>| (!items.is_empty()).then(|| items.join(" OR "));
    Expressions {
        words: join(terms.iter().map(|term| quoted(term)).collect()),
        fragments: join(
            terms
                .iter()
                .filter(|term| fragmentable(term))
                .map(|term| quoted(term))
                .collect(),
        ),
    }
}

fn fragmentable(term: &str) -> bool {
    term.chars().count() >= 3
}

pub(super) async fn rank(
    pool: &SqlitePool,
    terms: &[String],
    limit: usize,
    reach: Reach<'_>,
    purpose: Purpose,
) -> Result<Vec<Hit>> {
    let expressions = expressions(terms);
    let depth = (limit * DEPTH).max(20);
    let words = match &expressions.words {
        Some(expression) => list(pool, "memory", expression, depth, reach).await?,
        None => Vec::new(),
    };
    let fragments = match &expressions.fragments {
        Some(expression) => list(pool, "memorygram", expression, depth, reach).await?,
        None => Vec::new(),
    };
    let fused = fuse(&[words, fragments], usize::MAX);
    Ok(match purpose {
        Purpose::Recall => {
            let held = coverage(pool, terms, &fused).await?;
            select(fused, &held, limit)
        }
        Purpose::Browse => fused.into_iter().take(limit).collect(),
    })
}

/// Floor before limit, so a dropped hit makes room for a stronger one below.
fn select(fused: Vec<Hit>, held: &HashMap<i64, usize>, limit: usize) -> Vec<Hit> {
    floor(fused, held).into_iter().take(limit).collect()
}

/// How many of `terms` each hit matches in either index.
///
/// Looks up each candidate by rowid. FTS5 seeks directly on `rowid = ?` but
/// reads the term's whole doclist before filtering an `IN` list, which for a
/// common word is the entire store.
async fn coverage(
    pool: &SqlitePool,
    terms: &[String],
    hits: &[Hit],
) -> Result<HashMap<i64, usize>> {
    let mut held: HashMap<i64, usize> = HashMap::new();
    // Nothing to compare with a single term.
    if terms.len() < 2 || hits.is_empty() {
        return Ok(held);
    }
    let distinct: HashSet<&String> = terms.iter().collect();
    for term in distinct {
        let tables: &[&str] = match fragmentable(term) {
            true => &["memory", "memorygram"],
            false => &["memory"],
        };
        let mut holding: HashSet<i64> = HashSet::new();
        for table in tables {
            let sql = format!("SELECT rowid FROM {table} WHERE {table} MATCH ? AND rowid = ?");
            let unplaced: Vec<i64> = hits
                .iter()
                .map(|hit| hit.id)
                .filter(|id| !holding.contains(id))
                .collect();
            for id in unplaced {
                let found = sqlx::query_scalar::<_, i64>(&sql)
                    .bind(quoted(term))
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .with_context(|| format!("could not check {table} for a query term"))?;
                holding.extend(found);
            }
        }
        for id in holding {
            *held.entry(id).or_default() += 1;
        }
    }
    Ok(held)
}

/// Drops hits matching fewer than half as many query terms as the best hit.
///
/// Relative, so the best match always survives even if it's weak. Has no effect
/// until the best hit matches three terms.
pub(super) fn floor(hits: Vec<Hit>, held: &HashMap<i64, usize>) -> Vec<Hit> {
    let best = hits
        .iter()
        .map(|hit| held.get(&hit.id).copied().unwrap_or(0))
        .max()
        .unwrap_or(0);
    let needed = best.div_ceil(2);
    hits.into_iter()
        .filter(|hit| held.get(&hit.id).copied().unwrap_or(0) >= needed)
        .collect()
}

/// One index's ranking: ids with an excerpt, best first.
async fn list(
    pool: &SqlitePool,
    table: &str,
    expression: &str,
    depth: usize,
    reach: Reach<'_>,
) -> Result<Vec<(i64, Option<String>)>> {
    let filter = match reach {
        Reach::Visible(_) => format!("AND {}", super::store::VISIBLE),
        Reach::Everything => String::new(),
    };
    let sql = format!(
        "SELECT {table}.rowid, snippet({table}, 0, char(2), char(3), '…', 32) \
         FROM {table} JOIN memorymeta meta ON meta.memoryid = {table}.rowid \
         WHERE {table} MATCH ? {filter} \
         ORDER BY bm25({table}, {WEIGHTS}) LIMIT ?"
    );
    let mut query = sqlx::query_as::<_, (i64, Option<String>)>(&sql).bind(expression);
    if let Reach::Visible(project) = reach {
        query = query.bind(project);
    }
    query
        .bind(depth as i64)
        .fetch_all(pool)
        .await
        .with_context(|| format!("could not search {table}"))
}

/// Reciprocal rank fusion. Ties go to whichever came first in the earlier
/// list, so the word index — the one a person can predict — breaks them.
fn fuse(lists: &[Vec<(i64, Option<String>)>], limit: usize) -> Vec<Hit> {
    // Each hit with the order it was first seen in, for breaking ties.
    let mut hits: HashMap<i64, (Hit, usize)> = HashMap::new();
    for list in lists {
        for (position, (id, excerpt)) in list.iter().enumerate() {
            let seen = hits.len();
            let (hit, _) = hits.entry(*id).or_insert_with(|| {
                let hit = Hit {
                    id: *id,
                    score: 0.0,
                    excerpt: None,
                };
                (hit, seen)
            });
            hit.score += 1.0 / (K + (position + 1) as f64);
            // The fragment index holds no text, so it has no excerpt to give;
            // a later list can supply one an earlier one could not.
            if hit.excerpt.is_none() {
                hit.excerpt = excerpt.clone();
            }
        }
    }
    let mut hits: Vec<(Hit, usize)> = hits.into_values().collect();
    hits.sort_by(|(a, first), (b, second)| b.score.total_cmp(&a.score).then(first.cmp(second)));
    hits.into_iter().take(limit).map(|(hit, _)| hit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(ids: &[i64]) -> Vec<(i64, Option<String>)> {
        ids.iter().map(|id| (*id, None)).collect()
    }

    #[test]
    fn a_memory_both_indexes_like_beats_one_either_ranks_first() {
        let fused = fuse(&[list(&[1, 2, 3]), list(&[4, 2, 5])], 5);
        let ids: Vec<i64> = fused.iter().map(|hit| hit.id).collect();
        assert_eq!(ids[0], 2);
        assert_eq!(&ids[1..3], &[1, 4], "a tie goes to the word index");
    }

    #[test]
    fn the_limit_is_applied_after_fusion() {
        assert_eq!(fuse(&[list(&[1, 2, 3]), list(&[3, 4])], 2).len(), 2);
    }

    fn hits(ids: &[i64]) -> Vec<Hit> {
        ids.iter()
            .map(|id| Hit {
                id: *id,
                score: 0.0,
                excerpt: None,
            })
            .collect()
    }

    fn ids(hits: &[Hit]) -> Vec<i64> {
        hits.iter().map(|hit| hit.id).collect()
    }

    #[test]
    fn the_floor_is_half_of_what_the_best_hit_holds() {
        let held = HashMap::from([(1, 3), (2, 1), (3, 2), (4, 1), (5, 3)]);
        assert_eq!(ids(&floor(hits(&[1, 2, 3, 4, 5]), &held)), vec![1, 3, 5]);

        let held = HashMap::from([(1, 5), (2, 3), (3, 2), (4, 4)]);
        assert_eq!(
            ids(&floor(hits(&[2, 1, 3, 4]), &held)),
            vec![2, 1, 4],
            "five terms need three, and order is kept"
        );
    }

    #[test]
    fn the_floor_never_cuts_below_what_the_best_hit_reaches() {
        let held = HashMap::from([(1, 2), (2, 1), (3, 1)]);
        assert_eq!(ids(&floor(hits(&[1, 2, 3]), &held)), vec![1, 2, 3]);
        let held = HashMap::from([(1, 1), (2, 1)]);
        assert_eq!(ids(&floor(hits(&[1, 2]), &held)), vec![1, 2]);
        // Single-term queries aren't counted.
        assert_eq!(ids(&floor(hits(&[1, 2]), &HashMap::new())), vec![1, 2]);
        assert!(floor(Vec::new(), &HashMap::new()).is_empty());
    }

    #[test]
    fn the_floor_never_drops_the_best_hit() {
        for best in 1..=8 {
            let held = HashMap::from([(1, best), (2, 1)]);
            let kept = floor(hits(&[2, 1]), &held);
            assert!(ids(&kept).contains(&1), "best holding {best}");
        }
    }

    #[test]
    fn the_floor_is_applied_before_the_limit() {
        let held = HashMap::from([(1, 1), (2, 1), (3, 2), (4, 3)]);
        assert_eq!(ids(&select(hits(&[1, 2, 3, 4]), &held, 2)), vec![3, 4]);
        let cut_late: Vec<i64> = ids(&floor(
            hits(&[1, 2, 3, 4]).into_iter().take(2).collect(),
            &held,
        ));
        assert_eq!(cut_late, vec![1, 2], "the order this guards against");
    }

    #[test]
    fn short_terms_are_searched_as_words_only() {
        let terms = vec!["ci".to_owned(), "deploy".to_owned()];
        assert_eq!(
            expressions(&terms),
            Expressions {
                words: Some("\"ci\" OR \"deploy\"".to_owned()),
                fragments: Some("\"deploy\"".to_owned()),
            }
        );
        assert_eq!(expressions(&[]), Expressions::default());
    }
}

#[cfg(test)]
mod search {
    use crate::brain::{Brain, MemoryScope::Global};

    async fn brain(bodies: &[(&str, &str)]) -> (tempfile::TempDir, Brain) {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        for (body, source) in bodies {
            brain
                .rememberscoped(body, Some(source), Global, None)
                .await
                .unwrap();
        }
        (directory, brain)
    }

    async fn recalled(brain: &Brain, query: &str, limit: u32) -> Vec<String> {
        brain
            .recallscoped(query, limit, Some(crate::brain::Optimization::Full), None)
            .await
            .unwrap()
            .1
            .into_iter()
            .map(|memory| memory.body)
            .collect()
    }

    #[tokio::test]
    async fn memories_sharing_one_word_with_a_question_are_not_handed_over() {
        let (_directory, brain) = brain(&[
            (
                "Kafka credentials rotate weekly through the vault rotate command.",
                "ops",
            ),
            (
                "Kafka topics are named after the service that owns them.",
                "kafka",
            ),
            (
                "Kafka consumers commit offsets after processing, never before.",
                "kafka",
            ),
            (
                "Database credentials are read from the environment at startup.",
                "db",
            ),
            (
                "Credentials never go in a commit, even for a test account.",
                "security",
            ),
            ("Rotate the log files daily on the build machine.", "ops"),
        ])
        .await;
        let found = recalled(&brain, "rotate kafka credentials", 8).await;
        assert_eq!(
            found,
            vec!["Kafka credentials rotate weekly through the vault rotate command."]
        );
        // Browsing still sees everything.
        assert_eq!(
            brain
                .search("rotate kafka credentials", 8)
                .await
                .unwrap()
                .len(),
            6
        );
    }

    #[tokio::test]
    async fn a_question_nothing_answers_well_still_gets_its_best_matches() {
        let (_directory, brain) = brain(&[
            (
                "Kafka topics are named after the service that owns them.",
                "kafka",
            ),
            ("Credentials never go in a commit.", "security"),
        ])
        .await;
        let found = recalled(&brain, "rotate kafka credentials", 8).await;
        assert_eq!(
            found.len(),
            2,
            "one term each, and one is the best there is"
        );
    }

    async fn first(brain: &Brain, query: &str) -> Option<String> {
        brain
            .recallscoped(query, 5, None, None)
            .await
            .unwrap()
            .1
            .into_iter()
            .next()
            .map(|memory| memory.body)
    }

    #[tokio::test]
    async fn a_word_is_found_in_another_form() {
        let (_directory, brain) = brain(&[
            ("Deploys run from the release branch.", "ops"),
            ("Use bun for every script.", "tooling"),
        ])
        .await;
        assert_eq!(
            first(&brain, "deploying").await.as_deref(),
            Some("Deploys run from the release branch.")
        );
    }

    #[tokio::test]
    async fn a_fragment_of_an_identifier_is_found() {
        let (_directory, brain) = brain(&[
            (
                "Admin hashes come from scripts/hashpw.sh, never argon2.",
                "auth",
            ),
            ("Use bun for every script.", "tooling"),
        ])
        .await;
        assert_eq!(
            first(&brain, "hashpw").await.as_deref(),
            Some("Admin hashes come from scripts/hashpw.sh, never argon2.")
        );
        assert!(
            first(&brain, "pw.s").await.is_some(),
            "a piece inside a word"
        );
    }

    /// A topic word finds what was filed under it, but loses to a memory that
    /// says the word in its own text.
    #[tokio::test]
    async fn the_source_is_searched_at_a_lower_weight_than_the_body() {
        let (_directory, brain) = brain(&[
            ("Prefer short status updates.", "tooling"),
            (
                "Tooling: every repository pins its bun version.",
                "decision",
            ),
        ])
        .await;
        let (_, recalled) = brain.recallscoped("tooling", 5, None, None).await.unwrap();
        let bodies: Vec<&str> = recalled.iter().map(|memory| memory.body.as_str()).collect();
        assert_eq!(
            bodies,
            vec![
                "Tooling: every repository pins its bun version.",
                "Prefer short status updates."
            ]
        );
    }

    #[tokio::test]
    async fn explain_shows_both_expressions_and_a_score_higher_for_the_better_match() {
        let (_directory, brain) = brain(&[
            ("Deploys run from the release branch after signing.", "ops"),
            ("The release notes live in docs.", "docs"),
        ])
        .await;
        let explanation = brain.explain("deploy release", 10).await.unwrap();
        assert_eq!(
            explanation.expression.as_deref(),
            Some("\"deploy\" OR \"release\"")
        );
        assert_eq!(
            explanation.fragments.as_deref(),
            Some("\"deploy\" OR \"release\"")
        );
        let scores: Vec<f64> = explanation
            .matches
            .iter()
            .map(|hit| hit.rank.unwrap())
            .collect();
        assert_eq!(scores.len(), 2);
        assert!(scores[0] > scores[1], "fused scores read higher-is-better");
        assert!(explanation.matches[0].memory.body.starts_with("Deploys"));
    }
}
