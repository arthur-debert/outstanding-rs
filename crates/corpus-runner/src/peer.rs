//! Which process holds a loopback connection, and can it hand that
//! descriptor to a child?
//!
//! Loopback TCP carries no kernel peer credential, so the credential broker
//! answers both questions from the OS socket tables: procfs on Linux,
//! libproc on macOS. Given a connection's two ports, [`ownership`] reports
//! whether the named process holds that socket and whether its descriptor
//! is close-on-exec. A descriptor that survives exec is an inheritable
//! capability, and the broker refuses to serve a channel that corpus-authored
//! code could inherit.
//!
//! The macOS half decodes a kernel struct this crate declares itself, so
//! [`self_check`] validates the decoding against the running kernel — the
//! broker runs it before it accepts anything. A layout that stopped
//! matching would otherwise fail silently in the dangerous direction: not
//! by denying everything, but by matching the wrong socket.

use std::net::{TcpListener, TcpStream};

/// Identified as the *client* sees it, which is how its socket table records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientSocket {
    pub client_port: u16,
    pub broker_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// The process holds the connection on a close-on-exec descriptor.
    Held,
    /// The process holds it on a descriptor that would survive exec.
    HeldInheritable,
    /// No descriptor of that process matches the connection.
    Absent,
}

/// Connects to a throwaway listener from this process and requires the tables to name it.
pub fn self_check() -> Result<(), String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("binding a probe listener: {e}"))?;
    let broker_port = listener
        .local_addr()
        .map_err(|e| format!("reading the probe listener address: {e}"))?
        .port();
    let _client = TcpStream::connect(("127.0.0.1", broker_port))
        .map_err(|e| format!("connecting to the probe listener: {e}"))?;
    let (_served, peer) = listener
        .accept()
        .map_err(|e| format!("accepting the probe connection: {e}"))?;
    let socket = ClientSocket {
        client_port: peer.port(),
        broker_port,
    };
    match ownership(std::process::id(), socket)? {
        Ownership::Held => Ok(()),
        other => Err(format!(
            "the socket tables report {other:?} for this process's own \
             loopback connection {socket:?}"
        )),
    }
}

#[cfg(target_os = "macos")]
pub use macos::ownership;

#[cfg(target_os = "linux")]
pub use linux::ownership;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn ownership(_pid: u32, _socket: ClientSocket) -> Result<Ownership, String> {
    Err("peer resolution needs macOS libproc or Linux procfs".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{ClientSocket, Ownership};

    const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
    const PROC_FP_CLEXEC: u32 = 2;
    const SOCKINFO_TCP: i32 = 2;

    // sys/proc_info.h `socket_fdinfo`, decoded only as far as the two ports
    // `in_sockinfo` starts with.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ProcFileInfo {
        fi_openflags: u32,
        fi_status: u32,
        fi_offset: i64,
        fi_type: i32,
        fi_guardflags: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SockbufInfo {
        sbi_cc: u32,
        sbi_hiwat: u32,
        sbi_mbcnt: u32,
        sbi_mbmax: u32,
        sbi_lowat: u32,
        sbi_flags: i16,
        sbi_timeo: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SocketFdInfoHead {
        pfi: ProcFileInfo,
        soi_stat: libc::vinfo_stat,
        soi_so: u64,
        soi_pcb: u64,
        soi_type: i32,
        soi_protocol: i32,
        soi_family: i32,
        soi_options: i16,
        soi_linger: i16,
        soi_state: i16,
        soi_qlen: i16,
        soi_incqlen: i16,
        soi_qlimit: i16,
        soi_timeo: i16,
        soi_error: u16,
        soi_oobmark: u32,
        soi_rcv: SockbufInfo,
        soi_snd: SockbufInfo,
        soi_kind: i32,
        rfu_1: u32,
        insi_fport: i32,
        insi_lport: i32,
    }

    #[repr(C, align(8))]
    struct InfoBuffer([u8; 2048]);

    pub fn ownership(pid: u32, socket: ClientSocket) -> Result<Ownership, String> {
        let mut found = Ownership::Absent;
        for fd in socket_fds(pid)? {
            let Some(info) = socket_info(pid as i32, fd) else {
                continue;
            };
            if info.soi_kind != SOCKINFO_TCP {
                continue;
            }
            if port(info.insi_lport) != socket.client_port
                || port(info.insi_fport) != socket.broker_port
            {
                continue;
            }
            found = if info.pfi.fi_status & PROC_FP_CLEXEC != 0 {
                Ownership::Held
            } else {
                Ownership::HeldInheritable
            };
            // An inheritable descriptor decides the answer even beside a close-on-exec one.
            if found == Ownership::HeldInheritable {
                break;
            }
        }
        Ok(found)
    }

    // in_sockinfo keeps ports in network byte order inside an int.
    fn port(raw: i32) -> u16 {
        u16::from_be(raw as u16)
    }

    fn socket_fds(pid: u32) -> Result<Vec<i32>, String> {
        let pid = pid as libc::c_int;
        let entry = std::mem::size_of::<libc::proc_fdinfo>();
        // SAFETY: a null buffer with size 0 asks libproc for the table size.
        let sized =
            unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        if sized <= 0 {
            return Err(format!(
                "listing descriptors of pid {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }
        // Room for descriptors opened between the sizing call and this one.
        let capacity = sized as usize / entry + 64;
        let mut table: Vec<libc::proc_fdinfo> = vec![
            libc::proc_fdinfo {
                proc_fd: 0,
                proc_fdtype: 0,
            };
            capacity
        ];
        let bytes = (capacity * entry) as libc::c_int;
        // SAFETY: `table` owns `bytes` initialized bytes of `proc_fdinfo`.
        let used = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDLISTFDS,
                0,
                table.as_mut_ptr().cast(),
                bytes,
            )
        };
        if used <= 0 {
            return Err(format!(
                "listing descriptors of pid {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }
        table.truncate(used as usize / entry);
        Ok(table
            .into_iter()
            .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
            .map(|fd| fd.proc_fd)
            .collect())
    }

    fn socket_info(pid: i32, fd: i32) -> Option<SocketFdInfoHead> {
        let mut buffer = InfoBuffer([0u8; 2048]);
        // SAFETY: the buffer outlives the call and is larger than the
        // `socket_fdinfo` the flavor writes; the return value is the byte
        // count actually written, checked below before any read.
        let written = unsafe {
            libc::proc_pidfdinfo(
                pid,
                fd,
                PROC_PIDFDSOCKETINFO,
                buffer.0.as_mut_ptr().cast(),
                buffer.0.len() as libc::c_int,
            )
        };
        if (written as usize) < std::mem::size_of::<SocketFdInfoHead>() {
            return None;
        }
        // SAFETY: the kernel wrote at least a whole `SocketFdInfoHead`
        // prefix into this 8-aligned buffer.
        Some(unsafe { *buffer.0.as_ptr().cast::<SocketFdInfoHead>() })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{ClientSocket, Ownership};

    use std::collections::HashMap;

    pub fn ownership(pid: u32, socket: ClientSocket) -> Result<Ownership, String> {
        let Some(inode) = connection_inode(socket)? else {
            return Ok(Ownership::Absent);
        };
        let held = held_descriptors(pid)?;
        match held.get(&inode) {
            None => Ok(Ownership::Absent),
            Some(true) => Ok(Ownership::Held),
            Some(false) => Ok(Ownership::HeldInheritable),
        }
    }

    // The client end of the connection, as /proc/net/tcp records it.
    fn connection_inode(socket: ClientSocket) -> Result<Option<u64>, String> {
        for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
            let Ok(text) = std::fs::read_to_string(table) else {
                continue;
            };
            for line in text.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 10 {
                    continue;
                }
                if endpoint_port(fields[1]) != Some(socket.client_port)
                    || endpoint_port(fields[2]) != Some(socket.broker_port)
                {
                    continue;
                }
                return fields[9]
                    .parse::<u64>()
                    .map(Some)
                    .map_err(|e| format!("parsing socket inode {:?}: {e}", fields[9]));
            }
        }
        Ok(None)
    }

    // "0100007F:1F90" — the port half is big-endian hex.
    fn endpoint_port(field: &str) -> Option<u16> {
        u16::from_str_radix(field.rsplit(':').next()?, 16).ok()
    }

    /// Socket inode to whether its descriptor is close-on-exec.
    fn held_descriptors(pid: u32) -> Result<HashMap<u64, bool>, String> {
        let mut held = HashMap::new();
        let dir = format!("/proc/{pid}/fd");
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("listing descriptors in {dir}: {e}"))?;
        for entry in entries.flatten() {
            let Ok(target) = std::fs::read_link(entry.path()) else {
                continue;
            };
            let target = target.to_string_lossy();
            let Some(inode) = target
                .strip_prefix("socket:[")
                .and_then(|rest| rest.strip_suffix(']'))
                .and_then(|digits| digits.parse::<u64>().ok())
            else {
                continue;
            };
            let cloexec = close_on_exec(pid, &entry.file_name().to_string_lossy());
            held.entry(inode)
                .and_modify(|current| *current &= cloexec)
                .or_insert(cloexec);
        }
        Ok(held)
    }

    fn close_on_exec(pid: u32, fd: &str) -> bool {
        let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/fdinfo/{fd}")) else {
            // Unreadable flags count as inheritable: what cannot be attributed is denied.
            return false;
        };
        for line in text.lines() {
            let Some(value) = line.strip_prefix("flags:") else {
                continue;
            };
            let Ok(flags) = i32::from_str_radix(value.trim(), 8) else {
                return false;
            };
            return flags & libc::O_CLOEXEC != 0;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_tables_name_this_process_for_its_own_connection() {
        self_check().unwrap();
    }

    #[test]
    fn a_connection_this_process_does_not_hold_is_absent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let broker_port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", broker_port)).unwrap();
        let client_port = client.local_addr().unwrap().port();
        let (_served, _peer) = listener.accept().unwrap();

        // Same connection, a process that is not its client.
        let elsewhere = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .unwrap();
        let verdict = ownership(
            elsewhere.id(),
            ClientSocket {
                client_port,
                broker_port,
            },
        );
        let mut elsewhere = elsewhere;
        let _ = elsewhere.kill();
        let _ = elsewhere.wait();
        assert_eq!(verdict.unwrap(), Ownership::Absent);
    }

    #[test]
    fn an_inheritable_descriptor_is_reported_as_inheritable() {
        use std::os::fd::AsRawFd;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let broker_port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", broker_port)).unwrap();
        let client_port = client.local_addr().unwrap().port();
        let (_served, _peer) = listener.accept().unwrap();

        // SAFETY: clearing FD_CLOEXEC on a descriptor this test owns.
        let cleared = unsafe { libc::fcntl(client.as_raw_fd(), libc::F_SETFD, 0) };
        assert_eq!(cleared, 0, "{}", std::io::Error::last_os_error());

        assert_eq!(
            ownership(
                std::process::id(),
                ClientSocket {
                    client_port,
                    broker_port,
                },
            )
            .unwrap(),
            Ownership::HeldInheritable
        );
    }
}
