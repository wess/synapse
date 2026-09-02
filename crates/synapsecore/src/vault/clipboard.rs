use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// What owns the pasteboard. Overridable so a test can prove a value arrived
/// without taking over the clipboard of whoever is running it.
fn program() -> String {
    std::env::var("SYNAPSE_CLIPBOARD").unwrap_or_else(|_| "pbcopy".to_owned())
}

/// The value goes over stdin, never argv: an argument is readable in `ps` by
/// every process on the machine, which is the same reason `secret set` refuses
/// to take one.
pub fn copy(value: &str) -> Result<()> {
    let program = program();
    let mut child = Command::new(&program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not run {program}"))?;
    child
        .stdin
        .take()
        .context("the clipboard command took no input")?
        .write_all(value.as_bytes())
        .with_context(|| format!("could not write to {program}"))?;
    let status = child
        .wait()
        .with_context(|| format!("could not wait for {program}"))?;
    anyhow::ensure!(status.success(), "{program} exited with {status}");
    Ok(())
}
