use crate::brain::{Memory, Optimization, Settings};
use std::collections::HashSet;

pub fn recall(memories: Vec<Memory>, settings: Settings) -> Vec<Memory> {
    if settings.optimization == Optimization::Full {
        return memories;
    }
    let mut seen = HashSet::new();
    let mut remaining = settings.characterbudget.unwrap_or(usize::MAX);
    let mut optimized = Vec::new();
    for mut memory in memories {
        memory.body = compact(&memory.body);
        if !seen.insert(memory.body.clone()) || remaining == 0 {
            continue;
        }
        let used = memory.body.chars().count();
        if used > remaining {
            memory.body = truncate(&memory.body, remaining);
        }
        remaining = remaining.saturating_sub(memory.body.chars().count());
        if !memory.body.is_empty() {
            optimized.push(memory);
        }
    }
    optimized
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
}
