//! When a message was sent, in words the voice agent can say as they are ("5 minutes ago, at
//! 3:40 PM"), in this computer's time zone, so the model never does date math itself.

use chrono::{DateTime, Datelike, Local, TimeZone};

/// `timestamp_ms` (milliseconds since the epoch) relative to `now`.
pub(super) fn spoken(timestamp_ms: i64, now: DateTime<Local>) -> Option<String> {
    let at = Local.timestamp_millis_opt(timestamp_ms).single()?;
    let clock = at.format("%-I:%M %p").to_string();
    let minutes = (now - at).num_minutes();
    let days = (now.date_naive() - at.date_naive()).num_days();
    Some(match (minutes, days) {
        (..=0, _) => "just now".to_string(),
        (1, _) => format!("a minute ago, at {clock}"),
        (2..=59, _) => format!("{minutes} minutes ago, at {clock}"),
        (60..=119, 0) => format!("an hour ago, at {clock}"),
        (_, 0) => format!("{} hours ago, at {clock}", minutes / 60),
        (_, 1) => format!("yesterday at {clock}"),
        (_, 2..=6) => format!("on {} at {clock}", at.format("%A")),
        _ if at.year() == now.year() => format!("on {} at {clock}", at.format("%-d %B")),
        _ => format!("on {} at {clock}", at.format("%-d %B %Y")),
    })
}
