//! What a person types at the mesh, and what it means.
//!
//! There are two surfaces where a human sits on the mesh rather than an agent —
//! `synapse mux` in a terminal and the Console in the desktop — and they have to
//! agree about two things that nothing else would catch if they drifted.
//!
//! The first is addressing. `@name`, `#channel`, `!` and a bare line going
//! wherever the focus is are a small grammar, and a small grammar implemented
//! twice is a small grammar that means two things by next year.
//!
//! The second is who counts as a person. `human = 1` is what tells every agent
//! on the mesh to ask this row questions and never delegate to it, and a headless
//! worker running with its permission prompts off has no other way to find
//! somebody to ask. So exactly one function sets it, and both surfaces call
//! that one.

use crate::relay::{Mesh, MessageKind, Registration};
use anyhow::Result;
use std::path::Path;

/// What a typed line turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// Whitespace. Nothing happened and nothing should be said about it.
    Blank,
    /// A slash command, without its leading `/`.
    Command(String),
    Message {
        kind: MessageKind,
        target: Option<String>,
        body: String,
    },
    /// Addressed at somebody, with nothing to say to them.
    Empty,
    /// Addressed at nobody. Carries the sentence to show, because the fix
    /// depends on the surface and neither surface should have to guess it.
    Undirected,
}

/// Read one typed line, with `focus` as where a bare line goes.
///
/// Deliberately total and side-effect free: it decides nothing about whether the
/// recipient exists, because a message to a name nobody answers to yet is how a
/// supervisor briefs a worker it is still starting.
pub fn read(line: &str, focus: Option<&str>) -> Line {
    let line = line.trim_end();
    if line.trim().is_empty() {
        return Line::Blank;
    }
    if let Some(rest) = line.strip_prefix('/') {
        return Line::Command(rest.trim().to_owned());
    }

    let (kind, target, body) = if let Some(rest) = line.strip_prefix('@') {
        let (name, body) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        (MessageKind::Direct, Some(name.to_owned()), body)
    } else if let Some(rest) = line.strip_prefix('#') {
        let (name, body) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        (MessageKind::Channel, Some(name.to_owned()), body)
    } else if let Some(rest) = line.strip_prefix('!') {
        (MessageKind::Broadcast, None, rest)
    } else {
        match focus {
            Some(name) => (MessageKind::Direct, Some(name.to_owned()), line),
            None => return Line::Undirected,
        }
    };

    let body = body.trim();
    if body.is_empty() {
        return Line::Empty;
    }
    // An `@` or `#` with no name is a target of empty string, which would be
    // delivered to nobody rather than refused.
    if target.as_deref().is_some_and(str::is_empty) {
        return Line::Undirected;
    }
    Line::Message {
        kind,
        target,
        body: body.to_owned(),
    }
}

/// What to say when a line was addressed at nobody. One sentence, and it names
/// every way out of the situation.
pub const UNDIRECTED: &str =
    "nobody is focused — use @name, #channel, or ! for everyone, or focus an agent first";

/// The name to join under when none was given.
///
/// A login name is what the person already answers to, and it is already the
/// shape a mesh name has to be. Shared for the same reason the rest of this
/// module is: a person who is `wess` in the terminal and `me` in the window is
/// two rows on the roster, and an agent asking the wrong one waits forever.
pub fn whoami() -> String {
    let candidate = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "me".to_owned());
    crate::relay::store::validname(&candidate).unwrap_or_else(|_| "me".to_owned())
}

/// Put a person on the roster.
///
/// The only place `human` is ever set. A tool calling `register` is always an
/// agent, whoever is sitting in front of it; this is for a surface that *is* the
/// person, and the flag is what stops every agent on the mesh from treating them
/// as somewhere to delegate work.
/// `surface` is what to record as the tool — "synapse mux", "Synapse" — so a
/// roster row says which of a person's own windows they are reachable at.
pub async fn arrive(mesh: &Mesh, name: &str, root: &Path, surface: &str) -> Result<()> {
    let taken = mesh
        .agents()
        .await?
        .into_iter()
        .find(|agent| agent.name == name);
    if let Some(existing) = taken {
        anyhow::ensure!(
            !existing.online || existing.human,
            "`{name}` is already on the mesh as an agent; pick another name"
        );
    }
    mesh.register(&Registration {
        name: name.to_owned(),
        role: "human".to_owned(),
        capabilities: String::new(),
        project: root.display().to_string(),
        tool: surface.to_owned(),
        human: true,
    })
    .await
}

/// Take a person back off it. Leaving quietly is worse than not arriving: a
/// roster row that is still there is one an agent will keep addressing
/// questions to.
pub async fn depart(mesh: &Mesh, name: &str) -> Result<()> {
    mesh.forget(name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(line: &str, focus: Option<&str>) -> (MessageKind, Option<String>, String) {
        match read(line, focus) {
            Line::Message { kind, target, body } => (kind, target, body),
            other => panic!("expected a message from {line:?}, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_line_goes_to_whoever_is_focused() {
        let (kind, target, body) = message("and the index too", Some("backend"));
        assert_eq!(kind, MessageKind::Direct);
        assert_eq!(target.as_deref(), Some("backend"));
        assert_eq!(body, "and the index too");

        assert_eq!(read("and the index too", None), Line::Undirected);
    }

    #[test]
    fn the_three_prefixes_address_one_agent_a_channel_and_everybody() {
        let (kind, target, body) = message("@backend add a default", None);
        assert_eq!(kind, MessageKind::Direct);
        assert_eq!(target.as_deref(), Some("backend"));
        assert_eq!(body, "add a default");

        let (kind, target, _) = message("#build freezing the schema", None);
        assert_eq!(kind, MessageKind::Channel);
        assert_eq!(target.as_deref(), Some("build"));

        let (kind, target, body) = message("!stop and report", None);
        assert_eq!(kind, MessageKind::Broadcast);
        assert_eq!(target, None);
        assert_eq!(body, "stop and report");
    }

    #[test]
    fn a_prefix_beats_the_focus_for_that_line_only() {
        let (_, target, _) = message("@qa run it", Some("backend"));
        assert_eq!(target.as_deref(), Some("qa"));
        // The focus is not consumed: the next bare line still goes to it.
        let (_, target, _) = message("carry on", Some("backend"));
        assert_eq!(target.as_deref(), Some("backend"));
    }

    #[test]
    fn a_slash_command_keeps_its_arguments_and_loses_its_slash() {
        assert_eq!(
            read("/focus backend", None),
            Line::Command("focus backend".into())
        );
        assert_eq!(read("/quit", Some("backend")), Line::Command("quit".into()));
    }

    #[test]
    fn nothing_is_sent_for_an_address_with_no_message() {
        assert_eq!(read("@backend", None), Line::Empty);
        assert_eq!(read("@backend   ", None), Line::Empty);
        assert_eq!(read("!", None), Line::Empty);
    }

    #[test]
    fn an_address_with_no_name_is_refused_rather_than_sent_to_nobody() {
        assert_eq!(read("@ hello", None), Line::Undirected);
        assert_eq!(read("# hello", None), Line::Undirected);
    }

    #[test]
    fn whitespace_is_nothing_and_says_nothing() {
        assert_eq!(read("", Some("backend")), Line::Blank);
        assert_eq!(read("   \t ", Some("backend")), Line::Blank);
    }

    #[test]
    fn a_body_keeps_its_own_punctuation() {
        // The split is on the first whitespace, so everything after it is the
        // message — including another `@`, which is text and not a second
        // recipient.
        let (_, target, body) = message("@lead ask @qa about the flake", None);
        assert_eq!(target.as_deref(), Some("lead"));
        assert_eq!(body, "ask @qa about the flake");
    }
}
