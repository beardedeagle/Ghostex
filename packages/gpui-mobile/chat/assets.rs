//! The chat's asset source: the desktop's `assets.rs` with the folders a chat reads, named from this
//! crate's manifest (the desktop's relative `#[folder]`s do not resolve from here). The `assets/`
//! entry beside this file is a symlink to `apps/desktop/src/assets`, whose three modules turn the
//! shared chat artwork JSON into SVGs.
//!
//! `debug-embed` is on for `rust-embed` (Cargo.toml): a phone cannot read the repository at run time,
//! so even a debug build carries its files.
use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[path = "assets/chat_message_actions.rs"]
mod chat_message_actions;
#[path = "assets/chat_references.rs"]
pub(crate) mod chat_references;
#[path = "assets/chat_working.rs"]
pub(crate) mod chat_working;

#[derive(RustEmbed)]
#[folder = "../../../apps/desktop/assets"]
#[include = "titlebar/**/*.svg"]
#[include = "modals/**/*.svg"]
struct GhostexEmbeddedAssets;

#[derive(RustEmbed)]
#[folder = "../../../packages/core-ui/assets"]
#[include = "*.svg"]
struct GhostexAgentIconAssets;

/// The desktop `assets.rs`'s DM Sans faces, which the shared `native_chat/fonts.rs` registers.
pub(crate) static DM_SANS_UPRIGHT_FACES: [&[u8]; 4] = [
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/400.ttf"),
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/500.ttf"),
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/600.ttf"),
    include_bytes!("../../../.dependencies/app-fonts/dm-sans/static/700.ttf"),
];

pub(crate) struct GhostexAssets;

impl AssetSource for GhostexAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(key) = path.strip_prefix("chat-references/") {
            return chat_references::asset(key)
                .map(|svg| Some(Cow::Borrowed(svg)))
                .ok_or_else(|| anyhow!("unknown chat reference asset {key:?}"));
        }
        if let Some(key) = path.strip_prefix("chat-actions/") {
            return chat_message_actions::asset(key)
                .map(|svg| Some(Cow::Owned(svg.into_bytes())))
                .ok_or_else(|| anyhow!("unknown message action asset {key:?}"));
        }
        if let Some(key) = path.strip_prefix("chat-working/") {
            return chat_working::asset(key)
                .map(|svg| Some(Cow::Owned(svg.into_bytes())))
                .ok_or_else(|| anyhow!("unknown working strip asset {key:?}"));
        }
        if path.starts_with("titlebar/") || path.starts_with("modals/") {
            return GhostexEmbeddedAssets::get(path)
                .map(|asset| Some(asset.data))
                .ok_or_else(|| anyhow!("could not find embedded Ghostex asset at path {path:?}"));
        }
        if let Some(agent_icon_path) = path.strip_prefix("agent-icons/") {
            return GhostexAgentIconAssets::get(agent_icon_path)
                .map(|asset| Some(asset.data))
                .ok_or_else(|| anyhow!("could not find embedded agent icon at path {path:?}"));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = GhostexEmbeddedAssets::iter()
            .filter_map(|asset| asset.starts_with(path).then(|| asset.into()))
            .collect::<Vec<_>>();
        assets.extend(GhostexAgentIconAssets::iter().filter_map(|asset| {
            let asset = format!("agent-icons/{asset}");
            asset.starts_with(path).then(|| asset.into())
        }));
        assets.extend(gpui_component_assets::Assets.list(path)?);
        assets.sort_unstable();
        assets.dedup();
        Ok(assets)
    }
}
