/// A tool's row on the Connections page. The core type, not a copy of it: the
/// terminal dashboard draws the same reading, and two structs that happen to
/// agree today are two structs that stop agreeing the first time one of them
/// learns something.
pub use synapsecore::agent::Connection as Row;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Connections,
    Memories,
    Mesh,
    Console,
    Skills,
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
