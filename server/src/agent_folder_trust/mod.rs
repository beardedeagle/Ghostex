//! Trust project folders automatically: gxserver marks the folder a session runs in as trusted in
//! its agent's own records before starting the agent, so no session stops at a folder-trust prompt.

mod files;
mod launch;
mod stores;

pub(crate) use launch::trust_session_folder_before_launch;
