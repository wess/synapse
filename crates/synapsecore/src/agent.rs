mod catalog;
mod config;
mod detect;
mod guidance;
mod hooks;
mod model;
mod setup;
mod teardown;

pub use catalog::agents;
pub use detect::{command, detect, searchpath};
pub use guidance::{GuidanceState, adopt, pointermatches, state as guidancestate, sync};
pub use hooks::{State as HookState, apply as applynotice, remove as removenotice};
pub use model::{Agent, Detection, Kind};
pub use setup::setup;
pub use teardown::{Removed, disconnect};
