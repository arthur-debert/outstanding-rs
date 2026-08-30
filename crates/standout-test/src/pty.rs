use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
pub fn open_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let mut winsize = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: out-params are valid pointers to locals we own; name/termios
    // args are documented to accept null.
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
    // OwnedFds become their sole owners.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    // SAFETY: `slave` is a valid, owned fd for the lifetime of this block;
    // `termios` is a valid out/in-param for tcgetattr/tcsetattr.
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
