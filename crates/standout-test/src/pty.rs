//! The pty seam: one function that opens a configured pseudo-terminal
//! master/slave pair, and the only unsafe code in the crate — three C calls
//! (`openpty`, `tcgetattr`, `tcsetattr`) wrapped straight into owned fds.
//!
//! Public because it is the harness's terminal boundary and other harnesses
//! build on it: [`TestHarness::run_pty`](crate::TestHarness::run_pty) uses it
//! for TTY-positive process runs, and the corpus runner attaches produced-
//! binary streams to it when an acceptance case declares `tty` (see
//! `corpus/README.md`, Run semantics).

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

/// Opens a pty pair sized 80×24 with `ONLCR` off, returning
/// `(master, slave)`.
///
/// `ONLCR` is the line discipline's `\n` → `\r\n` output rewrite; with
/// it on, the master would record a translation of the child's bytes
/// instead of the bytes, and captures promise a recording. The fixed
/// window size keeps width-sensitive rendering deterministic instead of
/// inheriting a 0×0 window.
///
/// Returns the operating-system error when any of the three C calls fails.
/// Callers decide whether that environment failure should be recorded or
/// treated as a test-harness panic.
pub fn open_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let mut winsize = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: openpty only writes the two fd out-parameters and reads
    // the winsize; the name and termios parameters are documented to
    // accept null. The winsize argument is a raw borrow because libc
    // declares `winp` as `*mut` on Apple and `*const` on Linux — `&raw
    // mut` satisfies both, where `&mut` trips Linux clippy's
    // `unnecessary_mut_passed` and `&` fails the Apple build.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut winsize,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: on success both fds are freshly opened and unowned; these
    // OwnedFds are their sole owners from here on.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };

    // SAFETY: tcgetattr/tcsetattr read and write the termios
    // out-parameter for an fd this function owns.
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        let rc = libc::tcgetattr(slave.as_raw_fd(), &mut termios);
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        termios.c_oflag &= !(libc::ONLCR as libc::tcflag_t);
        let rc = libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &termios);
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok((master, slave))
}
