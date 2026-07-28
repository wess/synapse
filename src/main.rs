mod agent;
mod brain;
mod cli;
mod database;
mod files;
mod instructions;
mod mcp;
mod shellsetup;
mod ui;
mod vault;

fn main() -> anyhow::Result<()> {
    match cli::run(std::env::args_os().skip(1).collect())? {
        cli::Outcome::App => {
            ui::run();
            Ok(())
        }
        cli::Outcome::Exit(0) => Ok(()),
        cli::Outcome::Exit(code) => std::process::exit(code),
    }
}
