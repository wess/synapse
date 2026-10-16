//! Where skill and MCP searches look.
//!
//! A source is typed, because what Synapse can do with one depends entirely on
//! what it is: an index that speaks skills.sh's API, a registry that speaks the
//! MCP Registry's, or a GitHub repository holding `SKILL.md` folders and
//! `server.json` files — which is how a team publishes its own.
//!
//! Sources layer the way tool descriptors do. Two are built in. A person's own
//! list in the data folder replaces them once it exists, so a built-in can be
//! removed. A repository can carry `.synapse/sources.toml`, which adds to that
//! — but only once somebody has approved its exact contents, the same bargain
//! `.synapse.yaml` makes, because a file somebody else committed decides where
//! your searches go and whose instructions come back.

pub mod github;
mod model;
mod store;

pub use model::{Kind, Origin, Source};
pub use store::{
    PROJECTFILE, add, allow, infer, project, projectfile, remove, resolved, token, userfile,
};
