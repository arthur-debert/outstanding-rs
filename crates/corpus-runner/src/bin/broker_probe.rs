//! The stand-in agent for the credential broker's negative test: it does
//! what a real session does — reach the API through `ANTHROPIC_BASE_URL` —
//! and then what a real session's build script would do, which is try the
//! same thing on a connection of its own and enumerate what it inherited.
//!
//! It is a test fixture, not a runner feature. It exists as a binary
//! because the broker answers a *process*, so the caller has to be a real
//! exec'd program rather than a shell or a thread.

use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    let role = std::env::args().nth(1).unwrap_or_default();
    let base_url = std::env::var("ANTHROPIC_BASE_URL").unwrap_or_default();

    if role == "build-script" {
        let attempt = request(&base_url, r#"{"from":"build script"}"#);
        print!(
            "{}",
            serde_json::json!({
                "status": attempt.status,
                "body": attempt.body,
                "inherited_broker_sockets": inherited_broker_sockets(broker_port(&base_url)),
            })
        );
        return;
    }

    let session = request(&base_url, r#"{"from":"agent session"}"#);
    std::fs::write("agent-environment.txt", environment_dump()).expect("dumping the environment");

    // A build script is not the agent: cargo execs it, so it starts from a
    // fresh descriptor table and its own pid.
    let build_script = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("build-script")
        .output()
        .expect("spawning the build script");

    let result = serde_json::json!({
        "agent": {"status": session.status, "body": session.body},
        "build_script": serde_json::from_slice::<serde_json::Value>(&build_script.stdout)
            .unwrap_or_else(|_| serde_json::json!({"stdout": String::from_utf8_lossy(&build_script.stdout)})),
    });
    std::fs::write("broker-probe.json", result.to_string()).expect("writing the probe result");

    // The runner reads session statistics off this line.
    println!(r#"{{"type":"result","num_turns":1,"usage":{{"input_tokens":1,"output_tokens":1}}}}"#);
}

struct Attempt {
    status: String,
    body: String,
}

fn request(base_url: &str, body: &str) -> Attempt {
    let Some(authority) = base_url.strip_prefix("http://") else {
        return Attempt {
            status: format!("no brokered base url in the environment ({base_url:?})"),
            body: String::new(),
        };
    };
    let mut stream = match TcpStream::connect(authority) {
        Ok(stream) => stream,
        Err(err) => {
            return Attempt {
                status: format!("connect failed: {err}"),
                body: String::new(),
            }
        }
    };
    let request = format!(
        "POST /v1/messages HTTP/1.1\r\nHost: {authority}\r\n\
         content-type: application/json\r\nanthropic-version: 2023-06-01\r\n\
         x-api-key: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        std::env::var("ANTHROPIC_AUTH_TOKEN").unwrap_or_default(),
        body.len()
    );
    if let Err(err) = stream.write_all(request.as_bytes()) {
        return Attempt {
            status: format!("write failed: {err}"),
            body: String::new(),
        };
    }
    let mut response = String::new();
    if let Err(err) = stream.read_to_string(&mut response) {
        return Attempt {
            status: format!("read failed: {err}"),
            body: response,
        };
    }
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    Attempt {
        status: head.lines().next().unwrap_or_default().to_string(),
        body: body.to_string(),
    }
}

fn broker_port(base_url: &str) -> u16 {
    base_url
        .rsplit(':')
        .next()
        .and_then(|port| port.parse().ok())
        .unwrap_or_default()
}

/// Descriptors this process inherited that are already connected to the
/// broker — a channel it could use without being attributed at accept time.
fn inherited_broker_sockets(broker_port: u16) -> Vec<i32> {
    let mut inherited = Vec::new();
    for fd in 0..1024 {
        let mut address: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut length = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        // SAFETY: `address` and `length` are live for the call, which only
        // reports on a descriptor this process already holds.
        let named = unsafe {
            libc::getpeername(
                fd,
                std::ptr::addr_of_mut!(address).cast(),
                std::ptr::addr_of_mut!(length),
            )
        };
        if named != 0 || address.ss_family != libc::AF_INET as libc::sa_family_t {
            continue;
        }
        // SAFETY: the kernel reported an AF_INET peer, so the storage holds
        // a `sockaddr_in`.
        let peer = unsafe { *std::ptr::addr_of!(address).cast::<libc::sockaddr_in>() };
        if u16::from_be(peer.sin_port) == broker_port {
            inherited.push(fd);
        }
    }
    inherited
}

fn environment_dump() -> String {
    std::env::vars()
        .map(|(key, value)| format!("{key}={value}\n"))
        .collect()
}
