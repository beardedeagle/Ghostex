//! What a claude.ai page in the Browser shows: a cloud session working, waiting for the user or
//! idle, the account it is signed in to, and the session's branch and pull request. Asked over the
//! DevTools protocol like the keep-awake question (page_keep_awake.rs), and answered into a
//! thread-local the app's sweep reads (apps/desktop/src/app/claude_cloud_page/).
//!
//! CDXC:ClaudeCloud 2026-10-23 WHY:
//! claude.ai's markup is not Ghostex's and changes without notice, so the reader keys on what a person sees rather than on class names: a Stop or Interrupt control or an `aria-busy` region means working; Approve, Allow, Deny or a choice list, or "waiting for your input", means it needs the user; an enabled message box with neither means idle. Anything else, the login page included, is `unknown` and changes nothing, so the branch and pull request status stays. The signed-in email comes from claude.ai's own account answer on the same origin (`/api/account`), asked once per page; a page that will not say leaves the email unknown, which never counts as a mismatch.

use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, WrapDevToolsMessageObserver,
    wrap_dev_tools_message_observer,
};
use std::time::{Duration, Instant};

/// Answers a JSON string: `{url, status, email?, branch?, prUrl?, signedOut?, limit?, request?}`;
/// `request` is the action of a notice button the user clicked (`signOut`, `copy`).
const CLAUDE_CLOUD_PAGE_QUESTION: &str = r#"(() => {
  const out = { url: location.href, status: 'unknown' };
  const body = document.body;
  if (!body) return JSON.stringify(out);
  const visible = (el) => {
    const box = el.getBoundingClientRect();
    return box.width > 0 && box.height > 0 && getComputedStyle(el).visibility !== 'hidden';
  };
  const label = (el) => ((el.getAttribute('aria-label') || '') + ' ' + (el.innerText || el.textContent || '')).trim().toLowerCase();
  const controls = [...document.querySelectorAll('button, [role="button"], a')].filter(visible).map(label);
  const boxes = [...document.querySelectorAll('textarea, [contenteditable="true"]')].filter(visible);
  const tail = (body.innerText || '').slice(-6000);
  if (/^\/(login|logout|signup)\b/.test(location.pathname)
      || (!boxes.length && controls.some((c) => /^(log in|sign in|continue with (google|email))\b/.test(c)))) {
    out.signedOut = true;
  }
  const working = controls.some((c) => /\b(stop|interrupt)\b/.test(c) && !/stop sharing|stop session|stop following/.test(c))
    || !!document.querySelector('main [aria-busy="true"]');
  const asking = controls.some((c) => /^(approve|allow|deny|reject|submit answer)\b/.test(c))
    || !!document.querySelector('main [role="radiogroup"]')
    || /\b(waiting for your (input|answer|approval)|needs your (input|approval))\b/i.test(tail);
  if (!out.signedOut) {
    if (working) out.status = 'working';
    else if (asking) out.status = 'needsInput';
    else if (boxes.some((box) => !box.closest('[aria-disabled="true"]') && !box.disabled)) out.status = 'idle';
  }
  const branch = (body.innerText || '').match(/\bclaude\/[A-Za-z0-9._\/-]{3,160}/);
  if (branch) out.branch = branch[0].replace(/[.,)\]]+$/, '');
  const pull = [...document.querySelectorAll('a[href^="https://github.com/"][href*="/pull/"]')].map((a) => a.href)[0];
  if (pull) out.prUrl = pull;
  const limit = tail.match(/[^\n]{0,60}(usage limit|limit reached|out of (extra )?usage)[^\n]{0,80}/i);
  if (limit) out.limit = limit[0].trim().slice(0, 160);
  const key = Symbol.for('ghostex.claudeCloud');
  const memory = window[key] || (window[key] = { email: null, askedAt: 0 });
  if (!memory.email && !out.signedOut && Date.now() - memory.askedAt > 60000) {
    memory.askedAt = Date.now();
    fetch('/api/account', { credentials: 'include' })
      .then((answer) => (answer.ok ? answer.json() : null))
      .then((account) => {
        const email = account && (account.email_address || account.email || (account.account && account.account.email_address));
        if (typeof email === 'string' && email.includes('@')) memory.email = email.trim().toLowerCase();
      })
      .catch(() => {});
  }
  if (memory.email) out.email = memory.email;
  if (memory.request) { out.request = memory.request; memory.request = null; }
  return JSON.stringify(out);
})()"#;

/// How long an answer stands before the page is asked again.
const ANSWER_TTL: Duration = Duration::from_secs(10);
/// A question with no answer by then is treated as lost and asked again.
const QUESTION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct ClaudeCloudPageProbe {
    registration: Option<cef::Registration>,
    question: Option<(c_int, Instant)>,
    answer: Option<(String, Instant)>,
}

thread_local! {
    static CLAUDE_CLOUD_PAGE_PROBES: RefCell<HashMap<c_int, ClaudeCloudPageProbe>> = RefCell::new(HashMap::new());
}

wrap_dev_tools_message_observer! {
    struct ClaudeCloudPageObserver {}

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            browser: Option<&mut cef::Browser>,
            message_id: c_int,
            success: c_int,
            result: Option<&[u8]>,
        ) {
            let Some(browser) = browser else {
                return;
            };
            let browser_id = browser.identifier();
            CLAUDE_CLOUD_PAGE_PROBES.with(|probes| {
                let mut probes = probes.borrow_mut();
                let Some(probe) = probes.get_mut(&browser_id) else {
                    return;
                };
                if probe.question.map(|(id, _)| id) != Some(message_id) {
                    return;
                }
                probe.question = None;
                let answer = (success != 0)
                    .then_some(result)
                    .flatten()
                    .and_then(|result| serde_json::from_slice::<serde_json::Value>(result).ok())
                    .and_then(|value| value.pointer("/result/value")?.as_str().map(str::to_string));
                if let Some(answer) = answer {
                    probe.answer = Some((answer, Instant::now()));
                }
            });
        }
    }
}

pub(crate) fn forget_claude_cloud_page_probe(browser_id: c_int) {
    CLAUDE_CLOUD_PAGE_PROBES.with(|probes| {
        probes.borrow_mut().remove(&browser_id);
    });
}

impl CefBrowser {
    /// The page's latest answer while it is fresh (a JSON string), and takes it, so each answer
    /// is handled once.
    pub fn take_claude_cloud_page_answer(&self) -> Option<String> {
        let browser_id = self.identifier();
        CLAUDE_CLOUD_PAGE_PROBES.with(|probes| {
            let mut probes = probes.borrow_mut();
            let probe = probes.get_mut(&browser_id)?;
            let (_, at) = probe.answer.as_ref()?;
            if at.elapsed() >= ANSWER_TTL {
                probe.answer = None;
                return None;
            }
            probe.answer.take().map(|(answer, _)| answer)
        })
    }

    /// Asks the page again unless a question is already on its way.
    pub fn ask_claude_cloud_page(&self) {
        let browser = self.browser.borrow().clone();
        let Some(host) = browser.host() else {
            return;
        };
        let browser_id = browser.identifier();
        let (asking, needs_observer) = CLAUDE_CLOUD_PAGE_PROBES.with(|probes| {
            let probes = probes.borrow();
            let probe = probes.get(&browser_id);
            let asking = probe
                .and_then(|probe| probe.question)
                .is_some_and(|(_, at)| at.elapsed() < QUESTION_TIMEOUT);
            (
                asking,
                probe.is_none_or(|probe| probe.registration.is_none()),
            )
        });
        if asking {
            return;
        }
        // Host calls stay outside the registry borrow: CEF may run callbacks while they execute.
        let registration = if needs_observer {
            let mut observer = ClaudeCloudPageObserver::new();
            host.add_dev_tools_message_observer(Some(&mut observer))
        } else {
            None
        };
        let Some(mut params) = cef::dictionary_value_create() else {
            return;
        };
        params.set_string(
            Some(&CefString::from("expression")),
            Some(&CefString::from(CLAUDE_CLOUD_PAGE_QUESTION)),
        );
        params.set_bool(Some(&CefString::from("returnByValue")), 1);
        let message_id = next_page_appearance_devtools_message_id();
        let sent = host.execute_dev_tools_method(
            message_id,
            Some(&CefString::from("Runtime.evaluate")),
            Some(&mut params),
        ) != 0;
        CLAUDE_CLOUD_PAGE_PROBES.with(|probes| {
            let mut probes = probes.borrow_mut();
            let probe = probes.entry(browser_id).or_default();
            if registration.is_some() {
                probe.registration = registration;
            }
            if sent {
                probe.question = Some((message_id, Instant::now()));
            }
        });
    }
}

/// The script that shows (or, with `None`, removes) Ghostex's notice at the top of a claude.ai
/// page: a sentence and, optionally, one button. The button hands Ghostex its `action` (the next
/// answer carries it as `request`), or opens a link.
pub(crate) fn claude_cloud_page_notice_script(notice: Option<&serde_json::Value>) -> String {
    let notice = notice
        .map(serde_json::Value::to_string)
        .unwrap_or_else(|| "null".to_string());
    format!(
        r#"(() => {{
  const notice = {notice};
  const id = 'ghostex-claude-cloud-notice';
  let bar = document.getElementById(id);
  if (!notice) {{ if (bar) bar.remove(); return; }}
  if (!bar) {{
    bar = document.createElement('div');
    bar.id = id;
    Object.assign(bar.style, {{ position: 'fixed', top: '0', left: '0', right: '0', zIndex: '2147483647',
      display: 'flex', gap: '12px', alignItems: 'center', padding: '8px 14px',
      font: '13px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      background: '#1e1a10', color: '#e9d7a6', borderBottom: '1px solid #5a4a22' }});
    document.documentElement.appendChild(bar);
  }}
  if (bar.dataset.notice === JSON.stringify(notice)) return;
  bar.dataset.notice = JSON.stringify(notice);
  bar.textContent = '';
  const text = document.createElement('span');
  text.style.flex = '1';
  text.textContent = notice.text;
  bar.appendChild(text);
  if (notice.button) {{
    const button = document.createElement('button');
    button.textContent = notice.button;
    Object.assign(button.style, {{ font: 'inherit', padding: '4px 10px', borderRadius: '6px',
      border: '1px solid #8a7440', background: '#2a2416', color: '#f2e3b8', cursor: 'pointer' }});
    button.onclick = () => {{
      if (notice.action) {{
        const key = Symbol.for('ghostex.claudeCloud');
        (window[key] || (window[key] = {{ email: null, askedAt: 0 }})).request = notice.action;
        button.disabled = true;
      }} else if (notice.href) {{
        location.href = notice.href;
      }}
    }};
    bar.appendChild(button);
  }}
}})()"#
    )
}
