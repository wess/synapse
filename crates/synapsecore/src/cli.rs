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
mod wrap;

pub use install::{InstallStatus, destination, install, status};

pub enum Outcome {
    App,
    Exit(i32),
}

pub fn run(arguments: Vec<std::ffi::OsString>) -> anyhow::Result<Outcome> {
    command::run(arguments)
}
