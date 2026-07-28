use crate::agent::{Agent, Detection};

#[derive(Clone)]
pub struct Row {
    pub agent: Agent,
    pub detection: Detection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Connections,
    Memories,
    Vaults,
    Settings,
}

#[derive(Clone)]
pub enum Notice {
    Ready,
    Success(String),
    Error(String),
}

impl Notice {
    pub fn message(&self) -> &str {
        match self {
            Notice::Ready => "Memory stays on this Mac and is shared only with tools you connect.",
            Notice::Success(message) | Notice::Error(message) => message,
        }
    }
}
