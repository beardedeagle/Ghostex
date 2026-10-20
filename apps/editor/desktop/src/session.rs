use std::path::PathBuf;

use serde_json::{Value, json};
use tao::window::WindowId;

use crate::*;

impl EditorApp {
    pub(crate) fn request_session_save_and_close(&mut self, request_id: &str) {
        let Some(session) = self.sessions.get(request_id) else {
            return;
        };
        if let Some(window) = self.windows.get(&session.window_id) {
            let javascript = r#"
document.dispatchEvent(new KeyboardEvent("keydown", {
  key: "s",
  metaKey: true,
  ctrlKey: true,
  bubbles: true,
  cancelable: true
}));
"#;
            let _ = window.webview.evaluate_script(javascript);
        } else {
            self.finish_session(request_id, CloseAction::Save);
        }
    }

    pub(crate) fn update_session_draft(&mut self, window_id: WindowId, text: &str) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        if let Some(session) = self.sessions.get_mut(&request_id) {
            session.latest_draft = text.to_string();
        }
    }

    pub(crate) fn update_session_cursor_offset(
        &mut self,
        window_id: WindowId,
        cursor_offset: usize,
    ) {
        let Some(request_id) = self.request_id_for_window(window_id) else {
            return;
        };
        if let Some(session) = self.sessions.get_mut(&request_id) {
            session.latest_cursor_offset = Some(cursor_offset);
        }
    }

    pub(crate) fn save_session_draft_without_closing(&self, request_id: &str) {
        if let Some(session) = self.sessions.get(request_id)
            && let Err(error) = write_draft_atomically(&session.file_path, &session.latest_draft)
        {
            eprintln!(
                "ghostex-editor: save failed for {}: {error}",
                session.file_path.display()
            );
        }
    }

    pub(crate) fn finish_session(&mut self, request_id: &str, action: CloseAction) {
        let Some((window_id, cursor_snapshot)) =
            self.sessions.get_mut(request_id).and_then(|session| {
                if session.is_finishing {
                    return None;
                }
                session.is_finishing = true;

                match action {
                    CloseAction::Save => {
                        if let Err(error) =
                            write_draft_atomically(&session.file_path, &session.latest_draft)
                        {
                            eprintln!(
                                "ghostex-editor: save failed for {}: {error}",
                                session.file_path.display()
                            );
                        }
                        write_status(&session.status_file, "saved");
                        session.opener.send(json!({
                            "type": "closed",
                            "requestId": session.request_id,
                            "status": "saved",
                        }));
                    }
                    CloseAction::Cancel => {
                        write_status(&session.status_file, "cancelled");
                        session.opener.send(json!({
                            "type": "closed",
                            "requestId": session.request_id,
                            "status": "cancelled",
                        }));
                    }
                }

                let cursor_snapshot =
                    session
                        .latest_cursor_offset
                        .map(|cursor_offset| CursorSnapshot {
                            text: session.latest_draft.clone(),
                            cursor_offset,
                        });
                Some((session.window_id, cursor_snapshot))
            })
        else {
            return;
        };
        if let Some(cursor_snapshot) = cursor_snapshot {
            self.last_cursor_snapshot = Some(cursor_snapshot);
        }
        // Every session window is presented at open handling, so its frame is
        // the user's latest; the never-presented warm window is not a session.
        if let Some(window) = self.windows.get(&window_id) {
            save_window_frame(&window.window);
        }
        self.sessions.remove(request_id);
        self.windows.remove(&window_id);
        self.notify_open_count_watchers();
        if self.pending_shutdown && self.sessions.is_empty() {
            self.should_exit = true;
        } else if !self.pending_shutdown && self.warm_window.is_none() {
            let _ = self.proxy.send_event(DaemonEvent::EnsureWarm);
        }
    }

    pub(crate) fn request_id_for_window(&self, window_id: WindowId) -> Option<String> {
        self.windows
            .get(&window_id)
            .and_then(|window| window.session_request_id.clone())
    }

    pub(crate) fn save_all_sessions_and_exit(&mut self) {
        self.pending_shutdown = true;
        let request_ids: Vec<String> = self.sessions.keys().cloned().collect();
        for request_id in request_ids {
            self.finish_session(&request_id, CloseAction::Save);
        }
        self.should_exit = true;
    }

    pub(crate) fn cleanup_socket(&mut self) {
        if let Some(path) = self.socket_cleanup_path.take() {
            remove_stale_socket(&Some(path));
        }
    }
}

pub(crate) fn string_field(request: &Value, key: &str) -> Option<String> {
    request.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn originating_session_id_field(request: &Value) -> Option<String> {
    let value = request.get("originatingSessionId")?.as_str()?;
    let (project_id, session_id) = value.split_once(':')?;
    (valid_originating_session_id_part(project_id, b'P')
        && valid_originating_session_id_part(session_id, b'G'))
    .then(|| value.to_string())
}

fn valid_originating_session_id_part(value: &str, prefix: u8) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 5
        && bytes[0] == prefix
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

pub(crate) fn session_window_title(session_title: &str) -> String {
    if session_title == DEFAULT_TITLE {
        return APP_WINDOW_TITLE.to_string();
    }
    format!("{session_title} — {APP_WINDOW_TITLE}")
}

pub(crate) fn absolute_path_field(request: &Value, key: &str) -> Option<PathBuf> {
    let value = request.get(key).and_then(Value::as_str)?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

pub(crate) fn cursor_offset_field(request: &Value, key: &str) -> Option<usize> {
    request
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}
