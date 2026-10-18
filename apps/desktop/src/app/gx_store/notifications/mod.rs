//! The notification feed's reads and commands: `feed.rs` talks to gxserver (web-ready: `gx_rpc`
//! and gx-core only), `feeds.rs` holds every computer's feed and merges them into the bell, and
//! `commands.rs` performs what the bell's panel and the jump keys ask for on the computer that
//! owns the row.

mod commands;
mod feed;
mod feeds;

pub(crate) use feeds::NotificationFeedsHost;
