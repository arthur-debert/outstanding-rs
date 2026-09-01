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
//! runs: [`Broker::shutdown`] closes the connections it is serving and kills
//! the upstream transports they started, so nothing outlives the session
//! still holding the credential. It reads the host Claude subscription's
//! OAuth token from the host credential store, and the agent session's
//! environment carries only `ANTHROPIC_BASE_URL` pointing here plus a
//! placeholder token, so the real credential never enters the agent's
//! process tree. Each forwarded request gets the authorization injected
//! here, on the host side; the Seatbelt Keychain denial in
//! [`crate::sandbox`] is unchanged, because nothing inside the sandbox
//! reads the store.
//!
//! Where that credential may go is fixed rather than configured: a token
//! from the host store forwards to [`DEFAULT_UPSTREAM`] and nowhere else
//! (see [`Origin`]), so no flag, and no 401 retry, can aim it at a
//! destination somebody chose.
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
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context};

use crate::exec;
use crate::peer::{self, ClientSocket, Ownership};

/// What the agent session gets instead of a credential. Deliberately not
/// `sk-ant-` shaped: the committed-evidence scanner treats that prefix as a
/// secret, and a placeholder is not one.
pub const PLACEHOLDER_TOKEN: &str = "corpus-broker-placeholder-not-a-credential";

pub const DEFAULT_UPSTREAM: &str = "https://api.anthropic.com";

pub const AGENT_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
pub const AGENT_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";

/// What a brokered agent session carries beyond the blind baseline.
/// [`Broker::apply_agent_env`] sets exactly these, and the report's
/// `blindness.env_allowlist` names them for a brokered run.
pub const AGENT_ENV_KEYS: &[&str] = &[AGENT_BASE_URL_ENV, AGENT_TOKEN_ENV];

/// Where the upstream transport is looked for. Not a PATH search: the
/// process that receives the credential has to be the one this module
/// vouched for, not whatever an earlier PATH entry calls `curl`.
const TRANSPORT_CANDIDATES: &[&str] = &["/usr/bin/curl", "/bin/curl"];

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const AUTHORIZATION_WAIT: Duration = Duration::from_secs(10);
const REQUEST_HEAD_CAP: usize = 256 * 1024;
const REQUEST_BODY_CAP: usize = 64 * 1024 * 1024;
const DENIAL_LOG_CAP: usize = 32;
const CLOSE_DRAIN: Duration = Duration::from_millis(250);

/// Where a brokered credential came from, and so what the broker may do
/// with it. The distinction is the boundary the broker exists to hold: a
/// token read from the host store is the user's real subscription, and it
/// may only ever reach the origin it was issued for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// Read from the host credential store by [`Credential::from_host_store`].
    /// Reaches [`DEFAULT_UPSTREAM`] and nowhere else, and is the only kind
    /// the broker re-reads after a 401.
    HostStore,
    /// A value the caller made up — what the tests broker. May target any
    /// upstream, and is never replaced from the host store, so a test
    /// configuration cannot turn into a host-credential one.
    Injected,
}

/// The host credential, kept out of `Debug` output and off every argument
/// vector: the only place it is written is the broker's forwarding
/// configuration, on a pipe.
#[derive(Clone)]
pub struct Credential {
    token: String,
    source: String,
    origin: Origin,
}

impl Credential {
    /// A credential the caller supplies rather than the host store, for
    /// tests. It carries [`Origin::Injected`], so it can name a test
    /// upstream and can never be refreshed into a host token.
    pub fn new(token: String, source: String) -> Self {
        Self {
            token,
            source,
            origin: Origin::Injected,
        }
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

    pub fn origin(&self) -> Origin {
        self.origin
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
    Ok(Credential {
        token: token.to_string(),
        source,
        origin: Origin::HostStore,
    })
}

/// What one broker forwards, and where. The fields are private and the
/// constructors are the only way in, because the pairing is the security
/// property: [`Origin::HostStore`] pairs with [`DEFAULT_UPSTREAM`] and
/// nothing else, so no caller can aim the host subscription token at a
/// destination of its choosing.
#[derive(Clone, Debug)]
pub struct BrokerConfig {
    credential: Credential,
    /// Base URL every forwarded request is rebased onto.
    upstream: String,
    /// Deadline for one forwarded request, end to end.
    request_timeout: Duration,
}

impl BrokerConfig {
    /// The real configuration: the host credential, forwarded to the origin
    /// it was issued for. There is no parameter for the destination.
    pub fn for_host(credential: Credential) -> Self {
        Self {
            credential,
            upstream: DEFAULT_UPSTREAM.to_string(),
            request_timeout: Duration::from_secs(600),
        }
    }

    /// A test double's configuration: any upstream, but only behind a
    /// credential the caller made up. A host-store credential is refused
    /// here, which is what keeps an arbitrary destination from receiving
    /// the host token.
    pub fn for_test_upstream(credential: Credential, upstream: String) -> anyhow::Result<Self> {
        if credential.origin() == Origin::HostStore {
            bail!(
                "a host-store credential (from {}) may only reach {DEFAULT_UPSTREAM}, not \
                 {upstream:?}; a custom upstream takes a credential the caller supplied",
                credential.source()
            );
        }
        Ok(Self {
            credential,
            upstream,
            request_timeout: Duration::from_secs(600),
        })
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn upstream(&self) -> &str {
        &self.upstream
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
    /// Everything a shutdown has to interrupt. Revoking authorization stops
    /// the *next* connection; without this, a request already admitted keeps
    /// using the credential after the agent exits, and one waiting on an
    /// unresponsive upstream holds the runner for the whole request timeout.
    inflight: Mutex<Inflight>,
    next_inflight: AtomicU64,
}

#[derive(Default)]
struct Inflight {
    /// Cloned handles on the connections being served, so shutdown can close
    /// them under the threads reading and writing them.
    clients: Vec<(u64, TcpStream)>,
    /// The upstream transports those threads started. The child stays owned
    /// here for as long as it runs, so a kill can never land on a pid the
    /// OS has already handed to somebody else.
    transports: Vec<(u64, Arc<Mutex<Option<Child>>>)>,
    closed: bool,
}

impl State {
    /// Close every connection being served and kill every upstream transport
    /// started for one. Registrations are refused afterwards, so a
    /// connection admitted a moment before the flag went up cannot start a
    /// transport of its own.
    fn close_inflight(&self) {
        let mut inflight = self
            .inflight
            .lock()
            .expect("broker in-flight registry poisoned");
        inflight.closed = true;
        for (_, stream) in inflight.clients.drain(..) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        for (_, transport) in inflight.transports.drain(..) {
            kill_transport(&transport);
        }
    }
}

fn kill_transport(transport: &Mutex<Option<Child>>) {
    if let Some(child) = transport
        .lock()
        .expect("broker transport poisoned")
        .as_mut()
    {
        let _ = child.kill();
    }
}

/// One in-flight registration, dropped when the request that made it ends.
struct Registration<'a> {
    state: &'a State,
    id: u64,
    kind: Kind,
}

#[derive(Clone, Copy)]
enum Kind {
    Client,
    Transport,
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        let mut inflight = self
            .state
            .inflight
            .lock()
            .expect("broker in-flight registry poisoned");
        match self.kind {
            Kind::Client => inflight.clients.retain(|(id, _)| *id != self.id),
            Kind::Transport => inflight.transports.retain(|(id, _)| *id != self.id),
        }
    }
}

/// Register a connection so a shutdown can close it. `None` means this
/// connection is not to be served: either the broker is already shutting
/// down, or the descriptor could not be cloned and so could not be reached
/// by a shutdown — both answer the same way, since a credential-bearing
/// connection the broker cannot end is one it should not start.
fn register_client<'a>(state: &'a State, stream: &TcpStream) -> Option<Registration<'a>> {
    let clone = stream.try_clone().ok()?;
    let mut inflight = state
        .inflight
        .lock()
        .expect("broker in-flight registry poisoned");
    if inflight.closed {
        let _ = clone.shutdown(std::net::Shutdown::Both);
        return None;
    }
    let id = state.next_inflight.fetch_add(1, Ordering::Relaxed);
    inflight.clients.push((id, clone));
    Some(Registration {
        state,
        id,
        kind: Kind::Client,
    })
}

/// A running upstream transport, owned by the registry for as long as it
/// runs so a shutdown can kill it, and reaped through [`Transport::reap`].
struct Transport<'a> {
    child: Arc<Mutex<Option<Child>>>,
    // Field order matters: `Transport::drop` reaps while still registered,
    // and this deregisters afterwards.
    _registration: Registration<'a>,
}

fn register_transport<'a>(state: &'a State, child: Child) -> Transport<'a> {
    let child = Arc::new(Mutex::new(Some(child)));
    let mut inflight = state
        .inflight
        .lock()
        .expect("broker in-flight registry poisoned");
    let id = state.next_inflight.fetch_add(1, Ordering::Relaxed);
    if inflight.closed {
        kill_transport(&child);
    } else {
        inflight.transports.push((id, Arc::clone(&child)));
    }
    Transport {
        child,
        _registration: Registration {
            state,
            id,
            kind: Kind::Transport,
        },
    }
}

impl Transport<'_> {
    /// Wait for the transport to exit. Polling rather than blocking is what
    /// keeps the child reachable to a concurrent shutdown, which is the only
    /// thing that ends a transport hung on an unresponsive upstream.
    fn reap(&self) {
        loop {
            let mut slot = self.child.lock().expect("broker transport poisoned");
            let Some(child) = slot.as_mut() else { return };
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    *slot = None;
                    return;
                }
                Ok(None) => {}
            }
            drop(slot);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Transport<'_> {
    fn drop(&mut self) {
        self.reap();
    }
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
            inflight: Mutex::new(Inflight::default()),
            next_inflight: AtomicU64::new(0),
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
        command.env(AGENT_BASE_URL_ENV, self.base_url());
        command.env(AGENT_TOKEN_ENV, PLACEHOLDER_TOKEN);
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

    /// Stop answering, end everything already admitted, and join the accept
    /// loop. Called on drop, and worth calling explicitly the moment the
    /// agent session ends: revoking authorization alone would only stop the
    /// next connection, leaving a request in flight free to keep using the
    /// credential and to hold the runner for its whole timeout.
    pub fn shutdown(&mut self) {
        self.state.shutdown.store(true, Ordering::SeqCst);
        self.revoke();
        self.state.authorization_set.notify_all();
        self.state.close_inflight();
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
                // The listener polls, so it is non-blocking — and on macOS an
                // accepted connection inherits that. Left inherited, every
                // read that has to wait for the caller's next segment fails
                // with EWOULDBLOCK instead of waiting, which turns a request
                // body that arrives in two pieces into a rejected request.
                let _ = stream.set_nonblocking(false);
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
    // Past admission this connection can carry the credential, so a
    // shutdown has to be able to reach it.
    let Some(_registered) = register_client(&state, &stream) else {
        return;
    };
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
        // caller's answer. Only a credential that came from the store is
        // refreshed from it: an injected one belongs to whoever supplied
        // it, and re-reading would put the host token behind that
        // configuration's upstream.
        Ok(Forwarded::Unauthorized) => {
            let credential = match credential.origin() {
                Origin::HostStore => match Credential::from_host_store() {
                    Ok(fresh) if fresh.token != credential.token => {
                        *state.credential.lock().expect("broker credential poisoned") =
                            fresh.clone();
                        fresh
                    }
                    _ => credential.clone(),
                },
                Origin::Injected => credential.clone(),
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

    let mut child = transport_command(state)?
        .spawn()
        .map_err(|e| format!("spawning the upstream transport: {e}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or("upstream transport has no stdin")?;
    let stderr = child.stderr.take().map(exec::capped_reader);
    let mut stdout = child
        .stdout
        .take()
        .ok_or("upstream transport has no stdout")?;
    let transport = register_transport(state, child);

    let config = curl_config(request, state, credential, body_file.as_ref());
    let configured = stdin.write_all(config.as_bytes());
    drop(stdin);
    configured.map_err(|e| format!("configuring the upstream transport: {e}"))?;

    let head = read_response_head(&mut stdout);
    let outcome = match head {
        Ok((head, buffered)) => {
            if status_code(&head) == Some(401) && retry == Retry::Available {
                transport.reap();
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
    transport.reap();

    outcome.map_err(|detail| {
        let diagnosis = stderr.map(exec::Capture::text).unwrap_or_default();
        if diagnosis.trim().is_empty() {
            detail
        } else {
            format!("{detail} ({})", diagnosis.trim())
        }
    })
}

/// The transport that carries the credential, configured so nothing on the
/// host can redirect or record it: an absolute program rather than a PATH
/// lookup, `--disable` first so no `.curlrc` is read, an empty environment
/// so no proxy or configuration variable is inherited, `--noproxy` so the
/// request goes to the upstream directly, and no `--location`, so a
/// response cannot aim the next request somewhere else.
fn transport_command(state: &State) -> Result<Command, String> {
    let program = TRANSPORT_CANDIDATES
        .iter()
        .map(Path::new)
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("no upstream transport at any of {TRANSPORT_CANDIDATES:?}"))?;
    let mut command = Command::new(program);
    command
        .arg("--disable")
        .args([
            "--silent",
            "--show-error",
            "--http1.1",
            "--no-buffer",
            "--include",
            "--noproxy",
            "*",
            "--max-time",
        ])
        .arg(state.request_timeout.as_secs().to_string())
        .args(["--config", "-"])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
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
    /// Created, never opened: a path somebody else got to first is an error
    /// rather than a write into their file, and the mode keeps the agent's
    /// prompt unreadable to other users of a shared temporary directory.
    fn write(body: &[u8]) -> Result<Self, String> {
        use std::os::unix::fs::OpenOptionsExt;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir();
        let mut taken = String::new();
        for _ in 0..8 {
            let path = dir.join(format!(
                "corpus-broker-{}-{}-{}.body",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|since| since.as_nanos())
                    .unwrap_or_default(),
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(mut file) => {
                    let staged = Self { path };
                    return file
                        .write_all(body)
                        .map(|()| staged)
                        .map_err(|e| format!("staging the request body: {e}"));
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    taken = format!("{} already exists", path.display());
                }
                Err(err) => return Err(format!("staging the request body: {err}")),
            }
        }
        Err(format!("staging the request body: {taken}"))
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
            inflight: Mutex::new(Inflight::default()),
            next_inflight: AtomicU64::new(0),
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
    fn a_host_credential_can_only_be_pointed_at_the_anthropic_origin() {
        let host = Credential {
            token: "real-token".to_string(),
            source: "the host store".to_string(),
            origin: Origin::HostStore,
        };
        assert_eq!(
            BrokerConfig::for_host(host.clone()).upstream(),
            DEFAULT_UPSTREAM
        );

        let err = BrokerConfig::for_test_upstream(host, "http://192.0.2.1:9000".to_string())
            .unwrap_err()
            .to_string();
        assert!(err.contains(DEFAULT_UPSTREAM), "{err}");
        assert!(!err.contains("real-token"), "{err}");

        // A credential the caller made up is the only kind a test upstream
        // gets, and it names one.
        let injected = Credential::new("made-up".to_string(), "a test".to_string());
        assert_eq!(injected.origin(), Origin::Injected);
        let config =
            BrokerConfig::for_test_upstream(injected, "http://127.0.0.1:9000".to_string()).unwrap();
        assert_eq!(config.upstream(), "http://127.0.0.1:9000");
    }

    #[test]
    fn the_upstream_transport_ignores_the_hosts_curl_configuration() {
        let state = State {
            credential: Mutex::new(Credential::new("t".into(), "s".into())),
            credential_source: "s".to_string(),
            upstream: "https://api.example".to_string(),
            request_timeout: Duration::from_secs(30),
            authorized: Mutex::new(None),
            authorization_set: Condvar::new(),
            shutdown: AtomicBool::new(false),
            admitted: AtomicUsize::new(0),
            denied: AtomicUsize::new(0),
            denials: Mutex::new(Vec::new()),
            inflight: Mutex::new(Inflight::default()),
            next_inflight: AtomicU64::new(0),
        };
        let command = transport_command(&state).unwrap();

        let program = Path::new(command.get_program());
        assert!(program.is_absolute(), "{program:?}");
        assert!(program.is_file(), "{program:?}");

        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args.first().map(String::as_str),
            Some("--disable"),
            "{args:?}"
        );
        assert!(args.iter().any(|arg| arg == "--noproxy"), "{args:?}");
        assert!(!args.iter().any(|arg| arg == "--location"), "{args:?}");
        assert!(args.iter().any(|arg| arg == "30"), "{args:?}");
    }

    #[test]
    fn a_staged_request_body_is_created_fresh_and_kept_private() {
        use std::os::unix::fs::PermissionsExt;

        let staged = BodyFile::write(b"the agent's own prompt").unwrap();
        let path = staged.path.clone();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        assert_eq!(std::fs::read(&path).unwrap(), b"the agent's own prompt");

        drop(staged);
        assert!(!path.exists(), "{path:?} outlived the request");
    }

    #[test]
    fn informational_heads_are_skipped_and_the_body_remainder_is_kept() {
        let stream = b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nX: y\r\n\r\n{\"a\":1}";
        let (head, rest) = read_response_head(&mut &stream[..]).unwrap();
        assert_eq!(status_code(&head), Some(200));
        assert_eq!(rest, b"{\"a\":1}");
    }
}
