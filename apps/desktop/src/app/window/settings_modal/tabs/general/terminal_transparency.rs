//! Terminal's Background opacity and Background blur rows. Both act on the window's glass, so they
//! work only while Enable transparency is on; the rule and the user's decision are on
//! `TERMINAL_GLASS_PERCENT` in app/helpers/terminal_glass.rs.
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{
    ButtonVariant, SliderBinding, SliderSaver, reset_key, setting_row, settings_button,
    slider_number_field_with,
};
use super::super::super::store::SettingsStore;
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, App, Context, Entity, Window};
use serde_json::{Value, json};
use std::rc::Rc;

/// Why the window shows nothing through its glass, when the system's own switch is the reason.
fn system_note(blocked: bool) -> &'static str {
    match (blocked, cfg!(target_os = "windows")) {
        (false, _) => "",
        (true, true) => {
            " Transparency effects is off in Windows Settings > Personalization > Colors, so the window stays opaque until you turn it on there and restart Ghostex."
        }
        (true, false) => {
            " Reduce transparency is on in System Settings > Accessibility > Display, so the window stays opaque until you turn it off."
        }
    }
}

pub(super) fn transparency_rows(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<AnyElement> {
    let s = "terminal";
    let p = g.p;
    let glass = g.values.string("windowGlass");
    let glass_on = glass != "opaque";
    let blocked = page
        .store
        .read(cx)
        .hud()
        .and_then(|hud| hud.get("windowGlassBlockedBySystem"))
        .and_then(Value::as_bool)
        == Some(true);
    let note = system_note(blocked);
    let mut rows = Vec::new();
    if g.visible(s, "terminalBackgroundOpacity") {
        if glass_on {
            let description = format!(
                "How much of the work area's glass tint terminals keep. 100% matches the rest of the work area; lower shows more of what is behind the window. Applies while transparency is on (Settings > Theme > Transparency){}.{note}",
                if glass == "auto" {
                    ", which is dark mode only"
                } else {
                    ""
                },
            );
            rows.extend(page.slider(
                g,
                s,
                "terminalBackgroundOpacity",
                "Background opacity (%)",
                &description,
                (0.0, 100.0, 1.0),
                false,
                window,
                cx,
            ));
        } else {
            let restart = if cfg!(target_os = "windows") {
                " On Windows it takes effect the next time Ghostex starts."
            } else {
                ""
            };
            let spec = g
                .spec(
                    "terminalBackgroundOpacity",
                    "Background opacity (%)",
                    &format!(
                        "Terminals can only be see-through while the window is. Turn on transparency to show what is behind the window through them; it picks Dark only, which you can change under Settings > Theme > Transparency.{restart}"
                    ),
                )
                .readout("Needs transparency");
            let control = settings_button(
                &p,
                "terminal-turn-on-transparency",
                "Turn on transparency",
                None,
                ButtonVariant::Outline,
                false,
                None,
                // `glass_for_transparency`: turning transparency on from Never picks Dark only.
                |page: &mut GeneralTab, _window, cx| save(page, "windowGlass", json!("auto"), cx),
                cx,
            );
            rows.push(setting_row(
                &p,
                "terminalBackgroundOpacity",
                spec,
                Some(reset_key::<GeneralTab>("terminalBackgroundOpacity")),
                control,
                cx,
            ));
        }
    }
    let blur_hidden = ghostex_settings_catalog::availability::row_hidden(
        ghostex_settings_catalog::Platform::current(),
        "terminalBackgroundBlur",
        |key| Some(g.values.string(key)),
    );
    if glass_on && g.visible(s, "terminalBackgroundBlur") && !blur_hidden {
        let saver: SliderSaver = Rc::new(
            |store: &Entity<SettingsStore>, value: f64, commit: bool, cx: &mut App| {
                let value = json!(value.round() as i64);
                store.update(cx, |store, cx| {
                    if commit {
                        store.update_setting("windowGlassBlurRadius", value, cx);
                    } else {
                        store.update_setting_debounced("windowGlassBlurRadius", value, cx);
                    }
                });
            },
        );
        let description = if cfg!(target_os = "macos") {
            format!(
                "How soft what shows behind the window looks, in points; 0 shows it sharp. This is the window's Blur, the same setting as Settings > Theme > Transparency, because a window has one blur.{note}"
            )
        } else {
            format!(
                "How soft the wallpaper, picture or video behind the window looks, in points; 0 shows it sharp. Desktop and windows uses your system's own blur. This is the window's Blur, the same setting as Settings > Theme > Transparency, because a window has one blur.{note}"
            )
        };
        let number = |name: &str| settings_catalog().number(module::SETTINGS, name);
        rows.push(slider_number_field_with(
            page,
            &p,
            g.spec("windowGlassBlurRadius", "Background blur", &description),
            Some(reset_key::<GeneralTab>("windowGlassBlurRadius")),
            SliderBinding {
                key: "terminalBackgroundBlur",
                min: number("MIN_WINDOW_GLASS_BLUR_RADIUS"),
                max: number("MAX_WINDOW_GLASS_BLUR_RADIUS"),
                step: 1.0,
            },
            g.values.f64("windowGlassBlurRadius"),
            saver,
            window,
            cx,
        ));
    }
    // On Windows with Desktop and windows the window's Blur is the Windows blur choice.
    if glass_on
        && g.visible(s, "terminalBackgroundBlur")
        && super::super::theme::transparency::windows_blur_shown(&g.values)
    {
        rows.push(super::super::theme::transparency::windows_blur_field(
            &p,
            g.spec(
                "windowGlassWindowsBlur",
                "Background blur",
                &format!(
                    "{} This is the window's Blur, the same setting as Settings > Theme > Transparency, because a window has one blur.{note}",
                    super::super::theme::transparency::WINDOWS_BLUR_DESCRIPTION
                ),
            ),
            &g.values,
            Some(reset_key::<GeneralTab>("windowGlassWindowsBlur")),
            |page: &mut GeneralTab, next, _window, cx| {
                save(page, "windowGlassWindowsBlur", json!(next), cx)
            },
            cx,
        ));
    }
    rows
}
