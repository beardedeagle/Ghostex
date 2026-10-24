#import <Foundation/Foundation.h>
#import <mach-o/dyld.h>
#import <stdint.h>

/*
 CDXC:CrashReports 2026-10-24 WHY:
 GitHub #291: an uncaught Objective-C exception (AppKit's Touch Bar finder throwing from the display cycle) ended the app and left nothing in Ghostex's logs: the panic hook only sees Rust panics, and Ghostty's Sentry signal handler turns the abort that follows into a plain exit, so macOS writes no crash report either.
 This handler writes the exception's name, reason and call stack to gpui-crash-reports.log (support_logs.rs), then calls the handler installed before it, so the process still ends exactly as before. It records and never catches or resumes.
 The load address of the executable is recorded so the unsymbolicated Ghostex frames can be resolved with `atos -l`. Exceptions AppKit catches itself and then traps on (inside view layout, for example) never reach an uncaught-exception handler and still leave only AppKit's unified-log line.
*/
extern void GhostexGpuiRecordUncaughtException(const char *name,
                                               const char *reason,
                                               const char *callStack,
                                               uint64_t executableLoadAddress);

static NSUncaughtExceptionHandler *GhostexGpuiPreviousUncaughtExceptionHandler;

static void GhostexGpuiHandleUncaughtException(NSException *exception) {
  NSString *callStack =
      [exception.callStackSymbols componentsJoinedByString:@"\n"];
  GhostexGpuiRecordUncaughtException(
      exception.name.UTF8String, exception.reason.UTF8String,
      callStack.UTF8String, (uint64_t)(uintptr_t)_dyld_get_image_header(0));
  if (GhostexGpuiPreviousUncaughtExceptionHandler) {
    GhostexGpuiPreviousUncaughtExceptionHandler(exception);
  }
}

void GhostexGpuiInstallUncaughtExceptionHandler(void) {
  NSUncaughtExceptionHandler *current = NSGetUncaughtExceptionHandler();
  if (current == GhostexGpuiHandleUncaughtException) {
    return;
  }
  GhostexGpuiPreviousUncaughtExceptionHandler = current;
  NSSetUncaughtExceptionHandler(GhostexGpuiHandleUncaughtException);
}
