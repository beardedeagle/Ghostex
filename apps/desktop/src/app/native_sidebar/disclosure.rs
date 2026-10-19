use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::fold_motion::{FoldMotion, fold_duration, render_fold};
use gpui::AnyElement;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct SidebarDisclosures {
    items: HashMap<String, Disclosure>,
}
struct Disclosure {
    motion: FoldMotion,
    session_ids: Vec<String>,
}
impl SidebarDisclosures {
    pub(crate) fn sync(&mut self, snapshot: &NativeSidebarSnapshot, reduce_motion: bool) {
        let duration = fold_duration(
            snapshot.hud["settings"]["sidebarCollapseAnimationDurationMs"]
                .as_u64()
                .unwrap_or_default(),
            reduce_motion,
        );
        // CDXC:Sidebar 2026-09-20 WHY:
        // Keyed rather than a list scanned per held item: with the sidebar list built in Rust this
        // runs on every change the list draws, a focus step during a held hotkey included, and the
        // scan was one comparison per drawn heading per held heading.
        let mut next: HashMap<String, (bool, Vec<String>)> = HashMap::new();
        for group in &snapshot.groups {
            next.insert(
                format!("group:{}", group.group_id),
                (group.collapsed, Vec::new()),
            );
            for section in &group.sections {
                next.insert(
                    format!("section:{}:{}", group.group_id, section.id),
                    (section.collapsed, section.session_ids.clone()),
                );
            }
        }
        for collection in &snapshot.collections {
            next.insert(
                format!("collection:{}", collection.collection_id),
                (collection.collapsed, Vec::new()),
            );
        }
        self.items.retain(|key, _| next.contains_key(key));
        for (key, (collapsed, ids)) in next {
            if let Some(item) = self.items.get_mut(&key) {
                item.motion.fold_to(collapsed, duration);
                if !collapsed {
                    item.session_ids = ids;
                }
            } else {
                self.items.insert(
                    key,
                    Disclosure {
                        motion: FoldMotion::settled(collapsed),
                        session_ids: ids,
                    },
                );
            }
        }
    }
    pub(crate) fn present(&self, key: &str, collapsed: bool) -> bool {
        !collapsed
            || self
                .items
                .get(key)
                .is_some_and(|item| item.motion.animating())
    }
    pub(crate) fn section_ids<'a>(
        &'a self,
        key: &str,
        current: &'a [String],
        collapsed: bool,
    ) -> &'a [String] {
        if collapsed {
            self.items
                .get(key)
                .map(|item| item.session_ids.as_slice())
                .unwrap_or(current)
        } else {
            current
        }
    }
}
impl GhostexGpuiApp {
    pub(crate) fn render_native_disclosure(
        &self,
        key: String,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let motion = self
            .native_sidebar
            .disclosures
            .items
            .get(&key)
            .map(|item| &item.motion);
        render_fold(
            cx.entity().clone(),
            motion,
            move |app: &mut GhostexGpuiApp| {
                app.native_sidebar
                    .disclosures
                    .items
                    .get_mut(&key)
                    .map(|item| &mut item.motion)
            },
            content,
        )
    }
}
