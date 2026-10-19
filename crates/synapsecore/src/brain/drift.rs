//! Drift: memory that no longer matches reality. A file it names was moved, a
//! later memory contradicts it without superseding it, or the same fact was
//! written down twice.
//!
//! Recall and the session hook check what they hand over, and the session hook
//! leaves flagged memories out. `synapse memory drift` checks everything a
//! folder can see. Nothing is deleted or superseded automatically: a "missing"
//! path may have been renamed, and two memories that look opposed can both be
//! right in context.

mod checkout;
mod compare;
mod references;

pub use checkout::{Checkout, Presence};
pub use references::references;

use crate::brain::store::{COLUMNS, VISIBLE};
use crate::brain::{Brain, Memory, MemoryScope};
use anyhow::{Context, Result};
use compare::{Disagreement, Profile, Relation};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Newest memories read by a full check.
pub const SCANCEILING: i64 = 5_000;

/// Candidates read when looking for one memory's counterparts. A contradiction
/// shares most of its words, so it ranks near the top.
const COUNTERPARTS: i64 = 8;

/// Words from the memory used for that search.
const PROBE: usize = 16;

/// Characters of a counterpart quoted in a finding.
const QUOTED: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Concern {
    /// Names a path that is gone.
    Stale,
    /// Same subject as another memory, opposite claim or different value.
    Conflict,
    /// Repeats another memory.
    Duplicate,
}

/// Ids are oldest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Finding {
    pub concern: Concern,
    pub ids: Vec<i64>,
    pub note: String,
    /// Paths a stale memory names that are gone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// Result of a full check from one folder.
#[derive(Debug, Serialize)]
pub struct Drift {
    pub root: Option<PathBuf>,
    pub project: Option<String>,
    pub scanned: usize,
    /// More than [`SCANCEILING`] memories, so only the newest were read.
    pub capped: bool,
    pub findings: Vec<Finding>,
}

impl Brain {
    pub async fn drift(&self, folder: Option<&Path>) -> Result<Drift> {
        let checkout = Checkout::at(folder);
        let project = folder.map(crate::brain::projectkey).transpose()?.flatten();
        let memories = sqlx::query_as::<_, Memory>(&format!(
            "SELECT {COLUMNS} FROM memorymeta meta JOIN memory ON memory.rowid = meta.memoryid \
             WHERE {VISIBLE} ORDER BY meta.created DESC, meta.memoryid DESC LIMIT ?"
        ))
        .bind(project.clone().unwrap_or_default())
        .bind(SCANCEILING + 1)
        .fetch_all(&self.pool)
        .await
        .context("could not read memory to check")?;
        let capped = memories.len() as i64 > SCANCEILING;
        let memories = &memories[..memories.len().min(SCANCEILING as usize)];
        Ok(Drift {
            root: checkout.root().map(Path::to_path_buf),
            project,
            scanned: memories.len(),
            capped,
            findings: scan(memories, &checkout),
        })
    }
}

impl Brain {
    /// [`scan`] over recall results, plus any newer memory outside them that
    /// contradicts one. That correction didn't match the query, so without
    /// this the agent would only see the outdated version. It gets quoted.
    pub async fn drifted(
        &self,
        memories: &[Memory],
        folder: Option<&Path>,
    ) -> Result<Vec<Finding>> {
        let checkout = Checkout::at(folder);
        let project = folder
            .map(crate::brain::projectkey)
            .transpose()?
            .flatten()
            .unwrap_or_default();
        let mut findings = scan(memories, &checkout);
        let present: HashSet<i64> = memories.iter().map(|memory| memory.id).collect();
        for memory in memories {
            for (other, relation) in self.counterparts(memory, &project).await? {
                let newer = (other.created, other.id) > (memory.created, memory.id);
                if present.contains(&other.id) || !newer {
                    continue;
                }
                let mut finding = paired(memory, &other, relation);
                finding.note.push_str(&format!(
                    " #{} is not in this response; it says: {}",
                    other.id,
                    quoted(&other.body)
                ));
                if !findings.iter().any(|known| known.ids == finding.ids) {
                    findings.push(finding);
                }
            }
        }
        findings.sort_by(|a, b| a.concern.cmp(&b.concern).then(a.ids.cmp(&b.ids)));
        Ok(findings)
    }

    /// What a newly written memory contradicts or repeats.
    pub async fn related(&self, id: i64, folder: Option<&Path>) -> Result<Vec<Finding>> {
        let Some(memory) = self.memory(id).await? else {
            return Ok(Vec::new());
        };
        let project = match memory.scope {
            MemoryScope::Project => memory.project.clone(),
            MemoryScope::Global => folder
                .map(crate::brain::projectkey)
                .transpose()?
                .flatten()
                .unwrap_or_default(),
        };
        let mut findings: Vec<Finding> = self
            .counterparts(&memory, &project)
            .await?
            .into_iter()
            .map(|(other, relation)| paired(&memory, &other, relation))
            .collect();
        findings.sort_by(|a, b| a.concern.cmp(&b.concern).then(a.ids.cmp(&b.ids)));
        Ok(findings)
    }

    /// Visible memories related to `memory`, found by searching its own words.
    async fn counterparts(
        &self,
        memory: &Memory,
        project: &str,
    ) -> Result<Vec<(Memory, Relation)>> {
        let own = compare::profile(&memory.body);
        if !own.comparable() {
            return Ok(Vec::new());
        }
        let (terms, _) = crate::brain::store::terms(&memory.body);
        let terms: Vec<String> = terms.into_iter().take(PROBE).collect();
        let Some(expression) = crate::brain::rank::expressions(&terms).words else {
            return Ok(Vec::new());
        };
        let found = sqlx::query_as::<_, Memory>(&format!(
            "SELECT {COLUMNS} FROM memory JOIN memorymeta meta ON meta.memoryid = memory.rowid \
             WHERE memory MATCH ? AND {VISIBLE} AND memory.rowid != ? \
             ORDER BY bm25(memory, 1.0, 0.3) LIMIT ?"
        ))
        .bind(expression)
        .bind(project)
        .bind(memory.id)
        .bind(COUNTERPARTS)
        .fetch_all(&self.pool)
        .await
        .context("could not look for related memories")?;
        Ok(found
            .into_iter()
            .filter_map(|other| {
                compare::relate(&own, &compare::profile(&other.body))
                    .map(|relation| (other, relation))
            })
            .collect())
    }
}

fn quoted(body: &str) -> String {
    let first = body.trim().lines().next().unwrap_or_default();
    match first.chars().count() > QUOTED {
        true => format!(
            "“{}…”",
            first.chars().take(QUOTED).collect::<String>().trim_end()
        ),
        false => format!("“{first}”"),
    }
}

pub fn scan(memories: &[Memory], checkout: &Checkout) -> Vec<Finding> {
    let mut findings: Vec<Finding> = memories
        .iter()
        .filter_map(|memory| stale(memory, checkout))
        .collect();
    let profiles: Vec<Profile> = memories
        .iter()
        .map(|memory| compare::profile(&memory.body))
        .collect();
    for (first, second) in candidates(&profiles) {
        if let Some(relation) = compare::relate(&profiles[first], &profiles[second]) {
            findings.push(paired(&memories[first], &memories[second], relation));
        }
    }
    findings.sort_by(|a, b| a.concern.cmp(&b.concern).then(a.ids.cmp(&b.ids)));
    findings
}

fn stale(memory: &Memory, checkout: &Checkout) -> Option<Finding> {
    let relative = memory.scope == MemoryScope::Project && checkout.holds(&memory.project);
    let missing: Vec<String> = references(&memory.body)
        .into_iter()
        .filter(|reference| checkout.presence(reference, relative) == Presence::Missing)
        .collect();
    if missing.is_empty() {
        return None;
    }
    let listed = missing
        .iter()
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let verb = if missing.len() == 1 { "is" } else { "are" };
    Some(Finding {
        concern: Concern::Stale,
        ids: vec![memory.id],
        note: format!(
            "#{} names {listed}, which {verb} no longer there.",
            memory.id
        ),
        missing,
    })
}

/// Candidate pairs via prefix filtering (as in set-similarity joins). Sort each
/// memory's words rarest first. Two memories sharing at least `α` words must
/// share one within the first `size - α + 1` of each. `relate` never reports a
/// pair with fewer than [`compare::MINSHARED`] shared words, so indexing just
/// those prefixes misses nothing, and rare words keep the posting lists short.
fn candidates(profiles: &[Profile]) -> Vec<(usize, usize)> {
    let mut frequency: HashMap<&String, usize> = HashMap::new();
    for profile in profiles.iter().filter(|profile| profile.comparable()) {
        for word in profile.words() {
            *frequency.entry(word).or_default() += 1;
        }
    }
    let mut postings: HashMap<&String, Vec<usize>> = HashMap::new();
    for (index, profile) in profiles.iter().enumerate() {
        if !profile.comparable() {
            continue;
        }
        let mut ordered: Vec<&String> = profile.words().collect();
        ordered.sort_by(|a, b| frequency[a].cmp(&frequency[b]).then(a.cmp(b)));
        let prefix = profile.size() + 1 - compare::MINSHARED;
        for word in ordered.into_iter().take(prefix) {
            postings.entry(word).or_default().push(index);
        }
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for holders in postings.values() {
        for (position, first) in holders.iter().enumerate() {
            for second in &holders[position + 1..] {
                pairs.push((*first.min(second), *first.max(second)));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

fn paired(first: &Memory, second: &Memory, relation: Relation) -> Finding {
    let (older, newer, swapped) = match (first.created, first.id) <= (second.created, second.id) {
        true => (first, second, false),
        false => (second, first, true),
    };
    let (a, b) = (older.id, newer.id);
    let (concern, note) = match relation {
        Relation::Duplicate => (
            Concern::Duplicate,
            format!("#{a} and #{b} say nearly the same thing."),
        ),
        Relation::Conflict(disagreement) => {
            let said = match disagreement {
                Disagreement::Opposites(this, that) => {
                    let (old, new) = if swapped { (that, this) } else { (this, that) };
                    format!("#{a} says `{old}` where #{b} says `{new}` about the same thing")
                }
                Disagreement::Values(these, those) => {
                    let (old, new) = if swapped {
                        (those, these)
                    } else {
                        (these, those)
                    };
                    format!(
                        "#{a} says {} where #{b} says {} about the same thing",
                        old.join(", "),
                        new.join(", ")
                    )
                }
                Disagreement::Negation => {
                    format!("#{a} and #{b} say the same thing, but one of them negates it")
                }
            };
            (
                Concern::Conflict,
                format!("{said}. If they disagree, #{b} is the newer."),
            )
        }
    };
    Finding {
        concern,
        ids: vec![a, b],
        note,
        missing: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
