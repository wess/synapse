use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Codex,
    Claude,
    /// pi, which reaches Synapse through a package rather than an MCP client of
    /// its own. Everything a connection means elsewhere — the tools, the
    /// startup notice, the status line — arrives with that package.
    Pi,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub kind: Kind,
    pub name: &'static str,
    pub command: &'static str,
    pub instructions: PathBuf,
    pub settings: PathBuf,
    pub integration: PathBuf,
    /// Where this tool reads personal Agent Skills from.
    pub skills: PathBuf,
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
