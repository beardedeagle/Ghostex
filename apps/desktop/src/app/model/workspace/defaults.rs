use crate::*;

#[derive(Clone)]
pub(crate) struct WorkspaceModel {
    pub(crate) terminal_sessions: Vec<TerminalSession>,
    pub(crate) root: WorkspaceNode,
    pub(crate) focused_pane: WorkspacePaneId,
    /// Most recently focused panes, oldest first. Runtime-only; seeded with the restored focused pane.
    pub(crate) pane_focus_history: Vec<WorkspacePaneId>,
    pub(crate) focus_mode_pane: Option<WorkspacePaneId>,
    pub(crate) next_pane_id: u64,
    pub(crate) next_split_id: u64,
    pub(crate) next_session_id: u64,
}

impl WorkspaceModel {
    pub(crate) fn empty_default() -> Self {
        let pane_id = WorkspacePaneId(1);
        Self {
            terminal_sessions: Vec::new(),
            root: workspace_empty_leaf_node(pane_id),
            focused_pane: pane_id,
            pane_focus_history: vec![pane_id],
            focus_mode_pane: None,
            next_pane_id: 2,
            next_split_id: 1,
            next_session_id: 1,
        }
    }
}
