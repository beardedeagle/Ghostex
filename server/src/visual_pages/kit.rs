//! The question kit every published page carries: an agent writes its questions as markup and the
//! kit draws them the same way on every page.
//!
//! ```html
//! <gx-questions submit="Send to the agent">
//!   <h1>Settings redesign</h1>
//!   <gx-wide>…a full-width mockup the questions refer to…</gx-wide>
//!   <gx-question tag="Q1" title="Which layout?">
//!     <p>Why this matters.</p>
//!     <gx-choice recommended>Tabs<small>One page per area.</small></gx-choice>
//!     <gx-choice>One long page</gx-choice>
//!   </gx-question>
//!   <gx-question title="Anything else?" type="text"></gx-question>
//! </gx-questions>
//! ```
//!
//! A mockup can stand for one choice: an element with `data-question` (the question's `name`,
//! title or tag) and `data-choice` (the choice's value, label or letter), usually the element
//! holding the variant's title and its mockup, gets a Pick button at the right end of its first
//! heading's row that picks that choice (and shows it picked, both there and in the question), and a
//! note box under it whose text arrives as `"<question> · <choice> (note)"`.
//!
//! ```html
//! <section data-question="Q1" data-choice="tabs"><h3>A. Tabs</h3>…mockup…</section>
//! ```
//!
//! `<gx-questions>` lays its children out in one centred column (`--gx-column`, 720px) and lets a
//! `<gx-wide>` child span the page's full width; what a wide section holds that is narrower than
//! the page is centred, so it lines up symmetrically with the column. A choice question
//! (`multiple` for several picks) letters its choices, ends with "Other / none of these", and has
//! a note box; a `type="text"` question is a text box. A question left blank when the answers are
//! sent goes as `"skipped"`, and the kit adds a Skip all button that sends every question as
//! `"skipped"` at once (see the
//! SessionChat 2026-10-16 DECISION on `SKIPPED` in packages/gx-visual/src/questions.rs). The kit adds the send button, posts the answers with `ghostex.submit`
//! keyed by each question's `name` (its title when it has none), with a non-empty note as
//! `"<key> (note)"`, and shows a sent page's answers locked when it is opened again. Until then it
//! keeps the reader's picks as the page's draft on the host (theme.rs `PAGE_API`), so closing and
//! reopening the page brings them back.
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User: "please same style for the html modal also and the inline questions": each question is a "Clear ask, clear answer" card like the chat's (apps/desktop/src/app/native_chat/ask_card.rs): a tile beside the tag chip and title (see the later decision on the yellow "?"), picks that turn green with it, options that light up amber when picked (the mockup Pick button too), and a footer bar with the status and a filled send button showing its key. It takes the chat card's own colours too: its light-grey Recommended badge and send button and its bright labels and titles at medium weight. The card is darker ("The colors are too light for the bg of the question cards please make it darker", then "background for the whole question component still too light"), and its amber is quieter ("please make the yellow more subtle"); the chat's cards then took both ("yellow + dark all"): a picked option, the tile, the mockup ring and the Picked button take a light amber tint instead of a strong one or a solid fill. Only the tile stays beside the tag chip and title, since a page spreads its questions between mockups.
//! User, about the yellow "?" tile at the left of every question heading: "remove this yellow thing here please so i can tell what i answered and what i didn't answer". A question shows the green check as soon as it has an answer (a pick made, text typed, or a restored draft). After the page is sent, an answered question keeps the green check and one left blank keeps the plain tile. User (2026-10-19): "can we make this match the look of other icons we just worked on please?", so that tile is the chat's shared one (the SessionChat 2026-10-18 DECISION on `SINGLE_LINE_CARD` in apps/desktop/src/app/native_chat/cards.rs: 28px neutral tile, question-bubble glyph, a 12px green check badge), superseding the tinted green and muted squares. Then, of the empty slot an unanswered question kept: "fix this not showing an icon here, should show one with this color question mark #D2A024", so a question still waiting shows the chat's waiting tile (the checklist glyph with a #D2A024 "?" badge, Q305) until it has an answer, superseding the empty slot from "remove this yellow thing here please"; the green check still tells an answered question from a waiting one.
//! User picked from the looks page: a question's tag is a filled chip ("make it show like eyebrow"), and a choice keeps its radio or checkbox with the letter as a muted keycap on the right; the in-chat form (apps/desktop/src/app/native_chat/question_form.rs, the phone's QuestionForm.tsx) matches. User (2026-10-19): "please move the Q294 to be right aligned same row as the question", so the chip sits at the right end of the title's first line and a long title wraps beside it, superseding the chip above the title.
//! User: Ctrl+Enter (Cmd+Enter on macOS) presses the page's send button from anywhere on it, a note box included; the in-chat form does the same.
//! User: a reader picks an option right at its mockup (a Pick button by the mockup's title that picks that choice in its question, shown picked on both and cleared from either), and writes a note under each mockup that arrives with the answers keyed to that question and choice, is kept as a draft, and shows in the answers card.
//! User: "question cards in the html having different width is an absolute NO GO … all take up the width as they should": every question card fills the column, even when a page's own CSS centres it with auto margins (which shrink a grid item to its content).
//! User: the picked mockup's outline must never be cut off and needs more padding so it is not tight on the design: a tagged option gets 14px of room and a 2px ring inside its own box (content-box, so a fixed-width mockup keeps its width), because an outline drawn outside the box is clipped by a scrolling wrapper such as an `overflow-x:auto` frame.
//! User: every question page offers "Other / none of these" with a note box, consistently, instead of each agent hand-rolling it, and the reader may leave a question unanswered. No choice is ever preselected; an agent marks the one it recommends with a Recommended badge and the reader picks. Mockups can use the full width of the page between or above the questions, and the Answered note is as wide as the question cards.
//!
//! CDXC:SessionChat 2026-10-24 DECISION:
//! User, after an agent's page showed its variants A, B, C but listed the question's choices C, B, A (recommended first): "some agent gave me the options flipped in the selection area below!! ... the order of the options above, has to match their order below!" The kit enforces it instead of trusting every agent: before lettering, a question's choices are sorted into the order their `data-question` / `data-choice` mockups appear in the document (matched by the choice's value or label; a bare letter cannot rank because letters come from the order). Choices no mockup names keep their relative order after the matched ones, "Other / none of these" stays last, and a question with no mockups is left as written. skills/ghostex-visuals/SKILL.md teaches the same rule.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The answers card folds the `"<key> (note)"` entries into their question's row (packages/gx-chat-core/src/transcript/page_answers.rs); skills/ghostex-visuals/SKILL.md teaches agents this markup. Change them together.

/// The kit's look. It comes before the page's own styles, and its heading and paragraph defaults
/// have no specificity (`:where`), so a page's own CSS always wins.
///
/// CDXC:SessionChat 2026-10-11 SEE-ALSO:
/// The `--gx-*` tokens at its top are the chat card's palette (apps/desktop/src/app/native_chat/appearance.rs: `foreground`, `card_muted`, `control_primary`, `card_panel`, `card_row`, `card_footer`, `input_border`, `control_border`), worked out from the page's `--background` by the same rule the chat uses on its own (dark mode lifts the card 3.5%, its rows 1.5% and the footer 2.5% toward white), with `light-dark()` choosing the theme. The phone's copy is apps/mobile/app/src/chat/native/transcript/theme.ts. Change them together.
pub(super) const KIT_STYLE: &str = r#"<style id="ghostex-kit">
:root{--gx-fg:light-dark(#27272a,#fcfcfc);--gx-card-muted:light-dark(#71717b,#b4b8bf);--gx-control:light-dark(#18181b,#e5e5e5);--gx-card:light-dark(color-mix(in srgb,#fff 60%,var(--background)),color-mix(in srgb,#fff 3.5%,var(--background)));--gx-footer:light-dark(color-mix(in srgb,#fff 10%,var(--background)),color-mix(in srgb,#fff 2.5%,var(--background)));--gx-option:light-dark(var(--background),color-mix(in srgb,#fff 1.5%,var(--background)));--gx-card-border:light-dark(color-mix(in srgb,#000 9.5%,var(--background)),rgba(255,255,255,.08));--gx-option-border:light-dark(color-mix(in srgb,#000 9.5%,var(--background)),rgba(255,255,255,.06))}
gx-questions{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,var(--gx-column,720px)) minmax(0,1fr);row-gap:12px;align-content:start;padding:12px 0 24px}
gx-questions>*{grid-column:2;min-width:0}
gx-questions>gx-wide{grid-column:1/-1}
gx-wide>*{margin-left:auto;margin-right:auto}
gx-wide,gx-choice{display:block;min-width:0}
:where(gx-questions>h1,gx-questions>h2,gx-questions>h3,gx-questions>p){margin:0}
:where(gx-questions>h1){font-size:18px;font-weight:600;line-height:1.35}
:where(gx-questions>h2){margin-top:8px;font-size:15px;font-weight:600}
:where(gx-questions>p){color:var(--muted-foreground)}
gx-question{display:block;border:1px solid var(--gx-card-border);background:var(--gx-card);color:var(--gx-card-muted);border-radius:14px;padding:12px 14px 14px}
gx-questions gx-question{width:100%;box-sizing:border-box;min-width:0}
.gx-q-head{display:flex;align-items:center;gap:10px;margin-bottom:12px}
.gx-q-tile{position:relative;flex:none;display:grid;place-items:center;width:28px;height:28px;border-radius:8px;color:color-mix(in srgb,var(--gx-fg) 80%,transparent);background:color-mix(in srgb,var(--gx-fg) 6%,transparent)}
.gx-q-tile>svg{width:17px;height:17px}
.gx-q-done,gx-question.gx-has-answer .gx-q-wait,gx-question.gx-answered .gx-q-wait,gx-question.gx-unanswered .gx-q-wait{display:none}
gx-question.gx-has-answer .gx-q-done,gx-question.gx-answered .gx-q-done,gx-question.gx-unanswered .gx-q-done{display:block}
.gx-q-badge{position:absolute;right:-4px;bottom:-4px;display:grid;place-items:center;width:16px;height:16px;box-sizing:border-box;border-radius:50%;border:2px solid var(--gx-card);background:#16a34a;color:#fff}
.gx-q-badge.gx-ask{background:#D2A024;color:#161616}
.gx-q-badge.gx-check,gx-question.gx-has-answer .gx-q-badge.gx-ask,gx-question.gx-answered .gx-q-badge.gx-ask{display:none}
gx-question.gx-has-answer .gx-q-badge.gx-check,gx-question.gx-answered .gx-q-badge.gx-check{display:grid}
.gx-q-badge svg{width:6px;height:6px}
gx-question.gx-unanswered .gx-q-badge.gx-ask,gx-question.gx-unanswered .gx-q-badge.gx-check{display:none}
.gx-q-heading{display:flex;flex:1;min-width:0;align-items:flex-start;gap:10px}
.gx-q-tag{flex:none;margin-top:2px;font:500 10.5px/18px var(--font-mono);color:var(--muted-foreground);background:color-mix(in srgb,var(--muted-foreground) 18%,transparent);border-radius:999px;padding:0 7px}
.gx-q-title{flex:1;min-width:0;color:var(--gx-fg);font-size:15px;font-weight:500;line-height:22px}
.gx-q-desc{color:var(--muted-foreground);font-size:13px;line-height:1.5;margin:-4px 0 12px}
.gx-q-desc>:first-child{margin-top:0}.gx-q-desc>:last-child{margin-bottom:0}
.gx-choices{display:flex;flex-direction:column;gap:8px}
.gx-choice{display:flex;gap:12px;align-items:flex-start;width:100%;box-sizing:border-box;margin:0;text-align:left;padding:10px 12px;border:1px solid var(--gx-option-border);border-radius:10px;background:var(--gx-option);color:var(--gx-fg);font:inherit;font-size:14px;line-height:20px;cursor:pointer}
.gx-choice:hover:not(:disabled):not([aria-checked=true]){border-color:color-mix(in srgb,var(--gx-fg) 14%,transparent);background:color-mix(in srgb,var(--gx-fg) 3%,var(--gx-option))}
.gx-choice:focus-visible{outline:none;border-color:var(--ring);box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 20%,transparent)}
.gx-choice[aria-checked=true]{border-color:color-mix(in srgb,var(--warning) 32%,transparent);background:color-mix(in srgb,var(--warning) 4%,var(--gx-card))}
.gx-choice:disabled{cursor:default}
.gx-choice:disabled:not([aria-checked=true]){opacity:.55}
.gx-mark{flex-shrink:0;width:16px;height:16px;margin-top:2px;box-sizing:border-box;border:1.5px solid color-mix(in srgb,var(--muted-foreground) 70%,transparent);border-radius:50%}
.gx-choice[role=checkbox] .gx-mark{border-radius:4px}
.gx-choice[aria-checked=true] .gx-mark{border-color:color-mix(in srgb,var(--warning) 85%,transparent);background:color-mix(in srgb,var(--warning) 85%,transparent);box-shadow:inset 0 0 0 3px var(--gx-card)}
.gx-choice[role=checkbox][aria-checked=true] .gx-mark{box-shadow:inset 0 0 0 2px var(--gx-card)}
gx-question.gx-answered .gx-choice[aria-checked=true]{border-color:color-mix(in srgb,var(--success) 55%,transparent);background:color-mix(in srgb,var(--success) 8%,var(--gx-card))}
gx-question.gx-answered .gx-choice[aria-checked=true] .gx-mark{border-color:var(--success);background:var(--success)}
.gx-letter{flex:none;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;margin-top:1px;border:1px solid color-mix(in srgb,var(--gx-fg) 12%,transparent);border-radius:5px;background:color-mix(in srgb,var(--background) 50%,transparent);font:500 11px/16px var(--font-mono);color:var(--muted-foreground);text-align:center}
.gx-choice[aria-checked=true] .gx-letter{color:var(--gx-fg)}
.gx-choice-body{flex:1;min-width:0;font-weight:500}
.gx-other .gx-choice-body{color:var(--gx-card-muted)}
.gx-other[aria-checked=true] .gx-choice-body{color:inherit}
.gx-rec{display:inline-block;margin-left:6px;padding:0 6px;border:1px solid color-mix(in srgb,var(--gx-control) 60%,transparent);border-radius:999px;color:var(--gx-control);font-size:10.5px;font-weight:600;line-height:16px;vertical-align:1px}
.gx-choice-body small{display:block;color:var(--muted-foreground);font-size:13px;font-weight:400;line-height:19px;margin-top:1px}
.gx-note,.gx-text{display:block;width:100%;box-sizing:border-box;margin:8px 0 0;min-height:34px;resize:vertical;field-sizing:content;font:inherit;font-size:13.5px;line-height:20px;color:var(--gx-fg);background:var(--gx-option);border:1px solid var(--gx-card-border);border-radius:9px;padding:6px 10px}
.gx-text{min-height:64px}
.gx-note:focus,.gx-text:focus{outline:none;border-color:var(--ring);box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 20%,transparent)}
.gx-note::placeholder,.gx-text::placeholder{color:var(--muted-foreground)}
.gx-note[readonly],.gx-text[readonly]{resize:none}
.gx-note[hidden],.gx-text[hidden]{display:none}
.gx-has-pick{display:flex;align-items:center;gap:8px}
.gx-pick{display:inline-flex;flex:none;align-items:center;gap:4px;margin-left:auto;padding:2px 10px;vertical-align:middle;border:1px solid var(--border);border-radius:999px;background:transparent;color:var(--foreground);font:500 12px/1.5 var(--font-sans);cursor:pointer}
.gx-pick:hover:not(:disabled):not([aria-pressed=true]){background:var(--accent);color:var(--accent-foreground)}
.gx-pick[aria-pressed=true]{background:color-mix(in srgb,var(--warning) 12%,transparent);border-color:color-mix(in srgb,var(--warning) 40%,transparent);color:var(--warning)}
.gx-pick[aria-pressed=true]:hover:not(:disabled),.gx-pick[aria-pressed=true]:active:not(:disabled){background:color-mix(in srgb,var(--warning) 20%,transparent);border-color:color-mix(in srgb,var(--warning) 55%,transparent);color:var(--warning)}
.gx-pick:focus-visible{outline:none;box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 35%,transparent)}
.gx-pick:disabled{cursor:default}
.gx-pick:disabled:not([aria-pressed=true]){opacity:.5}
.gx-option-head{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px;font-weight:600}
[data-question][data-choice]:not(h1,h2,h3,h4,h5,h6){box-sizing:content-box;padding:14px;border:2px solid transparent;border-radius:16px}
:where([data-question][data-choice]:not(h1,h2,h3,h4,h5,h6)){max-width:calc(100% - 32px)}
[data-question][data-choice].gx-picked:not(h1,h2,h3,h4,h5,h6){border-color:color-mix(in srgb,var(--warning) 40%,transparent)}
.gx-option-note{display:block;width:100%;box-sizing:border-box;margin:8px 0 0;min-height:34px;resize:vertical;field-sizing:content;font:inherit;font-size:12.5px;color:var(--foreground);background:transparent;border:1px solid var(--border);border-radius:var(--radius);padding:7px 9px}
.gx-option-note:focus{outline:none;border-color:var(--ring);box-shadow:0 0 0 3px color-mix(in srgb,var(--ring) 20%,transparent)}
.gx-option-note::placeholder{color:var(--muted-foreground)}
.gx-option-note[readonly]{resize:none}
.gx-option-note[hidden]{display:none}
.gx-send{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-top:4px;padding:10px 14px;border:1px solid var(--gx-card-border);border-radius:14px;background:var(--gx-footer)}
.gx-send-button{display:inline-flex;align-items:center;gap:6px;margin-left:auto;height:28px;box-sizing:border-box;padding:0 11px;font:inherit;font-size:13px;font-weight:500;border-radius:8px;border:1px solid var(--gx-control);background:var(--gx-control);color:var(--background);cursor:pointer}
.gx-send-button:hover:not(:disabled){opacity:.9}
.gx-send-button:disabled{opacity:.5;cursor:default}
.gx-skip-all{display:inline-flex;align-items:center;gap:6px;margin-left:auto;height:28px;box-sizing:border-box;padding:0 11px;font:inherit;font-size:13px;font-weight:500;border-radius:8px;border:1px solid transparent;background:light-dark(color-mix(in srgb,var(--foreground) 7%,transparent),color-mix(in srgb,var(--foreground) 10%,transparent));color:var(--foreground);cursor:pointer}
.gx-skip-all:hover{background:light-dark(color-mix(in srgb,var(--foreground) 11%,transparent),color-mix(in srgb,var(--foreground) 15%,transparent))}
.gx-send-button,.gx-skip-all{position:relative}
.gx-send-button[data-tip]::after,.gx-skip-all[data-tip]::after{content:attr(data-tip);position:absolute;bottom:calc(100% + 6px);right:0;white-space:nowrap;padding:4px 8px;border-radius:6px;border:1px solid var(--border);background:var(--background);color:var(--foreground);font:500 12px/16px var(--font-mono);opacity:0;pointer-events:none;transition:opacity .08s ease 0s;z-index:5}
.gx-send-button[data-tip]:hover:not(:disabled)::after,.gx-skip-all[data-tip]:hover::after{opacity:1;transition-delay:.15s}
.gx-skip-all+.gx-send-button{margin-left:0}
.gx-send-status{flex:1;min-width:0;font-size:12.5px;color:var(--muted-foreground)}
.gx-send-status.gx-error{color:var(--destructive)}
.gx-send-info{position:relative;display:inline-flex;align-items:center;justify-content:center;width:20px;height:20px;padding:0;border:0;background:none;color:var(--muted-foreground);font:inherit;cursor:default}
.gx-send-info::after{content:attr(data-tip);position:absolute;bottom:calc(100% + 6px);left:0;width:max-content;max-width:240px;padding:4px 8px;border-radius:6px;border:1px solid var(--border);background:var(--background);color:var(--foreground);font:500 12px/16px var(--font-mono);text-align:left;opacity:0;pointer-events:none;transition:opacity .08s ease 0s;z-index:5}
.gx-send-info:hover::after,.gx-send-info:focus-visible::after{opacity:1;transition-delay:.15s}
</style>"#;

/// The kit's behaviour: turns the first `<gx-questions>` into a working form once the page has
/// loaded, after `PAGE_API` (theme.rs) has its answered state.
pub(super) const KIT_SCRIPT: &str = r#"<script>(function(){
var OTHER="Other / none of these",SKIPPED="skipped";
var TILE_ICON='<svg class="gx-q-wait" viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 4.5 3.6 5.6 5.5 3.6M2.5 10.5l1.1 1.1 1.9-2M8 4.6h5.5M8 10.6h5.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg><svg class="gx-q-done" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 3.5h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H7l-3 2.5V11.5H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/><path d="M6.6 6.2a1.5 1.5 0 1 1 2.1 1.4c-.5.2-.7.5-.7 1" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><circle cx="8" cy="10" r=".7" fill="currentColor"/></svg><span class="gx-q-badge gx-ask"><svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3.4 3.6a1.7 1.7 0 1 1 2.4 1.5c-.5.25-.8.55-.8 1.1" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><circle cx="5" cy="8.1" r=".8" fill="currentColor"/></svg></span><span class="gx-q-badge gx-check"><svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2 5.2 4.2 7.4 8 2.8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg></span>';
function el(tag,cls,text){var e=document.createElement(tag);if(cls)e.className=cls;if(text!=null)e.textContent=text;return e;}
function kids(node){return Array.prototype.slice.call(node.childNodes);}
function labelText(node){var c=node.cloneNode(true);c.querySelectorAll("small").forEach(function(s){s.remove();});return c.textContent.replace(/\s+/g," ").trim();}
/* The choices of a question that has mockups tied to it, in the order those mockups appear on the page, so the list below always reads like the variants above it. A mockup matches a choice by its value or label (a bare letter is only known once the choices are lettered, so it cannot rank); choices no mockup names keep their order after the matched ones. */
function inVariantOrder(els,key,tag){
  var wanted=[];
  Array.prototype.forEach.call(document.querySelectorAll("[data-question][data-choice]"),function(node){
    if(node.closest("gx-question"))return;
    var asked=(node.getAttribute("data-question")||"").trim();
    if(asked===key||(tag&&asked===tag))wanted.push((node.getAttribute("data-choice")||"").trim());
  });
  if(!wanted.length)return els;
  var ranked=els.map(function(ce,i){
    var label=labelText(ce),value=(ce.getAttribute("value")||"").trim()||label,rank=Infinity;
    for(var w=0;w<wanted.length;w++)if(wanted[w]===value||wanted[w]===label){rank=w;break;}
    return{ce:ce,i:i,rank:rank};
  });
  ranked.sort(function(a,b){return a.rank===b.rank?a.i-b.i:a.rank<b.rank?-1:1;});
  return ranked.map(function(r){return r.ce;});
}
function question(qe,index,used,changed){
  var title=(qe.getAttribute("title")||"").trim(),key=(qe.getAttribute("name")||"").trim()||title||"Question "+(index+1);
  for(var n=2;used[key];n++)key=key.replace(/ \(\d+\)$/,"")+" ("+n+")";
  used[key]=true;
  var multiple=qe.hasAttribute("multiple"),text=(qe.getAttribute("type")||"").toLowerCase()==="text";
  var titleId="gx-q-"+index,head=el("div","gx-q-head"),heading=el("div","gx-q-heading"),tile=el("span","gx-q-tile"),tag=qe.getAttribute("tag");
  qe.removeAttribute("title");
  tile.innerHTML=TILE_ICON;head.append(tile,heading);
  var titleEl=el("div","gx-q-title",title);titleEl.id=titleId;heading.append(titleEl);
  if(tag)heading.append(el("span","gx-q-tag",tag));
  /* The tile shows the waiting checklist with the "?" badge until the question has an answer, then the question bubble with the green check; once sent, a question left blank shows the bubble without a badge. */
  var has=function(){return false;};
  function live(){qe.classList.toggle("gx-has-answer",has());}
  function notify(){live();changed();}
  function answered(given){if(given)qe.classList.add("gx-answered");else qe.classList.add("gx-unanswered");}
  var choiceEls=[],desc=el("div","gx-q-desc");
  kids(qe).forEach(function(node){if(node.nodeName==="GX-CHOICE")choiceEls.push(node);else desc.append(node);});
  choiceEls=inVariantOrder(choiceEls,key,(tag||"").trim());
  qe.textContent="";qe.append(head);
  if(desc.textContent.trim()||desc.querySelector("img,svg,canvas,table"))qe.append(desc);
  if(text){
    var area=el("textarea","gx-text");area.placeholder=qe.getAttribute("placeholder")||"Your answer (optional)";area.setAttribute("aria-labelledby",titleId);
    has=function(){return !!area.value.trim();};area.addEventListener("input",notify);qe.append(area);
    return{key:key,tag:(tag||"").trim(),choices:[],collect:function(out){out[key]=area.value.trim()||SKIPPED;},
      restore:function(answers){if(typeof answers[key]==="string"&&answers[key]!==SKIPPED)area.value=answers[key];live();},
      lock:function(){answered(!!area.value.trim());area.readOnly=true;if(!area.value)area.hidden=true;}};
  }
  var list=el("div","gx-choices"),choices=[];
  list.setAttribute("role",multiple?"group":"radiogroup");list.setAttribute("aria-labelledby",titleId);
  var note=el("textarea","gx-note");note.rows=1;note.setAttribute("aria-label","Note");note.addEventListener("input",notify);
  function sync(){note.placeholder=other.on()?"What would you prefer instead? (optional)":"Note (optional)";}
  function choice(nodes,value,recommended,label){
    var button=el("button","gx-choice"),body=el("span","gx-choice-body"),index=choices.length,letter=index<26?String.fromCharCode(97+index):String(index+1);
    button.type="button";button.setAttribute("role",multiple?"checkbox":"radio");button.setAttribute("aria-checked","false");

    nodes.forEach(function(node){body.append(node);});
    if(recommended)body.insertBefore(el("span","gx-rec","Recommended"),body.querySelector(":scope>small"));
    button.append(el("span","gx-mark"),body,el("span","gx-letter",letter));list.append(button);
    var c={button:button,value:value,label:label,letter:letter,on:function(){return button.getAttribute("aria-checked")==="true";},set:function(v){button.setAttribute("aria-checked",v?"true":"false");}};
    /* A pick toggles; one of N replaces the pick. */
    button.addEventListener("click",function(){var on=!c.on();if(on)choices.forEach(function(o){if(!multiple||o.exclusive||c.exclusive)o.set(false);});c.set(on);sync();notify();if(on&&c===other)note.focus();});
    choices.push(c);return c;
  }
  choiceEls.forEach(function(ce){var label=labelText(ce);choice(kids(ce),(ce.getAttribute("value")||"").trim()||label,ce.hasAttribute("recommended"),label);});
  var other=choice([document.createTextNode(OTHER)],OTHER,false,OTHER);other.button.classList.add("gx-other");
  has=function(){return choices.some(function(c){return c.on();})||!!note.value.trim();};
  sync();qe.append(list,note);
  return{key:key,tag:(tag||"").trim(),choices:choices,collect:function(out){var on=choices.filter(function(c){return c.on();}).map(function(c){return c.value;});
      var written=note.value.trim();out[key]=on.length?(multiple?on:on[0]):written?null:SKIPPED;if(written)out[key+" (note)"]=written;},
    restore:function(answers){var v=answers[key],picked=Array.isArray(v)?v:v==null?[]:[v];
      choices.forEach(function(c){c.set(picked.indexOf(c.value)>=0);});
      if(typeof answers[key+" (note)"]==="string")note.value=answers[key+" (note)"];sync();live();},
    lock:function(){answered(choices.some(function(c){return c.on();})||!!note.value.trim());choices.forEach(function(c){c.button.disabled=true;});note.readOnly=true;if(!note.value)note.hidden=true;}};
}
/* A mockup tied to a choice (`data-question` + `data-choice`): a Pick button by its title that
   clicks that choice, and a note box under it, sent as "<question> · <choice> (note)". */
function bindOptions(questions,changed){
  var found=[];
  Array.prototype.forEach.call(document.querySelectorAll("[data-question][data-choice]"),function(node){
    if(node.closest("gx-question"))return;
    var asked=(node.getAttribute("data-question")||"").trim(),wanted=(node.getAttribute("data-choice")||"").trim();
    var q=questions.filter(function(x){return x.key===asked||(x.tag&&x.tag===asked);})[0];
    var c=q&&q.choices.filter(function(x){return x.value===wanted||x.label===wanted||x.letter===wanted.toLowerCase();})[0];
    if(!c){if(window.console)console.warn("ghostex: no choice \""+wanted+"\" in question \""+asked+"\"");return;}
    var heading=/^H[1-6]$/.test(node.tagName)?node:node.querySelector("h1,h2,h3,h4,h5,h6");
    var pick=el("button","gx-pick");pick.type="button";
    /* The Pick button sits at the right end of the title's row. */
    if(heading){heading.classList.add("gx-has-pick");heading.append(pick);}
    else{var head=el("div","gx-option-head");head.append(el("span","gx-option-label",c.label),pick);node.insertBefore(head,node.firstChild);}
    var note=el("textarea","gx-option-note");note.rows=1;note.placeholder="Note about this option (optional)";note.setAttribute("aria-label","Note about "+c.label);
    if(heading===node){var mockup=node.nextElementSibling;node.parentNode.insertBefore(note,mockup?mockup.nextSibling:null);}else node.append(note);
    var key=q.key+" \u00b7 "+c.value+" (note)";
    pick.addEventListener("click",function(){c.button.click();});
    note.addEventListener("input",changed);
    found.push({
      sync:function(){var on=c.on();node.classList.toggle("gx-picked",on);pick.setAttribute("aria-pressed",on?"true":"false");pick.textContent=on?"\u2713 Picked":"Pick";pick.title=on?"Click again to clear":"Pick \""+c.label+"\"";pick.disabled=c.button.disabled;},
      collect:function(out){var written=note.value.trim();if(written)out[key]=written;},
      restore:function(answers){if(typeof answers[key]==="string")note.value=answers[key];},
      lock:function(){note.readOnly=true;if(!note.value)note.hidden=true;}});
  });
  return found;
}
function setup(root){
  var used={},questions=[],options=[];
  function collect(){var out={};questions.forEach(function(q){q.collect(out);});options.forEach(function(o){o.collect(out);});return out;}
  function restore(answers){questions.forEach(function(q){q.restore(answers);});options.forEach(function(o){o.restore(answers);});}
  function syncOptions(){options.forEach(function(o){o.sync();});}
  /* The reader's picks so far are kept on the host (PAGE_API's draft), so closing and reopening the page loses nothing. */
  function changed(){syncOptions();if(window.ghostex&&ghostex.__saveDraft)ghostex.__saveDraft("kit",collect());}
  questions=Array.prototype.map.call(root.querySelectorAll("gx-question"),function(qe,index){return question(qe,index,used,changed);});
  options=bindOptions(questions,changed);syncOptions();
  var bar=el("div","gx-send"),button=el("button","gx-send-button"),label=el("span",null,root.getAttribute("submit")||"Send answers"),status=el("span","gx-send-status"),info=el("button","gx-send-info");
  /* The sentence about blank questions is an (i) icon with the quick tooltip, not a line of text beside the buttons (the SessionChat 2026-10-24 DECISION in apps/desktop/src/app/native_chat/ask_card.rs `ask_info_hint`). Any status message replaces the icon. */
  info.type="button";info.setAttribute("aria-label","Questions you leave blank are sent as skipped.");info.setAttribute("data-tip","Questions you leave blank are sent as skipped.");info.innerHTML='<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"/><path d="M12 9h.01"/><path d="M11 12h1v4h1"/></svg>';status.append(info);
  var mac=/Mac|iPhone|iPad/.test(navigator.platform||"");
  /* The shortcuts show in a quick tooltip (CSS, data-tip), not on the buttons. */
  button.setAttribute("data-tip","Send answers · "+(mac?"⌘↵":"Ctrl+Enter"));
  /* Skip all: sends at once with every question as skipped and no notes, whatever is picked or typed (see the SessionChat DECISION on SKIPPED in packages/gx-visual/src/questions.rs). */
  var skipAll=el("button","gx-skip-all");skipAll.type="button";skipAll.append(el("span",null,"Skip all"));skipAll.setAttribute("data-tip","Skip all · "+(mac?"⌘⇧↵":"Ctrl+Shift+Enter"));
  button.type="button";button.append(label);bar.append(status,skipAll,button);root.append(bar);
  skipAll.addEventListener("click",function(){
    if(button.disabled)return;
    var out={};questions.forEach(function(q){out[q.key]=SKIPPED;});
    /* Once sent, the page shows what went: no picks and no notes. */
    send(out,function(){restore({});root.querySelectorAll("textarea").forEach(function(t){t.value="";});});
  });
  function lock(){questions.forEach(function(q){q.lock();});options.forEach(function(o){o.lock();});syncOptions();button.disabled=true;skipAll.remove();label.textContent="Sent";button.removeAttribute("data-tip");status.classList.remove("gx-error");status.textContent="Sent to the agent.";}
  if(!window.ghostex){button.disabled=true;skipAll.remove();status.textContent="This Ghostex can't send answers from a page; type them in the chat.";return;}
  if(ghostex.answered){restore(ghostex.answers||{});lock();return;}
  var saved=ghostex.__draft&&ghostex.__draft("kit");if(saved&&typeof saved==="object"){restore(saved);syncOptions();}
  function send(out,sent){
    button.disabled=true;status.classList.remove("gx-error");status.textContent="Sending…";
    /* A send that can't reach Ghostex keeps trying with everything left as typed, and says so (PAGE_API's submit). */
    ghostex.submit(out,function(message){status.classList.add("gx-error");status.textContent=message;}).then(function(){if(sent)sent();lock();},function(error){button.disabled=false;status.classList.add("gx-error");status.textContent=error&&error.message||"Your answers were not sent.";});
  }
  button.addEventListener("click",function(){send(collect());});
  /* Ctrl+Enter (Cmd+Enter on macOS) presses the send button from anywhere on the page, a note box included; with Shift it presses Skip all. */
  document.addEventListener("keydown",function(e){if(e.key==="Enter"&&(e.ctrlKey||e.metaKey)&&!e.isComposing&&!button.disabled){e.preventDefault();(e.shiftKey?skipAll:button).click();}});
}
function init(){var root=document.querySelector("gx-questions");if(root)setup(root);}
if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",init);else init();
})();</script>"#;
