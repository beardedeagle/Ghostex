//! Coordinators: one agent session the user talks to, which starts, briefs and supervises thread
//! sessions and reports back. The supervisor's clock is `server/coordinator_runtime.rs`.

mod brief;
mod create;
mod delivery;
mod endpoint;
mod panel;
mod playbook;
mod presentation;
mod promote;
mod records;
mod rename;
mod restore;
mod role;
mod sleep_threads;
mod state;
mod title;

pub use brief::*;
pub use create::*;
pub use delivery::*;
pub use endpoint::*;
pub use panel::*;
pub use playbook::*;
pub use presentation::*;
pub use promote::*;
pub use records::*;
pub use rename::*;
pub use restore::*;
pub use role::*;
pub use sleep_threads::*;
pub use state::*;
pub use title::*;
