//! The usage-limit card while every account is full.
//!
//! CDXC:AgentProviders 2026-10-11 DECISION:
//! User, about a session that sat quietly at its usage limit while every account was full: "dont know what u mean but fix this best way i trust u". While gxserver's recovery waits with no account that has room, it carries `accountWait` (`limit`, `resetsAt`, `account`, each optional) on the session's usage-limit notice, and the card turns into a warning that says which limit was hit, that every account is full, and when work continues: at the soonest reset among the accounts, or as soon as an account has room. The card keeps the usage-limit notice's own Switch account button, and it goes away with the notice when the session switches or resumes.
//! SEE-ALSO: server/src/accounts/limit_wait.rs (the facts and the bell row), apps/desktop/src/app/native_chat/notice.rs and apps/mobile/app/src/chat/native/cards/NoticeCard.tsx (both draw the decorated notice as is).

use serde_json::{json, Map, Value};

use crate::extras::save_markdown_paths::civil_from_days;
use crate::jstime::parse_iso_millis;
use crate::menus::accounts_data::mask_account_text;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Rewrites a usage-limit notice that carries `accountWait` into the every-account-full warning.
pub fn decorate(
    object: &mut Map<String, Value>,
    hide_account_emails: bool,
    now_ms: f64,
    utc_offset_minutes: i32,
) {
    if object.get("kind").and_then(Value::as_str) != Some("usageLimit") {
        return;
    }
    let Some(wait) = object
        .get("accountWait")
        .and_then(Value::as_object)
        .cloned()
    else {
        return;
    };
    let text = |key: &str| {
        wait.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let limit = text("limit").unwrap_or("Usage limit");
    let reset = text("resetsAt")
        .and_then(|at| parse_iso_millis(at, utc_offset_minutes))
        .filter(|at| *at > now_ms);
    let when = match (reset, text("account")) {
        (Some(at), Some(account)) => {
            let account = if hide_account_emails {
                mask_account_text(account)
            } else {
                account.to_string()
            };
            format!(
                "It continues automatically {}, when {account}\u{2019}s limit resets.",
                local_time(at, now_ms, utc_offset_minutes)
            )
        }
        _ => "It continues automatically as soon as an account has room.".to_string(),
    };
    object.insert(
        "title".to_string(),
        json!(format!("{limit} reached on every account")),
    );
    object.insert(
        "detail".to_string(),
        json!(format!(
            "Every account this session can switch to is full, so it waits instead of switching. {when}"
        )),
    );
    object.insert("tone".to_string(), json!("warning"));
}

/// `at 12:00 PM`, `tomorrow at 12:00 PM` or `on Oct 18 at 2:00 PM`, in the user's own timezone.
fn local_time(at_ms: f64, now_ms: f64, utc_offset_minutes: i32) -> String {
    let offset = f64::from(utc_offset_minutes) * 60_000.0;
    let local = at_ms + offset;
    let day = (local / 86_400_000.0).floor() as i64;
    let today = ((now_ms + offset) / 86_400_000.0).floor() as i64;
    let minute_of_day = ((local - day as f64 * 86_400_000.0) / 60_000.0).floor() as i64;
    let (hour, minute) = (minute_of_day / 60, minute_of_day % 60);
    let clock = format!(
        "{}:{minute:02} {}",
        if hour % 12 == 0 { 12 } else { hour % 12 },
        if hour < 12 { "AM" } else { "PM" }
    );
    if day == today {
        format!("at {clock}")
    } else if day == today + 1 {
        format!("tomorrow at {clock}")
    } else {
        let (_, month, date) = civil_from_days(day);
        let month = MONTHS
            .get(usize::try_from(month - 1).unwrap_or_default())
            .copied()
            .unwrap_or_default();
        format!("on {month} {date} at {clock}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(wait: Value) -> Map<String, Value> {
        json!({
            "kind": "usageLimit",
            "severity": "warning",
            "title": "Usage limit reached",
            "accountWait": wait,
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    // 2026-10-11T11:52:21Z, in Asia/Dubai (UTC+4).
    const NOW: f64 = 1_791_719_541_000.0;
    const DUBAI: i32 = 240;

    #[test]
    fn names_the_limit_the_account_and_the_local_reset() {
        let mut object = notice(json!({
            "limit": "7d limit",
            "resetsAt": "2026-10-12T08:00:00+00:00",
            "account": "sharptabs.com@gmail.com",
        }));
        decorate(&mut object, false, NOW, DUBAI);
        assert_eq!(object["title"], "7d limit reached on every account");
        assert_eq!(object["tone"], "warning");
        assert_eq!(
            object["detail"],
            "Every account this session can switch to is full, so it waits instead of switching. It continues automatically tomorrow at 12:00 PM, when sharptabs.com@gmail.com\u{2019}s limit resets."
        );
    }

    #[test]
    fn falls_back_without_a_known_reset_and_masks_emails() {
        let mut object = notice(json!({ "limit": null, "resetsAt": null, "account": null }));
        decorate(&mut object, true, NOW, DUBAI);
        assert_eq!(object["title"], "Usage limit reached on every account");
        assert!(object["detail"]
            .as_str()
            .unwrap()
            .ends_with("as soon as an account has room."));

        let mut object = notice(json!({
            "limit": "5h limit",
            "resetsAt": "2026-10-11T12:30:00Z",
            "account": "sharptabs.com@gmail.com",
        }));
        decorate(&mut object, true, NOW, DUBAI);
        let detail = object["detail"].as_str().unwrap();
        assert!(detail.contains("at 4:30 PM"));
        assert!(!detail.contains("sharptabs.com@gmail.com"));
    }

    #[test]
    fn leaves_other_notices_alone() {
        let mut object = notice(json!({ "limit": "7d limit" }));
        object.insert("kind".to_string(), json!("loginExpired"));
        decorate(&mut object, false, NOW, DUBAI);
        assert_eq!(object["title"], "Usage limit reached");
        let mut object = notice(Value::Null);
        decorate(&mut object, false, NOW, DUBAI);
        assert_eq!(object["title"], "Usage limit reached");
        assert!(object.get("tone").is_none());
    }
}
