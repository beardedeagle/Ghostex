//! A workspace's apps through Executor (an open-source tool hub): Ghostex downloads one pinned
//! Executor build, runs a separate copy per workspace with its own data, port and keys, and
//! manages its apps, sign-ins and tools over its local API.

pub(super) mod agent;
mod apps;
mod client;
mod connections;
mod live;
mod ops;
mod process;
mod release;
mod settings;
mod tools;

pub(crate) use live::set_changed_listener;
pub(crate) use ops::{
    add_catalog_app, add_mcp_server, call_tool, connect_app, disable, enable, list_tools,
    remove_app, search_catalog, status,
};
pub(crate) use process::{start_idle_stop, stop_all};
