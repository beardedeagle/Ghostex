//! Floating permission bar preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (waiting
//! for Screen Recording), `granted` (it just landed), `finished` (both permissions are on).
use super::permission_guide_bar::*;
use gpui::{App, AppContext as _, Context, IntoElement, Render, Window};

struct PermissionGuideDemo {
    state: String,
}

impl Render for PermissionGuideDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, detail, landed, step, steps) = match self.state.as_str() {
            "granted" => (
                "Screen Recording is on",
                "Next: Accessibility.",
                true,
                Some(2),
                [BarStep::Todo, BarStep::Done],
            ),
            "finished" => (
                "SpaceO is all set",
                "SpaceO restarts once the agents using it finish.",
                true,
                None,
                [BarStep::Done, BarStep::Done],
            ),
            _ => (
                "Screen Recording for SpaceO",
                "Switch on spaceo. Not listed? Drag the tile in.",
                false,
                Some(2),
                [BarStep::Done, BarStep::Current],
            ),
        };
        render_bar(
            BarContent {
                title: title.to_string(),
                detail: detail.to_string(),
                file_name: "spaceo".to_string(),
                landed,
                step,
                steps,
            },
            |_: &mut Self, _, _| eprintln!("drag spaceo"),
            |_, _, cx| cx.quit(),
            cx,
        )
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let _ = super::open_modal_window(
        BAR_WIDTH,
        BAR_HEIGHT,
        move |_, cx| cx.new(|_| PermissionGuideDemo { state }),
        cx,
    );
}
