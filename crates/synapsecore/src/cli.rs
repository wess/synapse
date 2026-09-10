mod command;
mod connect;
mod doctor;
mod editor;
mod guidance;
mod install;
mod launch;
mod layers;
mod memory;
mod mux;
mod relay;
mod remove;
mod session;
mod shell;
mod skills;
pub(crate) mod tokens;
mod wrap;

pub use install::{InstallStatus, destination, install, status};
// The terminal dashboard is the only caller, and it is behind a feature. An
// embedder taking the library without a screen would otherwise be told about an
// unused import in somebody else's crate.
#[cfg(feature = "tui")]
pub(crate) use layers::describetool;

pub enum Outcome {
    App,
    Exit(i32),
}

pub fn run(arguments: Vec<std::ffi::OsString>) -> anyhow::Result<Outcome> {
    command::run(arguments)
}
