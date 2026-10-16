use super::*;

/// Which device a Browser tab claims to be when it sends a mobile user agent.
///
/// CDXC:Browser 2026-10-16 WHY:
/// The user agent goes through the DevTools Emulation domain rather than a request-handler header, so `navigator.userAgent` and `navigator.platform` agree with what the server saw and sites that detect phones in script switch layouts too. `mobile: true` metrics with a zero size keep the real view size (the GPUI frame sets it) while turning on viewport-meta handling and overlay scrollbars like a phone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserMobileUserAgent {
    IPhone,
    IPad,
    Android,
}

impl BrowserMobileUserAgent {
    fn user_agent(self) -> &'static str {
        match self {
            Self::IPhone => {
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1"
            }
            Self::IPad => {
                "Mozilla/5.0 (iPad; CPU OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1"
            }
            Self::Android => {
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36"
            }
        }
    }

    fn platform(self) -> &'static str {
        match self {
            Self::IPhone => "iPhone",
            Self::IPad => "iPad",
            Self::Android => "Linux armv81",
        }
    }
}

/// Sets or clears the tab's mobile user agent and phone-style viewport handling. Takes effect on the next request; callers reload a page that is already showing.
pub(crate) fn apply_browser_mobile_user_agent(
    browser: &cef::Browser,
    agent: Option<BrowserMobileUserAgent>,
) {
    let Some(host) = browser.host() else {
        return;
    };
    let Some(mut user_agent_params) = cef::dictionary_value_create() else {
        return;
    };
    // An empty user agent removes the override (Chromium's EmulationHandler).
    user_agent_params.set_string(
        Some(&CefString::from("userAgent")),
        Some(&CefString::from(
            agent.map_or("", BrowserMobileUserAgent::user_agent),
        )),
    );
    if let Some(agent) = agent {
        user_agent_params.set_string(
            Some(&CefString::from("platform")),
            Some(&CefString::from(agent.platform())),
        );
    }
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from("Emulation.setUserAgentOverride")),
        Some(&mut user_agent_params),
    );

    if agent.is_none() {
        host.execute_dev_tools_method(
            next_page_appearance_devtools_message_id(),
            Some(&CefString::from("Emulation.clearDeviceMetricsOverride")),
            None,
        );
        return;
    }
    let Some(mut metrics_params) = cef::dictionary_value_create() else {
        return;
    };
    // Zero width, height and scale keep the real view; only the mobile flag changes.
    metrics_params.set_int(Some(&CefString::from("width")), 0);
    metrics_params.set_int(Some(&CefString::from("height")), 0);
    metrics_params.set_double(Some(&CefString::from("deviceScaleFactor")), 0.0);
    metrics_params.set_bool(Some(&CefString::from("mobile")), 1);
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from("Emulation.setDeviceMetricsOverride")),
        Some(&mut metrics_params),
    );
}
