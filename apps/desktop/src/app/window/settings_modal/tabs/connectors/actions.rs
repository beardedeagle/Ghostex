//! Every call the Connectors page makes to gxserver, and what it does with each answer. Each
//! action marks its button busy, shows a failure as a toast, and reads the connectors again.
use super::super::super::store::{post_store_message, store_gxserver_rpc};
use super::ConnectorsTab;
use super::data::{self, busy_key};
use gpui::Context;
use serde_json::{Value, json};
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// Composio is called over the network.
const COMPOSIO_TIMEOUT: Duration = Duration::from_secs(120);
/// Executor may have to start first (up to 90 seconds), and an added app is built before the
/// answer comes back.
const EXECUTOR_TIMEOUT: Duration = Duration::from_secs(300);
/// While Executor downloads or starts (which sends no event until it is done), the page reads
/// again this often.
const PENDING_REREAD: Duration = Duration::from_secs(3);

impl ConnectorsTab {
    /// Reads every workspace's connectors.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            self.error = Some("Ghostex's background service is not reachable.".to_string());
            cx.notify();
            return;
        }
        self.loading = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/readConnectors",
            json!({}),
            READ_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.loading = false;
                    match result {
                        Ok(result) => {
                            page.connectors = Some(result);
                            page.error = None;
                        }
                        Err(error) => page.error = Some(error),
                    }
                    page.reread_while_pending(cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Reads again in a moment while a workspace's Executor is downloading or starting.
    fn reread_while_pending(&mut self, cx: &mut Context<Self>) {
        let pending = data::workspaces(self.connectors.as_ref())
            .iter()
            .filter_map(|workspace| workspace.get("executor"))
            .any(|executor| {
                let installing = executor
                    .get("install")
                    .and_then(|install| install.get("status"))
                    .and_then(Value::as_str)
                    .is_some_and(|status| matches!(status, "queued" | "running"));
                installing || data::flag(executor, "starting")
            });
        if !pending || self.pending_reread.is_some() {
            return;
        }
        self.pending_reread = Some(cx.spawn(async move |page, cx| {
            cx.background_executor().timer(PENDING_REREAD).await;
            let _ = page.update(cx, |page, cx| {
                page.pending_reread = None;
                page.reload(cx);
            });
        }));
    }

    fn toast(&self, level: &str, title: &str, description: &str, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.toast(level, title, description, cx));
    }

    pub(super) fn open_url(&self, url: &str, cx: &mut Context<Self>) {
        post_store_message(
            &self.store,
            json!({ "type": "openExternalUrl", "url": url }),
            cx,
        );
    }

    fn note(&mut self, workspace: &str, section: &str, text: impl Into<String>) {
        self.notes.insert(busy_key(workspace, section), text.into());
    }

    fn clear_note(&mut self, workspace: &str, section: &str) {
        self.notes.remove(&busy_key(workspace, section));
    }

    /// One action for `workspace`: `action` names its busy button, a failure is a toast titled
    /// `failure`, and `done` gets the answer before the page reads the connectors again.
    #[allow(clippy::too_many_arguments)]
    fn act(
        &mut self,
        workspace: &str,
        action: &str,
        path: &'static str,
        mut params: Value,
        timeout: Duration,
        failure: &'static str,
        done: impl FnOnce(&mut Self, &str, Value, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let key = busy_key(workspace, action);
        if !self.busy.insert(key.clone()) {
            return;
        }
        params["workspaceId"] = json!(workspace);
        cx.notify();
        let this = cx.weak_entity();
        let workspace = workspace.to_string();
        store_gxserver_rpc(
            &self.store.clone(),
            path,
            params,
            timeout,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.busy.remove(&key);
                    match result {
                        Ok(result) => done(page, &workspace, result, cx),
                        Err(error) => page.toast("error", failure, &error, cx),
                    }
                    page.reload(cx);
                });
            },
            cx,
        );
    }

    /// Opens what a sign-in or connect answered (`url`), or says it was already done.
    fn open_answer(
        &mut self,
        workspace: &str,
        section: &str,
        result: &Value,
        what: &str,
        cx: &mut Context<Self>,
    ) {
        let name = Some(data::text(result, "name"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| what.to_string());
        match result.get("url").and_then(Value::as_str) {
            Some(url) if !data::flag(result, "alreadyConnected") => {
                self.open_url(url, cx);
                self.note(
                    workspace,
                    section,
                    format!("Finish connecting {name} in your browser. This page updates when you're done."),
                );
            }
            _ => self.note(workspace, section, format!("{name} is already connected.")),
        }
    }

    // ---- Composio ------------------------------------------------------------------------------

    pub(super) fn composio_sign_in(&mut self, workspace: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            "composio-sign-in",
            "/api/signInComposio",
            json!({}),
            COMPOSIO_TIMEOUT,
            "Couldn't start the Composio sign-in",
            |page, workspace, result, cx| match result.get("signInUrl").and_then(Value::as_str) {
                Some(url) => {
                    page.open_url(url, cx);
                    page.note(
                        workspace,
                        "composio",
                        "Finish signing in to Composio in your browser. This page updates when you're done.",
                    );
                }
                None => page.note(workspace, "composio", "Already signed in to Composio."),
            },
            cx,
        );
    }

    pub(super) fn composio_sign_out(&mut self, workspace: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            "composio-sign-out",
            "/api/signOutComposio",
            json!({}),
            READ_TIMEOUT,
            "Couldn't sign out of Composio",
            |page, workspace, _, _| page.clear_note(workspace, "composio"),
            cx,
        );
    }

    pub(super) fn composio_refresh(&mut self, workspace: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            "composio-refresh",
            "/api/refreshComposio",
            json!({}),
            COMPOSIO_TIMEOUT,
            "Couldn't read your Composio apps",
            |_, _, _, _| {},
            cx,
        );
    }

    pub(super) fn composio_connect(&mut self, workspace: &str, app: &str, cx: &mut Context<Self>) {
        let app_name = app.to_string();
        self.act(
            workspace,
            &format!("composio-connect-{app}"),
            "/api/connectComposioApp",
            json!({ "app": app }),
            COMPOSIO_TIMEOUT,
            "Couldn't connect the app",
            move |page, workspace, result, cx| {
                page.open_answer(workspace, "composio", &result, &app_name, cx)
            },
            cx,
        );
    }

    // ---- Executor ------------------------------------------------------------------------------

    pub(super) fn executor_set_enabled(
        &mut self,
        workspace: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let (path, failure) = if enabled {
            ("/api/enableExecutor", "Couldn't turn Executor on")
        } else {
            ("/api/disableExecutor", "Couldn't turn Executor off")
        };
        self.act(
            workspace,
            "executor-enable",
            path,
            json!({}),
            EXECUTOR_TIMEOUT,
            failure,
            |_, _, _, _| {},
            cx,
        );
    }

    pub(super) fn executor_search(
        &mut self,
        workspace: &str,
        query: String,
        cx: &mut Context<Self>,
    ) {
        self.act(
            workspace,
            "executor-search",
            "/api/searchExecutorCatalog",
            json!({ "query": query }),
            EXECUTOR_TIMEOUT,
            "Couldn't search Executor's catalog",
            |page, workspace, result, _| {
                page.catalog_results
                    .insert(workspace.to_string(), data::list(&result, "apps"));
            },
            cx,
        );
    }

    pub(super) fn executor_add(
        &mut self,
        workspace: &str,
        catalog_id: &str,
        cx: &mut Context<Self>,
    ) {
        let added = catalog_id.to_string();
        self.act(
            workspace,
            &format!("executor-add-{catalog_id}"),
            "/api/addExecutorApp",
            json!({ "app": catalog_id }),
            EXECUTOR_TIMEOUT,
            "Couldn't add the app",
            move |page, workspace, result, _| {
                if let Some(results) = page.catalog_results.get_mut(workspace) {
                    results.retain(|entry| data::text(entry, "id") != added);
                }
                let name = result
                    .get("app")
                    .map(|app| data::text(app, "name"))
                    .filter(|name| !name.is_empty())
                    .unwrap_or(added);
                page.note(
                    workspace,
                    "executor",
                    format!("Added {name}. Sign it in below."),
                );
            },
            cx,
        );
    }

    pub(super) fn executor_add_mcp(
        &mut self,
        workspace: &str,
        name: String,
        url: String,
        cx: &mut Context<Self>,
    ) {
        self.act(
            workspace,
            "executor-add-mcp",
            "/api/addExecutorMcpServer",
            json!({ "name": name, "url": url }),
            EXECUTOR_TIMEOUT,
            "Couldn't add the MCP server",
            |page, workspace, _, _| {
                for field in ["mcp-name", "mcp-url"] {
                    page.fields
                        .texts
                        .remove(&super::executor::input_id(workspace, field));
                }
                page.note(workspace, "executor", "Added. Sign it in below if it asks.");
            },
            cx,
        );
    }

    pub(super) fn executor_remove(&mut self, workspace: &str, app: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            &format!("executor-remove-{app}"),
            "/api/removeExecutorApp",
            json!({ "app": app }),
            EXECUTOR_TIMEOUT,
            "Couldn't remove the app",
            |_, _, _, _| {},
            cx,
        );
    }

    pub(super) fn executor_connect(&mut self, workspace: &str, app: &str, cx: &mut Context<Self>) {
        let app_name = app.to_string();
        self.act(
            workspace,
            &format!("executor-connect-{app}"),
            "/api/connectExecutorApp",
            json!({ "app": app }),
            EXECUTOR_TIMEOUT,
            "Couldn't start the sign-in",
            move |page, workspace, result, cx| {
                page.open_answer(workspace, "executor", &result, &app_name, cx)
            },
            cx,
        );
    }

    // ---- Rules ---------------------------------------------------------------------------------

    /// `tool: None` sets the workspace's rule for every call; `rule` `default` drops a tool's own.
    pub(super) fn set_rule(
        &mut self,
        workspace: &str,
        tool: Option<&str>,
        rule: &str,
        cx: &mut Context<Self>,
    ) {
        let mut params = json!({ "rule": rule });
        if let Some(tool) = tool {
            params["tool"] = json!(tool);
        }
        self.act(
            workspace,
            &format!("rule-{}", tool.unwrap_or("all")),
            "/api/setConnectorRule",
            params,
            READ_TIMEOUT,
            "Couldn't change the rule",
            |_, _, _, _| {},
            cx,
        );
    }

    // ---- Command-line sign-ins -----------------------------------------------------------------

    pub(super) fn cli_sign_in(&mut self, workspace: &str, tool: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            &format!("cli-sign-in-{tool}"),
            "/api/signInConnectorCli",
            json!({ "tool": tool }),
            COMPOSIO_TIMEOUT,
            "Couldn't start the sign-in",
            |page, workspace, result, cx| {
                if let Some(url) = result.get("url").and_then(Value::as_str) {
                    page.open_url(url, cx);
                }
                let code = data::text(&result, "code");
                let message = data::text(&result, "message");
                let note = match (code.is_empty(), message.is_empty()) {
                    (false, false) => format!("{message} Code: {code}"),
                    (false, true) => format!("Enter this code in your browser: {code}"),
                    (true, false) => message,
                    (true, true) => {
                        "Finish signing in in your browser. This page updates when you're done."
                            .to_string()
                    }
                };
                page.note(workspace, "cli", note);
            },
            cx,
        );
    }

    pub(super) fn cli_sign_out(&mut self, workspace: &str, tool: &str, cx: &mut Context<Self>) {
        self.act(
            workspace,
            &format!("cli-sign-out-{tool}"),
            "/api/signOutConnectorCli",
            json!({ "tool": tool }),
            READ_TIMEOUT,
            "Couldn't sign out",
            |page, workspace, _, _| page.clear_note(workspace, "cli"),
            cx,
        );
    }
}
