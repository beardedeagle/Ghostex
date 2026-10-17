use gpui::{Bounds, FocusHandle, Pixels, Point};
use serde_json::Value;

/// Where a panel sits relative to its anchor point.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum SidebarMenuPlacement {
    /// Centred across the sidebar at the anchor's height: submenus, and menus with nothing to hang
    /// from (a right-click on the list's empty space).
    #[default]
    Pointer,
    /// Hung from what opened it, a sidebar row or a button (`owner`): `gap` below it, and above it
    /// instead when the sidebar has no room below. Across, its left edge (or, with `align_right`,
    /// its right edge) is on `from`'s: the button that opened it, a zero-width span at a
    /// right-click's pointer, or the owner itself; the other edge of `from` when that side runs
    /// out of sidebar.
    Attached {
        owner: Bounds<Pixels>,
        from: Bounds<Pixels>,
        align_right: bool,
        gap: Pixels,
    },
}

#[derive(Clone)]
pub(crate) struct SidebarMenuPanel {
    pub(crate) items: Vec<Value>,
    pub(crate) scroll: gpui::ScrollHandle,
    pub(crate) anchor: Point<Pixels>,
    pub(crate) placement: SidebarMenuPlacement,
    pub(crate) selected: Option<usize>,
    pub(crate) pages: Vec<Vec<Value>>,
    /// Border-box height of the rendered rows, measured after layout.
    pub(crate) measured_height: Option<Pixels>,
    /// The row of this panel that opened the panel stacked on top of it, if one is open.
    pub(crate) child_item: Option<usize>,
}

pub(crate) struct SidebarMenuState {
    pub(crate) window: gpui::AnyWindowHandle,
    pub(crate) account_panel: Option<(String, usize)>,
    pub(crate) panels: Vec<SidebarMenuPanel>,
    pub(crate) focus: FocusHandle,
    pub(crate) previous_focus: Option<FocusHandle>,
    /// The button that opened the menu, when a button did: pressing it again closes the menu.
    pub(crate) trigger: Option<String>,
    pub(crate) scale: f32,
    /// Keeps `window`'s tooltips hidden while the menu is up (`Root::suppress_tooltips`). `None`
    /// only while the pointer is on a row's (i), whose tooltip is the one exception.
    pub(crate) tooltips: Option<gpui::TooltipSuppression>,
}

impl SidebarMenuPanel {
    pub(crate) fn new(items: Vec<Value>, anchor: Point<Pixels>) -> Self {
        Self {
            items,
            scroll: Default::default(),
            anchor,
            placement: SidebarMenuPlacement::Pointer,
            selected: None,
            pages: vec![],
            measured_height: None,
            child_item: None,
        }
    }

    /// Swaps the rows in place; the old selection and measured height describe rows that are gone.
    pub(crate) fn replace_items(&mut self, items: Vec<Value>) {
        self.items = items;
        self.selected = None;
        self.measured_height = None;
        self.child_item = None;
    }

    pub(crate) fn is_agent_launcher(&self) -> bool {
        self.items
            .first()
            .is_some_and(|item| item["menuStyle"] == "agentLauncher")
    }

    /// CDXC:ContextMenus 2026-09-21 WHY:
    /// Compact session menus stay at 178px (nested 204px) to match React, but Configure Machines wraps onto a second line there. Grow to the longest label so every row stays on one line without widening short menus.
    fn fitted_width(&self, nested: bool) -> f32 {
        if self.is_agent_launcher() {
            return super::agent_launcher_menu::AGENT_LAUNCHER_MENU_WIDTH;
        }
        let min = if nested { 204.0 } else { 178.0 };
        // Panel padding 6+6, border 1+1, row inset 10+10, icon 16, gap 8.
        const CHROME: f32 = 58.0;
        // 13px UI font; slightly wide so a label cannot wrap after rounding.
        const PX_PER_CHAR: f32 = 8.0;
        self.items
            .iter()
            .filter(|item| item["separator"] != true)
            .map(|item| {
                let label = item["label"].as_str().unwrap_or("");
                let suffix = item["suffix"].as_str().unwrap_or("");
                let mut extra = 0.0;
                if item.get("children").is_some()
                    || item["command"]["type"] == "sessionAccounts"
                        && item["command"]["action"] == "load"
                {
                    extra += 18.0;
                }
                if item.get("secondary").is_some() {
                    extra += 36.0;
                }
                if item["checked"] == true {
                    extra += 22.0;
                }
                if item["supportsChat"] == true {
                    extra += 18.0;
                }
                CHROME
                    + extra
                    + (label.chars().count() + suffix.chars().count()) as f32 * PX_PER_CHAR
            })
            .fold(min, f32::max)
            .ceil()
    }

    /// CDXC:AgentLauncher 2026-09-19 DECISION:
    /// User: the GPUI Select Agent menu and its account page must fit every account without cutting off the last row, like the React sidebar.
    /// The panel takes the measured height of its rows; the per-row estimate only places the first frame, because two-line account rows and wrapped hints outgrow any fixed row height.
    pub(crate) fn bounds(
        &self,
        sidebar: Bounds<Pixels>,
        scale: f32,
        nested: bool,
    ) -> Bounds<Pixels> {
        let margin = gpui::px(12.0 * scale);
        let launcher = self.is_agent_launcher();
        let width = gpui::px(self.fitted_width(nested) * scale)
            .min((sidebar.size.width - margin * 2.0).max(gpui::px(0.0)));
        let estimate = if launcher {
            super::agent_launcher_menu::estimated_height(&self.items, width, scale)
        } else {
            gpui::px(
                (14.0
                    + self
                        .items
                        .iter()
                        .map(|item| {
                            if item["separator"] == true {
                                13.0
                            } else if item["heading"] == true {
                                24.0
                            } else {
                                34.0
                            }
                        })
                        .sum::<f32>())
                    * scale,
            )
        };
        let height = self
            .measured_height
            .unwrap_or(estimate)
            .min((sidebar.size.height - margin * 2.0).max(gpui::px(0.0)));
        if let SidebarMenuPlacement::Attached {
            owner,
            from,
            align_right,
            gap,
        } = self.placement
        {
            // No margin against the sidebar's sides: the owner sits inside them, and the panel's
            // edge has to meet the owner's exactly.
            let fits = |left: Pixels| left >= sidebar.left() && left + width <= sidebar.right();
            let (preferred, flipped) = if align_right {
                (from.right() - width, from.left())
            } else {
                (from.left(), from.right() - width)
            };
            let left = if fits(preferred) || !fits(flipped) {
                preferred
            } else {
                flipped
            }
            .min(sidebar.right() - width)
            .max(sidebar.left());
            let below = owner.bottom() + gap;
            let above = owner.top() - gap - height;
            let top = if below + height <= sidebar.bottom() - margin {
                below
            } else if above >= sidebar.top() + margin {
                above
            } else if sidebar.bottom() - owner.bottom() >= owner.top() - sidebar.top() {
                below.min(sidebar.bottom() - height - margin)
            } else {
                above.max(sidebar.top() + margin)
            };
            return Bounds {
                origin: Point::new(left, top),
                size: gpui::size(width, height),
            };
        }
        // The React launcher sits right-aligned under its chevron; other sidebar menus are centered in the sidebar.
        let left = if launcher {
            (self.anchor.x - width)
                .min(sidebar.right() - width - margin)
                .max(sidebar.left() + margin)
        } else {
            sidebar.left() + (sidebar.size.width - width) / 2.0
        };
        Bounds {
            origin: Point::new(
                left,
                self.anchor
                    .y
                    .min(sidebar.bottom() - height - margin)
                    .max(sidebar.top() + margin),
            ),
            size: gpui::size(width, height),
        }
    }

    pub(crate) fn move_selection(&mut self, key: &str) {
        let enabled: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item["separator"] != true && item["heading"] != true && item["disabled"] != true
            })
            .map(|(index, _)| index)
            .collect();
        if enabled.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|selected| enabled.iter().position(|index| *index == selected));
        let index = match key {
            "home" => 0,
            "end" => enabled.len() - 1,
            "up" => current
                .map(|index| (index + enabled.len() - 1) % enabled.len())
                .unwrap_or(enabled.len() - 1),
            _ => current
                .map(|index| (index + 1) % enabled.len())
                .unwrap_or(0),
        };
        self.selected = Some(enabled[index]);
        self.scroll.scroll_to_item(enabled[index]);
    }
}

impl SidebarMenuState {
    /// Whether the button `trigger` opened this menu.
    pub(crate) fn opened_by(&self, trigger: &str) -> bool {
        self.trigger.as_deref() == Some(trigger)
    }

    pub(crate) fn refresh(&mut self, snapshot: &super::model::NativeSidebarSnapshot) {
        fn collect(value: &Value, items: &mut std::collections::HashMap<String, Value>) {
            match value {
                Value::Array(values) => {
                    for value in values {
                        collect(value, items);
                    }
                }
                Value::Object(object) => {
                    if let Some(command) = object.get("command") {
                        items.insert(command.to_string(), value.clone());
                    }
                    if let Some(children) = object.get("children") {
                        collect(children, items);
                    }
                }
                _ => {}
            }
        }
        let mut current = std::collections::HashMap::new();
        collect(&snapshot.more_menu, &mut current);
        for group in &snapshot.groups {
            collect(&group.menu, &mut current);
        }
        for collection in &snapshot.collections {
            collect(&collection.menu, &mut current);
        }
        for panel in &mut self.panels {
            for item in panel
                .items
                .iter_mut()
                .chain(panel.pages.iter_mut().flatten())
            {
                if let Some(updated) = item
                    .get("command")
                    .and_then(|command| current.get(&command.to_string()))
                {
                    for key in ["label", "checked", "disabled"] {
                        if let Some(value) = updated.get(key) {
                            item[key] = value.clone();
                        }
                    }
                }
            }
        }
    }
}
