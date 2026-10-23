//! How long one page window open took, from the chat card's click to the page's first load: one
//! `gpui.visualPage.openTiming` line in the app-modal log (`gpui-app-modal-debug.jsonl`, written
//! while debug controls and the `gpui.app.modal` scenario are on), with each step's time since
//! the click. Nothing is written while the steps happen, so the log adds no time to the open.

use std::cell::RefCell;
use std::time::Instant;

struct OpenTiming {
    started: Instant,
    web_runtime_running: bool,
    steps: Vec<(&'static str, f64)>,
}

thread_local! {
    static OPEN: RefCell<Option<OpenTiming>> = const { RefCell::new(None) };
}

/// The card was clicked. `web_runtime_running`: the web runtime was already up, so no start
/// stands between the click and the page.
pub(crate) fn start(web_runtime_running: bool) {
    finish("replaced");
    OPEN.with_borrow_mut(|open| {
        *open = Some(OpenTiming {
            started: Instant::now(),
            web_runtime_running,
            steps: Vec::new(),
        });
    });
}

/// Records `step` the first time it happens during the open being timed.
pub(crate) fn mark(step: &'static str) {
    OPEN.with_borrow_mut(|open| {
        if let Some(open) = open
            && !open.steps.iter().any(|(seen, _)| *seen == step)
        {
            let ms = open.started.elapsed().as_secs_f64() * 1000.0;
            open.steps.push((step, (ms * 10.0).round() / 10.0));
        }
    });
}

/// Writes the open's line; `outcome` says how it ended (`loaded`, `closed`, ...).
pub(crate) fn finish(outcome: &'static str) {
    let Some(open) = OPEN.with_borrow_mut(Option::take) else {
        return;
    };
    let steps: Vec<_> = open
        .steps
        .iter()
        .map(|(step, ms)| serde_json::json!({ "step": step, "ms": ms }))
        .collect();
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::AppModal,
        "gpui.visualPage.openTiming",
        serde_json::json!({
            "outcome": outcome,
            "webRuntimeRunning": open.web_runtime_running,
            "steps": steps,
        }),
    );
}
