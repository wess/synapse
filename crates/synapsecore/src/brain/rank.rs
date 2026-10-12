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

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::collections::HashMap;

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

/// Builds both expressions from the terms a query kept.
///
/// A fragment search needs at least three characters, because that is the
/// smallest piece the trigram index holds; a shorter term would match
/// nothing there and is left to the word index.
pub(super) fn expressions(terms: &[String]) -> Expressions {
    let quoted = |term: &String| format!("\"{}\"", term.replace('"', "\"\""));
    let join = |items: Vec<String>| (!items.is_empty()).then(|| items.join(" OR "));
    Expressions {
        words: join(terms.iter().map(quoted).collect()),
        fragments: join(
            terms
                .iter()
                .filter(|term| term.chars().count() >= 3)
                .map(quoted)
                .collect(),
        ),
    }
}

/// Ranks `expressions` and returns at most `limit` hits, best first.
pub(super) async fn rank(
    pool: &SqlitePool,
    expressions: &Expressions,
    limit: usize,
    reach: Reach<'_>,
) -> Result<Vec<Hit>> {
    let depth = (limit * DEPTH).max(20);
    let words = match &expressions.words {
        Some(expression) => list(pool, "memory", expression, depth, reach).await?,
        None => Vec::new(),
    };
    let fragments = match &expressions.fragments {
        Some(expression) => list(pool, "memorygram", expression, depth, reach).await?,
        None => Vec::new(),
    };
    Ok(fuse(&[words, fragments], limit))
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
