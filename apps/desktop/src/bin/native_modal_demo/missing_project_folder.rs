//! Preview host for the Missing Project Folder dialog. States: the default
//! short path, or `longpath` for a path that clips with an ellipsis. Locate
//! keeps the window open and, like the app, closes it after a simulated
//! successful relocation two seconds later.
use super::missing_project_folder_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;
use std::time::Duration;

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let project_path = if demo.state == "longpath" {
        "/Volumes/External Drive/Archive/2026/clients/acme-corporation/platform/services/ghostex-monorepo-with-a-very-long-name"
    } else {
        "/Users/story/dev/_active/Ghostex"
    };
    let host: MissingProjectFolderModalHost = Rc::new(move |command, cx: &mut App| match command {
        MissingProjectFolderModalCommand::Locate => {
            eprintln!("pickReplacementProjectFolder (window stays open; simulated close in 2s)");
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
        MissingProjectFolderModalCommand::Remove => {
            eprintln!("removeProject");
            cx.quit();
        }
        MissingProjectFolderModalCommand::Cancel => {
            eprintln!("cancel");
            cx.quit();
        }
    });
    let config = MissingProjectFolderModalConfig {
        project_name: "Ghostex".to_string(),
        project_path: project_path.to_string(),
        palette: demo.palette,
    };
    super::open_modal_window(
        MISSING_PROJECT_FOLDER_MODAL_WIDTH,
        MISSING_PROJECT_FOLDER_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiMissingProjectFolderModalWindow::new(config, host, window, cx))
        },
        cx,
    );
}
