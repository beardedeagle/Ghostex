//! `SoundField`: a sound select with a Play button.
use super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::palette::SettingsPalette;
use super::controls::{ButtonVariant, settings_icon_button};
use super::row::{CONTROL_LANE_WIDTH, PageAction, RowSpec, setting_row};
use super::select::settings_select;
use super::{SettingsPage, icon};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    px,
};
use gpui_component::h_flex;
use serde_json::json;

/// `COMPLETION_SOUND_OPTIONS`, with `Off` first when the field allows it.
pub(crate) fn sound_options(allow_off: bool) -> Vec<SettingOption> {
    let mut options = Vec::new();
    if allow_off {
        options.push(SettingOption {
            label: "Off".to_string(),
            value: "off".to_string(),
        });
    }
    options
        .extend(settings_catalog().options(module::COMPLETION_SOUND, "COMPLETION_SOUND_OPTIONS"));
    options
}

/// `SoundField`: the select and a Play button (`playCompletionSoundPreview`), which is disabled
/// with a reason while the value is Off.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sound_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    key: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    allow_off: bool,
    value: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let options = sound_options(allow_off);
    let label = spec.label.clone();
    let select = settings_select(
        page,
        p,
        key,
        &options,
        value,
        None,
        false,
        None,
        move |page: &mut V, next, _window, cx| {
            if !allow_off && next == "off" {
                return;
            }
            let store = page.settings_store().clone();
            store.update(cx, |store, cx| store.update_setting(key, json!(next), cx));
        },
        window,
        cx,
    );
    let off = value == "off";
    let sound = value.to_string();
    let play = settings_icon_button(
        p,
        SharedString::from(format!("{key}-play")),
        icon::PLAYER_PLAY,
        16.0,
        32.0,
        ButtonVariant::Outline,
        Some(if off {
            "Choose a sound to preview it.".into()
        } else {
            "Play selected sound".into()
        }),
        off,
        move |page: &mut V, _window, cx| {
            let store = page.settings_store().clone();
            super::super::store::post_store_message(
                &store,
                json!({ "sound": sound, "type": "playCompletionSoundPreview" }),
                cx,
            );
        },
        cx,
    );
    let _ = label;
    let control = h_flex()
        .w(px(CONTROL_LANE_WIDTH))
        .max_w_full()
        .items_center()
        .gap(px(8.0))
        .child(div().flex_1().min_w_0().child(select))
        .child(play)
        .into_any_element();
    setting_row(p, key, spec, on_reset, control, cx)
}
