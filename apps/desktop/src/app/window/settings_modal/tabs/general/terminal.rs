//! The Terminal group of General: Terminal (with the terminal config buttons and
//! the Windows environment rows), Terminal Behavior and Terminal Scrolling.
use super::super::super::super::native_modal_kit::hsla;
use super::super::super::catalog::{SettingOption, module, settings_catalog};
#[cfg(target_os = "macos")]
use super::super::super::fields::toggle_field_with;
use super::super::super::fields::{
    ButtonVariant, card_inset, color_field, reset_key, select_field, settings_button,
    settings_section, text_field, tooltip_text,
};
use super::super::super::page::PageBlock;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::SettingsStore;
use super::terminal_font::normalize_ghostty_font_family;
use super::{GeneralCx, GeneralTab, save};
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The colour Terminal background starts from when Follow theme is turned off.
const TERMINAL_BACKGROUND_STARTING_COLOR: &str = "#111111";
const SHADER_BACKGROUND_OVERRIDE_REASON: &str = "Custom shaders use the background from Ghostex's terminal config. To edit this override, turn Custom shaders off (enable Experimental Features first if the switch is hidden); your saved choice will return.";

fn option(label: &str, value: &str) -> SettingOption {
    SettingOption {
        label: label.to_string(),
        value: value.to_string(),
    }
}

/// `normalizeWindowsWslDistribution`: no NULs or line breaks, trimmed, at most 128 characters.
fn normalize_wsl_distribution(text: &str) -> String {
    text.replace(['\0', '\r', '\n'], "")
        .trim()
        .chars()
        .take(128)
        .collect()
}

/// `applyRecommendedGhosttySettings` / `resetGhosttySettingsToDefault`: the visible terminal
/// controls change with the managed Ghostty config keys the app rewrites.
fn ghostty_preset(page: &mut GeneralTab, recommended: bool, cx: &mut Context<GeneralTab>) {
    let store = page.store.clone();
    store.update(cx, |store: &mut SettingsStore, cx| {
        let mut settings = store.settings().clone();
        let defaults = settings_catalog().defaults();
        let default = |key: &str| defaults.get(key).cloned().unwrap_or(Value::Null);
        let changes: Vec<(&str, Value)> = if recommended {
            vec![
                ("terminalCursorStyle", json!("bar")),
                ("terminalFontFamily", json!("JetBrains Mono")),
                ("terminalFontSize", json!(13)),
                ("terminalFontWeight", json!(400)),
                ("terminalLetterSpacing", json!(0)),
                ("terminalLineHeight", json!(1.2)),
                ("terminalMouseScrollMultiplierDiscrete", json!(1)),
                ("terminalMouseScrollMultiplierPrecision", json!(1)),
            ]
        } else {
            [
                "terminalCursorStyle",
                "terminalFontFamily",
                "terminalFontSize",
                "terminalFontWeight",
                "terminalLetterSpacing",
                "terminalLineHeight",
                "terminalMouseScrollMultiplierDiscrete",
                "terminalMouseScrollMultiplierPrecision",
                "terminalScrollToBottomWhenTyping",
                "terminalBoldUsesHeavierFont",
            ]
            .into_iter()
            .map(|key| (key, default(key)))
            .collect()
        };
        for (key, value) in changes {
            settings.insert(key.to_string(), value);
        }
        store.apply_settings(settings, "settings:bulk", cx);
        let action = if recommended {
            "applyRecommendedGhosttySettings"
        } else {
            "resetGhosttySettingsToDefault"
        };
        store.post_message(json!({ "type": action }), cx);
    });
}

fn post(page: &mut GeneralTab, message: Value, cx: &mut Context<GeneralTab>) {
    let store = page.store.clone();
    super::super::super::store::post_store_message(&store, message, cx);
}

/// One cell of the Terminal config grid: every column takes an equal share of the row, and the
/// button inside stretches to fill it.
fn grid_cell(child: AnyElement) -> AnyElement {
    v_flex()
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .child(child)
        .into_any_element()
}

/// CDXC:Terminal 2026-10-14 DECISION:
/// User: "organise them better and give them equal widths", grouped by purpose, "each group laid out as an equal-width grid that fills the row, with short labels that fit at the Settings window's minimum width", all buttons filled, the docs link a quieter action. Row one is the config files (Ghostex's, the Ghostty app's, Import), row two the presets, and the docs link sits in the last cell so every button keeps the same width.
fn terminal_config_actions(p: &SettingsPalette, cx: &mut Context<GeneralTab>) -> AnyElement {
    let recommended_lines = settings_catalog()
        .string_list(
            module::GHOSTTY_CONFIG_ACTIONS,
            "GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES",
            None,
        )
        .join("\n");
    let button = |id: &'static str,
                  label: &'static str,
                  tooltip: String,
                  message: &'static str,
                  cx: &mut Context<GeneralTab>| {
        grid_cell(
            v_flex()
                .id(SharedString::from(format!("{id}-tooltip")))
                .tooltip(tooltip_text(tooltip))
                .child(settings_button(
                    p,
                    id,
                    label,
                    None,
                    ButtonVariant::Outline,
                    false,
                    None,
                    move |page: &mut GeneralTab, _window, cx| match message {
                        "applyRecommendedGhosttySettings" => ghostty_preset(page, true, cx),
                        "resetGhosttySettingsToDefault" => ghostty_preset(page, false, cx),
                        message => post(page, json!({ "type": message }), cx),
                    },
                    cx,
                ))
                .into_any_element(),
        )
    };
    let docs_link = grid_cell(
        h_flex()
            .h_full()
            .justify_center()
            .child(
                div()
                    .id("ghostty-open-docs")
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .text_color(hsla(p.muted))
                    .hover(|this| this.underline())
                    .on_click(cx.listener(|page, _: &ClickEvent, _window, cx| {
                        post(page, json!({ "type": "openGhosttySettingsDocs" }), cx)
                    }))
                    .child("Ghostty docs \u{2197}"),
            )
            .into_any_element(),
    );
    v_flex()
        .w_full()
        .gap(px(8.0))
        .child(
            h_flex()
                .w_full()
                .gap(px(8.0))
                .child(button(
                    "ghostex-open-terminal-config",
                    "Open Ghostex config",
                    "Ghostex's terminals read only this file.".to_string(),
                    "openGhostexTerminalConfigFile",
                    cx,
                ))
                .child(button(
                    "ghostty-open-config",
                    "Open Ghostty config",
                    "The Ghostty app's own config. Ghostex never changes it.".to_string(),
                    "openGhosttyConfigFile",
                    cx,
                ))
                .child(button(
                    "ghostty-import-config",
                    "Import from Ghostty",
                    "Copy your Ghostty fonts, line height and cursor into Ghostex. The theme and colors stay as they are.".to_string(),
                    "importGhosttyConfig",
                    cx,
                )),
        )
        .child(
            h_flex()
                .w_full()
                .gap(px(8.0))
                .child(button(
                    "ghostty-apply-recommended",
                    "Apply recommended",
                    recommended_lines,
                    "applyRecommendedGhosttySettings",
                    cx,
                ))
                .child(button(
                    "ghostty-reset-defaults",
                    "Reset to defaults",
                    "Put the terminal settings back to Ghostex's defaults.".to_string(),
                    "resetGhosttySettingsToDefault",
                    cx,
                ))
                .child(docs_link),
        )
        .into_any_element()
}

fn terminal_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("terminal", g.power_visible) {
        return None;
    }
    let s = "terminal";
    let p = g.p;
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible(s, "ghosttySettingsActions") {
        rows.push(card_inset(terminal_config_actions(&p, cx)));
    }
    if settings_catalog().flag(module::SEARCH_CATALOG, "IS_WINDOWS_HOST") {
        // `WindowsTerminalFields`.
        if g.visible(s, "windowsTerminalBackend") {
            let spec = g.spec(
                "windowsTerminalBackend",
                "Windows Environment",
                "PowerShell is the default and runs native Windows projects and agents without WSL. WSL uses your Linux projects. Changing environments prompts you to restart Ghostex; existing sessions stay in their original environment.",
            );
            let options = vec![
                option("PowerShell (Native)", "powershell"),
                option("WSL (Linux)", "wsl"),
            ];
            let value = g.values.choice(
                "windowsTerminalBackend",
                &["powershell".to_string(), "wsl".to_string()],
            );
            rows.push(select_field(
                page,
                &p,
                "windowsTerminalBackend",
                spec,
                Some(reset_key::<GeneralTab>("windowsTerminalBackend")),
                &options,
                &value,
                None,
                |page: &mut GeneralTab, next, _window, cx| {
                    let next = if next == "powershell" {
                        "powershell"
                    } else {
                        "wsl"
                    };
                    save(page, "windowsTerminalBackend", json!(next), cx)
                },
                window,
                cx,
            ));
        }
        if g.values.string("windowsTerminalBackend") != "wsl" {
            rows.extend(page.powershell_row(g, cx));
        }
        if g.values.string("windowsTerminalBackend") == "wsl"
            && g.visible(s, "windowsWslDistribution")
        {
            let spec = g.spec(
                "windowsWslDistribution",
                "WSL Distribution",
                "Leave blank to use an initialized WSL2 distribution, or enter its exact name from wsl.exe --list --verbose.",
            );
            let value = g.values.string("windowsWslDistribution");
            rows.push(text_field(
                page,
                &p,
                "windowsWslDistribution",
                spec,
                Some(reset_key::<GeneralTab>("windowsWslDistribution")),
                &value,
                Some("Automatic"),
                Some(normalize_wsl_distribution),
                None,
                window,
                cx,
            ));
        }
    }
    if g.visible(s, "terminalBackgroundMode") {
        let spec = g.spec(
            "terminalBackgroundMode",
            "Terminal background",
            "Only changes the terminal panes. Black / white is pure black in dark mode and pure white in light mode.",
        ).disabled_reason((cfg!(target_os = "macos") && g.values.bool("terminalShadersEnabled")).then(|| SHADER_BACKGROUND_OVERRIDE_REASON.into()));
        let options = vec![
            option("Black / white", "pure"),
            option("Follow theme", "theme"),
            option("Custom color", "custom"),
        ];
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice("terminalBackgroundMode", &allowed);
        rows.push(select_field(
            page,
            &p,
            "terminalBackgroundMode",
            spec,
            Some(reset_key::<GeneralTab>("terminalBackgroundMode")),
            &options,
            &value,
            None,
            |page: &mut GeneralTab, next, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    let mut patch = serde_json::Map::new();
                    if next == "custom" && store.string("workspaceBackgroundColor").is_empty() {
                        patch.insert(
                            "workspaceBackgroundColor".into(),
                            json!(TERMINAL_BACKGROUND_STARTING_COLOR),
                        );
                    }
                    patch.insert("terminalBackgroundMode".into(), json!(next));
                    store.apply_patch(patch, "settings:control", cx);
                });
            },
            window,
            cx,
        ));
    }
    if g.values.string("terminalBackgroundMode") == "custom"
        && g.visible(s, "workspaceBackgroundColor")
    {
        let spec = g
            .spec(
                "workspaceBackgroundColor",
                "Terminal background color",
                "Painted behind terminal text in dark mode. Light mode and window glass keep the theme.",
            )
            .dependent()
            .disabled_reason((cfg!(target_os = "macos") && g.values.bool("terminalShadersEnabled")).then(|| SHADER_BACKGROUND_OVERRIDE_REASON.into()));
        let value = g.values.string("workspaceBackgroundColor");
        rows.push(color_field(
            page,
            &p,
            "workspaceBackgroundColor",
            spec,
            Some(reset_key::<GeneralTab>("workspaceBackgroundColor")),
            &value,
            window,
            cx,
        ));
    }
    rows.extend(super::terminal_transparency::transparency_rows(
        page, g, window, cx,
    ));
    if g.visible(s, "terminalBackgroundImage") {
        let spec = g.spec(
            "terminalBackgroundImage",
            "Background Image",
            "Absolute path to an image drawn behind terminal panes. Leave blank for none.",
        );
        let value = g.values.string("terminalBackgroundImage");
        rows.push(text_field(
            page,
            &p,
            "terminalBackgroundImage",
            spec,
            Some(reset_key::<GeneralTab>("terminalBackgroundImage")),
            &value,
            Some("/Users/you/Pictures/background.png"),
            None,
            Some((
                "Choose image file".into(),
                std::rc::Rc::new(
                    |page: &mut GeneralTab, _window: &mut Window, cx: &mut Context<GeneralTab>| {
                        post(
                            page,
                            json!({ "type": "pickTerminalBackgroundImageFile" }),
                            cx,
                        )
                    },
                ),
            )),
            window,
            cx,
        ));
    }
    rows.extend(page.slider(
        g,
        s,
        "terminalBackgroundImageOpacity",
        "Background Image Opacity",
        "Blend the background image toward the terminal background color.",
        (0.0, 1.0, 0.05),
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "terminalBackgroundImageFit",
        "Background Image Fit",
        "How the background image is scaled inside each pane.",
        vec![
            option("Cover", "cover"),
            option("Contain", "contain"),
            option("Stretch", "stretch"),
            option("Natural size", "natural"),
        ],
        None,
        false,
        window,
        cx,
    ));
    if g.visible(s, "terminalFontFamily") {
        let spec = g.spec(
            "terminalFontFamily",
            "Font Family",
            "Type a Ghostty font-family name. Leave blank to use Ghostex's terminal config or the platform default.",
        );
        // The draft React shows is `normalizeghostexSettings(settings)`, so a legacy preset name
        // reads as the family it saves as.
        let value = normalize_ghostty_font_family(&g.values.string("terminalFontFamily"));
        rows.push(text_field(
            page,
            &p,
            "terminalFontFamily",
            spec,
            Some(reset_key::<GeneralTab>("terminalFontFamily")),
            &value,
            Some("Ghostty default"),
            Some(normalize_ghostty_font_family),
            None,
            window,
            cx,
        ));
    }
    rows.extend(page.slider(
        g,
        s,
        "terminalFontSize",
        "Font Size",
        "Set terminal text size.",
        (8.0, 32.0, 0.5),
        false,
        window,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "terminalFontWeight",
        "Font Weight",
        "Set terminal text weight.",
        (100.0, 900.0, 50.0),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalBoldUsesHeavierFont",
        "Bold text uses a heavier font",
        "Draw bold terminal text in a heavier weight. Turn off to keep bold text at your normal font weight, so it stands out by color only. A font-style-bold or font-variation-bold line in Ghostex's terminal config takes priority.",
        false,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "terminalLineHeight",
        "Line Height",
        "Adjust terminal row height.",
        (0.8, 2.0, 0.1),
        false,
        window,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "terminalLetterSpacing",
        "Letter Spacing",
        "Adjust spacing between glyphs.",
        (-2.0, 8.0, 0.1),
        false,
        window,
        cx,
    ));
    if g.visible(s, "terminalViewWidthMode") {
        let spec = g.spec(
            "terminalViewWidthMode",
            "Terminal Width",
            "Use the full pane, match the chat transcript, or set an independent terminal width. Narrow panes stay full-width.",
        );
        rows.extend(page.segmented(
            g,
            s,
            "terminalViewWidthMode",
            spec,
            Some(reset_key::<GeneralTab>("terminalViewWidthMode")),
            g.options("TERMINAL_VIEW_WIDTH_MODE_OPTIONS"),
            cx,
        ));
    }
    if g.values.string("terminalViewWidthMode") == "custom" {
        rows.extend(page.slider(
            g,
            s,
            "terminalViewWidthPercent",
            "Terminal Width (%)",
            "Set the centered terminal body width. Panes 1070px wide or narrower remain full-width.",
            (
                g.number("MIN_TERMINAL_VIEW_WIDTH_PERCENT"),
                g.number("MAX_TERMINAL_VIEW_WIDTH_PERCENT"),
                g.number("TERMINAL_VIEW_WIDTH_PERCENT_STEP"),
            ),
            true,
            window,
            cx,
        ));
    }
    if g.values.string("terminalViewWidthMode") != "full" {
        rows.extend(page.toggle(g, s, "terminalWidthApplyToCommandPaneTerminals", "Apply Width to Command Pane Terminals", "Use the same centered width for terminals in the command pane. Padding remains shared across terminal types.", true, cx));
    }
    let padding = (
        g.number("MIN_TERMINAL_PANE_PADDING_PX"),
        g.number("MAX_TERMINAL_PANE_PADDING_PX"),
        1.0,
    );
    rows.extend(page.slider(
        g,
        s,
        "terminalPaneHorizontalPaddingPx",
        "Horizontal Padding",
        "Add left and right inner padding inside the terminal content area.",
        padding,
        false,
        window,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "terminalPaneVerticalPaddingPx",
        "Vertical Padding",
        "Add top and bottom inner padding inside the terminal content area.",
        padding,
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "terminalCursorStyle",
        "Cursor Style",
        "Choose the cursor shape.",
        vec![
            option("Line", "bar"),
            option("Block", "block"),
            option("Underline", "underline"),
        ],
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalCursorStyleBlink",
        "Cursor blink",
        "Blink the terminal cursor.",
        false,
        cx,
    ));
    rows.extend(page.toggle(g, s, "clickToWakeSleepingSessions", "Click to wake sleeping panes", "Selecting a sleeping pane tab shows a black placeholder; click the pane body to wake the session.", false, cx));
    rows.extend(page.toggle(g, s, "showQuickModelPickerInTerminal", "Model picker in terminal view", "Show a model button in the terminal bar and open the model picker with its shortcut. Turn off to let the terminal handle that shortcut.", false, cx));
    rows.extend(page.toggle(
        g,
        s,
        "showSessionIdInTerminalPanes",
        "Show session id in the top right of each terminal pane",
        "Show the provider session id in the top-right corner of each terminal pane.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "showNotificationOnTerminalBell",
        "Show notification on terminal bell",
        "Treat terminal bell events as session attention.",
        false,
        cx,
    ));
    if g.visible(s, "promptEditorBackend") {
        let ctrl_g = settings_catalog().hotkey_label("ctrl+g");
        rows.extend(page.select(
            g,
            s,
            "promptEditorBackend",
            &format!("{ctrl_g} prompt editor"),
            &format!("Choose which editor new terminals use when {ctrl_g} asks the shell to edit prompt text."),
            g.options("PROMPT_EDITOR_BACKEND_OPTIONS"),
            None,
            false,
            window,
            cx,
        ));
    }
    #[cfg(target_os = "macos")]
    if g.values.bool("showBetaFeatures") && g.visible(s, "terminalShadersEnabled") {
        rows.push(toggle_field_with(
            &p,
            "terminalShadersEnabled",
            g.spec(
                "terminalShadersEnabled",
                "Custom shaders (experimental)",
                "Apply the shaders from Ghostex's terminal config in order. macOS with Metal only; tested on Apple Silicon. Intel Mac rendering is not yet validated. Windows and Linux are unsupported. Turn off to restore ordinary rendering in the same sessions.",
            ).experimental(),
            g.values.bool("terminalShadersEnabled"),
            Some(reset_key::<GeneralTab>("terminalShadersEnabled")),
            |page: &mut GeneralTab, enabled, _window, cx| {
                save(page, "terminalShadersEnabled", json!(enabled), cx);
            },
            cx,
        ));
    }
    settings_section(&p, "Terminal", None, None, rows)
        .map(|section| PageBlock::section("terminal", section))
}

fn behavior_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g
        .search
        .subsection_visible("terminalBehavior", g.power_visible)
    {
        return None;
    }
    let s = "terminalBehavior";
    let catalog = settings_catalog();
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.slider(g, s, "terminalScrollbackLimitMb", "Scrollback limit", "Scrollback memory per terminal surface. Ghostty default is 10 MB and changes affect new terminals.", (1.0, 200.0, 1.0), false, window, cx));
    rows.extend(page.select(
        g,
        s,
        "terminalCopyOnSelect",
        "Copy on select",
        &catalog.text(module::SEARCH_CATALOG, "COPY_ON_SELECT_DESCRIPTION"),
        catalog.options(module::SEARCH_CATALOG, "COPY_ON_SELECT_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "terminalConfirmCloseSurface",
        "Confirm close",
        "Confirm before closing terminal surfaces.",
        g.options("GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalClipboardTrimTrailingSpaces",
        "Trim trailing spaces on copy",
        "Trim trailing whitespace when copying terminal text.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalClipboardPasteProtection",
        "Paste protection",
        "Ask before pasting text Ghostty considers unsafe.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalPastePreviewableImages",
        "Paste previewable images",
        &catalog.text(
            module::SEARCH_CATALOG,
            "PASTE_PREVIEWABLE_IMAGES_DESCRIPTION",
        ),
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalMouseHideWhileTyping",
        "Hide mouse while typing",
        "Hide the pointer while typing in the terminal.",
        false,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "terminalScrollbar",
        "Scrollbar",
        "Control whether Ghostty shows its native scrollback scrollbar.",
        g.options("GHOSTTY_SCROLLBAR_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    settings_section(&g.p, "Terminal Behavior", None, None, rows)
        .map(|section| PageBlock::section("terminalBehavior", section))
}

fn scrolling_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g
        .search
        .subsection_visible("terminalScrolling", g.power_visible)
    {
        return None;
    }
    let s = "terminalScrolling";
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.slider(
        g,
        s,
        "terminalMouseScrollMultiplierPrecision",
        "Precision scroll multiplier",
        "Trackpads and high-resolution scroll wheels. Ghostty default is 1.",
        (0.25, 8.0, 0.25),
        false,
        window,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "terminalMouseScrollMultiplierDiscrete",
        "Discrete scroll multiplier",
        "Traditional notched mouse wheels. Ghostty default is 3.",
        (0.25, 8.0, 0.25),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "terminalScrollToBottomWhenTyping",
        "Scroll to bottom when typing",
        "Keep the prompt visible while typing.",
        false,
        cx,
    ));
    settings_section(&g.p, "Terminal Scrolling", None, None, rows)
        .map(|section| PageBlock::section("terminalScrolling", section))
}

pub(super) fn sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        terminal_section(page, g, window, cx),
        behavior_section(page, g, window, cx),
        scrolling_section(page, g, window, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}
