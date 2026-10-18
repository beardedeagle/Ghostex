//! Every computer's notification feed as this app holds it, and the one merged list the bell
//! draws (gx-core `merged_notification_feed_message` decides what it shows, counts and jumps to).
//!
//! A computer's feed is read when its stream goes live (`Effect::MachineLive`) and whenever its
//! daemon announces `notificationFeedChanged`; the list is merged again when a feed lands and
//! when a remote computer goes offline, comes back or is removed from the sidebar. An offline
//! computer's last feed is kept, so its rows come back as they were the moment it reconnects,
//! before the read that refreshes them answers.

use std::collections::BTreeMap;

use ghostex_gx_core::{
    ConnectionPhase, MachineId, MachineNotificationFeed, NotificationFeedTab,
    merged_notification_feed_message,
};
use serde_json::Value;

use super::feed::read_notification_feed;
use crate::GhostexGpuiApp;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

#[derive(Default)]
pub(crate) struct NotificationFeedsHost {
    /// Each computer's last state message.
    feeds: BTreeMap<MachineId, Value>,
    /// The remote computers in the sidebar and whether each was online, as the last merge saw
    /// them: a store change that moves neither merges nothing.
    remote_machines: Vec<(MachineId, bool)>,
}

impl NotificationFeedsHost {
    /// Keeps a computer's fresh feed without merging; the caller merges once for several.
    pub(super) fn hold(&mut self, machine: MachineId, message: Value) {
        self.feeds.insert(machine, message);
    }
}

/// Where a computer's feed calls go: `Some(None)` is this computer, `Some(Some(_))` a remote one
/// through its tunnel, and `None` a computer that cannot be reached now.
pub(super) type NotificationFeedRoute = Option<Option<GpuiRemoteGxserverRequestTarget>>;

impl GhostexGpuiApp {
    pub(super) fn gx_store_notification_feed_route(
        &self,
        machine: &MachineId,
    ) -> NotificationFeedRoute {
        let Some(machine_id) = machine.remote_id() else {
            return Some(None);
        };
        if !self.gx_store_notification_machine_online(machine) {
            return None;
        }
        self.remote_gxserver_connections
            .get(machine_id)
            .map(|connection| Some(connection.request_target()))
    }

    /// A remote computer is online while its stream is live and its tunnel is up: the same "live"
    /// that keeps its sidebar rows from drawing stale.
    fn gx_store_notification_machine_online(&self, machine: &MachineId) -> bool {
        let Some(machine_id) = machine.remote_id() else {
            return true;
        };
        self.remote_gxserver_connections.contains_key(machine_id)
            && self
                .gx_store
                .core
                .presentation()
                .machine(machine)
                .is_some_and(|entry| entry.connection().phase == ConnectionPhase::Live)
    }

    /// The remote computers the sidebar has tabs for, in its order, with their names.
    fn gx_store_notification_remote_machines(&self) -> Vec<(MachineId, String)> {
        self.gx_store
            .remote
            .tabs()
            .iter()
            .filter(|tab| !tab.is_local())
            .map(|tab| (MachineId::Remote(tab.machine_id.clone()), tab.label.clone()))
            .collect()
    }

    /// Every computer that can be reached now: this one first, then the online remote ones.
    pub(super) fn gx_store_online_notification_machines(&self) -> Vec<MachineId> {
        self.gx_store_notification_machines_on(NotificationFeedTab::All)
    }

    /// The computers an all-rows command on `tab` goes to (gx-core decides which).
    pub(super) fn gx_store_notification_machines_on(
        &self,
        tab: NotificationFeedTab,
    ) -> Vec<MachineId> {
        let machines = self.gx_store_notification_machine_list();
        let feeds: Vec<MachineNotificationFeed<'_>> = machines
            .iter()
            .map(|(machine, name, online)| MachineNotificationFeed {
                machine,
                machine_name: name,
                online: *online,
                message: None,
            })
            .collect();
        tab.command_machines(&feeds)
    }

    /// This computer, then every remote computer in the sidebar, with its name and whether it is
    /// online.
    fn gx_store_notification_machine_list(&self) -> Vec<(MachineId, String, bool)> {
        let mut machines = vec![(MachineId::Local, String::new(), true)];
        machines.extend(
            self.gx_store_notification_remote_machines()
                .into_iter()
                .map(|(machine, name)| {
                    let online = self.gx_store_notification_machine_online(&machine);
                    (machine, name, online)
                }),
        );
        machines
    }

    /// Reads one computer's feed and merges it into the bell. A computer that cannot be reached
    /// keeps what the bell has.
    pub(crate) fn gx_store_refresh_notification_feed(
        &mut self,
        machine: MachineId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(route) = self.gx_store_notification_feed_route(&machine) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let Some(message) = read_notification_feed(route).await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_store_notification_feed(machine, message, cx);
            });
        })
        .detach();
    }

    /// One computer's fresh feed: kept, then the bell is merged again. Returns the merged message.
    pub(super) fn gx_store_store_notification_feed(
        &mut self,
        machine: MachineId,
        message: Value,
        cx: &mut gpui::Context<Self>,
    ) -> Value {
        self.gx_store.notifications.hold(machine, message);
        self.gx_store_publish_notification_feeds(cx)
    }

    /// Merges every held feed into the bell's state, dropping the feeds of computers removed from
    /// the sidebar. Returns the merged message.
    pub(super) fn gx_store_publish_notification_feeds(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Value {
        let machines = self.gx_store_notification_machine_list();
        let host = &mut self.gx_store.notifications;
        host.feeds
            .retain(|machine, _| machines.iter().any(|(listed, ..)| listed == machine));
        host.remote_machines = machines
            .iter()
            .filter(|(machine, ..)| !machine.is_local())
            .map(|(machine, _, online)| (machine.clone(), *online))
            .collect();
        let feeds: Vec<MachineNotificationFeed<'_>> = machines
            .iter()
            .map(|(machine, name, online)| MachineNotificationFeed {
                machine,
                machine_name: name,
                online: *online,
                message: host.feeds.get(machine),
            })
            .collect();
        let merged = merged_notification_feed_message(&feeds);
        self.receive_notification_feed_state_message(&merged, cx);
        merged
    }

    /// The sidebar list is being brought up to date: merge again when a remote computer went
    /// offline, came back, or joined or left the sidebar. A tab removed in Settings moves no
    /// connection, so this compares the machines themselves rather than reading the changes.
    pub(in crate::app::gx_store) fn gx_store_notification_feeds_store_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_machines: Vec<(MachineId, bool)> = self
            .gx_store_notification_machine_list()
            .into_iter()
            .filter(|(machine, ..)| !machine.is_local())
            .map(|(machine, _, online)| (machine, online))
            .collect();
        if remote_machines != self.gx_store.notifications.remote_machines {
            self.gx_store_publish_notification_feeds(cx);
        }
    }
}
