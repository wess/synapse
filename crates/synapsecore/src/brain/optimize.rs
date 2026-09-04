use crate::brain::{Memory, Optimization, Settings};
use std::collections::HashSet;

/// Below this, an abstract is not worth sending. A hundred and sixty characters
/// is about a sentence; less than that is a fragment with no verb in it, and a
/// fragment costs tokens to say nothing.
const FLOOR: usize = 160;

/// How much of a memory its opening counts as. Long enough for a sentence that
/// carries a qualifier, short enough that four of them fit in the lean budget.
const LEAD: usize = 240;

/// How much of two memories' wording has to agree before the later one is read
/// as the same thing said twice.
///
/// Deliberately high. Dropping a memory somebody wrote is the expensive
/// mistake here and returning one they did not need is the cheap one, so this
/// is set where it catches a fact recorded twice in slightly different words
/// and leaves two genuinely different notes about one subject alone. Four
/// fifths of the meaningful words in common, in both directions.
const SAMENESS: f32 = 0.8;

/// Below this many meaningful words, similarity says nothing. Two five-word
/// memories sharing four words may be the same note or may be two settings with
/// the same name, and there is not enough text to tell.
const COMPARABLE: usize = 8;

/// Fits a ranked list of memories into the response budget.
///
/// The budget used to be spent in rank order and the memory that straddled the
/// end was cut at whatever character the count landed on, with everything after
/// it dropped. Both halves of that were wrong. Half a memory reads exactly like
/// a whole one — `never deploy from main unless` is a rule with its condition
/// amputated, and an agent acts on it — and dropping the rest meant one long
/// memory at the top could take the whole response with it.
///
/// So a memory that does not fit is replaced by its opening line and marked
/// `abridged`, which is a true, shorter statement rather than a truncated one,
/// and the walk carries on until even an abstract will not fit.
pub fn recall(memories: Vec<Memory>, settings: Settings) -> Vec<Memory> {
    if settings.optimization == Optimization::Full {
        return memories;
    }
    let mut seen = HashSet::new();
    let mut kept: Vec<HashSet<String>> = Vec::new();
    let mut remaining = settings.characterbudget.unwrap_or(usize::MAX);
    let mut optimized = Vec::new();
    for mut memory in memories {
        memory.body = compact(&memory.body);
        if memory.body.is_empty() || !seen.insert(memory.body.clone()) {
            continue;
        }
        // The same fact written twice costs the budget twice and tells the
        // session nothing the second time. Exact matching above catches a
        // duplicated memory; this catches the one somebody wrote again in
        // slightly different words months later, which is the ordinary way it
        // happens. The higher-ranked one stays, as with an exact match.
        let words = meaningful(&memory.body);
        if kept.iter().any(|held| sameas(held, &words)) {
            continue;
        }
        kept.push(words);
        if memory.body.chars().count() > remaining {
            let opening = lead(&memory.body);
            memory.body = if opening.chars().count() <= remaining {
                opening
            } else if remaining >= FLOOR {
                // Not even the opening fits, but there is still room for a
                // usable piece of it. Better than skipping the best match in
                // the list to make room for a worse one.
                truncate(&opening, remaining)
            } else {
                continue;
            };
            memory.abridged = true;
        }
        remaining = remaining.saturating_sub(memory.body.chars().count());
        optimized.push(memory);
    }
    optimized
}

/// The meaningful words of a body, lowercased and deduplicated.
///
/// Punctuation and case go, because "Never deploy from main." and "never deploy
/// from main" are one rule. Very short words go too: two memories sharing `the`,
/// `a` and `to` have nothing in common, and leaving them in drags every pair's
/// similarity toward the threshold from below.
fn meaningful(body: &str) -> HashSet<String> {
    body.split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() > 3)
        .map(str::to_lowercase)
        .collect()
}

/// Whether two word sets say the same thing.
///
/// Overlap measured against the *smaller* set, then required of the larger one
/// too. Against the smaller alone, a one-line memory whose every word appears
/// somewhere in a long one would be swallowed by it, and a short rule is
/// usually the sharper of the two. Requiring both directions means one memory
/// has to be a restatement of the other rather than a passage inside it.
fn sameas(left: &HashSet<String>, right: &HashSet<String>) -> bool {
    if left.len() < COMPARABLE || right.len() < COMPARABLE {
        return false;
    }
    let shared = left.intersection(right).count() as f32;
    shared / left.len() as f32 >= SAMENESS && shared / right.len() as f32 >= SAMENESS
}

/// A memory's opening: its first real line, cut at the first sentence end if
/// that line runs on.
///
/// Fences and their content are skipped over rather than opened. The first line
/// of a fenced block is a run of backticks and a language name, which says
/// nothing about what the memory is for.
fn lead(body: &str) -> String {
    let mut fenced = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced || trimmed.is_empty() {
            continue;
        }
        return sentence(trimmed);
    }
    // Nothing but code. Its first line is still better than nothing.
    truncate(body.lines().next().unwrap_or_default().trim(), LEAD)
}

/// The first sentence of a line, or the line cut at [`LEAD`] when it holds no
/// sentence end near enough to be one.
///
/// Two things are not sentence ends even though they look like one. A full stop
/// inside a number — `v1.2.3`, `0.5` — is caught by requiring whitespace after
/// it. A full stop after a single letter is `e.g.`, `i.e.`, or an initial, and
/// is caught by looking at the letter before it.
fn sentence(line: &str) -> String {
    let mut count = 0;
    let mut characters = line.chars().peekable();
    let mut taken = String::new();
    let mut run = 0;
    while let Some(character) = characters.next() {
        taken.push(character);
        count += 1;
        let ends = matches!(character, '.' | '!' | '?')
            && run != 1
            && characters
                .peek()
                .is_none_or(|next| next.is_whitespace() || *next == '"');
        if ends {
            return taken;
        }
        if count >= LEAD {
            return truncate(line, LEAD);
        }
        run = if character.is_alphanumeric() {
            run + 1
        } else {
            0
        };
    }
    taken
}

fn compact(value: &str) -> String {
    let mut output = String::new();
    let mut blank = false;
    let mut fenced = false;
    for line in value.replace("\r\n", "\n").lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
        }
        if trimmed.is_empty() {
            if !output.is_empty() && !blank {
                output.push('\n');
            }
            blank = true;
            continue;
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        if fenced || line.starts_with("    ") || line.starts_with('\t') {
            output.push_str(line.trim_end());
        } else {
            output.push_str(&trimmed.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        blank = false;
    }
    output.trim().to_owned()
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    if limit <= 1 {
        return "…".chars().take(limit).collect();
    }
    let mut shortened = value.chars().take(limit - 1).collect::<String>();
    shortened.push('…');
    shortened
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MemoryScope;

    /// The same memory with an id of its own, for the tests that need to say
    /// which of two survived.
    fn numbered(id: i64, body: &str) -> Memory {
        Memory { id, ..memory(body) }
    }

    fn memory(body: &str) -> Memory {
        Memory {
            id: 1,
            body: body.to_owned(),
            source: String::new(),
            scope: MemoryScope::Global,
            project: String::new(),
            created: 0,
            superseded: 0,
            abridged: false,
        }
    }

    fn budget(characters: usize) -> Settings {
        Settings {
            optimization: Optimization::Lean,
            resultlimit: 25,
            characterbudget: Some(characters),
        }
    }

    /// The same rule written twice, months apart, in slightly different words.
    /// Both survive supersession — nobody corrected anything — and both spend
    /// budget to say one thing.
    #[test]
    fn a_fact_recorded_twice_in_different_words_is_returned_once() {
        let first = "Never deploy the billing service from main on a Friday afternoon, \
                     because the on-call rotation changes at six and nobody owns a rollback.";
        let second = "Never deploy billing from main on Friday afternoons: the on-call \
                      rotation changes at six, so nobody owns the rollback.";
        let recalled = recall(
            vec![numbered(1, first), numbered(2, second)],
            Settings::from(Optimization::Balanced),
        );
        assert_eq!(recalled.len(), 1, "got {recalled:#?}");
        // The higher-ranked one stays, which is what exact matching already did.
        assert_eq!(recalled[0].id, 1);
    }

    /// The expensive mistake is dropping something somebody wrote, so two notes
    /// about one subject that actually say different things both come back.
    #[test]
    fn two_different_notes_about_one_subject_both_survive() {
        let first = "The billing service deploys from main through the release workflow, \
                     which tags, builds, signs and notarises before publishing.";
        let second = "The billing service keeps its credentials in the vault under the \
                      billing scope, and the staging key is separate from production.";
        let recalled = recall(
            vec![numbered(1, first), numbered(2, second)],
            Settings::from(Optimization::Balanced),
        );
        assert_eq!(recalled.len(), 2, "got {recalled:#?}");
    }

    /// A short memory whose every word appears inside a long one is not the same
    /// memory. It is usually the sharper of the two, and swallowing it would be
    /// the one failure mode worth engineering against.
    #[test]
    fn a_short_rule_is_not_swallowed_by_a_long_passage_containing_it() {
        let rule = "Never deploy billing from main without a rollback owner.";
        let passage = "Never deploy billing from main without a rollback owner. The release \
                       workflow tags the commit, builds and signs the bundle, notarises it \
                       with Apple, publishes the draft, and finally updates the Homebrew \
                       cask, and every one of those steps has its own failure mode worth \
                       reading about before you start.";
        let recalled = recall(
            vec![numbered(1, rule), numbered(2, passage)],
            Settings::from(Optimization::Balanced),
        );
        assert_eq!(recalled.len(), 2, "got {recalled:#?}");
    }

    /// Full asks for everything and gets it. Similarity is a budget measure, and
    /// there is no budget here.
    #[test]
    fn full_returns_near_duplicates_untouched() {
        let first = "Never deploy the billing service from main on a Friday afternoon, \
                     because the on-call rotation changes at six and nobody owns a rollback.";
        let second = "Never deploy billing from main on Friday afternoons: the on-call \
                      rotation changes at six, so nobody owns the rollback.";
        let recalled = recall(
            vec![numbered(1, first), numbered(2, second)],
            Settings::from(Optimization::Full),
        );
        assert_eq!(recalled.len(), 2);
    }

    /// Two short memories can share most of their words and mean different
    /// things, and there is not enough text to tell them apart.
    #[test]
    fn short_memories_are_never_compared() {
        let recalled = recall(
            vec![
                numbered(1, "staging database host is warehouse"),
                numbered(2, "staging database host was warehouse"),
            ],
            Settings::from(Optimization::Balanced),
        );
        assert_eq!(recalled.len(), 2, "got {recalled:#?}");
    }

    #[test]
    fn compacts_prose_but_preserves_fenced_code() {
        let input = "  One   useful fact.  \n\n\n```rust\nlet  x = 1;  \n```\n";
        assert_eq!(
            compact(input),
            "One useful fact.\n```rust\nlet  x = 1;\n```"
        );
    }

    #[test]
    fn truncates_on_character_boundaries() {
        assert_eq!(truncate("memory 🧠 context", 9), "memory 🧠…");
    }

    /// The bug: a memory that outran the budget was cut wherever the count
    /// landed, which turns a rule into its own opposite. Its opening sentence
    /// is shorter *and* true.
    #[test]
    fn a_memory_too_big_for_the_budget_is_summarised_not_severed() {
        let rule = format!(
            "Never deploy from main unless the release tag is already signed. {}",
            "Background follows. ".repeat(40)
        );

        let recalled = recall(vec![memory(&rule)], budget(300));

        assert_eq!(recalled.len(), 1);
        assert_eq!(
            recalled[0].body,
            "Never deploy from main unless the release tag is already signed."
        );
        assert!(
            recalled[0].abridged,
            "an abstract has to say that it is one"
        );
    }

    /// One long memory at the top used to spend the entire budget and take
    /// every other match with it.
    #[test]
    fn one_long_memory_no_longer_swallows_the_whole_response() {
        let long = format!("The deploy story. {}", "and then. ".repeat(200));

        let recalled = recall(
            vec![memory(&long), memory("Postgres pools at five.")],
            budget(400),
        );

        assert_eq!(recalled.len(), 2, "got {recalled:?}");
        assert_eq!(recalled[0].body, "The deploy story.");
        assert!(recalled[0].abridged);
        assert_eq!(recalled[1].body, "Postgres pools at five.");
        assert!(
            !recalled[1].abridged,
            "a memory that fits whole is not an abstract"
        );
    }

    #[test]
    fn a_memory_that_fits_is_left_exactly_as_it_was_written() {
        let recalled = recall(
            vec![memory("Deploys run from the deploy branch.")],
            budget(500),
        );
        assert_eq!(recalled[0].body, "Deploys run from the deploy branch.");
        assert!(!recalled[0].abridged);
    }

    #[test]
    fn the_opening_skips_a_fence_to_find_the_sentence_that_explains_it() {
        assert_eq!(
            lead("```sh\ncargo test --locked\n```\nRun this before every release."),
            "Run this before every release."
        );
    }

    /// A version number and an abbreviation both contain a full stop followed
    /// by more sentence, and cutting at either loses the half that matters.
    #[test]
    fn a_full_stop_inside_a_number_or_an_abbreviation_is_not_a_sentence_end() {
        assert_eq!(
            sentence("Use v1.2.3 of the parser, never the fork. It drops comments."),
            "Use v1.2.3 of the parser, never the fork."
        );
        assert_eq!(
            sentence("Pin the runtime, e.g. bun 1.1, in the image. Nowhere else."),
            "Pin the runtime, e.g. bun 1.1, in the image."
        );
    }

    /// Without a floor the tail of a response is a stream of three-word
    /// fragments, each costing tokens and none saying anything.
    #[test]
    fn a_budget_too_small_for_a_sentence_ends_the_response() {
        // One unpunctuated 399-character line each, so no opening is shorter
        // than LEAD and the budget is what decides.
        let recalled = recall(
            vec![
                memory(&"a ".repeat(200)),
                memory(&"b ".repeat(200)),
                memory(&"c ".repeat(200)),
            ],
            budget(700),
        );

        assert_eq!(recalled.len(), 2, "got {recalled:?}");
        assert!(!recalled[0].abridged, "the first one fit whole");
        assert!(recalled[1].abridged);
        assert!(
            recalled[1].body.chars().count() <= LEAD,
            "the second is an opening, not the rest of the budget"
        );
    }

    #[test]
    fn full_recall_abridges_nothing() {
        let long = "detail ".repeat(1_000);
        let recalled = recall(vec![memory(&long)], Settings::from(Optimization::Full));
        assert_eq!(recalled[0].body, long);
        assert!(!recalled[0].abridged);
    }
}
