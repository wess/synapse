mod ui;
#[cfg(all(feature = "voice", target_os = "macos"))]
mod voice;

use synapsecore::cli::{self, Outcome};

fn main() -> anyhow::Result<()> {
    match cli::run(std::env::args_os().skip(1).collect())? {
        Outcome::App => {
            ui::run();
            Ok(())
        }
        Outcome::Exit(0) => Ok(()),
        Outcome::Exit(code) => std::process::exit(code),
    }
}
