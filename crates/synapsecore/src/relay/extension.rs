//! The pi extension, carried in the binary.
//!
//! Claude Code and Codex can be handed a Synapse server for the life of one
//! process, which is what lets `synapse launch` work on a machine where
//! `synapse connect` was never run. pi has no MCP client to hand a server to:
//! what it reads is an extension, and an extension has to exist as a file
//! before pi can be pointed at it. So the package ships inside the binary too,
//! and a launch writes it into Synapse's own folder and passes `--extension`.
//!
//! The files below are the package under `pi/` — the same source npm publishes,
//! included rather than copied, because two copies of an extension in one
//! repository is two answers to what `recall` does. Nothing is written into
//! pi's own configuration by any of this; a permanent connection is
//! `synapse connect pi`, which is a separate decision.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Every file of the extension, in the layout pi expects to find them in.
///
/// Reaching outside the crate for these is deliberate: `pi/` is what npm
/// publishes and what a contributor edits, and a second copy inside the crate
/// would be the copy that quietly falls behind.
const FILES: &[(&str, &str)] = &[
    (
        "index.ts",
        include_str!("../../../../pi/extensions/synapse/index.ts"),
    ),
    (
        "binary.ts",
        include_str!("../../../../pi/extensions/synapse/binary.ts"),
    ),
    (
        "client.ts",
        include_str!("../../../../pi/extensions/synapse/client.ts"),
    ),
    (
        "command.ts",
        include_str!("../../../../pi/extensions/synapse/command.ts"),
    ),
    (
        "commands.ts",
        include_str!("../../../../pi/extensions/synapse/commands.ts"),
    ),
    (
        "guidance.ts",
        include_str!("../../../../pi/extensions/synapse/guidance.ts"),
    ),
    (
        "render.ts",
        include_str!("../../../../pi/extensions/synapse/render.ts"),
    ),
    (
        "session.ts",
        include_str!("../../../../pi/extensions/synapse/session.ts"),
    ),
    (
        "tools.ts",
        include_str!("../../../../pi/extensions/synapse/tools.ts"),
    ),
];

/// Write the extension out and return the entry point to point pi at.
///
/// Rewritten on every launch rather than checked, so an upgraded Synapse does
/// not leave a session running last release's extension.
pub fn write() -> Result<PathBuf> {
    let directory = super::directory()?.join("pi").join("synapse");
    for (name, content) in FILES {
        crate::files::write(&directory.join(name), content)
            .with_context(|| format!("could not write the pi extension file `{name}`"))?;
    }
    Ok(directory.join(entry()))
}

fn entry() -> &'static Path {
    Path::new(FILES[0].0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Adding a file to the package and forgetting it here would ship a launch
    /// that half-loads: pi resolves the import, finds nothing, and the session
    /// starts with no Synapse tools and no explanation.
    #[test]
    fn every_file_of_the_package_is_carried() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../pi/extensions/synapse")
            .canonicalize()
            .expect("the pi package should sit beside the crates");
        let mut written: Vec<String> = std::fs::read_dir(&source)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".ts"))
            .collect();
        written.sort();
        let mut carried: Vec<String> = FILES.iter().map(|(name, _)| (*name).to_owned()).collect();
        carried.sort();

        assert_eq!(carried, written);
    }

    #[test]
    fn the_entry_point_is_what_pi_is_pointed_at() {
        assert_eq!(entry(), Path::new("index.ts"));
        assert!(FILES[0].1.contains("export default"));
    }
}
