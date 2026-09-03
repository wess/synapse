use crate::relay::Source;
use std::path::PathBuf;

/// Which tool with behaviour beyond a descriptor this is, or that it is none
/// of them.
///
/// This is not the tool's identity — that is [`Agent::slug`], and it is a string
/// because there is no fixed set of them. What this narrows is the two pieces of
/// a connection that are behaviour rather than configuration and so cannot live
/// in a descriptor: Claude Code's session hook and status line, and pi's
/// extension write-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Codex,
    Claude,
    /// pi, which reaches Synapse through a package rather than an MCP client of
    /// its own. Everything a connection means elsewhere — the tools, the
    /// startup notice, the status line — arrives with that package.
    Pi,
    /// A tool somebody described themselves. It gets everything a built-in gets
    /// except the two behaviours above, which no descriptor can express.
    Custom,
}

/// A connectable tool: where it keeps its files, what to run to connect it, how
/// to read that back, and how to start it. Built from a descriptor — see
/// [`crate::agent::tool`] — including for the tools Synapse ships.
#[derive(Debug, Clone)]
pub struct Agent {
    pub kind: Kind,
    /// The descriptor's file name, and what a person types: `codex`, `hermes`.
    pub slug: String,
    pub name: String,
    pub command: String,
    pub instructions: PathBuf,
    pub settings: PathBuf,
    pub integration: PathBuf,
    /// Where this tool reads personal Agent Skills from.
    pub skills: PathBuf,
    /// Where it reads a project's own Agent Skills from, relative to that
    /// project's root. Empty for a tool that has no such place, which is a fact
    /// about the tool and not a misconfiguration: a project skill simply has
    /// nowhere to go there.
    pub projectskills: String,
    pub connect: super::tool::Connect,
    pub detect: super::tool::Detect,
    pub launch: super::tool::Launch,
    /// Which layer the descriptor resolved from, so the dashboard can say
    /// whether a tool is built in or somebody's own.
    pub source: Source,
}

impl Agent {
    /// The package a connection installs, for a tool whose connection is a
    /// package rather than an MCP server entry. The override exists for the same
    /// reason every real path has one: without it the integration tests would
    /// have to reach the registry, and somebody working on the package could not
    /// point a connection at their checkout.
    pub fn package(&self) -> String {
        if !self.detect.env.is_empty()
            && let Ok(value) = std::env::var(&self.detect.env)
            && !value.is_empty()
        {
            return value;
        }
        self.detect.source.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Detection {
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub registered: bool,
    pub configured: bool,
    /// What the tool's own settings say about the Synapse session notice and
    /// status line. Only Claude Code supports them, so it stays empty elsewhere.
    pub hooks: crate::agent::HookState,
}

impl Detection {
    pub fn missing() -> Self {
        Self {
            executable: None,
            version: None,
            registered: false,
            configured: false,
            hooks: crate::agent::HookState::default(),
        }
    }
}
