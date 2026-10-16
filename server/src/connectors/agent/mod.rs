//! Connectors as agents use them: the `ghostex connectors mcp` server every Claude and Codex
//! session loads asks gxserver for its workspace's connector tools and runs each call here, where
//! a call that would change something waits for Ghostex's card. Split by concern; the route
//! functions are re-exported.

mod api;
mod calls;
mod cards;
mod launch;
mod rules;
mod session;

pub(crate) use api::*;
pub(crate) use cards::answer_from_chat as answer_connector_card_from_chat;
pub(crate) use launch::{with_connector_tools, CONNECTORS_MCP_SERVER_NAME};
