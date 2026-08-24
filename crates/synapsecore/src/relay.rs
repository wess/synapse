mod bus;
mod extension;
mod harness;
mod launch;
pub(crate) mod layer;
mod model;
mod process;
pub mod role;
pub(crate) mod store;
pub mod team;
mod worker;

pub use bus::{
    PARKSECONDS, PROGRESSSECONDS, ack, awaitmessages, awaitstatus, deliver, reportstatus,
};
pub use launch::{Launch, Options, launch};
pub use layer::{Source, valid as validlayername};
pub use model::{AgentView, ChannelView, Message, MessageKind, Registration, WorkerView};
pub use store::Mesh;
pub use worker::{DEFAULTWORKERS, Spec, Supervisor, WORKERCEILING, reapstrays};

/// Where the mesh keeps the files a launched agent needs: the MCP config handed
/// to a tool that is not connected yet, and one log per background worker. Under
/// the data directory, so `SYNAPSE_DATA` keeps tests hermetic.
pub fn directory() -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::files::data()?.join("relay"))
}
