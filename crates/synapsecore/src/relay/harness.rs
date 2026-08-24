//! The opening instruction a launched agent receives.
//!
//! Two shapes. A **parked worker** registers, joins its channels, then loops on
//! `wait`: do work, report back, park again. Idle costs one tool call every few
//! minutes. A **driver** — the human-driven lead of a team, or a supervisor
//! launched on its own — registers and then hands control back to the person in
//! its terminal, only calling `wait` to gather replies after it has delegated,
//! so it never parks the human out of their own session.
//!
//! Both shapes are told explicitly that an empty *or failed* `wait` is routine
//! and to call it again. That line is load-bearing: without it an agent reads a
//! park that timed out as a failure, writes an explanation instead of another
//! tool call, and a headless run exits the moment the model ends its turn.

/// Render the harness. `brief` is the role's description and `task` the
/// per-launch focus. `optimize` trades the spelled-out protocol for a terse one
/// so a launch that carries no task costs noticeably fewer tokens; it changes
/// wording only, never which instructions survive.
/// `a` or `an` for a role name. Roles are open — anybody can write one — so the
/// article cannot be baked into the sentence, and "a overseer" is the kind of
/// wrong that makes a prompt read as machine-written to the model reading it.
fn article(role: &str) -> &'static str {
    match role
        .chars()
        .next()
        .is_some_and(|first| matches!(first.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'))
    {
        true => "an",
        false => "a",
    }
}

pub fn prompt(
    name: &str,
    role: &str,
    brief: &str,
    channels: &[String],
    task: Option<&str>,
    interactive: bool,
    optimize: bool,
) -> String {
    let join = if channels.is_empty() {
        String::new()
    } else if optimize {
        format!(" `join` {};", channels.join(", "))
    } else {
        format!(
            "- After registering, `join` these channels: {}.\n",
            channels.join(", ")
        )
    };
    let brief = match brief.trim() {
        "" => String::new(),
        brief => format!("\nYour role:\n{brief}\n"),
    };
    let task = task
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("\nYour standing focus: {value}\n"))
        .unwrap_or_default();

    let protocol = match (optimize, interactive) {
        (true, true) => {
            "Protocol: `register` name=\"{name}\" role=\"{role}\".{join} Stay interactive for \
             the human's goal, split it into tasks, delegate with `send`/`post`, then `wait` \
             for replies and report back. An empty or failed `wait` means replies are still \
             coming — call it again rather than giving up.\n"
        }
        (true, false) => {
            "Protocol: `register` name=\"{name}\" role=\"{role}\".{join} Then loop: `wait` for \
             work, do it, report with `send`/`post`, `wait` again. `reportstatus` with a \
             one-line `note` as your task changes — it is the only view anyone has of you. An \
             empty or failed `wait` is normal — call it again. Never stop the loop.\n"
        }
        (false, true) => {
            "Protocol — follow exactly:\n\
             - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
             {join}\
             - Then stop and let the human in this terminal give you a goal — do NOT call \
             `wait` yet. Stay interactive so they can type.\n\
             - When you have a goal, break it into tasks and delegate with `send` (to one \
             agent) or `post` (to a channel).\n\
             - After delegating, call `wait` to collect replies, integrate them, and report \
             progress back to the human here. Return control to the human whenever you need \
             their input.\n\
             - `wait` returning no messages, or failing with an error, does NOT mean the work \
             is finished — it is a normal timeout. Call `wait` again until every task you \
             delegated has reported back.\n"
        }
        (false, false) => {
            "Protocol — follow exactly:\n\
             - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
             {join}\
             - Call `wait` to receive work; it blocks until a message arrives.\n\
             - Do the requested work in this session, then report back with `send` to the \
             message's sender, or `post` to the relevant channel.\n\
             - Call `reportstatus` whenever your state or your task changes, and give it a \
             one-line `note` saying what you are doing. You have no terminal anyone can watch, \
             so that note is the only way a person can see what you are working on.\n\
             - `wait` returning no messages, or failing with an error, is normal and expected \
             — it just means nothing arrived in time. Call `wait` again immediately. Never \
             treat it as a reason to stop or to report a problem.\n\
             - If you hit a decision you should not make alone, call `agents` and look for a \
             row marked `human`. That is a person. `send` them the specific question, \
             `reportstatus` `blocked`, and `wait` for their answer rather than guessing.\n\
             - ALWAYS end your turn by calling `wait` again so you stay reachable. Never stop \
             the wait loop.\n"
        }
    };
    let protocol = protocol
        .replace("{name}", name)
        .replace("{role}", role)
        .replace("{join}", &join);

    let intro = if optimize {
        format!("You are \"{name}\" ({role}) on the Synapse mesh (the Synapse MCP tools).\n")
    } else {
        format!(
            "You are \"{name}\", {} {role} on the Synapse mesh. You coordinate with the other \
             agents through the Synapse MCP tools, and you share one durable memory with them \
             through `recall` and `remember`.\n",
            article(role)
        )
    };
    format!("{intro}{protocol}{brief}{task}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_survives_an_empty_or_failed_wait() {
        for interactive in [true, false] {
            for optimize in [true, false] {
                let rendered = prompt("a", "worker", "", &[], None, interactive, optimize);
                assert!(
                    rendered.contains("empty or failed `wait`")
                        || rendered.contains("failing with an error"),
                    "interactive={interactive} optimize={optimize} never mentions a failed wait:\n{rendered}"
                );
            }
        }
    }

    #[test]
    fn a_worker_parks_on_the_wait_loop() {
        let rendered = prompt("backend", "backend", "", &[], None, false, false);
        assert!(rendered.contains("Call `wait` to receive work"));
        assert!(rendered.contains("Never stop the wait loop"));
        assert!(!rendered.contains("human in this terminal"));
    }

    #[test]
    fn a_driver_stays_interactive_instead_of_parking() {
        let rendered = prompt("lead", "supervisor", "", &[], None, true, false);
        assert!(rendered.contains("do NOT call `wait` yet"));
        assert!(rendered.contains("human in this terminal"));
        assert!(!rendered.contains("Never stop the wait loop"));
        assert!(rendered.contains("register"));
    }

    #[test]
    fn channels_and_focus_thread_into_the_protocol() {
        let channels = ["frontend".to_owned(), "ui".to_owned()];
        let rendered = prompt(
            "fe",
            "frontend",
            "You own the frontend.",
            &channels,
            Some("build the login page"),
            false,
            false,
        );
        assert!(rendered.contains("`join` these channels: frontend, ui"));
        assert!(rendered.contains("You own the frontend."));
        assert!(rendered.contains("standing focus: build the login page"));
    }

    #[test]
    fn the_optimized_shape_is_shorter_but_keeps_the_essentials() {
        let full = prompt("w", "worker", "", &[], None, false, false);
        let lean = prompt("w", "worker", "", &[], None, false, true);
        assert!(lean.len() < full.len());
        assert!(lean.contains("register"));
        assert!(lean.contains("`wait`"));
        assert!(lean.contains("Never stop the loop"));
    }

    #[test]
    fn an_empty_brief_or_focus_adds_no_stray_headings() {
        let rendered = prompt("w", "worker", "   ", &[], Some("  "), false, false);
        assert!(!rendered.contains("Your role:"));
        assert!(!rendered.contains("standing focus"));
    }

    #[test]
    fn a_role_starting_with_a_vowel_gets_the_right_article() {
        let overseer = prompt("lead", "overseer", "", &[], None, false, false);
        assert!(overseer.contains("an overseer"), "got {overseer}");
        let worker = prompt("hand", "worker", "", &[], None, false, false);
        assert!(worker.contains("a worker"), "got {worker}");
        assert_eq!(article(""), "a");
    }
}
