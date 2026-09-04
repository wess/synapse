//! `synapse tokens` — what Synapse costs a session before the first turn.
//!
//! Recall has had a budget for a long time: three optimization levels, a result
//! limit, a character cap, and an abridged form for a memory that overruns it.
//! That is the part somebody worried about token use would look at first, and it
//! is the part that was already careful.
//!
//! The expensive surfaces are the ones nobody was counting. The whole of
//! `SOUL.md` is handed to every MCP client as the server's instructions, so it
//! is paid once per session per connected tool whether or not a memory is ever
//! recalled. Every installed skill's description is loaded eagerly by the tool
//! holding it, for the same price, forever. The mesh's sixteen tool definitions
//! are already gated behind a setting — but nothing anywhere said what that
//! setting was worth.
//!
//! So this counts them. It changes nothing and writes nothing; it exists because
//! a cost nobody can see is a cost nobody trims, and because the three levers
//! that do exist (the optimization level, the mesh switch, the learn switch)
//! cannot be weighed without a number beside them.
//!
//! **The token figures are estimates.** Characters divided by four, which is the
//! usual rule of thumb for English prose and close enough for JSON. No tokenizer
//! is consulted: the point is the proportions between these lines and whether a
//! number is growing, and both survive the approximation. The character counts
//! are exact.

use crate::brain::{Brain, Optimization};
use crate::cli::Outcome;
use anyhow::Result;
use std::ffi::OsString;

/// Characters per token. A rule of thumb, and stated as one everywhere it is
/// shown — see the module note.
const PERTOKEN: usize = 4;

/// Where shared guidance stops being shared guidance and starts being a book
/// every session has to read. Three thousand tokens is already the largest
/// single thing Synapse asks for, and it is asked for again on every session of
/// every connected tool.
const GUIDANCEGUIDE: usize = 12_000;

/// The whole skill library's descriptions. Each one should be a line saying when
/// to reach for the skill; a thousand tokens of them means they have started
/// explaining themselves instead.
const SKILLSGUIDE: usize = 4_000;

/// One skill's description. Long enough for two full sentences. Past this it is
/// documentation, and documentation belongs in the skill body, which is read
/// only when the skill is actually used.
const ONESKILLGUIDE: usize = 500;

/// What the session hook carries before anybody has asked a question. Two
/// thousand tokens is a generous opening brief; past that the recall budget is
/// doing the work of a search nobody ran.
const RECALLGUIDE: usize = 8_000;

/// `1 skill`, `2 skills`, `3 memories`. Every line here is counting something,
/// and a report that says "1 skills" reads as a report nobody looked at. Both
/// forms are given rather than an `s` appended, because English does not.
fn plural(count: usize, one: &str, many: &str) -> String {
    match count {
        1 => format!("1 {one}"),
        other => format!("{other} {many}"),
    }
}

/// One line of the report.
pub struct Line {
    pub label: &'static str,
    pub detail: String,
    pub chars: usize,
    /// Set when the line is over its guide, and why.
    pub warning: Option<String>,
}

impl Line {
    pub fn tokens(&self) -> usize {
        self.chars / PERTOKEN
    }
}

pub fn tokens(arguments: &[OsString]) -> Result<Outcome> {
    let json = arguments.iter().any(|value| value == "--json");
    let runtime = tokio::runtime::Runtime::new()?;
    let lines = runtime.block_on(measure())?;

    let total: usize = lines.iter().map(|line| line.chars).sum();
    if json {
        let body = serde_json::json!({
            "estimated": true,
            "characterspertoken": PERTOKEN,
            "total": { "chars": total, "tokens": total / PERTOKEN },
            "lines": lines.iter().map(|line| serde_json::json!({
                "label": line.label,
                "detail": line.detail,
                "chars": line.chars,
                "tokens": line.chars / PERTOKEN,
                "warning": line.warning,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(Outcome::Exit(0));
    }

    println!("What Synapse costs a session, before the first turn");
    println!();
    for line in &lines {
        println!(
            "  {:<16} {:>7} chars  ~{:>6} tokens   {}",
            line.label,
            thousands(line.chars),
            thousands(line.chars / PERTOKEN),
            line.detail
        );
    }
    println!("  {}", "─".repeat(62));
    println!(
        "  {:<16} {:>7} chars  ~{:>6} tokens   per session, per connected tool",
        "Total",
        thousands(total),
        thousands(total / PERTOKEN)
    );
    println!();
    println!("  Token figures are characters ÷ {PERTOKEN}, not a tokenizer. Counts are exact.");

    let warnings: Vec<&Line> = lines.iter().filter(|line| line.warning.is_some()).collect();
    if !warnings.is_empty() {
        println!();
        for line in warnings {
            if let Some(warning) = &line.warning {
                println!("  ! {warning}");
            }
        }
    }
    Ok(Outcome::Exit(0))
}

/// Every always-on surface, measured as the model receives it.
///
/// Shared with `doctor`, which reports the same numbers rather than working
/// them out again: a second implementation of "what does this cost" is a second
/// answer to it.
pub async fn measure() -> Result<Vec<Line>> {
    let database = crate::files::database()?;
    let soul = crate::files::soul()?;
    let brain = Brain::glance(&database).await?;
    let optimization = brain
        .settings()
        .await
        .map(|settings| settings.optimization)
        .unwrap_or_default();
    let mesh = brain.mesh().await.unwrap_or(false);
    let learn = brain.learn().await.unwrap_or(false);

    let mut lines = Vec::new();
    lines.push(guidance(&soul, mesh, learn)?);
    lines.push(tooldefinitions(mesh, learn));
    lines.push(skills()?);
    lines.push(startuprecall(&brain, optimization).await);
    Ok(lines)
}

/// `SOUL.md` as the MCP server sends it: the file plus whichever guidance blocks
/// the current settings add. Measured through `modelfacing` rather than by
/// reading the file, because the blocks are part of the bill.
fn guidance(soul: &std::path::Path, mesh: bool, learn: bool) -> Result<Line> {
    let text = crate::instructions::ensure(soul)?;
    let sent = crate::instructions::modelfacing(&text, mesh, learn);
    let chars = sent.chars().count();
    let extra = match (mesh, learn) {
        (true, true) => " + mesh and learn guidance",
        (true, false) => " + mesh guidance",
        (false, true) => " + learn guidance",
        (false, false) => "",
    };
    Ok(Line {
        label: "Guidance",
        detail: format!("SOUL.md{extra}"),
        chars,
        warning: (chars > GUIDANCEGUIDE).then(|| {
            format!(
                "Guidance is {} characters, over the {} it is worth keeping to. \
                 Every session of every connected tool reads all of it. \
                 Move what is reference rather than instruction into a skill, \
                 which is read only when it is needed.",
                thousands(chars),
                thousands(GUIDANCEGUIDE)
            )
        }),
    })
}

/// The tool schemas, which is where the mesh and learn settings show their
/// worth: both gate the router, so turning one off removes the definitions from
/// every session rather than merely refusing the calls.
fn tooldefinitions(mesh: bool, learn: bool) -> Line {
    let tools = crate::mcp::toolcost(mesh, learn);
    let chars: usize = tools.iter().map(|(_, size)| size).sum();
    // What the setting is worth, in the same units, whichever way it is set: a
    // switch is a decision with a number beside it rather than a preference.
    let without: usize = crate::mcp::toolcost(false, learn)
        .iter()
        .map(|(_, size)| size)
        .sum();
    let difference = thousands(chars.abs_diff(without) / PERTOKEN);
    let saved = match mesh {
        true => format!(" · mesh on, costing ~{difference} tokens"),
        false => format!(" · mesh off, saving ~{difference} tokens"),
    };
    Line {
        label: "Tool schemas",
        detail: format!("{}{saved}", plural(tools.len(), "tool", "tools")),
        chars,
        warning: None,
    }
}

/// Every library skill's name and description — what a tool holding the skill
/// loads on every session, whether or not the skill is ever used. The body is
/// not counted: that is read on demand, which is the whole point of the format.
fn skills() -> Result<Line> {
    let (skills, _) = crate::skill::library::all()?;
    let mut chars = 0;
    let mut longest: Option<(String, usize)> = None;
    for skill in &skills {
        let cost = skill.name.chars().count() + skill.description.chars().count();
        chars += cost;
        let description = skill.description.chars().count();
        if longest.as_ref().is_none_or(|(_, held)| description > *held) {
            longest = Some((skill.name.clone(), description));
        }
    }
    let overlong = longest.filter(|(_, size)| *size > ONESKILLGUIDE);
    Ok(Line {
        label: "Skill library",
        detail: format!(
            "{}, descriptions only",
            plural(skills.len(), "skill", "skills")
        ),
        chars,
        warning: match (chars > SKILLSGUIDE, overlong) {
            (true, _) => Some(format!(
                "Skill descriptions total {} characters, over the {} they are worth keeping to. \
                 Every one is loaded eagerly by every tool holding it.",
                thousands(chars),
                thousands(SKILLSGUIDE)
            )),
            (false, Some((name, size))) => Some(format!(
                "`{name}`'s description is {size} characters. A description says when to \
                 reach for a skill; what it does belongs in the body, which is read only then."
            )),
            (false, None) => None,
        },
    })
}

/// What the session hook carries into context before the first turn. Bounded
/// already by the configured optimization, and here so the level can be weighed
/// against the surfaces that have no budget at all.
async fn startuprecall(brain: &Brain, optimization: Optimization) -> Line {
    let root = std::env::current_dir()
        .ok()
        .and_then(|current| crate::brain::projectroot(&current).ok().flatten());
    let recalled = brain
        .recallscoped("", u32::MAX, Some(optimization), root.as_deref())
        .await
        .map(|(_, memories)| memories)
        .unwrap_or_default();
    let chars: usize = recalled
        .iter()
        .map(|memory| memory.body.chars().count())
        .sum();
    Line {
        label: "Startup recall",
        detail: format!(
            "{} at {}",
            plural(recalled.len(), "memory", "memories"),
            optimization.name().to_lowercase()
        ),
        chars,
        // The one line on the page with a setting behind it. Full asks for
        // twenty-five memories and caps nothing, which is the right answer when
        // somebody wants everything and an expensive one to be on by accident.
        warning: (chars > RECALLGUIDE).then(|| match optimization {
            Optimization::Full => format!(
                "The session opens with {} characters of recall, over the {}. \
                 The budget is `full`, which caps nothing — `synapse settings \
                 optimization balanced` holds it to eight memories and 6,000 characters.",
                thousands(chars),
                thousands(RECALLGUIDE)
            ),
            _ => format!(
                "The session opens with {} characters of recall, over the {}, \
                 even at the {} budget. Memories this long are worth splitting.",
                thousands(chars),
                thousands(RECALLGUIDE),
                optimization.name().to_lowercase()
            ),
        }),
    }
}

/// `16450` as `16,450`. Every number here is compared against another one, and
/// digits in a row are hard to compare at a glance.
fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_of_a_thing_is_not_reported_as_one_things() {
        assert_eq!(plural(0, "skill", "skills"), "0 skills");
        assert_eq!(plural(1, "skill", "skills"), "1 skill");
        assert_eq!(plural(2, "skill", "skills"), "2 skills");
        assert_eq!(plural(1, "memory", "memories"), "1 memory");
        assert_eq!(plural(3, "memory", "memories"), "3 memories");
    }

    #[test]
    fn numbers_are_grouped_for_comparing() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(16_450), "16,450");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    /// The mesh setting is the largest lever there is, and the report's job is
    /// to say so in the same units as everything else.
    #[test]
    fn turning_the_mesh_on_is_the_biggest_single_cost() {
        let base = tooldefinitions(false, false);
        let withmesh = tooldefinitions(true, false);
        assert!(
            withmesh.chars > base.chars * 2,
            "sixteen more tools should dominate three: {} vs {}",
            withmesh.chars,
            base.chars
        );
        // And the saving is named while it is being made, not only once it is
        // too late to make it.
        assert!(base.detail.contains("saving"), "got {}", base.detail);
        assert!(
            !withmesh.detail.contains("saving"),
            "got {}",
            withmesh.detail
        );
    }
}
