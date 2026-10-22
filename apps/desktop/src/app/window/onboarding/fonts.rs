//! The onboarding's three families: Manrope for the headings, DM Sans for the copy and IBM Plex
//! Mono for the eyebrows, labels and the scan console (packages/core-ui/onboarding/fonts.ts (deleted 2026-10-01)).
use gpui::App;

pub(crate) const MANROPE: &str = "Manrope";
pub(crate) const DM_SANS: &str = "DM Sans";
pub(crate) const PLEX_MONO: &str = "IBM Plex Mono";
/// The 500 face ships as its own family (no typographic family name), so it is asked for by that name.
pub(crate) const PLEX_MONO_MEDIUM: &str = "IBM Plex Mono Medium";

/// The face the text system should draw a CSS `font-family` + `font-weight` with.
///
/// CDXC:Onboarding 2026-09-28 WHY:
/// The React page registered DM Sans 400/500 and Plex Mono 400/500 only, so Chromium drew every 600
/// and 700 as a synthetic bold of the 500 face, which is as heavy as the real Bold. DM Sans 600+ is
/// therefore drawn with the real 700 face; Plex Mono 500 uses the Medium family, and Plex Mono 600+
/// asks that family for 700 so the platform text system emboldens it the same way.
pub(crate) fn face(family: &'static str, weight: f32) -> (&'static str, f32) {
    match family {
        PLEX_MONO if weight >= 600.0 => (PLEX_MONO_MEDIUM, 700.0),
        PLEX_MONO if weight >= 500.0 => (PLEX_MONO_MEDIUM, 500.0),
        DM_SANS if weight >= 600.0 => (dm_sans(), 700.0),
        DM_SANS => (dm_sans(), weight),
        _ => (family, weight),
    }
}

static DM_SANS_FAMILY: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// The name the platform text system knows DM Sans by.
///
/// CDXC:Onboarding 2026-09-28 WHY:
/// DirectWrite names the static DM Sans faces after their STAT optical-size value ("DM Sans 14pt"),
/// so on Windows a request for "DM Sans" silently fell back to the system font. The registered
/// family list says which name resolves.
pub(crate) fn dm_sans() -> &'static str {
    DM_SANS_FAMILY.get().copied().unwrap_or(DM_SANS)
}

/// CDXC:Onboarding 2026-09-28 WHY:
/// The page only ever asks for Manrope 700 and Plex Mono 400/500, so only those faces ship; DM Sans
/// comes from the static family the native chat already bundles (.dependencies/app-fonts), which
/// adds the 600 and 700 faces the browser used to fake from 500.
pub(crate) fn register(cx: &App) {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let mut fonts = vec![
            include_bytes!(
                "../../../../../../packages/core-ui/onboarding/assets/fonts/Manrope-700.ttf"
            )
            .as_slice()
            .into(),
            include_bytes!(
                "../../../../../../packages/core-ui/onboarding/assets/fonts/IBMPlexMono-400.ttf"
            )
            .as_slice()
            .into(),
            include_bytes!(
                "../../../../../../packages/core-ui/onboarding/assets/fonts/IBMPlexMono-500.ttf"
            )
            .as_slice()
            .into(),
        ];
        fonts.extend(
            crate::assets::DM_SANS_UPRIGHT_FACES
                .iter()
                .map(|face| std::borrow::Cow::Borrowed(*face)),
        );
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            eprintln!("[ghostex-gpui] Could not register the onboarding fonts: {error}");
        }
        let names = cx.text_system().all_font_names();
        let family = if names.iter().any(|name| name == "DM Sans 14pt") {
            "DM Sans 14pt"
        } else {
            DM_SANS
        };
        let _ = DM_SANS_FAMILY.set(family);
    });
}
