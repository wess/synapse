mod command;
mod connect;
mod doctor;
mod drift;
mod editor;
mod eval;
mod guidance;
mod install;
mod launch;
mod layers;
mod learning;
mod memory;
mod mux;
mod plugins;
mod relay;
mod remove;
mod savings;
mod servers;
mod session;
mod shell;
mod skills;
mod sources;
pub(crate) mod tokens;
mod wrap;

pub use install::{InstallStatus, destination, install, status};
pub enum Outcome {
    App,
    Exit(i32),
}

pub fn run(arguments: Vec<std::ffi::OsString>) -> anyhow::Result<Outcome> {
    command::run(arguments)
}
