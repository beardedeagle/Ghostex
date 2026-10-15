//! The Hub's file editor: GPUI-Kit's code editor (`EditorState` + `Editor`, tree-sitter
//! highlighting) dressed as the Monaco editor the React Hub embedded.
//!
//! CDXC:AgentLauncher 2026-09-28 WHY:
//! The migration to GPUI-Kit components (see mod.rs) replaces Monaco with the Kit's code editor, the default the migration plan picked for the Hub. It keeps Monaco's look: 13px monospace on Monaco's row height (18px on Windows and Linux, 20px on macOS), a folding column, 16px of room above and below the text, Monaco's `vs-dark` / `vs` colours (the Hub pane under window glass) and no minimap. Tree-sitter colours a Markdown file's front matter as YAML where Monaco left it plain.
//!
//! CDXC:AgentLauncher 2026-10-15 DECISION:
//! User: "add wrapping for this also and remove the line numbers on the left". The Hub's viewer (every file tab: Skills, MDs, Hooks, Configs & MCPs) has no line numbers, and its header carries the Files view's wrap toggle, sharing that view's remembered choice (`ghostex.manage.wordWrap`) so one setting covers both viewers.
use super::super::native_modal_kit::{MODAL_MONO_FONT, hsla};
use super::palette::HubPalette;
use gpui::{
    AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement as _, Styled as _, Window,
    canvas, div, px,
};
use gpui_component::highlighter::HighlightTheme;
use gpui_component::input::{Editor, EditorState, TabSize};
use serde::Deserialize;
use std::sync::{Arc, LazyLock};

/// Monaco's `lineHeight: 0` rounds `fontSize` times 1.5 on macOS and 1.35 elsewhere.
pub(crate) const EDITOR_FONT_SIZE: f32 = 13.0;
pub(crate) const EDITOR_LINE_HEIGHT: f32 = if cfg!(target_os = "macos") {
    20.0
} else {
    18.0
};
/// `padding: { bottom: 16, top: 16 }`.
const EDITOR_PADDING_Y: f32 = 16.0;
/// Monaco's glyph margin: its line numbers end 36px from the editor's left edge.
const EDITOR_PADDING_LEFT: f32 = 14.0;

#[derive(Deserialize)]
struct EditorThemes {
    dark: HighlightTheme,
    light: HighlightTheme,
}

static EDITOR_THEMES: LazyLock<(Arc<HighlightTheme>, Arc<HighlightTheme>)> = LazyLock::new(|| {
    let themes: EditorThemes =
        serde_json::from_str(include_str!("editor-theme.json")).expect("the Hub editor theme");
    (Arc::new(themes.dark), Arc::new(themes.light))
});

fn editor_theme(light: bool) -> Arc<HighlightTheme> {
    if light {
        EDITOR_THEMES.1.clone()
    } else {
        EDITOR_THEMES.0.clone()
    }
}

/// A fresh editor on `content`, highlighted as `language` (a Kit language name).
pub(crate) fn new_editor_state(
    language: &str,
    content: &str,
    wrap: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<EditorState> {
    let language = language.to_string();
    let content = content.to_string();
    cx.new(|cx| {
        EditorState::new(window, cx)
            .language(language)
            .line_number(false)
            // Monaco folds too, and its folding column is what puts the code 63px from the edge.
            .folding(true)
            .soft_wrap(wrap)
            .searchable(true)
            .scroll_beyond_last_line(None)
            .tab_size(TabSize {
                tab_size: 4,
                hard_tabs: false,
            })
            .default_value(content)
    })
}

/// `InputEditorStyle` is not re-exported by gpui-component, so its default is reached through
/// the setter's own signature.
fn default_style_for<State, Style: Default>(_setter: fn(&mut State, Style)) -> Style {
    Style::default()
}

/// Paints the editor in the Hub's Monaco colours for the frame being drawn.
///
/// CDXC:AgentLauncher 2026-09-28 WHY:
/// The Kit's `Input` projects its editor colours from the one app-wide Kit theme on every render (native Docs and every other Kit input share it), so the Hub cannot give its editor Monaco's colours through the theme. The Hub instead replaces the projected style in a prepaint that runs after the `Input` rendered and before the editor lays out and paints its lines.
fn apply_hub_editor_style(state: &mut EditorState, hp: &HubPalette) {
    let colors = hp.editor;
    let mut style = default_style_for(EditorState::set_editor_style);
    style.foreground = hsla(colors.foreground);
    style.muted_foreground = hsla(colors.line_number);
    style.background = hsla(colors.background);
    style.border = hsla(hp.line);
    style.selection = hsla(colors.selection);
    style.caret = hsla(colors.caret);
    style.highlight_styles = editor_theme(hp.light);
    style.editor_active_line = None;
    style.editor_gutter_background = Some(hsla(colors.background));
    state.set_editor_style(style);
}

/// `.agents-hub-editor-body` holding the editor.
pub(crate) fn editor_body(hp: &HubPalette, editor: &Entity<EditorState>) -> AnyElement {
    let hp = *hp;
    let styled = editor.clone();
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .bg(hsla(hp.editor.background))
        .child(
            canvas(
                move |_bounds, _window, cx| {
                    styled.update(cx, |state, _cx| apply_hub_editor_style(state, &hp));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0(),
        )
        .child(
            Editor::new(editor)
                .appearance(false)
                .bordered(false)
                .size_full()
                .font_family(MODAL_MONO_FONT)
                .text_size(px(EDITOR_FONT_SIZE))
                .line_height(px(EDITOR_LINE_HEIGHT))
                .pt(px(EDITOR_PADDING_Y))
                .pb(px(EDITOR_PADDING_Y))
                .pl(px(EDITOR_PADDING_LEFT)),
        )
        .into_any_element()
}
