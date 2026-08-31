# The corpus blindness protocol

A corpus run measures what a blind implementer can build from the published
material alone, so the runner enforces blindness by construction where it can
and records it where it cannot. This ADR transcribes the decision recorded in
`corpus/README.md` ("Decision: the blindness protocol") during the ROB03
corpus pilot; the corpus cleanup epic (ROC02) mints it in ADR form. It is a
transcription of decided material, not new design; where wording drifts, the
runner's behavior and its tests are authoritative.

The spec's own warning frames the decision: blindness is fragile, and partial
blindness is acceptable only if it is *known*. Three clauses follow.

**The workspace contains no framework source.** Provisioning materializes
exactly: the archetype spec, an instructions file, the rendered exit
questionnaire, a snapshot of the *published* documentation set (the mdbook
sources: `index.md`, `intro.md`, `guides/`, `topics/`, `crates/` — never ADRs,
internal specs, proposals, or dev notes), and a cargo scaffold whose standout
dependencies are exact-version crates.io pins. No path or git dependencies, so
cargo cannot resolve into a local checkout, and the scaffold declares its own
empty `[workspace]` so cargo never adopts an enclosing checkout's workspace (a
leak the first live smoke run exposed). Symlinks in the docs source are
dereferenced only when their target stays inside an exact published root
(`docs/index.md`, `docs/intro.md`, `docs/guides`, `docs/topics`, or
`crates/<name>/docs`, which is how the mdbook mounts crate docs). A guide
cannot link into `docs/spec` and smuggle internal material into the snapshot;
every other link is a provisioning error, never a silent follow.

**Every untrusted-side process has its own environment and kernel boundary.**
The agent session, cargo build, and every produced-binary invocation get
`env_clear()` plus a small recorded key set. HOME, CARGO_HOME, and TMPDIR
point to separate disposable phase directories; their host values are never
inherited. The processes and all descendants also run inside macOS Seatbelt
or Linux Landlock. The policy admits the phase workspace, disposable home,
system runtime, and selected toolchain paths while excluding source and host
user-data roots; macOS Keychain brokers are denied too. A pre-run probe must
prove that the checkout and host home are unreadable or the run refuses to
start. The default Claude session is additionally hardened:
`--setting-sources ''` keeps host settings and plugins from loading and
`--strict-mcp-config` keeps MCP servers/connectors from attaching. The runner
grants no credential exception: an agent backend that requires a host HOME,
environment token, or Keychain item fails closed rather than exposing that
credential to agent-invoked build scripts.

**Blindness is recorded, not assumed.** The exit questionnaire asks two
dedicated questions — which provided docs were consulted, and what (if
anything) beyond them: web search, prior knowledge of standout internals,
other repositories. The answers land verbatim in the report's `blindness`
section next to the transcript link, so a partially-blind run is a known
partially-blind run, and runs remain comparable.

**Amendment (ROB07-WS01): the run-credential broker.** The second clause's
"no credential exception" gains exactly one exception, and only for the
agent phase: a broker the runner spawns on the host side, outside every
sandbox, alive only while the agent session runs. It is a loopback forward
proxy for the Anthropic API endpoint.

The broker holds the agent CLI's own credential — the OAuth access token of
the host's Claude subscription, read from the host credential store (the
macOS Keychain or the CLI's credentials file) — and injects the
authorization server-side on each forwarded request. On an auth failure it
re-reads the store once (the host CLI owns refresh); it never writes the
store. Billing therefore follows the subscription, not a metered key. The
agent session's environment carries only `ANTHROPIC_BASE_URL` pointing at
the broker plus a placeholder key so the CLI starts; the real credential
never enters the agent's process tree, so nothing a descendant inherits or
reads contains it.

The caller boundary is enforced, not conventional. On each connection the
broker resolves the connecting process and answers only the exact agent
process the runner spawned — peer-PID verification against the spawned
child's PID. Cargo build scripts, tests, and the produced binary are
descendants with other PIDs and are denied by construction. (An allowlisted
environment variable fails because descendants inherit the environment; an
open proxy fails because the agent and build phases have network — this
design is the narrow remainder.)

What is admitted is written into the report's existing
`blindness.credential_exceptions` field on every run. A negative
integration test spawns a build script from the agent session that attempts
an authenticated request through the mechanism and is denied — the test is
about using the credential, not printing it. An agent backend that needs
more (a host HOME, the Keychain, an inherited variable) still fails closed;
the answer is a different backend invocation, never a wider policy.
