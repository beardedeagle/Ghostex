#[cfg(unix)]
use super::*;

// ---------------------------------------------------------------------------
// terminal plumbing
// ---------------------------------------------------------------------------

#[cfg(unix)]
pub(super) mod term {
    use std::os::fd::AsRawFd;

    // Saved terminal state for restoration from a signal handler (which cannot
    // take arguments). Set while the TUI owns the terminal.
    static mut ORIGINAL: Option<libc::termios> = None;
    static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    pub fn restore() {
        use std::sync::atomic::Ordering;
        if !ACTIVE.swap(false, Ordering::SeqCst) {
            return;
        }
        let seq = super::LEAVE_TUI_SEQUENCE.as_bytes();
        unsafe {
            libc::write(1, seq.as_ptr() as *const libc::c_void, seq.len());
            #[allow(static_mut_refs)]
            if let Some(orig) = ORIGINAL {
                libc::tcsetattr(0, libc::TCSAFLUSH, &orig);
            }
        }
    }

    extern "C" fn on_signal(sig: libc::c_int) {
        restore();
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    pub fn install_signal_handlers() {
        unsafe {
            for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
                libc::signal(sig, on_signal as *const () as libc::sighandler_t);
            }
        }
    }

    pub fn enter_raw() -> Result<(), String> {
        unsafe {
            let mut orig: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut orig) != 0 {
                return Err("not a terminal".to_string());
            }
            ORIGINAL = Some(orig);
            let mut t = orig;
            t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
            t.c_iflag &= !(libc::IXON | libc::ICRNL);
            if libc::tcsetattr(0, libc::TCSAFLUSH, &t) != 0 {
                return Err("failed to enter raw mode".to_string());
            }
        }
        ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub fn leave_raw() {
        use std::sync::atomic::Ordering;
        ACTIVE.store(false, Ordering::SeqCst);
        unsafe {
            #[allow(static_mut_refs)]
            if let Some(orig) = ORIGINAL {
                libc::tcsetattr(0, libc::TCSAFLUSH, &orig);
            }
        }
    }

    /// Terminal size, rejecting implausible values. A bad ioctl can leave the
    /// struct uninitialised (0xAAAA has been seen on macOS), and a multi-thousand
    /// row "terminal" makes the renderer emit a giant blank frame that scrolls
    /// all real output off-screen.
    pub fn winsize(stdin: &std::io::Stdin) -> Option<(u16, u16)> {
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(stdin.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) != 0 {
                return None;
            }
            if ws.ws_row == 0 || ws.ws_col == 0 || ws.ws_row > 4096 || ws.ws_col > 4096 {
                return None;
            }
            Some((ws.ws_row, ws.ws_col))
        }
    }
}

#[cfg(not(unix))]
pub(super) mod term {
    pub fn install_signal_handlers() {}
    pub fn enter_raw() -> Result<(), String> {
        Err("the zehn picker requires a POSIX terminal".to_string())
    }
    pub fn leave_raw() {}
    pub fn winsize(_stdin: &std::io::Stdin) -> Option<(u16, u16)> {
        None
    }
}
