# Manifest: `jjlike`

## Missing capabilities (why this is a gap spec)

- **User-supplied runtime templates.** Standout templates are app-author artifacts
  registered at build time; there is no path for an end user to supply a template string
  at invocation time as the output surface.
- **Typed template functions as a stable surface.** Filters exist for app templates, but
  there is no contract that treats the function set as a user-facing, documented,
  diagnosable vocabulary.
- **Untrusted-template hardening.** Unknown names must yield diagnostics with a name and
  offset (not a panic), unknown tags must degrade per configured behavior, and rendering
  must respect a budget (not hang) — none of which is specified or tested framework-side
  today. ROB02 hardened *app-author* template mistakes at build time; the runtime,
  end-user direction is untouched.

## Interactions stressed

- Template parsing × diagnostics: parse/lookup failures of untrusted input carry the
  offending name and byte offset instead of panicking.
- Configured degradation × rendering: the same unknown-tag input either errors or
  renders inner text depending on one flag — behavior selection, not luck.
- Render loop × resource budget: bounded time on hostile input, failure as a diagnostic.

## Milestone ownership

One milestone group — **runtime-template hardening** (the whole suite): baseline
render, typed functions (`upper`, `lower`), unknown filter (including the byte-offset
contract), unknown tag (both configured behaviors), render budget.

Owning epic: **not yet minted** — the future runtime-templates parity epic; code
assignment is flagged for its grill (codes are human-assigned).

The acceptance suite lives in `corpus/gap-suites/tests/jjlike.rs`.
