// The floating permission bar beside System Settings: where the System Settings window is, showing
// or hiding the bar with it, and dragging the tool's file out of the bar into a Privacy list.
//
// CDXC:OsIntegration 2026-10-14 WHY:
// GPUI can neither read another app's window nor start a drag that leaves the app, so these two
// AppKit calls live here; the bar itself is a GPUI window (apps/desktop/src/app/window/permission_guide.rs)
// moved with Ghostex Capture's GhostexGpuiCaptureSetWindowFrame. Window bounds come from
// CGWindowListCopyWindowInfo, which needs no Screen Recording permission for bounds and owner.

#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>

/// The largest on-screen window of System Settings in GPUI's global space (points from the
/// top-left corner of the primary display, y down). False while it is not running, hidden,
/// minimized or on another Space.
bool GhostexGpuiSystemSettingsWindowFrame(double *x, double *y, double *width,
                                          double *height) {
  NSArray<NSRunningApplication *> *apps = [NSRunningApplication
      runningApplicationsWithBundleIdentifier:@"com.apple.systempreferences"];
  NSRunningApplication *settings = apps.firstObject;
  if (!settings || settings.isHidden) {
    return false;
  }
  pid_t pid = settings.processIdentifier;
  CFArrayRef windows = CGWindowListCopyWindowInfo(
      kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
      kCGNullWindowID);
  if (!windows) {
    return false;
  }
  CGRect best = CGRectNull;
  for (NSDictionary *info in (__bridge NSArray *)windows) {
    if ([info[(id)kCGWindowOwnerPID] intValue] != pid ||
        [info[(id)kCGWindowLayer] intValue] != 0) {
      continue;
    }
    CGRect bounds;
    if (!CGRectMakeWithDictionaryRepresentation(
            (__bridge CFDictionaryRef)info[(id)kCGWindowBounds], &bounds)) {
      continue;
    }
    if (CGRectIsNull(best) ||
        bounds.size.width * bounds.size.height > best.size.width * best.size.height) {
      best = bounds;
    }
  }
  CFRelease(windows);
  if (CGRectIsNull(best) || best.size.width < 200 || best.size.height < 200) {
    return false;
  }
  *x = best.origin.x;
  *y = best.origin.y;
  *width = best.size.width;
  *height = best.size.height;
  return true;
}

/// Shows the bar over other apps without activating Ghostex, or hides it.
void GhostexGpuiPermissionGuideSetVisible(void *native_view, bool visible) {
  NSView *view = (__bridge NSView *)native_view;
  NSWindow *window = view.window;
  if (!window) {
    return;
  }
  if (visible) {
    [window orderFrontRegardless];
  } else {
    [window orderOut:nil];
  }
}

@interface GhostexGpuiPermissionGuideDragSource : NSObject <NSDraggingSource>
@end

@implementation GhostexGpuiPermissionGuideDragSource
- (NSDragOperation)draggingSession:(NSDraggingSession *)session
    sourceOperationMaskForDraggingContext:(NSDraggingContext)context {
  return context == NSDraggingContextOutsideApplication
             ? (NSDragOperationCopy | NSDragOperationLink | NSDragOperationGeneric)
             : NSDragOperationNone;
}
@end

/// Starts dragging `path` (the file macOS lists in the Privacy pane) from the mouse-down that is
/// being handled, with the file's own icon under the pointer. False when no mouse event is current.
bool GhostexGpuiPermissionGuideBeginFileDrag(void *native_view, const char *path) {
  static GhostexGpuiPermissionGuideDragSource *source = nil;
  NSView *view = (__bridge NSView *)native_view;
  NSEvent *event = NSApp.currentEvent;
  if (!view || !path || !event ||
      (event.type != NSEventTypeLeftMouseDown && event.type != NSEventTypeLeftMouseDragged)) {
    return false;
  }
  if (!source) {
    source = [GhostexGpuiPermissionGuideDragSource new];
  }
  NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
  NSDraggingItem *item = [[NSDraggingItem alloc] initWithPasteboardWriter:url];
  NSImage *icon = [[NSWorkspace sharedWorkspace] iconForFile:url.path];
  icon.size = NSMakeSize(48, 48);
  NSPoint point = [view convertPoint:event.locationInWindow fromView:nil];
  [item setDraggingFrame:NSMakeRect(point.x - 24, point.y - 24, 48, 48) contents:icon];
  NSDraggingSession *session = [view beginDraggingSessionWithItems:@[ item ]
                                                             event:event
                                                            source:source];
  session.animatesToStartingPositionsOnCancelOrFail = YES;
  return session != nil;
}
