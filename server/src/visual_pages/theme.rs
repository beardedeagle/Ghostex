/// The `<style id="ghostex-theme">` every published page starts with: Ghostex's default dark
/// palette, the light palette when the viewer asks for it or the viewer's system is light, and a plain base so an unstyled
/// page already reads like the app. It comes first in `<head>`, so the page's own CSS wins.
///
/// Neutrals come from the app's default theme (`packages/core-ui/styles/theme.css` for dark,
/// `packages/core-ui/styles/modals-light.css` for light); status colors from the chat's status
/// tones (`packages/gx-chat-core/visual/status-tone.json`).
pub(super) const THEME_STYLE: &str = concat!(
    "<style id=\"ghostex-theme\">",
    ":root{color-scheme:dark;",
    "--background:#0e0e0e;--foreground:#c8cdd5;",
    "--muted:#2a2a2a;--muted-foreground:#747b85;",
    "--card:#252525;--card-foreground:#c8cdd5;",
    "--border:rgba(255,255,255,0.11);--input:rgba(255,255,255,0.15);--ring:#737373;",
    "--primary:#7da4f8;--primary-foreground:#0e0e0e;",
    "--accent:#2a2a2a;--accent-foreground:#e5e7eb;",
    "--destructive:#fb7185;--warning:#fbbf24;--success:#34d399;--info:#86d3f8;",
    "--code-background:#161616;--code-foreground:#e6edf3;",
    "--chart-1:#60a5fa;--chart-2:#f59e0b;--chart-3:#34d399;",
    "--chart-4:#f472b6;--chart-5:#a78bfa;--chart-6:#22d3ee;",
    "--radius:8px;",
    "--font-sans:-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif;",
    "--font-mono:\"SF Mono\", Menlo, Consolas, \"Liberation Mono\", monospace}",
    // The light palette applies when the viewer asks for it (`#gx-theme=light`, see
    // `PAGE_BOOTSTRAP`) or, without that, when the system is light.
    "@media (prefers-color-scheme: light){:root:not([data-gx-theme=dark]){color-scheme:light;",
    "--background:#ffffff;--foreground:#262626;",
    "--muted:#f1f1f1;--muted-foreground:#626262;",
    "--card:#ffffff;--card-foreground:#262626;",
    "--border:rgba(0,0,0,0.14);--input:rgba(0,0,0,0.16);--ring:#737373;",
    "--primary:#2563eb;--primary-foreground:#ffffff;",
    "--accent:#e9e9e9;--accent-foreground:#262626;",
    "--destructive:#b91c1c;--warning:#b45309;--success:#047857;--info:#0369a1;",
    "--code-background:#f6f8fa;--code-foreground:#1f2328;",
    "--chart-1:#2563eb;--chart-2:#d97706;--chart-3:#059669;",
    "--chart-4:#db2777;--chart-5:#7c3aed;--chart-6:#0891b2}}",
    ":root[data-gx-theme=light]{color-scheme:light;",
    "--background:#ffffff;--foreground:#262626;",
    "--muted:#f1f1f1;--muted-foreground:#626262;",
    "--card:#ffffff;--card-foreground:#262626;",
    "--border:rgba(0,0,0,0.14);--input:rgba(0,0,0,0.16);--ring:#737373;",
    "--primary:#2563eb;--primary-foreground:#ffffff;",
    "--accent:#e9e9e9;--accent-foreground:#262626;",
    "--destructive:#b91c1c;--warning:#b45309;--success:#047857;--info:#0369a1;",
    "--code-background:#f6f8fa;--code-foreground:#1f2328;",
    "--chart-1:#2563eb;--chart-2:#d97706;--chart-3:#059669;",
    "--chart-4:#db2777;--chart-5:#7c3aed;--chart-6:#0891b2}",
    "html{background:var(--background);color:var(--foreground);font-family:var(--font-sans);",
    "font-size:14px;line-height:1.5;-webkit-font-smoothing:antialiased}",
    "body{margin:0;padding:16px}",
    "code,kbd,pre,samp{font-family:var(--font-mono)}",
    "</style>",
);

/// Runs first in every published page, before the page's own styles and scripts.
///
/// It reads what a Ghostex window hands the page in its address: the theme (`#gx-theme=light` or
/// `#gx-theme=dark`), so a page opened over the chat follows the app's appearance rather than the
/// system's from its first paint, and `gx-floating=1` when the page is in the floating window, which
/// `PAGE_API` closes once answers are sent. It then drops that fragment so the page's own hash
/// routing never sees it. A web link the reader clicks opens in a new window, which the floating
/// window and the browser both send to the reader's browser, so the page itself is never navigated
/// away.
pub(super) const PAGE_BOOTSTRAP: &str = concat!(
    "<script>(function(){var d=document.documentElement;",
    "try{var h=location.hash,m=/[#&]gx-theme=(light|dark)/.exec(h);",
    "if(m)d.setAttribute(\"data-gx-theme\",m[1]);",
    "if(/[#&]gx-floating=1/.test(h))d.setAttribute(\"data-gx-floating\",\"\");",
    "if(/[#&]gx-/.test(h))history.replaceState(history.state,\"\",location.pathname+location.search);}catch(e){}",
    "document.addEventListener(\"click\",function(e){var t=e.target,l=t&&t.closest?t.closest(\"a[href]\"):null,u;",
    "if(!l)return;try{u=new URL(l.getAttribute(\"href\"),document.baseURI);}catch(x){return;}",
    "if(!/^https?:$/.test(u.protocol)||u.href.split(\"#\")[0]===location.href.split(\"#\")[0])return;",
    "l.setAttribute(\"target\",\"_blank\");l.setAttribute(\"rel\",\"noopener\");},true);})();</script>",
);

/// Where `serve` puts the page's answered state for `PAGE_API` to read.
pub(crate) const PAGE_STATE_MARKER: &str = "<!--gx-page-state-->";

/// The page's side of answering: `ghostex.submit(answers)` posts any JSON to the page's own
/// `/answers` route, and `ghostex.answered` / `ghostex.answers` tell a reopened page what was sent.
/// A page that was answered also shows a short note, so a reader who reopens it sees the answers
/// went through even when the page itself ignores the state. The note goes into the page's
/// `[data-gx-answered]` element when it has one, else at the top of the question kit's column
/// (`<gx-questions>`, kit.rs), so it is as wide as the questions, else at the top of the page.
/// In the floating window (`data-gx-floating`, `PAGE_BOOTSTRAP`) the page closes itself once the
/// answers are sent, which the window takes as its cue to close.
///
/// Until then the reader's work is kept on the host: the kit's picks (`__saveDraft("kit", …)`),
/// a page's own `ghostex.saveDraft(value)` (read back as `ghostex.draft`), and the value of every
/// named field outside the kit are posted to the page's `/draft` route a moment after each change
/// and when the page is hidden or closed, and handed back by serve.rs when the page opens again;
/// plain fields are filled back in by themselves.
/// The routes are answers.rs and drafts.rs; serve.rs injects the state.
///
/// CDXC:SessionChat 2026-10-26 WHY:
/// A page on the phone reaches gxserver through the phone's SSH forward, which goes away whenever the link drops and comes back a moment later: the user typed answers into every question on the phone, the drafts stopped landing when the link dropped (their failure was ignored), and Send answered "Failed to fetch" and gave up, so closing the page lost everything typed after the drop. So a draft that does not land stays pending and is posted again until it does, and a send that cannot reach the host (no answer, or a 5xx while gxserver restarts) keeps trying with the answers held, telling the page through `submit`'s second argument; only a refusal from gxserver itself (already sent, no such page, a bad body) ends a send. A retried send that finds the answers already stored (409 after an earlier try whose reply was lost) counts as sent.
/// SEE-ALSO: apps/mobile/app/modules/ghostex-native/android/src/main/java/expo/modules/ghostexnative/GhostexPortForward.kt (the forward keeps its phone port across reconnects, so the page's address works again once the link is back), apps/mobile/app/src/chat/native/cards/PageSheet.tsx (reads `ghostex.__pending()`, the draft not yet saved, when the sheet closes or the page has to move to a new port, and keeps it until it can be posted: apps/mobile/app/src/chat/native/cards/pageDrafts.ts).
pub(super) const PAGE_API: &str = r#"<script>(function(){var st=null,draft={},dirty=false,timer=0,posting=false,changes=0,pause=0,base=location.pathname.replace(/\/+$/,"");
function later(wait){return Math.min(wait?wait*2:1000,10000);}
function note(){if(!st||!st.answered||!document.body||document.getElementById("gx-answered"))return;var b=document.createElement("div");b.id="gx-answered";b.setAttribute("role","status");b.setAttribute("style","margin:0 0 12px;padding:8px 12px;border-radius:8px;font:500 12.5px/1.4 var(--font-sans);color:var(--success);background:color-mix(in srgb,var(--success) 12%,transparent);border:1px solid color-mix(in srgb,var(--success) 32%,transparent)");var when="";try{when=st.answeredAt?" on "+new Date(st.answeredAt).toLocaleString():"";}catch(e){}b.textContent="Answered"+when+". The agent has your answers.";var slot=document.querySelector("[data-gx-answered]"),kit=document.querySelector("gx-questions");if(slot)slot.appendChild(b);else if(kit){b.style.margin="0";kit.insertBefore(b,kit.firstChild);}else document.body.insertBefore(b,document.body.firstChild);}
function post(leaving){if(!dirty||(st&&st.answered))return;clearTimeout(timer);var body=JSON.stringify({draft:draft});if(leaving){if(navigator.sendBeacon)navigator.sendBeacon(base+"/draft",body);return;}if(posting)return;posting=true;var at=changes;
fetch(base+"/draft",{method:"POST",body:body,keepalive:true}).then(function(r){posting=false;if(r.status>=500)throw r;pause=0;if(changes===at)dirty=false;else post();}).catch(function(){posting=false;pause=later(pause);clearTimeout(timer);timer=setTimeout(post,pause);});}
function save(part,value){if(st&&st.answered)return;if(value==null)delete draft[part];else draft[part]=value;changes++;dirty=true;clearTimeout(timer);timer=setTimeout(post,300);}
addEventListener("pagehide",function(){post(true);});
document.addEventListener("visibilitychange",function(){if(document.visibilityState==="hidden")post();});
var restoring=false;
function fieldKey(el){if(!el||!el.tagName||(el.closest&&el.closest("gx-questions,[data-gx-nodraft]")))return null;var tag=el.tagName,type=(el.type||"").toLowerCase(),name=el.name||el.id;if(!name||!/^(INPUT|TEXTAREA|SELECT)$/.test(tag)||/^(password|file|hidden|submit|button|reset|image)$/.test(type))return null;if(type==="radio")return"radio:"+name;if(type==="checkbox")return"check:"+name+":"+el.value;return"value:"+name;}
function fieldValue(el){var type=(el.type||"").toLowerCase();if(type==="checkbox")return el.checked;if(type==="radio")return el.checked?el.value:undefined;return el.value;}
function noteField(e){if(restoring)return;var key=fieldKey(e.target);if(!key)return;var value=fieldValue(e.target);if(value===undefined)return;var fields=draft.fields&&typeof draft.fields==="object"?draft.fields:{};fields[key]=value;save("fields",fields);}
document.addEventListener("input",noteField,true);document.addEventListener("change",noteField,true);
function restoreFields(){var fields=draft.fields;if(!fields||typeof fields!=="object"||(st&&st.answered))return;restoring=true;try{Array.prototype.forEach.call(document.querySelectorAll("input,textarea,select"),function(el){var key=fieldKey(el);if(!key||!(key in fields))return;var value=fields[key],type=(el.type||"").toLowerCase();if(type==="checkbox")el.checked=!!value;else if(type==="radio")el.checked=el.value===value;else el.value=value;el.dispatchEvent(new Event("input",{bubbles:true}));el.dispatchEvent(new Event("change",{bubbles:true}));});}finally{restoring=false;}}
window.ghostex={get answered(){return!!(st&&st.answered);},get answers(){return st&&st.answered?st.answers:null;},
get draft(){return draft.page===undefined?null:draft.page;},saveDraft:function(value){save("page",value===undefined?null:value);},
submit:function(answers,retrying){var body=JSON.stringify({answers:answers===undefined?null:answers}),tries=0,wait=0;return new Promise(function(resolve,reject){
function sent(j){st={answered:true,answers:answers,answeredAt:j.answeredAt};draft={};dirty=false;clearTimeout(timer);note();if(document.documentElement.hasAttribute("data-gx-floating"))setTimeout(function(){window.close();},700);resolve(j);}
function again(){if(typeof retrying==="function")try{retrying("Not sent yet: Ghostex can't be reached. Trying again… Your answers are kept.");}catch(e){}wait=later(wait);setTimeout(attempt,wait);}
function attempt(){tries++;fetch(base+"/answers",{method:"POST",body:body}).then(function(r){return r.json().catch(function(){return{};}).then(function(j){if(r.ok&&j.ok)sent(j);else if(r.status===409&&tries>1)sent(j);else if(r.status>=500)again();else reject(new Error(j.error||"Your answers were not sent."));});},again);}
attempt();});},
__draft:function(part){return draft[part]===undefined?null:draft[part];},__saveDraft:save,__pending:function(){return dirty&&!(st&&st.answered)?draft:null;},
__state:function(s){st=s;draft=s&&!s.answered&&s.draft&&typeof s.draft==="object"?s.draft:{};var ready=function(){note();restoreFields();};if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",ready);else ready();}};})();</script>"#;
