//! The files list's type filter: the button that replaced the search box's magnifier, its menu of
//! file types, and the tree it draws while types are picked.
//!
//! CDXC:Docs 2026-10-25 DECISION:
//! User: "make this filter instead of search icon and make that icon a button so i can pick html/markdown/excalidraw/image/video so i can find those files faster". Several types can be on at once (each checked in the menu, "All files" clears them); with types on, the tree and the search results show only those files, with folders only where they hold one, and typing still narrows by name. The button takes the accent colour with a dot while types are on, its tooltip names them, and the choice stays while Files is open. The project Search view does not use it.

use std::cell::Cell;

use gpui::{Bounds, Context, Pixels, Window};
use serde_json::json;

use super::actions::action;
use super::files::DocsTreeRow;
use super::state::{
    BROWSER_IMAGE_EXTENSIONS, DocsEntryKind, IMAGE_EXTENSIONS, SYSTEM_APP_IMAGE_EXTENSIONS,
    VIDEO_EXTENSIONS,
};
use crate::GhostexGpuiApp;
use crate::app::context_menu::GpuiContextMenu;

thread_local! {
    /// Where the filter button is, so its menu opens below it.
    pub(crate) static FILTER_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DocsTypeFilter {
    Html,
    Markdown,
    Excalidraw,
    Image,
    Video,
}

/// Records the filter button's bounds for its menu.
pub(crate) fn filter_anchor() -> impl gpui::IntoElement {
    use gpui::Styled as _;
    gpui::canvas(
        |bounds, _, _| FILTER_MENU_ANCHOR.with(|cell| cell.set(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

impl DocsTypeFilter {
    pub(crate) const ALL: [Self; 5] = [
        Self::Html,
        Self::Markdown,
        Self::Excalidraw,
        Self::Image,
        Self::Video,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Markdown => "Markdown",
            Self::Excalidraw => "Excalidraw",
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Excalidraw => "excalidraw",
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    /// The extensions a type covers: the ones Files opens as that kind (`DocsFileKind` and
    /// `DocsMediaKind` in state.rs), pictures and videos it hands to the system app included.
    fn extensions(self) -> Vec<&'static str> {
        match self {
            Self::Html => vec!["html", "htm"],
            Self::Markdown => vec!["md", "mdx", "markdown"],
            Self::Excalidraw => vec!["excalidraw"],
            Self::Image => IMAGE_EXTENSIONS
                .iter()
                .chain(BROWSER_IMAGE_EXTENSIONS)
                .chain(SYSTEM_APP_IMAGE_EXTENSIONS)
                .copied()
                .collect(),
            Self::Video => VIDEO_EXTENSIONS.to_vec(),
        }
    }
}

/// The folders above each matching file, then the file, in path order: the tree the filter draws
/// when no name is typed. `collapsed` folders hide what is under them.
pub(crate) fn filtered_tree_rows(
    files: &[super::state::DocsEntry],
    collapsed: &std::collections::BTreeSet<String>,
) -> Vec<DocsTreeRow> {
    let mut files: Vec<&super::state::DocsEntry> = files
        .iter()
        .filter(|entry| entry.kind == DocsEntryKind::File)
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut rows = Vec::new();
    let mut open_folders: Vec<String> = Vec::new();
    for file in files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let display_parts: Vec<&str> = file.display_path.split('/').collect();
        let folders = parts.len().saturating_sub(1);
        // Keep the folders this file shares with the previous one, open the rest.
        let shared = open_folders
            .iter()
            .zip(&parts)
            .take_while(|(open, part)| open.as_str() == **part)
            .count()
            .min(folders);
        open_folders.truncate(shared);
        for depth in shared..folders {
            open_folders.push(parts[depth].to_string());
            let path = parts[..=depth].join("/");
            let hidden = (0..depth).any(|above| collapsed.contains(&parts[..=above].join("/")));
            if hidden {
                continue;
            }
            let display_path = if display_parts.len() == parts.len() {
                display_parts[..=depth].join("/")
            } else {
                path.clone()
            };
            rows.push(DocsTreeRow {
                expanded: !collapsed.contains(&path),
                name: display_parts
                    .get(depth)
                    .copied()
                    .unwrap_or(parts[depth])
                    .to_string(),
                path,
                display_path,
                kind: DocsEntryKind::Directory,
                depth,
                folder: None,
            });
        }
        let hidden = (0..folders).any(|above| collapsed.contains(&parts[..=above].join("/")));
        if !hidden {
            rows.push(DocsTreeRow {
                path: file.path.clone(),
                display_path: file.display_path.clone(),
                name: file.name.clone(),
                kind: DocsEntryKind::File,
                depth: folders,
                expanded: false,
                folder: None,
            });
        }
    }
    rows
}

impl GhostexGpuiApp {
    pub(crate) fn native_docs_type_filter_active(&self) -> bool {
        !self.native_docs.type_filter.is_empty()
    }

    /// The picked types' extensions, for the search request.
    pub(crate) fn native_docs_type_filter_extensions(&self) -> Vec<&'static str> {
        self.native_docs
            .type_filter
            .iter()
            .flat_map(|kind| kind.extensions())
            .collect()
    }

    /// What the search answered for, so a change of query or types reads as "Searching…".
    pub(crate) fn native_docs_search_key(&self) -> String {
        let types: Vec<&str> = self
            .native_docs
            .type_filter
            .iter()
            .map(|kind| kind.id())
            .collect();
        format!(
            "{}\u{0}{}",
            self.native_docs.search_query.trim(),
            types.join(",")
        )
    }

    /// The filter button's tooltip: the types it shows, or what it does.
    pub(crate) fn native_docs_type_filter_tooltip(&self) -> String {
        if self.native_docs.type_filter.is_empty() {
            return "Filter by file type".to_string();
        }
        let names: Vec<&str> = self
            .native_docs
            .type_filter
            .iter()
            .map(|kind| kind.label())
            .collect();
        format!("Showing {} files", names.join(", "))
    }

    /// The filter button's menu: each type with a check, then All files.
    pub(crate) fn show_native_docs_type_filter_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut menu = GpuiContextMenu::new();
        for kind in DocsTypeFilter::ALL {
            menu = menu.menu_with_check(
                kind.label(),
                self.native_docs.type_filter.contains(&kind),
                action(json!({ "type": "typeFilter", "filter": kind.id() })),
            );
        }
        let menu = menu.separator().menu_with_check(
            "All files",
            self.native_docs.type_filter.is_empty(),
            action(json!({ "type": "typeFilterClear" })),
        );
        let trigger = FILTER_MENU_ANCHOR.with(|cell| cell.get());
        self.native_docs_show_menu(menu, trigger, true, window, cx);
    }

    /// A menu row: turns one type on or off, or clears them (`id` None).
    pub(crate) fn native_docs_set_type_filter(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        match id.and_then(DocsTypeFilter::from_id) {
            Some(kind) => {
                if !self.native_docs.type_filter.remove(&kind) {
                    self.native_docs.type_filter.insert(kind);
                }
            }
            None => self.native_docs.type_filter.clear(),
        }
        self.native_docs.type_filter_collapsed.clear();
        self.native_docs_schedule_search(cx);
        self.native_docs_notify(cx);
    }
}
