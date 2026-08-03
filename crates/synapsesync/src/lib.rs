//! What the Synapse client and the sync server both have to agree on.
//!
//! The server is deliberately blind. It stores an opaque identifier and a
//! sealed envelope, hands back whatever it was given, and can neither read a
//! memory nor tell whether an operation stored one or removed one. Everything
//! it is allowed to understand lives in [`wire`]; everything it is not lives
//! in [`record`], behind [`envelope`].

pub mod envelope;
pub mod op;
pub mod record;
pub mod wire;

pub use envelope::{Key, open, seal};
pub use op::{Numbered, Op, opid, uid};
pub use record::{Record, Scope};
pub use wire::{PROTOCOL, PullRequest, PullResponse, PushRequest, PushResponse};
