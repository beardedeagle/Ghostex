//! The Browser tab's phone and tablet preview: which page size the tab shows and whether it sends a
//! mobile user agent.

use crate::cef::BrowserMobileUserAgent;

/// CDXC:Browser 2026-10-16 DECISION:
/// User (GitHub #246): per tab, a Browser toolbar control shows the page at common phone sizes (iPhone 15/16, a small Android, iPad) or at full width, with an option to send a mobile user agent. Desktop only for now.
/// Sizes are CSS pixels in portrait; the page view gets exactly this frame, so media queries and `innerWidth` see the device width. Each tab keeps its own choice, saved with the tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BrowserViewportSize {
    #[default]
    FullWidth,
    IPhone,
    IPhoneProMax,
    IPhoneSe,
    Android,
    SmallAndroid,
    IPadMini,
    IPad,
}

impl BrowserViewportSize {
    pub(crate) const ALL: [Self; 8] = [
        Self::FullWidth,
        Self::IPhone,
        Self::IPhoneProMax,
        Self::IPhoneSe,
        Self::Android,
        Self::SmallAndroid,
        Self::IPadMini,
        Self::IPad,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FullWidth => "Full width",
            Self::IPhone => "iPhone 15 / 16",
            Self::IPhoneProMax => "iPhone 15 / 16 Pro Max",
            Self::IPhoneSe => "iPhone SE",
            Self::Android => "Android",
            Self::SmallAndroid => "Small Android",
            Self::IPadMini => "iPad mini",
            Self::IPad => "iPad",
        }
    }

    /// Portrait width and height in CSS pixels; `None` fills the pane.
    pub(crate) fn portrait_size(self) -> Option<(f32, f32)> {
        match self {
            Self::FullWidth => None,
            Self::IPhone => Some((393.0, 852.0)),
            Self::IPhoneProMax => Some((430.0, 932.0)),
            Self::IPhoneSe => Some((375.0, 667.0)),
            Self::Android => Some((412.0, 915.0)),
            Self::SmallAndroid => Some((360.0, 800.0)),
            Self::IPadMini => Some((744.0, 1133.0)),
            Self::IPad => Some((820.0, 1180.0)),
        }
    }

    fn mobile_user_agent(self) -> BrowserMobileUserAgent {
        match self {
            Self::IPhone | Self::IPhoneProMax | Self::IPhoneSe => BrowserMobileUserAgent::IPhone,
            Self::IPadMini | Self::IPad => BrowserMobileUserAgent::IPad,
            Self::FullWidth | Self::Android | Self::SmallAndroid => BrowserMobileUserAgent::Android,
        }
    }

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::FullWidth => "full",
            Self::IPhone => "iphone",
            Self::IPhoneProMax => "iphone-pro-max",
            Self::IPhoneSe => "iphone-se",
            Self::Android => "android",
            Self::SmallAndroid => "android-small",
            Self::IPadMini => "ipad-mini",
            Self::IPad => "ipad",
        }
    }

    pub(crate) fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.slug() == slug)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BrowserDeviceView {
    pub(crate) size: BrowserViewportSize,
    pub(crate) landscape: bool,
    pub(crate) mobile_user_agent: bool,
}

impl BrowserDeviceView {
    pub(crate) fn is_default(self) -> bool {
        self == Self::default()
    }

    /// The page frame in CSS pixels, turned for landscape; `None` fills the pane.
    pub(crate) fn frame_size(self) -> Option<(f32, f32)> {
        let (width, height) = self.size.portrait_size()?;
        Some(if self.landscape {
            (height, width)
        } else {
            (width, height)
        })
    }

    /// The user agent the tab sends: the selected device's own browser, or Chrome on Android at full width.
    pub(crate) fn user_agent(self) -> Option<BrowserMobileUserAgent> {
        self.mobile_user_agent
            .then(|| self.size.mobile_user_agent())
    }

    pub(crate) fn caption(self) -> Option<String> {
        let (width, height) = self.frame_size()?;
        Some(format!(
            "{} · {} × {}",
            self.size.label(),
            width as u32,
            height as u32
        ))
    }
}
