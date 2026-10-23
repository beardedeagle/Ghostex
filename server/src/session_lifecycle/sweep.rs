use super::*;

/*
The periodic sweep. One pass owns three rules, applied in order to at most one
durable write per session:

1. Spent-snooze collection (see `SNOOZE_WAKE_RETENTION_MS`). Attention NEVER
   clears a snooze here: a snoozed session that raises its hand stays snoozed
   server-side and the client surfaces it, so the user's return ticket is not
   spent by the agent asking a question. This rule (and rule 2) runs for every
   user regardless of `sidebarVersion`: someone can snooze rows in V2 and then
   switch back to V1, and that state still has to be groomed.
2. Activity reset. Real activity newer than the override's stamp clears ANY
   override — a settled session wakes, an "active" pin unpins — mirroring
   activity-driven session unsetting rule.
3. Auto-settle, from either of two triggers: the inactivity window, or a merged
   or closed pull request on the session's branch. Runs only for Sidebar V2
   users (see `read_sweep_auto_settle_after_days`; a V1 machine passes
   `auto_settle_after_days: None` and `auto_settle_on_finished_pull_request:
   false`, which stops at rules 1 and 2). Both triggers share one set of guards:
   skip working/attention sessions (the same guards the settle command
   enforces), skip explicitly snoozed sessions (snoozed is not settled: the wake
   time is a stronger statement about when the row matters), and skip "active"
   pins. Either auto-settle deliberately leaves `settledAt` NULL: the settled
   shelf sorts unstamped rows by the meaningful-activity clock, so an automatic
   settle reads as "work ended then", not "the daemon noticed now".

Only sessions the sidebar can actually show (running/sleeping) are considered, so
a backlog of long-dead stopped rows never produces a delta storm, and those rows
are never even read.

`resolve_pull_request` is injected rather than read from the git-status cache
here so the rules stay testable without a probe cache, and so the sweep never
does I/O of its own: gxserver's background pass owns the probing, this pass only
reads its answer.
*/
pub fn run_session_lifecycle_sweep(
    repository: &DomainRepository<'_>,
    options: &SessionLifecycleSweepOptions,
    resolve_pull_request: &dyn Fn(&Value) -> PullRequestDisposition,
) -> Result<SessionLifecycleSweepOutcome, DomainStateError> {
    let now_iso = options.now_iso.as_str();
    let now_ms = parse_iso_ms(now_iso).ok_or_else(|| {
        DomainStateError::corrupt_state(format!("Invalid gxserver timestamp: {now_iso}."))
    })?;
    let mut outcome = SessionLifecycleSweepOutcome::default();
    // A stopped row is never active, so leaving it in SQL keeps the same rows in the same order
    // (and the same `max_mutations` cut) without hydrating the closed backlog every minute.
    for session in repository.list_sessions_excluding_stopped(None)? {
        if outcome.changed.len() >= options.max_mutations {
            break;
        }
        if !is_active(&session) {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        let current = SessionLifecycleFields::from_session(&session);
        let pull_request = if options.auto_settle_on_finished_pull_request {
            resolve_pull_request(&session)
        } else {
            PullRequestDisposition::Unknown
        };
        let next = resolve_swept_lifecycle(&session, &current, options, now_ms, pull_request);
        if next == current {
            continue;
        }
        let project_id = project_id.to_string();
        let session_id = session_id.to_string();
        repository.write_session_lifecycle(&project_id, &session_id, &next, now_iso)?;
        outcome.changed.push((project_id, session_id));
    }
    Ok(outcome)
}

/// Pure sweep decision for one session, split out so the rules are testable
/// without a database.
pub fn resolve_swept_lifecycle(
    session: &Value,
    current: &SessionLifecycleFields,
    options: &SessionLifecycleSweepOptions,
    now_ms: i64,
    pull_request: PullRequestDisposition,
) -> SessionLifecycleFields {
    let now_iso = options.now_iso.as_str();
    let mut next = current.clone();

    // 1. Spent-snooze collection.
    match next.snoozed_until.as_deref().map(str::trim) {
        None => {
            // An orphan `snoozedAt` without a wake time is unreachable state.
            next.snoozed_at = None;
        }
        Some(until) => match parse_iso_ms(until) {
            // Malformed data never hides a session, so it never lingers either.
            None => next.clear_snooze(),
            Some(wake_at_ms) if now_ms >= wake_at_ms + SNOOZE_WAKE_RETENTION_MS => {
                next.clear_snooze()
            }
            Some(_) => {}
        },
    }

    let activity = session_activity(session, now_iso);
    let last_activity_ms = session_last_activity_ms(session, now_iso);

    // 2. Activity reset of the settle override.
    if next.settled_override.is_some() || next.settled_at.is_some() {
        let stamped_ms = next.settled_override_at.as_deref().and_then(parse_iso_ms);
        if let (Some(stamped_ms), Some(last_activity_ms)) = (stamped_ms, last_activity_ms) {
            if last_activity_ms > stamped_ms {
                next.clear_settle();
            }
        }
    }

    // 3. Auto-settle: a finished pull request, or the inactivity window.
    if options.auto_settle_after_days.is_none() && !options.auto_settle_on_finished_pull_request {
        return next;
    }
    if next.settled_override.is_some() || next.settled_at.is_some() {
        return next;
    }
    if activity != "idle" || is_snoozed_by_clock(&next, now_ms) {
        return next;
    }
    /*
    A merged or closed pull request is a completion signal, not a staleness one,
    so it does not wait out the inactivity window — the branch's work is over the
    moment the forge says so. It still passes through every guard above: a
    session that is working or waiting on the user is never parked behind a
    settle, a snooze outranks it, and an "active" pin suppresses it until real
    activity clears the pin (rule 2).
    */
    let has_finished_pull_request = options.auto_settle_on_finished_pull_request
        && pull_request == PullRequestDisposition::Finished;
    let is_inactive = match (options.auto_settle_after_days, last_activity_ms) {
        (Some(auto_settle_after_days), Some(last_activity_ms)) => {
            let window_ms = (auto_settle_after_days * DAY_MS as f64) as i64;
            last_activity_ms < now_ms - window_ms
        }
        _ => false,
    };
    if has_finished_pull_request || is_inactive {
        next.settled_override = Some("settled".to_string());
        next.settled_override_at = Some(now_iso.to_string());
    }
    next
}
