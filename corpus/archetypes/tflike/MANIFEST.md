# Manifest: `tflike`

## Missing capabilities (why this is a gap spec)

- **NDJSON diagnostic/event stream.** Standout has no streaming machine mode; structured
  modes serialize one view document, and failures are prose on stderr regardless of the
  resolved output mode (`docs/spec/parity-machine-contract.md`, Context).
- **Detailed exit codes.** `ExitStatus` cannot express "succeeded, but there are
  changes/nothing to do" (`-detailed-exitcode`'s 0/2 split).
- **Progress seam with mode-aware suppression.** Standout has no progress API at all;
  correct suppression under structured modes requires the framework to own it
  (`docs/spec/parity-terminal-citizenship.md`, Context).

## Interactions stressed

- Diagnostic emission × output mode: a handler error while the resolved mode is
  structured must serialize into the stream, not fall back to prose.
- Exit-code vocabulary × plan result: the same command distinguishes empty / changed /
  failed without the caller parsing output.
- Progress × stream integrity: lifecycle events ride the same stream the diagnostics do,
  and human progress must vanish entirely when the stream is on.

## Milestone ownership

| Group | Assertions | Owning epic |
| --- | --- | --- |
| Diagnostic milestone | stream parseability, error diagnostic + range, `-detailed-exitcode`, handler-error-as-diagnostic | **PAR02** — machine contract |
| Progress milestone | apply-lifecycle events, progress suppression under structured mode | **PAR03** — terminal citizenship |

The acceptance suite lives in `corpus/gap-suites/tests/tflike_diagnostic.rs` and
`corpus/gap-suites/tests/tflike_progress.rs`, one file per milestone group.
