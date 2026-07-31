mod command;
mod editor;
mod guidance;
mod install;
mod launch;
mod memory;
mod relay;
mod roles;
mod session;
mod shell;
mod skills;

pub use install::{InstallStatus, destination, install, status};

pub enum Outcome {
    App,
    Exit(i32),
}

pub fn run(arguments: Vec<std::ffi::OsString>) -> anyhow::Result<Outcome> {
    command::run(arguments)
}
