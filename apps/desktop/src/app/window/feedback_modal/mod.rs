//! The Send Bug / Feedback pop-up the More options menu opens: write, review the exact issue,
//! send it through gxserver to the Ghostex feedback relay, then link to the issue it created.

mod images;
mod offline_issue;
mod render;
mod state;

pub(crate) use offline_issue::FeedbackScrubContext;
pub(crate) use state::*;
