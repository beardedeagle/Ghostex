use std::time::{Duration, Instant};

use ghostex_gx_client::{ClientDiagnostic, StartError, redact_quoted_values};
use ghostex_gx_core::{ConnectionUpdate, Core, Loadable, MachineId, ResubscribeReason};
use serde_json::json;

use super::focus_perform::FocusPerformCounters;
use super::focus_publish::FocusPublishCounters;
use super::host::GxStoreCounters;
use super::*;

impl GxStoreDiagnostics {
    /// One line per (re)load of the local machine: the first one is the store coming up.
    pub(in crate::app::gx_store) fn store_loaded(
        &mut self,
        core: &Core,
        counters: &GxStoreCounters,
        since_connect: Option<Duration>,
    ) {
        let store = core.presentation();
        let Some(loaded) = store.loaded(&MachineId::Local) else {
            return;
        };
        append(
            "gxStore.loaded",
            json!({
                "first": counters.reloads == 1,
                "revision": loaded.revision,
                "projects": loaded.projects().len(),
                "groups": loaded.groups().len(),
                "sessions": loaded.session_count(),
                "tabsGeneration": core.tabs_generation(),
                "connectToLoadedMs": since_connect.map(|elapsed| elapsed.as_millis() as u64),
                "clientStarts": counters.client_starts,
                "reloads": counters.reloads,
            }),
        );
    }

    /// One line per (re)load of a remote machine: the moment its rows reach the store.
    pub(in crate::app::gx_store) fn remote_machine_loaded(
        &mut self,
        core: &Core,
        machine: &MachineId,
    ) {
        let Some(loaded) = core.presentation().loaded(machine) else {
            return;
        };
        record(
            "gxStore.remote.loaded",
            json!({
                "machineId": log_text(machine.remote_id().unwrap_or_default()),
                "revision": loaded.revision,
                "projects": loaded.projects().len(),
                "groups": loaded.groups().len(),
                "sessions": loaded.session_count(),
            }),
        );
    }

    /// A remote client's thread is gone although nobody stopped it; a new one follows while the
    /// machine is still connected.
    pub(in crate::app::gx_store) fn remote_client_thread_ended(
        &mut self,
        machine_id: &str,
        restart_in: Duration,
    ) {
        self.warning(
            "gxStore.remote.clientThreadEnded.warning",
            json!({
                "machineId": log_text(machine_id),
                "restartInMs": restart_in.as_millis() as u64,
            }),
        );
    }

    /// The persisted focus seeded the core at startup. Ids only.
    pub(in crate::app::gx_store) fn focus_restored(&mut self, core: &Core) {
        let focus = core.focus();
        append(
            "gxStore.focusRestored",
            json!({
                "activeProjectId": focus.active_project.as_ref().map(|project| &project.project_id),
                "focusedSessionId": focus.focused_session.as_ref().map(|session| &session.session_id),
            }),
        );
    }

    /// The old runtime sent an empty tab list for a project the store does not see as empty (or
    /// cannot judge yet), so the workspace kept its tabs. A warning: it means the two readers of
    /// the daemon disagree, or the old runtime posted before it had rows.
    pub(in crate::app::gx_store) fn empty_tab_list_disputed(&mut self, core: &Core, total: u64) {
        let store_tabs = match core.active_tab_sessions() {
            Loadable::Loaded(tabs) => Some(tabs.len()),
            Loadable::NotLoaded | Loadable::Missing => None,
        };
        self.warning(
            "gxStore.emptyTabListDisputed.warning",
            json!({
                "storeRevision": store_revision(core),
                "storeActiveTabs": store_tabs,
                "total": total,
            }),
        );
    }

    /// A machine's stream changed state. Named, because since M4d several machines share this
    /// record and a support log that cannot say WHICH one dropped answers nothing.
    pub(in crate::app::gx_store) fn connection(
        &mut self,
        machine: &MachineId,
        update: &ConnectionUpdate,
    ) {
        let machine_id = machine.remote_id().unwrap_or("local");
        let details = match update {
            ConnectionUpdate::Connecting { attempt } => {
                json!({ "machineId": log_text(machine_id), "phase": "connecting", "attempt": attempt })
            }
            // `reason`, not `error`: a daemon restart is routine, and the support log writes any
            // line with an error-named key unconditionally.
            ConnectionUpdate::Lost { error } => {
                json!({ "machineId": log_text(machine_id), "phase": "lost", "reason": error })
            }
            _ => return,
        };
        record("gxStore.connection", details);
    }

    pub(in crate::app::gx_store) fn resubscribe_requested(&mut self, reason: &ResubscribeReason) {
        let reason = match reason {
            ResubscribeReason::CurrentWithoutSnapshot => "currentWithoutSnapshot",
            ResubscribeReason::ServerChanged => "serverChanged",
            _ => "other",
        };
        append("gxStore.resubscribeRequested", json!({ "reason": reason }));
    }

    pub(in crate::app::gx_store) fn skipped_rows(
        &mut self,
        projects: usize,
        groups: usize,
        sessions: usize,
        first_error: &str,
    ) {
        self.warning(
            "gxStore.snapshotRowsSkipped.warning",
            json!({
                "projects": projects,
                "groups": groups,
                "sessions": sessions,
                "firstError": redact_quoted_values(first_error),
            }),
        );
    }

    pub(in crate::app::gx_store) fn client_diagnostic(&mut self, diagnostic: &ClientDiagnostic) {
        let details = match diagnostic {
            ClientDiagnostic::FrameParseFailed {
                event_type,
                error,
                resubscribe_scheduled,
            } => json!({
                "kind": "frameParseFailed",
                "frameType": event_type,
                "error": error,
                "resubscribeScheduled": resubscribe_scheduled,
            }),
            ClientDiagnostic::DomainProjectsReadFailed { error } => {
                json!({ "kind": "domainProjectsReadFailed", "error": error })
            }
            ClientDiagnostic::SubscribeNotAcknowledged => {
                json!({ "kind": "subscribeNotAcknowledged" })
            }
            ClientDiagnostic::ProtocolMismatch { received } => {
                json!({ "kind": "protocolMismatch", "received": received })
            }
            ClientDiagnostic::ThreadStopped { reason } => {
                json!({ "kind": "threadStopped", "error": reason })
            }
        };
        self.warning("gxStore.client.warning", details);
    }

    pub(in crate::app::gx_store) fn client_start_failed(&mut self, error: &StartError) {
        self.warning(
            "gxStore.clientStart.error",
            json!({ "error": error.to_string() }),
        );
    }

    /// The client's thread is gone although nobody stopped it; a new client follows.
    pub(in crate::app::gx_store) fn client_thread_ended(
        &mut self,
        restart_attempt: u32,
        restart_in: Duration,
    ) {
        self.warning(
            "gxStore.clientThreadEnded.warning",
            json!({
                "restartAttempt": restart_attempt,
                "restartInMs": restart_in.as_millis() as u64,
            }),
        );
    }

    pub(in crate::app::gx_store) fn warning(&mut self, event: &str, details: serde_json::Value) {
        if self.warning_lines >= MAX_WARNING_LINES {
            return;
        }
        self.warning_lines += 1;
        append(event, details);
    }

    /// The focus the store published and performed, at most once a minute and only when it moved.
    /// Counts only.
    pub(in crate::app::gx_store) fn focus_summary(
        &mut self,
        publish: FocusPublishCounters,
        perform: FocusPerformCounters,
        core: &Core,
    ) {
        if (publish, perform) == self.focus_summary_written
            || self
                .focus_summary_considered_at
                .is_some_and(|at| at.elapsed() < PERIODIC_SUMMARY_INTERVAL)
        {
            return;
        }
        self.focus_summary_considered_at = Some(Instant::now());
        if !routine_logging_enabled() {
            return;
        }
        self.focus_summary_written = (publish, perform);
        let active_tabs = match core.active_tab_sessions() {
            Loadable::Loaded(tabs) => Some(tabs.len()),
            Loadable::NotLoaded | Loadable::Missing => None,
        };
        record(
            "gxStore.focus.summary",
            json!({
                "publishes": publish.publishes,
                "contexts": publish.contexts,
                "unplacedHolds": publish.unplaced_holds,
                "unplacedPlaced": publish.unplaced_placed,
                "startupRestores": publish.startup_restores,
                "sessionFocuses": perform.sessions,
                "remoteSessionFocuses": perform.remote_sessions,
                "groupFocuses": perform.groups,
                "wakes": perform.wakes,
                "refused": perform.refused,
                "storeRevision": store_revision(core),
                "storeActiveTabs": active_tabs,
                "tabsGeneration": core.tabs_generation(),
            }),
        );
    }
}
