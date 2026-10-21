use super::*;
use crate::*;

impl CommandPaneModel {
    fn find_split(&self, split_id: CommandPaneSplitId) -> Option<&CommandPaneSplit> {
        find_command_split(&self.root, split_id)
            .or_else(|| find_command_split(&self.view_root, split_id))
    }

    fn find_split_mut(&mut self, split_id: CommandPaneSplitId) -> Option<&mut CommandPaneSplit> {
        if find_command_split(&self.root, split_id).is_some() {
            find_command_split_mut(&mut self.root, split_id)
        } else {
            find_command_split_mut(&mut self.view_root, split_id)
        }
    }

    pub(crate) fn split_ratio(&self, split_id: CommandPaneSplitId) -> Option<f32> {
        self.find_split(split_id)
            .map(|split| workspace_split_ratio(split.ratio))
    }

    pub(crate) fn set_split_ratio(&mut self, split_id: CommandPaneSplitId, ratio: f32) -> bool {
        let next_ratio = workspace_split_ratio(ratio);
        let Some(split) = self.find_split_mut(split_id) else {
            return false;
        };

        if (workspace_split_ratio(split.ratio) - next_ratio).abs() < 0.001 {
            return false;
        }

        split.ratio = next_ratio;
        true
    }

    pub(crate) fn reset_split_ratio(&mut self, split_id: CommandPaneSplitId) -> bool {
        let Some(default_ratio) = self
            .find_split(split_id)
            .map(|split| command_split_native_default_ratio(split).unwrap_or(0.5))
        else {
            return false;
        };
        self.set_split_ratio(split_id, default_ratio)
    }

    pub(crate) fn split_drag_ratio_bounds(
        &self,
        split_id: CommandPaneSplitId,
        content_span: f32,
    ) -> Option<(f32, f32)> {
        let split = self.find_split(split_id)?;
        let minimum = split_pane_resize_minimum_for_axis(split.axis);
        split_drag_ratio_bounds_from_minimums(
            command_node_axis_pane_count(&split.first, split.axis) as f32 * minimum,
            command_node_axis_pane_count(&split.second, split.axis) as f32 * minimum,
            content_span,
        )
    }

    pub(crate) fn allocate_group_id(&mut self) -> CommandPaneGroupId {
        let group_id = CommandPaneGroupId(self.next_group_id);
        self.next_group_id += 1;
        group_id
    }

    pub(crate) fn allocate_split_id(&mut self) -> CommandPaneSplitId {
        let split_id = CommandPaneSplitId(self.next_split_id);
        self.next_split_id += 1;
        split_id
    }

    pub(crate) fn allocate_session_id(&mut self) -> CommandSessionId {
        let session_id = CommandSessionId(self.next_session_id);
        self.next_session_id += 1;
        session_id
    }

    pub(crate) fn collapse(&mut self) {
        self.mode = CommandPaneMode::Collapsed;
        self.focus_mode_group = None;
        self.resize_drag = None;
    }

    pub(crate) fn expand(&mut self) {
        self.mode = CommandPaneMode::Pinned;
    }

    pub(crate) fn toggle_expanded(&mut self) {
        if self.is_expanded() {
            self.collapse();
        } else {
            self.expand();
        }
    }

    pub(crate) fn reset_height_from_shared_settings(
        &mut self,
        content_height: f32,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        self.reset_height_with_default_height_px(
            content_height,
            command_pane_default_height_px_from_shared_settings(settings),
        );
    }

    pub(crate) fn reset_height_with_default_height_px(
        &mut self,
        content_height: f32,
        default_height_px: f32,
    ) {
        self.height_ratio = command_pane_default_height_ratio_for_default_height_px(
            default_height_px,
            content_height,
        );
        self.resize_drag = None;
    }

    pub(crate) fn reset_width_to_default(&mut self) {
        self.width_ratio = COMMAND_PANE_DEFAULT_WIDTH_RATIO;
        self.resize_drag = None;
    }
}
