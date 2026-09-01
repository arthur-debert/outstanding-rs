// The credential broker's boundary, exercised through the whole runner: the
// agent session authenticates, and a build script it spawns cannot — not by
// reading the credential (it is not in the process tree) and not by using
// the channel (its own connection is attributed to itself, and no broker
// descriptor survived the exec that started it).
//
// Runs in its own test binary because the hermetic build prepends to the
// process-wide PATH.

#![cfg(unix)]

mod common;

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::FromRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use corpus_runner::broker::{Broker, BrokerConfig, Credential, PLACEHOLDER_TOKEN};
use corpus_runner::{run, RunConfig, Timeouts};

const BROKERED_TOKEN: &str = "oauth-token-only-the-host-holds";

const SMOKE: &str = r#"echo 'smoke — a tiny fixed star catalog'"#;

#[test]
fn a_build_script_spawned_by_the_agent_cannot_use_the_brokered_credential() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = tempfile::tempdir().unwrap();
    let upstream = Upstream::start();

    let bin_dir = scratch.path().join("bin");
    common::install_fake_cargo(&bin_dir, "smoke", SMOKE);
    // The agent sandbox denies reads under the checkout, so the stand-in
    // agent has to be staged outside it like any other fixture.
    let agent = bin_dir.join("broker_probe");
    std::fs::copy(env!("CARGO_BIN_EXE_broker_probe"), &agent).unwrap();

    let config = RunConfig {
        archetype: "smoke".to_string(),
        archetypes_dir: repo.join("corpus/archetypes"),
        runs_dir: scratch.path().join("runs"),
        docs_dir: repo.join("docs"),
        agent_cmd: agent.display().to_string(),
        broker: Some(brokered(&upstream, Duration::from_secs(30))),
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts {
            agent: Duration::from_secs(120),
            build: Duration::from_secs(120),
            check: Duration::from_secs(30),
        },
    };

    let (report, run_dir) = run(&config).unwrap();
    let workspace = run_dir.join("workspace");
    let probe: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.join("broker-probe.json")).unwrap(),
    )
    .unwrap();

    // The agent's own request went through, and it went through
    // authenticated: the broker replaced the placeholder it was given.
    assert_eq!(probe["agent"]["status"], "HTTP/1.1 200 OK", "{probe:#}");
    let forwarded = upstream.seen();
    assert_eq!(forwarded.len(), 1, "{forwarded:#?}");
    assert_eq!(
        forwarded[0].header("authorization").as_deref(),
        Some(&format!("Bearer {BROKERED_TOKEN}")[..])
    );
    assert_eq!(forwarded[0].header("x-api-key"), None, "{forwarded:#?}");
    assert!(
        forwarded[0].body.contains("agent session"),
        "{forwarded:#?}"
    );

    // The build script's own attempt was denied, and nothing of it reached
    // the upstream.
    let build_script = &probe["build_script"];
    assert!(
        build_script["status"]
            .as_str()
            .unwrap_or_default()
            .contains("403"),
        "{probe:#}"
    );
    assert!(
        build_script["body"]
            .as_str()
            .unwrap_or_default()
            .contains("corpus_broker_denied"),
        "{probe:#}"
    );

    // Nor could it reuse the agent's channel: the exec that started it left
    // no broker descriptor behind.
    assert_eq!(
        build_script["inherited_broker_sockets"],
        serde_json::json!([]),
        "{probe:#}"
    );

    // The credential is not in the process tree at all — what the agent
    // (and so every descendant) can read is the placeholder.
    let environment = std::fs::read_to_string(workspace.join("agent-environment.txt")).unwrap();
    assert!(!environment.contains(BROKERED_TOKEN), "{environment}");
    assert!(
        environment.contains(&format!("ANTHROPIC_AUTH_TOKEN={PLACEHOLDER_TOKEN}\n")),
        "{environment}"
    );

    // And the run says what it admitted.
    // Including which environment keys the brokered session actually carried.
    assert!(
        report
            .blindness
            .env_allowlist
            .iter()
            .any(|key| key == "ANTHROPIC_BASE_URL"),
        "{:?}",
        report.blindness.env_allowlist
    );

    let admitted = report.blindness.credential_exceptions.join("\n");
    assert!(admitted.contains("host-side loopback broker"), "{admitted}");
    assert!(admitted.contains("a test double"), "{admitted}");
    assert!(admitted.contains("admitted 1 connection(s)"), "{admitted}");
    assert!(admitted.contains("denied 1 connection(s)"), "{admitted}");
    assert!(!admitted.contains(BROKERED_TOKEN), "{admitted}");
    assert!(
        !std::fs::read_to_string(run_dir.join("report.json"))
            .unwrap()
            .contains(BROKERED_TOKEN),
        "the report must not carry the credential"
    );
}

/// The broker configuration every test here uses: a credential the test made
/// up, which is the only kind allowed to name an upstream of its own.
fn brokered(upstream: &Upstream, request_timeout: Duration) -> BrokerConfig {
    BrokerConfig::for_test_upstream(
        Credential::new(BROKERED_TOKEN.to_string(), "a test double".to_string()),
        upstream.base_url.clone(),
    )
    .unwrap()
    .with_request_timeout(request_timeout)
}

#[test]
fn an_inheritable_descriptor_is_refused_even_from_the_authorized_process() {
    let upstream = Upstream::start();
    let broker = Broker::start(brokered(&upstream, Duration::from_secs(30))).unwrap();
    broker.authorize(std::process::id());
    let authority = broker.base_url().trim_start_matches("http://").to_string();

    // Rust's own sockets are close-on-exec, which is the shape the broker
    // requires.
    let served = ask(TcpStream::connect(&authority).unwrap());
    assert!(
        served.starts_with("HTTP/1.1 200 OK"),
        "{served}\ndenials: {:?}",
        broker.denials()
    );

    // The same process, on a descriptor that would survive an exec: the
    // broker refuses to put a credential behind an inheritable channel.
    let refused = ask(inheritable_connection(&authority));
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    assert!(refused.contains("corpus_broker_denied"), "{refused}");

    assert_eq!(upstream.seen().len(), 1);
    assert_eq!(broker.admitted(), 1);
    let denials = broker.denials().join("\n");
    assert!(denials.contains("survives exec"), "{denials}");
}

#[test]
fn shutdown_ends_a_request_still_waiting_on_the_upstream() {
    let upstream = Upstream::hanging();
    // A request timeout far longer than the test would tolerate: what bounds
    // the shutdown has to be the shutdown, not the timeout expiring.
    let mut broker = Broker::start(brokered(&upstream, Duration::from_secs(600))).unwrap();
    broker.authorize(std::process::id());
    let authority = broker.base_url().trim_start_matches("http://").to_string();

    let mut caller = TcpStream::connect(&authority).unwrap();
    let asking = std::thread::spawn(move || {
        let body = r#"{"from":"a test"}"#;
        caller
            .write_all(
                format!(
                    "POST /v1/messages HTTP/1.1\r\nHost: broker\r\n\
                     content-type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
        let mut response = String::new();
        let _ = caller.read_to_string(&mut response);
        response
    });

    // The request is genuinely in flight: it was admitted, forwarded, and
    // the upstream is sitting on it.
    upstream.await_request();

    let started = Instant::now();
    broker.shutdown();
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(30),
        "shutdown waited {took:?} on the upstream instead of ending the request"
    );

    // And the caller is not left holding an open connection either.
    let response = asking.join().unwrap();
    assert!(!response.contains("200 OK"), "{response}");
    assert_eq!(broker.admitted(), 1);
}

/// A real session sends bodies far larger than one segment, and the caller's
/// second segment can arrive after the broker has already read the first. The
/// broker waits for it: the alternative is a request refused for arriving in
/// pieces, which is how a long agent session dies mid-run.
#[test]
fn a_body_that_arrives_in_two_segments_is_forwarded_whole() {
    let upstream = Upstream::start();
    let broker = Broker::start(brokered(&upstream, Duration::from_secs(30))).unwrap();
    broker.authorize(std::process::id());
    let authority = broker.base_url().trim_start_matches("http://").to_string();

    let head = r#"{"from":"a test with a body in"#;
    let tail = r#" two segments"}"#;
    let mut caller = TcpStream::connect(&authority).unwrap();
    caller
        .write_all(
            format!(
                "POST /v1/messages HTTP/1.1\r\nHost: broker\r\n\
                 content-type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{head}",
                head.len() + tail.len()
            )
            .as_bytes(),
        )
        .unwrap();
    caller.flush().unwrap();
    // The tail waits on two observations rather than on a sleep, because a
    // sleep only makes the ordering likely: miss it under load and both
    // segments are readable at once, which is the one case a non-blocking
    // accepted socket also survives.
    //
    // First the connection is admitted, which the broker does after
    // authorizing the caller and before reading a request byte.
    let admitted = Instant::now();
    while broker.admitted() == 0 {
        assert!(
            admitted.elapsed() < Duration::from_secs(10),
            "the broker never admitted the connection"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // Then the broker says nothing while the body is short. A broker that
    // cannot wait for the rest fails its read at once and answers 400, so an
    // answer arriving here is the regression itself; silence is the broker
    // sitting in the read this test is about.
    caller
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut answered = [0u8; 1];
    match caller.read(&mut answered) {
        Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
        answer => panic!("the broker answered a body it had not finished reading: {answer:?}"),
    }
    caller.set_read_timeout(None).unwrap();
    caller.write_all(tail.as_bytes()).unwrap();

    let mut response = String::new();
    let _ = caller.read_to_string(&mut response);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");

    let forwarded = upstream.seen();
    assert_eq!(forwarded.len(), 1, "{forwarded:#?}");
    assert_eq!(forwarded[0].body, format!("{head}{tail}"));
}

/// The broker appends the caller's target to its configured upstream, so a
/// target that starts an authority rather than a path would carry the
/// credential to whatever host it names: `@host/v1` demotes the configured
/// upstream to a userinfo field. The listener standing in for that host is
/// a real one here, so the assertion is that nothing arrived at it, not
/// that the URL looked wrong.
#[test]
fn a_target_that_names_another_host_reaches_neither_host() {
    let upstream = Upstream::start();
    let elsewhere = Upstream::start();
    let broker = Broker::start(brokered(&upstream, Duration::from_secs(30))).unwrap();
    broker.authorize(std::process::id());
    let authority = broker.base_url().trim_start_matches("http://").to_string();

    let elsewhere_authority = elsewhere.base_url.trim_start_matches("http://").to_string();
    for target in [
        format!("@{elsewhere_authority}/v1/messages"),
        format!("//{elsewhere_authority}/v1/messages"),
        format!("http://{elsewhere_authority}/v1/messages"),
    ] {
        let mut caller = TcpStream::connect(&authority).unwrap();
        caller
            .write_all(
                format!(
                    "POST {target} HTTP/1.1\r\nHost: broker\r\n\
                     Content-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .unwrap();
        let mut response = String::new();
        let _ = caller.read_to_string(&mut response);
        assert!(
            response.starts_with("HTTP/1.1 403"),
            "{target} was not refused: {response}"
        );
        assert!(!response.contains(BROKERED_TOKEN), "{response}");
    }

    let reached = format!("{:#?}", elsewhere.seen());
    let arrived = format!("{:#?}", upstream.seen());
    assert_eq!(
        elsewhere.seen().len(),
        0,
        "the credential reached the host the target named: {reached}"
    );
    assert_eq!(upstream.seen().len(), 0, "{arrived}");

    // The same broker still serves the target it exists for, so the refusal
    // is the shape of the target and not the connection.
    assert!(ask(TcpStream::connect(&authority).unwrap()).starts_with("HTTP/1.1 200 OK"));
    let forwarded = upstream.seen();
    assert_eq!(
        forwarded[0].header("authorization").as_deref(),
        Some(format!("Bearer {BROKERED_TOKEN}").as_str())
    );
}

fn ask(mut stream: TcpStream) -> String {
    let body = r#"{"from":"a test"}"#;
    stream
        .write_all(
            format!(
                "POST /v1/messages HTTP/1.1\r\nHost: broker\r\n\
                 content-type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .unwrap();
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response
}

fn inheritable_connection(authority: &str) -> TcpStream {
    let port: u16 = authority.rsplit(':').next().unwrap().parse().unwrap();
    // SAFETY: a plain AF_INET socket, deliberately created without
    // SOCK_CLOEXEC, then connected to the broker on loopback.
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
        assert!(fd >= 0, "{}", std::io::Error::last_os_error());
        let mut address: libc::sockaddr_in = std::mem::zeroed();
        address.sin_family = libc::AF_INET as libc::sa_family_t;
        address.sin_port = port.to_be();
        address.sin_addr.s_addr = u32::from(std::net::Ipv4Addr::LOCALHOST).to_be();
        #[cfg(target_os = "macos")]
        {
            address.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
        }
        let connected = libc::connect(
            fd,
            std::ptr::addr_of!(address).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        );
        assert_eq!(connected, 0, "{}", std::io::Error::last_os_error());
        TcpStream::from_raw_fd(fd)
    }
}

/// A stand-in for the API: it answers everything and remembers what it was
/// asked, so a test can see which requests actually left the host.
struct Upstream {
    base_url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
}

#[derive(Debug)]
struct Seen {
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    }
}

impl Upstream {
    fn start() -> Self {
        Self::bind(true)
    }

    /// The same double, except it never answers: what a request blocked on an
    /// unresponsive API looks like from the broker's side.
    fn hanging() -> Self {
        Self::bind(false)
    }

    fn bind(respond: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        listener.set_nonblocking(true).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let served = Arc::clone(&seen);
        let stopped = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let served = Arc::clone(&served);
                        let stopped = Arc::clone(&stopped);
                        std::thread::spawn(move || answer(stream, served, respond, stopped));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url,
            seen,
            stop,
        }
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Vec<Seen>> {
        self.seen.lock().unwrap()
    }

    /// Block until the upstream has read a whole request, so a test acts on
    /// a request that is genuinely in flight rather than on a race.
    fn await_request(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.seen().is_empty() {
            assert!(Instant::now() < deadline, "no request reached the upstream");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Upstream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn answer(
    mut stream: TcpStream,
    seen: Arc<Mutex<Vec<Seen>>>,
    respond: bool,
    stopped: Arc<AtomicBool>,
) {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let Ok(read) = stream.read(&mut chunk) else {
            return;
        };
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&raw);
        let Some((head, body)) = text.split_once("\r\n\r\n") else {
            continue;
        };
        let length: usize = head
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            })
            .and_then(|(_, value)| value.trim().parse().ok())
            .unwrap_or(0);
        if body.len() >= length {
            break;
        }
    }

    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();
    seen.lock().unwrap().push(Seen {
        headers,
        body: body.to_string(),
    });

    if !respond {
        while !stopped.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        return;
    }

    let payload = r#"{"type":"message","content":[{"type":"text","text":"ok"}]}"#;
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
            payload.len()
        )
        .as_bytes(),
    );
}
