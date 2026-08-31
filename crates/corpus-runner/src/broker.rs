//! The run-credential broker: the agent phase's one credential exception
//! (ADR-0023's ROB07-WS01 amendment).
//!
//! A blind run needs an authenticated agent and a sandbox that keeps the
//! credential away from everything the agent spawns — build scripts, tests,
//! the produced binary. An environment variable cannot do that (descendants
//! inherit it) and an open loopback proxy cannot either (the agent and build
//! phases have network). So the runner holds the credential itself, outside
//! every sandbox, and lends its *use* to one process.
//!
//! The broker is a loopback forward proxy alive only while the agent session
//! runs. It reads the host Claude subscription's OAuth token from the host
//! credential store, and the agent session's environment carries only
//! `ANTHROPIC_BASE_URL` pointing here plus a placeholder token, so the real
//! credential never enters the agent's process tree. Each forwarded request
//! gets the authorization injected here, on the host side; the Seatbelt
//! Keychain denial in [`crate::sandbox`] is unchanged, because nothing
//! inside the sandbox reads the store.
//!
//! The caller boundary is enforced per connection, not per session: before
//! reading a single request byte the broker asks the OS socket tables who
//! holds the other end (see [`crate::peer`]) and serves only the agent
//! process itself, on a close-on-exec descriptor. A descendant therefore
//! cannot use the channel — its own connections resolve to its own pid, and
//! no broker descriptor survives the exec that starts it. What that leaves
//! is the agent forking without exec or deliberately passing a descriptor
//! on, which is the authorized principal cooperating in its own bypass; no
//! transport can police that, and this one does not claim to.
//!
//! Everything admitted is written into the report's
//! `blindness.credential_exceptions` (see [`Broker::credential_exceptions`]),
//! so a run that used the broker says so.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{bail, Context};

use crate::exec;
use crate::peer::{self, ClientSocket, Ownership};

/// What the agent session gets instead of a credential. Deliberately not
/// `sk-ant-` shaped: the committed-evidence scanner treats that prefix as a
/// secret, and a placeholder is not one.
pub const PLACEHOLDER_TOKEN: &str = "corpus-broker-placeholder-not-a-credential";

pub const DEFAULT_UPSTREAM: &str = "https://api.anthropic.com";

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const AUTHORIZATION_WAIT: Duration = Duration::from_secs(10);
const REQUEST_HEAD_CAP: usize = 256 * 1024;
const REQUEST_BODY_CAP: usize = 64 * 1024 * 1024;
const DENIAL_LOG_CAP: usize = 32;
const CLOSE_DRAIN: Duration = Duration::from_millis(250);

/// The host credential, kept out of `Debug` output and off every argument
/// vector: the only place it is written is the broker's forwarding
/// configuration, on a pipe.
#[derive(Clone)]
pub struct Credential {
    token: String,
    source: String,
}

impl Credential {
    pub fn new(token: String, source: String) -> Self {
        Self { token, source }
    }

    /// Read the agent CLI's own OAuth token from the host credential store:
    /// the macOS Keychain item the CLI writes, else its credentials file.
    /// The host CLI owns refreshing it; the broker only ever reads.
    pub fn from_host_store() -> anyhow::Result<Self> {
        let mut attempts = Vec::new();
        #[cfg(target_os = "macos")]
        {
            match keychain_item() {
                Ok(json) => {
                    return parse_credential(
                        &json,
                        format!("macOS Keychain item {KEYCHAIN_SERVICE:?}"),
                    )
                }
                Err(detail) => attempts.push(detail),
            }
        }
        match credentials_file() {
            Ok((json, path)) => return parse_credential(&json, format!("credentials file {path}")),
            Err(detail) => attempts.push(detail),
        }
        bail!(
            "no host Claude credential to broker ({}); the broker never prompts and \
             never widens the sandbox, so the answer is a host CLI that is logged in",
            attempts.join("; ")
        )
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    fn redact(&self, text: &str) -> String {
        text.replace(&self.token, "[broker-credential]")
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("token", &"[redacted]")
            .field("source", &self.source)
            .finish()
    }
}

#[cfg(target_os = "macos")]
fn keychain_item() -> Result<String, String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
        .output()
        .map_err(|e| format!("running security(1): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Keychain item {KEYCHAIN_SERVICE:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("Keychain item is not UTF-8: {e}"))
}

fn credentials_file() -> Result<(String, String), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is unset".to_string())?;
    let path = std::path::Path::new(&home)
        .join(".claude")
        .join(".credentials.json");
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    Ok((text, path.display().to_string()))
}

fn parse_credential(json: &str, source: String) -> anyhow::Result<Credential> {
    let value: serde_json::Value = serde_json::from_str(json.trim())
        .with_context(|| format!("parsing the credential from {source}"))?;
    let token = value
        .pointer("/claudeAiOauth/accessToken")
        .and_then(|token| token.as_str())
        .with_context(|| format!("{source} holds no claudeAiOauth.accessToken"))?;
    Ok(Credential::new(token.to_string(), source))
}

#[derive(Clone, Debug)]
pub struct BrokerConfig {
    pub credential: Credential,
    /// Base URL every forwarded request is rebased onto.
    pub upstream: String,
    /// Deadline for one forwarded request, end to end.
    pub request_timeout: Duration,
}

impl BrokerConfig {
    pub fn for_host(credential: Credential) -> Self {
        Self {
            credential,
            upstream: DEFAULT_UPSTREAM.to_string(),
            request_timeout: Duration::from_secs(600),
        }
    }
}

pub struct Broker {
    port: u16,
    state: Arc<State>,
    accept_loop: Option<JoinHandle<()>>,
}

struct State {
    credential: Mutex<Credential>,
    credential_source: String,
    upstream: String,
    request_timeout: Duration,
    /// The one process the broker answers. `None` until the agent is
    /// spawned and after it exits: an unauthorized broker serves nobody.
    authorized: Mutex<Option<u32>>,
    authorization_set: Condvar,
    shutdown: AtomicBool,
    admitted: AtomicUsize,
    denied: AtomicUsize,
    /// Why connections were denied, deduplicated: the reasons are evidence
    /// for the report, while [`State::denied`] is the count.
    denials: Mutex<Vec<String>>,
}

impl Broker {
    /// Bind the broker and start answering. The peer resolver is validated
    /// against the running kernel first: a broker that cannot attribute
    /// connections must not exist at all.
    pub fn start(config: BrokerConfig) -> anyhow::Result<Self> {
        peer::self_check().map_err(|detail| {
            anyhow::anyhow!(
                "the credential broker cannot attribute connections on this host ({detail}); \
                 refusing to start rather than serving an unattributed caller"
            )
        })?;
        let listener = TcpListener::bind("127.0.0.1:0").context("binding the credential broker")?;
        let port = listener.local_addr()?.port();
        listener
            .set_nonblocking(true)
            .context("making the broker listener pollable")?;

        let state = Arc::new(State {
            credential_source: config.credential.source().to_string(),
            credential: Mutex::new(config.credential),
            upstream: config.upstream.trim_end_matches('/').to_string(),
            request_timeout: config.request_timeout,
            authorized: Mutex::new(None),
            authorization_set: Condvar::new(),
            shutdown: AtomicBool::new(false),
            admitted: AtomicUsize::new(0),
            denied: AtomicUsize::new(0),
            denials: Mutex::new(Vec::new()),
        });

        let loop_state = Arc::clone(&state);
        let accept_loop = std::thread::spawn(move || accept_loop(listener, loop_state));
        Ok(Self {
            port,
            state,
            accept_loop: Some(accept_loop),
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Point one command's session at the broker. The placeholder is what
    /// the agent and every descendant can read; the credential is not here.
    pub fn apply_agent_env(&self, command: &mut Command) {
        command.env("ANTHROPIC_BASE_URL", self.base_url());
        command.env("ANTHROPIC_AUTH_TOKEN", PLACEHOLDER_TOKEN);
    }

    /// Name the one process the broker answers, once it exists.
    pub fn authorize(&self, pid: u32) {
        *self
            .state
            .authorized
            .lock()
            .expect("broker authorization poisoned") = Some(pid);
        self.state.authorization_set.notify_all();
    }

    pub fn revoke(&self) {
        *self
            .state
            .authorized
            .lock()
            .expect("broker authorization poisoned") = None;
    }

    pub fn admitted(&self) -> usize {
        self.state.admitted.load(Ordering::Relaxed)
    }

    pub fn denied(&self) -> usize {
        self.state.denied.load(Ordering::Relaxed)
    }

    /// The distinct reasons connections were refused.
    pub fn denials(&self) -> Vec<String> {
        self.state
            .denials
            .lock()
            .expect("broker denial log poisoned")
            .clone()
    }

    /// What the run admitted, for the report's `blindness` section.
    pub fn credential_exceptions(&self) -> Vec<String> {
        let denials = self.denials();
        let mut exceptions = vec![
            format!(
                "agent phase only: a host-side loopback broker on 127.0.0.1:{} forwarded the \
                 session's API requests to {} with the host Claude subscription credential \
                 (read from {}) injected on the host side; the agent's environment carried \
                 ANTHROPIC_BASE_URL and a placeholder token only, so the credential never \
                 entered the agent's process tree",
                self.port, self.state.upstream, self.state.credential_source
            ),
            format!(
                "broker admitted {} connection(s), each resolved from the OS socket tables to \
                 the agent process itself holding a close-on-exec descriptor, and denied {} \
                 connection(s) it could not attribute that way",
                self.admitted(),
                self.denied()
            ),
        ];
        exceptions.extend(
            denials
                .iter()
                .map(|denial| format!("broker denial: {denial}")),
        );
        exceptions
    }

    /// Stop answering and join the accept loop. Called on drop, and worth
    /// calling explicitly the moment the agent session ends.
    pub fn shutdown(&mut self) {
        self.state.shutdown.store(true, Ordering::SeqCst);
        self.revoke();
        if let Some(handle) = self.accept_loop.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(listener: TcpListener, state: Arc<State>) {
    let mut serving: Vec<JoinHandle<()>> = Vec::new();
    while !state.shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                serving.push(std::thread::spawn(move || serve(stream, state)));
                serving.retain(|handle| !handle.is_finished());
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
    for handle in serving {
        let _ = handle.join();
    }
}

fn serve(mut stream: TcpStream, state: Arc<State>) {
    match authorize_caller(&stream, &state) {
        Ok(()) => {
            state.admitted.fetch_add(1, Ordering::Relaxed);
        }
        Err(denial) => {
            record_denial(&state, &denial);
            respond(
                &mut stream,
                403,
                "corpus_broker_denied",
                "the run-credential broker answers only the agent process itself",
            );
            close(&mut stream);
            return;
        }
    }
    let _ = stream.set_read_timeout(Some(state.request_timeout));
    let _ = stream.set_write_timeout(Some(state.request_timeout));
    match read_request(&mut stream) {
        Ok(request) => forward(&request, &state, &mut stream),
        Err(detail) => respond(&mut stream, 400, "corpus_broker_bad_request", &detail),
    }
    close(&mut stream);
}

/// Closing on a request the broker never read costs the caller the response
/// too: the unread bytes turn the close into a reset, and the reset drops
/// what was already written. So read the caller out before hanging up.
fn close(stream: &mut TcpStream) {
    let _ = stream.flush();
    // The FIN goes first: with no content-length, it is what tells the
    // caller the response body ended.
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = stream.set_read_timeout(Some(CLOSE_DRAIN));
    let mut discarded = [0u8; 8192];
    while matches!(stream.read(&mut discarded), Ok(read) if read > 0) {}
}

/// Answer "is the other end the agent process, holding this connection on a
/// descriptor it cannot pass through an exec?" — before any request byte is
/// read, and for every connection, not once per session.
fn authorize_caller(stream: &TcpStream, state: &State) -> Result<(), String> {
    let (Ok(local), Ok(remote)) = (stream.local_addr(), stream.peer_addr()) else {
        return Err("connection has no resolvable loopback addresses".to_string());
    };
    if !remote.ip().is_loopback() {
        return Err(format!(
            "connection from non-loopback address {}",
            remote.ip()
        ));
    }
    let socket = ClientSocket {
        client_port: remote.port(),
        broker_port: local.port(),
    };
    let agent = awaited_authorization(state).ok_or_else(|| {
        "connection arrived while the broker had no authorized process".to_string()
    })?;
    match peer::ownership(agent, socket) {
        Ok(Ownership::Held) => Ok(()),
        Ok(Ownership::HeldInheritable) => Err(format!(
            "agent pid {agent} holds this connection on a descriptor that survives exec; \
             the broker refuses a channel corpus-authored code could inherit"
        )),
        Ok(Ownership::Absent) => Err(format!(
            "connection from port {} is not held by agent pid {agent} (a descendant or \
             another process)",
            socket.client_port
        )),
        Err(detail) => Err(format!("could not attribute the connection: {detail}")),
    }
}

// The agent cannot connect before it is spawned, but it can connect before
// the runner records the pid the spawn returned. Waiting in slices keeps a
// connection that arrives after the session from delaying shutdown.
fn awaited_authorization(state: &State) -> Option<u32> {
    let mut guard = state
        .authorized
        .lock()
        .expect("broker authorization poisoned");
    let deadline = std::time::Instant::now() + AUTHORIZATION_WAIT;
    loop {
        if let Some(pid) = *guard {
            return Some(pid);
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || state.shutdown.load(Ordering::SeqCst) {
            return None;
        }
        guard = state
            .authorization_set
            .wait_timeout(guard, remaining.min(Duration::from_millis(50)))
            .expect("broker authorization poisoned")
            .0;
    }
}

fn record_denial(state: &State, denial: &str) {
    state.denied.fetch_add(1, Ordering::Relaxed);
    let mut log = state.denials.lock().expect("broker denial log poisoned");
    if log.len() < DENIAL_LOG_CAP && !log.iter().any(|seen| seen == denial) {
        log.push(denial.to_string());
    }
}

#[derive(Debug)]
struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream);
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take((REQUEST_HEAD_CAP - head.len()) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("reading the request head: {e}"))?;
        if read == 0 {
            return Err("the caller closed before sending a complete request".to_string());
        }
        let blank = line == b"\r\n" || line == b"\n";
        head.extend_from_slice(&line);
        if blank {
            break;
        }
        if head.len() >= REQUEST_HEAD_CAP {
            return Err("request head exceeds the broker's cap".to_string());
        }
    }

    let head = String::from_utf8_lossy(&head).into_owned();
    let mut lines = head.lines();
    let start = lines.next().unwrap_or_default();
    let mut parts = start.split_whitespace();
    let method = parts
        .next()
        .ok_or("request line has no method")?
        .to_string();
    let target = parts
        .next()
        .ok_or("request line has no target")?
        .to_string();

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("malformed header line {line:?}"));
        };
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }

    if header(&headers, "transfer-encoding").is_some() {
        return Err("chunked request bodies are not brokered".to_string());
    }
    let length: usize = match header(&headers, "content-length") {
        Some(value) => value
            .parse()
            .map_err(|_| format!("malformed content-length {value:?}"))?,
        None => 0,
    };
    if length > REQUEST_BODY_CAP {
        return Err("request body exceeds the broker's cap".to_string());
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("reading the request body: {e}"))?;

    Ok(Request {
        method,
        target,
        headers,
        body,
    })
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

// Hop-by-hop headers and the ones the forwarder sets itself. The
// authorization headers go too: replacing them is the broker's whole job.
const DROPPED_REQUEST_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "host",
    "connection",
    "keep-alive",
    "proxy-authorization",
    "proxy-connection",
    "transfer-encoding",
    "upgrade",
    "expect",
    "content-length",
    "te",
    "trailer",
];

const DROPPED_RESPONSE_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "transfer-encoding",
    "content-length",
    "upgrade",
    "te",
    "trailer",
];

// The subscription credential is an OAuth token, so the API needs the OAuth
// beta opt-in even when the caller thinks it is talking to a gateway.
const OAUTH_BETA: &str = "oauth-2025-04-20";

fn forward(request: &Request, state: &State, client: &mut TcpStream) {
    let credential = state
        .credential
        .lock()
        .expect("broker credential poisoned")
        .clone();
    let outcome = match forward_once(request, state, &credential, client, Retry::Available) {
        // Upstream rejected the credential. The host CLI owns refresh, so
        // re-read the store once in case it rotated the token under us and
        // send the request again; whatever comes back this time is the
        // caller's answer.
        Ok(Forwarded::Unauthorized) => {
            let credential = match Credential::from_host_store() {
                Ok(fresh) if fresh.token != credential.token => {
                    *state.credential.lock().expect("broker credential poisoned") = fresh.clone();
                    fresh
                }
                _ => credential.clone(),
            };
            forward_once(request, state, &credential, client, Retry::Spent)
        }
        other => other,
    };
    if let Err(detail) = outcome {
        respond(
            client,
            502,
            "corpus_broker_upstream",
            &credential.redact(&detail),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Retry {
    /// A 401 may still be answered by re-reading the credential store, so
    /// hold it back instead of writing it to the caller.
    Available,
    Spent,
}

enum Forwarded {
    Completed,
    /// Upstream rejected the credential and nothing was written to the
    /// caller yet.
    Unauthorized,
}

fn forward_once(
    request: &Request,
    state: &State,
    credential: &Credential,
    client: &mut TcpStream,
    retry: Retry,
) -> Result<Forwarded, String> {
    let body_file = (!request.body.is_empty())
        .then(|| BodyFile::write(&request.body))
        .transpose()?;

    let mut child = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--http1.1",
            "--no-buffer",
            "--include",
            "--max-time",
            &state.request_timeout.as_secs().to_string(),
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawning the upstream transport: {e}"))?;

    let config = curl_config(request, state, credential, body_file.as_ref());
    child
        .stdin
        .take()
        .ok_or("upstream transport has no stdin")?
        .write_all(config.as_bytes())
        .map_err(|e| format!("configuring the upstream transport: {e}"))?;

    let stderr = child.stderr.take().map(exec::capped_reader);
    let mut stdout = child
        .stdout
        .take()
        .ok_or("upstream transport has no stdout")?;

    let head = read_response_head(&mut stdout);
    let outcome = match head {
        Ok((head, buffered)) => {
            if status_code(&head) == Some(401) && retry == Retry::Available {
                let _ = child.wait();
                return Ok(Forwarded::Unauthorized);
            }
            client
                .write_all(rewrite_response_head(&head).as_bytes())
                .and_then(|()| client.write_all(&buffered))
                .and_then(|()| std::io::copy(&mut stdout, client).map(|_| ()))
                .map(|()| Forwarded::Completed)
                .map_err(|e| format!("streaming the upstream response: {e}"))
        }
        Err(detail) => Err(detail),
    };
    let _ = child.wait();

    outcome.map_err(|detail| {
        let diagnosis = stderr.map(exec::Capture::text).unwrap_or_default();
        if diagnosis.trim().is_empty() {
            detail
        } else {
            format!("{detail} ({})", diagnosis.trim())
        }
    })
}

// Every option, including the credential, travels on curl's stdin: an
// argument vector is readable from inside the sandbox, and a file would put
// the token on disk.
fn curl_config(
    request: &Request,
    state: &State,
    credential: &Credential,
    body: Option<&BodyFile>,
) -> String {
    let mut config = String::new();
    config.push_str(&option(
        "url",
        &format!("{}{}", state.upstream, request.target),
    ));
    config.push_str(&option("request", &request.method));
    for (name, value) in &request.headers {
        if DROPPED_REQUEST_HEADERS
            .iter()
            .any(|dropped| name.eq_ignore_ascii_case(dropped))
        {
            continue;
        }
        let value = if name.eq_ignore_ascii_case("anthropic-beta") && !value.contains(OAUTH_BETA) {
            format!("{value},{OAUTH_BETA}")
        } else {
            value.clone()
        };
        config.push_str(&option("header", &format!("{name}: {value}")));
    }
    if header(&request.headers, "anthropic-beta").is_none() {
        config.push_str(&option("header", &format!("anthropic-beta: {OAUTH_BETA}")));
    }
    config.push_str(&option(
        "header",
        &format!("authorization: Bearer {}", credential.token),
    ));
    if let Some(body) = body {
        config.push_str(&option("data-binary", &format!("@{}", body.path.display())));
    }
    config
}

fn option(name: &str, value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{name} = \"{escaped}\"\n")
}

/// The request body, staged where curl can read it. It holds the agent's
/// own prompt, never the credential, and is removed as the request ends.
struct BodyFile {
    path: std::path::PathBuf,
}

impl BodyFile {
    fn write(body: &[u8]) -> Result<Self, String> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "corpus-broker-{}-{}.body",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, body).map_err(|e| format!("staging the request body: {e}"))?;
        Ok(Self { path })
    }
}

impl Drop for BodyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Read one response head, skipping the informational ones curl prints
/// ahead of the real answer. Returns the head and whatever body bytes came
/// in the same read.
fn read_response_head(stdout: &mut impl Read) -> Result<(String, Vec<u8>), String> {
    let mut pending = Vec::new();
    loop {
        let (head, rest) = match split_head(&pending) {
            Some(split) => split,
            None => {
                let mut chunk = [0u8; 8192];
                let read = stdout
                    .read(&mut chunk)
                    .map_err(|e| format!("reading the upstream response: {e}"))?;
                if read == 0 {
                    return Err("upstream transport produced no response".to_string());
                }
                pending.extend_from_slice(&chunk[..read]);
                continue;
            }
        };
        match status_code(&head) {
            Some(code) if (100..200).contains(&code) => {
                pending = rest;
                continue;
            }
            _ => return Ok((head, rest)),
        }
    }
}

fn split_head(buffer: &[u8]) -> Option<(String, Vec<u8>)> {
    let end = buffer.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    Some((
        String::from_utf8_lossy(&buffer[..end]).into_owned(),
        buffer[end..].to_vec(),
    ))
}

fn status_code(head: &str) -> Option<u16> {
    head.lines().next()?.split_whitespace().nth(1)?.parse().ok()
}

/// The caller gets the upstream's status and headers, minus the framing:
/// the broker delimits every response by closing the connection, which also
/// means the next request is a new connection and a fresh attribution.
fn rewrite_response_head(head: &str) -> String {
    let mut lines = head.lines();
    let mut rewritten = format!("{}\r\n", lines.next().unwrap_or("HTTP/1.1 200 OK"));
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let name = line.split(':').next().unwrap_or_default().trim();
        if DROPPED_RESPONSE_HEADERS
            .iter()
            .any(|dropped| name.eq_ignore_ascii_case(dropped))
        {
            continue;
        }
        rewritten.push_str(line);
        rewritten.push_str("\r\n");
    }
    rewritten.push_str("Connection: close\r\n\r\n");
    rewritten
}

fn respond(stream: &mut TcpStream, status: u16, kind: &str, message: &str) {
    let reason = match status {
        400 => "Bad Request",
        403 => "Forbidden",
        _ => "Bad Gateway",
    };
    let body = serde_json::json!({"type": "error", "error": {"type": kind, "message": message}})
        .to_string();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_never_prints_itself() {
        let credential = Credential::new("secret-token".to_string(), "a test".to_string());
        let printed = format!("{credential:?}");
        assert!(!printed.contains("secret-token"), "{printed}");
        assert_eq!(
            credential.redact("upstream said secret-token twice: secret-token"),
            "upstream said [broker-credential] twice: [broker-credential]"
        );
    }

    #[test]
    fn a_credential_is_read_from_the_stores_json_shape() {
        let credential = parse_credential(
            r#"{"claudeAiOauth":{"accessToken":"token-abc","expiresAt":123}}"#,
            "a test".to_string(),
        )
        .unwrap();
        assert_eq!(credential.token, "token-abc");

        let err = parse_credential(r#"{"other":{}}"#, "a test".to_string()).unwrap_err();
        assert!(
            err.to_string().contains("claudeAiOauth.accessToken"),
            "{err}"
        );
    }

    #[test]
    fn forwarding_replaces_the_callers_authorization_and_keeps_its_other_headers() {
        let request = Request {
            method: "POST".to_string(),
            target: "/v1/messages".to_string(),
            headers: vec![
                ("x-api-key".to_string(), PLACEHOLDER_TOKEN.to_string()),
                (
                    "authorization".to_string(),
                    "Bearer placeholder".to_string(),
                ),
                ("content-type".to_string(), "application/json".to_string()),
                ("content-length".to_string(), "2".to_string()),
                ("host".to_string(), "127.0.0.1:1234".to_string()),
                (
                    "anthropic-beta".to_string(),
                    "claude-code-20250219".to_string(),
                ),
            ],
            body: Vec::new(),
        };
        let state = State {
            credential: Mutex::new(Credential::new("t".into(), "s".into())),
            credential_source: "s".to_string(),
            upstream: "https://api.example".to_string(),
            request_timeout: Duration::from_secs(1),
            authorized: Mutex::new(None),
            authorization_set: Condvar::new(),
            shutdown: AtomicBool::new(false),
            admitted: AtomicUsize::new(0),
            denied: AtomicUsize::new(0),
            denials: Mutex::new(Vec::new()),
        };
        let config = curl_config(
            &request,
            &state,
            &Credential::new("real-token".into(), "s".into()),
            None,
        );

        assert!(
            config.contains("url = \"https://api.example/v1/messages\"\n"),
            "{config}"
        );
        assert!(
            config.contains("header = \"authorization: Bearer real-token\"\n"),
            "{config}"
        );
        assert!(!config.contains(PLACEHOLDER_TOKEN), "{config}");
        assert!(!config.contains("Bearer placeholder"), "{config}");
        assert!(!config.contains("127.0.0.1:1234"), "{config}");
        assert!(!config.contains("content-length"), "{config}");
        assert!(
            config.contains("header = \"content-type: application/json\"\n"),
            "{config}"
        );
        assert!(
            config.contains(&format!(
                "header = \"anthropic-beta: claude-code-20250219,{OAUTH_BETA}\"\n"
            )),
            "{config}"
        );
    }

    #[test]
    fn a_response_head_loses_its_framing_and_keeps_its_meaning() {
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                    Transfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n";
        let rewritten = rewrite_response_head(head);
        assert!(rewritten.starts_with("HTTP/1.1 200 OK\r\n"), "{rewritten}");
        assert!(
            rewritten.contains("Content-Type: text/event-stream\r\n"),
            "{rewritten}"
        );
        assert!(!rewritten.contains("Transfer-Encoding"), "{rewritten}");
        assert!(!rewritten.contains("keep-alive"), "{rewritten}");
        assert!(
            rewritten.ends_with("Connection: close\r\n\r\n"),
            "{rewritten}"
        );
    }

    #[test]
    fn informational_heads_are_skipped_and_the_body_remainder_is_kept() {
        let stream = b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nX: y\r\n\r\n{\"a\":1}";
        let (head, rest) = read_response_head(&mut &stream[..]).unwrap();
        assert_eq!(status_code(&head), Some(200));
        assert_eq!(rest, b"{\"a\":1}");
    }
}
