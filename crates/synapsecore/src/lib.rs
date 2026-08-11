//! Synapse without a window.
//!
//! Everything the product does other than draw: memory, credentials, the mesh,
//! skills, tool setup, the MCP server, and the whole command surface. The
//! desktop crate links this and adds a GPUI window over it; a terminal-only
//! machine builds this crate on its own and gets the same commands.

pub mod agent;
pub mod brain;
pub mod cli;
pub mod crashes;
pub mod database;
pub mod files;
pub mod imports;
pub mod instructions;
pub mod mcp;
pub mod relay;
pub mod shellsetup;
pub mod skill;
pub mod sync;
/// The terminal dashboard. Behind a default-on feature so an embedder that
/// wants the library and not a screen does not link a rendering stack.
#[cfg(feature = "tui")]
pub mod tui;
pub mod vault;
