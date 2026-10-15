#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

/*
 CDXC:Onboarding 2026-10-01 WHY:
 The first-run intro video plays in the system WKWebView, never CEF (the user decision is on apps/desktop/src/app/window/onboarding/intro_video.rs). The web view is a plain child of the onboarding window's GPUI view at the exact frame Rust lays out for it, created hidden and shown by the first frame write, so it never draws at a stale origin. wry was not used here: its macOS child web view activates the whole app on creation, and every other AppKit piece of this crate is a small Objective-C shim. YouTube refuses embeds that arrive without an identifying referrer (player error 153), so the player is an iframe in a page loaded with an https base URL, and WebKit sends that origin as the iframe's Referer (CDXC:Onboarding 2026-10-07 on intro_video.rs `embed_page_html`). Links the player opens (its logo, "Watch on YouTube") go to the default browser instead of replacing the video inside the small frame.
 SEE-ALSO: apps/desktop/src/app/window/onboarding/intro_web_view.rs (the Rust side and the Windows WebView2 twin).
*/
@interface GhostexIntroVideoWebView : WKWebView <WKNavigationDelegate, WKUIDelegate>
@end

static void GhostexIntroVideoOpenExternally(NSURL *url) {
  NSString *scheme = url.scheme.lowercaseString;
  if (url == nil || !([scheme isEqualToString:@"https"] || [scheme isEqualToString:@"http"])) {
    return;
  }
  [[NSWorkspace sharedWorkspace] openURL:url];
}

@implementation GhostexIntroVideoWebView

- (void)webView:(WKWebView *)webView
    decidePolicyForNavigationAction:(WKNavigationAction *)navigationAction
                    decisionHandler:(void (^)(WKNavigationActionPolicy))decisionHandler {
  // Only a link the user follows in the top-level page leaves the embed; the player's own frames
  // and redirects load as usual.
  if (navigationAction.navigationType == WKNavigationTypeLinkActivated &&
      navigationAction.targetFrame.isMainFrame) {
    GhostexIntroVideoOpenExternally(navigationAction.request.URL);
    decisionHandler(WKNavigationActionPolicyCancel);
    return;
  }
  decisionHandler(WKNavigationActionPolicyAllow);
}

- (WKWebView *)webView:(WKWebView *)webView
    createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration
               forNavigationAction:(WKNavigationAction *)navigationAction
                    windowFeatures:(WKWindowFeatures *)windowFeatures {
  GhostexIntroVideoOpenExternally(navigationAction.request.URL);
  return nil;
}

@end

void *GhostexGpuiIntroVideoWebViewCreate(void *parentView, const char *html,
                                         const char *baseURL) {
  NSView *parent = (__bridge NSView *)parentView;
  if (parent == nil || html == NULL || baseURL == NULL) {
    return NULL;
  }
  NSString *page = [NSString stringWithUTF8String:html];
  NSURL *pageBaseURL = [NSURL URLWithString:[NSString stringWithUTF8String:baseURL]];
  if (page == nil || pageBaseURL == nil) {
    return NULL;
  }
  WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
  // The web view is only made when the user presses Play on the page's still, so the player
  // starts right away; that press happened in GPUI, which WebKit does not count as a gesture.
  configuration.mediaTypesRequiringUserActionForPlayback = WKAudiovisualMediaTypeNone;
  GhostexIntroVideoWebView *webView =
      [[GhostexIntroVideoWebView alloc] initWithFrame:NSZeroRect configuration:configuration];
  webView.navigationDelegate = webView;
  webView.UIDelegate = webView;
  webView.autoresizingMask = NSViewNotSizable;
  webView.allowsBackForwardNavigationGestures = NO;
  webView.allowsMagnification = NO;
  // No white page before the player paints its own black; the frame Rust draws shows through.
  [webView setValue:@NO forKey:@"drawsBackground"];
  webView.hidden = YES;

  [webView loadHTMLString:page baseURL:pageBaseURL];
  [parent addSubview:webView positioned:NSWindowAbove relativeTo:nil];
  return (__bridge_retained void *)webView;
}

void GhostexGpuiIntroVideoWebViewSetFrame(void *handle, double x, double y, double width,
                                          double height) {
  GhostexIntroVideoWebView *webView = (__bridge GhostexIntroVideoWebView *)handle;
  if (webView == nil) {
    return;
  }
  NSView *parent = webView.superview;
  CGFloat nativeY = y;
  if (parent != nil && !parent.isFlipped) {
    nativeY = NSHeight(parent.bounds) - y - height;
  }
  webView.frame = NSMakeRect(x, nativeY, MAX(0.0, width), MAX(0.0, height));
  webView.hidden = NO;
}

void GhostexGpuiIntroVideoWebViewDestroy(void *handle) {
  GhostexIntroVideoWebView *webView = (__bridge_transfer GhostexIntroVideoWebView *)handle;
  if (webView == nil) {
    return;
  }
  // A click into the player made it first responder; give the keyboard back to the GPUI view.
  NSWindow *window = webView.window;
  NSResponder *firstResponder = window.firstResponder;
  if ([firstResponder isKindOfClass:[NSView class]] &&
      [(NSView *)firstResponder isDescendantOf:webView]) {
    [window makeFirstResponder:webView.superview];
  }
  [webView pauseAllMediaPlaybackWithCompletionHandler:nil];
  [webView stopLoading];
  webView.navigationDelegate = nil;
  webView.UIDelegate = nil;
  [webView removeFromSuperview];
}
