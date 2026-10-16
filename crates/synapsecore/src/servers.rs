//! The MCP server library: one place to add a server, and every connected tool
//! given it.
//!
//! Each tool keeps its own list of MCP servers in its own format, so the same
//! server is configured three times and its token sits in plaintext in three
//! files. Here a server is described once, in Synapse's own folder, with its
//! secrets held as vault references. What a tool is handed is
//! `synapse mcp run <name>`, written through the tool's own `mcp add`.
//!
//! That is not a proxy. `run` reads the library, fetches the secrets, and
//! replaces itself with the real server, so the tool talks to the server
//! directly and keeps everything its own client does — per-server toggles,
//! permissions, OAuth for whatever it supports natively. Synapse is in the path
//! for the instant it takes to start one.

mod adopt;
mod install;
mod launch;
mod library;
mod model;
pub mod registry;
mod secrets;

pub use adopt::{adopt, foreign};
pub use install::{Holding, Placement, install, ours, survey, uninstall};
pub use launch::{environment, run};
pub use library::{all, delete, directory, read, save};
pub use model::{Server, validname};
pub use secrets::{forgetsecret, setsecret};
