use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Codex,
    Claude,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub kind: Kind,
    pub name: &'static str,
    pub command: &'static str,
    pub instructions: PathBuf,
    pub settings: PathBuf,
    pub integration: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Detection {
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub registered: bool,
    pub configured: bool,
}

impl Detection {
    pub fn missing() -> Self {
        Self {
            executable: None,
            version: None,
            registered: false,
            configured: false,
        }
    }
}
