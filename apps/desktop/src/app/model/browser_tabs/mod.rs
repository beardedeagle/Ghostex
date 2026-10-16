//! Browser tab model: ids and tree types, runtime surface policies, the BrowserTabModel (tabs, then
//! pane layout), and the tab, tab group and navigation history helpers.

// C1 wave-3 re-cluster: browser tab/pane/split/leaf/node model types, tab drag state, navigation history, and favicon types, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

pub(crate) mod device_view;
pub(crate) mod model_layout;
pub(crate) mod model_tabs;
pub(crate) mod runtime_policy;
pub(crate) mod tab_and_history;
pub(crate) mod types;

pub(crate) use device_view::*;
pub(crate) use runtime_policy::*;
pub(crate) use tab_and_history::*;
pub(crate) use types::*;
