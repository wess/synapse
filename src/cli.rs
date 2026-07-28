mod command;
mod install;
mod shell;

pub use install::{InstallStatus, destination, install, status};

pub enum Outcome {
    App,
    Exit(i32),
}

pub fn run(arguments: Vec<std::ffi::OsString>) -> anyhow::Result<Outcome> {
    command::run(arguments)
}
