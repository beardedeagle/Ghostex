//! Preview host for the File diff dialog (`git-file-diff`), which Files opens to compare a
//! document with the copy on disk. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): the default is the
//! Storybook diff; `split`, `wrap` and `long`. It takes `+`-joined pointer steps after its state,
//! run 600ms after it shows its diff: `drag:x1,y1,x2,y2` drags a selection, `click2:x,y` and
//! `click3:x,y` double and triple click, `wheel:x,y,dy` scrolls, `copy` prints what Ctrl/Cmd+C would
//! copy and `ctrlc` presses it and prints the clipboard, then puts the old clipboard back (e.g.
//! `wrap+drag:150,260,300,330+copy`).
use super::file_diff_modal::*;
use gpui::{AnyWindowHandle, App, AppContext as _, Keystroke};
use std::rc::Rc;
use std::time::Duration;

fn long_patch() -> String {
    let mut lines = vec![
        "diff --git a/src/big.rs b/src/big.rs".to_string(),
        "@@ -1,400 +1,400 @@".to_string(),
    ];
    for index in 0..600 {
        lines.push(match index % 4 {
            0 => format!("+    let value_{index} = compute({index});"),
            1 => format!("-    let value_{index} = old_compute({index});"),
            _ => format!("     // unchanged line {index}"),
        });
    }
    lines.join("\n")
}

/// A `+`-joined state: the named states first, then the pointer steps.
fn split_state(state: &str) -> (Vec<String>, Vec<String>) {
    state
        .split('+')
        .map(str::to_string)
        .partition(|part| !is_pointer_step(part))
}

fn is_pointer_step(part: &str) -> bool {
    part == "copy"
        || part == "ctrlc"
        || ["drag:", "click2:", "click3:", "wheel:"]
            .iter()
            .any(|prefix| part.starts_with(prefix))
}

fn numbers(list: &str) -> Vec<f32> {
    list.split(',')
        .filter_map(|n| n.trim().parse().ok())
        .collect()
}

fn mouse_move(
    window: &mut gpui::Window,
    position: gpui::Point<gpui::Pixels>,
    pressed_button: Option<gpui::MouseButton>,
    cx: &mut App,
) {
    window.dispatch_event(
        gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position,
            pressed_button,
            modifiers: gpui::Modifiers::default(),
        }),
        cx,
    );
}

fn mouse_button(
    window: &mut gpui::Window,
    position: gpui::Point<gpui::Pixels>,
    down: bool,
    click_count: usize,
    cx: &mut App,
) {
    let (button, modifiers) = (gpui::MouseButton::Left, gpui::Modifiers::default());
    let input = if down {
        gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
            button,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        })
    } else {
        gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
            button,
            position,
            modifiers,
            click_count,
        })
    };
    window.dispatch_event(input, cx);
}

/// One pointer step: a drag in eight moves, a double or triple click, a wheel scroll, or a copy
/// printout.
fn pointer_step(step: &str, window: &mut gpui::Window, cx: &mut App) {
    let at = |n: &[f32], i: usize| gpui::point(gpui::px(n[i]), gpui::px(n[i + 1]));
    if let Some(list) = step.strip_prefix("drag:") {
        let n = numbers(list);
        if n.len() != 4 {
            return;
        }
        let (start, end) = (at(&n, 0), at(&n, 2));
        mouse_move(window, start, None, cx);
        mouse_button(window, start, true, 1, cx);
        for index in 1..=8 {
            let t = index as f32 / 8.0;
            let position = gpui::point(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            );
            mouse_move(window, position, Some(gpui::MouseButton::Left), cx);
        }
        mouse_button(window, end, false, 1, cx);
    } else if let Some((clicks, list)) = step
        .strip_prefix("click2:")
        .map(|list| (2, list))
        .or_else(|| step.strip_prefix("click3:").map(|list| (3, list)))
    {
        let n = numbers(list);
        if n.len() != 2 {
            return;
        }
        let position = at(&n, 0);
        mouse_move(window, position, None, cx);
        for click_count in 1..=clicks {
            mouse_button(window, position, true, click_count, cx);
            mouse_button(window, position, false, click_count, cx);
        }
    } else if let Some(list) = step.strip_prefix("wheel:") {
        let n = numbers(list);
        if n.len() != 3 {
            return;
        }
        let position = at(&n, 0);
        mouse_move(window, position, None, cx);
        window.dispatch_event(
            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(n[2]))),
                modifiers: gpui::Modifiers::default(),
                touch_phase: gpui::TouchPhase::Moved,
            }),
            cx,
        );
    } else if step == "ctrlc" {
        let before = cx.read_from_clipboard();
        let chord = if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        };
        window.dispatch_keystroke(Keystroke::parse(chord).expect("a valid chord"), cx);
        let copied = cx.read_from_clipboard().and_then(|item| item.text());
        eprintln!("ctrl-c {copied:?}");
        if let Some(before) = before {
            cx.write_to_clipboard(before);
        }
    } else if step == "copy" {
        let text = gpui_component::TextSelection::selected_text(window, cx);
        eprintln!("copy {:?}", text.trim());
    }
    window.refresh();
}

/// Runs the pointer steps 600ms after the dialog drew its diff (wrapped rows need a frame to
/// learn their width), 150ms apart.
fn run_pointer_steps(window: AnyWindowHandle, steps: Vec<String>, cx: &mut App) {
    if steps.is_empty() {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(600))
            .await;
        for step in steps {
            cx.update(|cx| {
                let _ = window.update(cx, |_root, window, cx| pointer_step(&step, window, cx));
            });
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
        }
    })
    .detach();
}

/// The palette the app would pass: tinted by the chrome colour, and frosted when
/// `GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1` (a translucent fill stands in for the app's blurred one).
fn demo_palette(demo: &super::DemoEnv) -> super::native_modal_kit::ModalPalette {
    let palette = demo.palette.tinted(demo.palette.surface);
    if super::env("GHOSTEX_NATIVE_MODAL_DEMO_GLASS") == "1" {
        let fill = super::native_modal_kit::rgba_of(palette.surface, 0.72);
        return palette.frosted(fill);
    }
    palette
}

pub(super) fn open_file_diff(demo: &super::DemoEnv, cx: &mut App) {
    let palette = demo_palette(demo);
    let (names, steps) = split_state(&demo.state);
    let patch = if names.iter().any(|name| name == "long") {
        long_patch()
    } else {
        [
            "@@ -12,7 +12,9 @@",
            " export const modalStories = [",
            "-  'Settings',",
            "+  'Settings',",
            "+  'Session Note',",
            "+  'Export Transcript',",
            "   'Add Project',",
        ]
        .join("\n")
    };
    let draft = GitFileDiffDraft {
        file_path: "packages/core-ui/modal-gallery/overview.stories.tsx".to_string(),
        patch,
        additions: Some(12),
        deletions: Some(5),
    };
    let host: GitFileDiffModalHost = Rc::new(|command, cx: &mut App| match command {
        GitFileDiffModalCommand::Close => {
            eprintln!("close");
            cx.quit();
        }
    });
    let state = names.first().cloned().unwrap_or_default();
    let (window, view) = super::open_large_modal_window(
        GIT_FILE_DIFF_MODAL_WIDTH,
        GIT_FILE_DIFF_MODAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiGitFileDiffModalWindow::new(draft, palette, host, window, cx))
        },
        cx,
    );
    let prefs = match state.as_str() {
        "split" => Some(GitDiffPrefs {
            view_mode: GitDiffViewMode::Split,
            ..GitDiffPrefs::default()
        }),
        "wrap" => Some(GitDiffPrefs {
            line_wrap: true,
            ..GitDiffPrefs::default()
        }),
        _ => None,
    };
    if let Some(prefs) = prefs {
        let _ = window.update(cx, |_root, _window, cx| {
            view.update(cx, |modal, cx| modal.preview_prefs(prefs, cx));
        });
    }
    run_pointer_steps(window.into(), steps, cx);
}
