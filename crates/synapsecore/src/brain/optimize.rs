use crate::brain::{Memory, Optimization, Settings};
use std::collections::HashSet;

/// Below this, an abstract is not worth sending. A hundred and sixty characters
/// is about a sentence; less than that is a fragment with no verb in it, and a
/// fragment costs tokens to say nothing.
const FLOOR: usize = 160;

/// How much of a memory its opening counts as. Long enough for a sentence that
/// carries a qualifier, short enough that four of them fit in the lean budget.
const LEAD: usize = 240;

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
    let mut remaining = settings.characterbudget.unwrap_or(usize::MAX);
    let mut optimized = Vec::new();
    for mut memory in memories {
        memory.body = compact(&memory.body);
        if memory.body.is_empty() || !seen.insert(memory.body.clone()) {
            continue;
        }
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
