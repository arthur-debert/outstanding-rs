# Freeze accepted implementations in a built corpus repository

Accepted implementations — produced apps that pass their acceptance suites —
are committed to a separate public repository, `arthur-debert/standout-corpus`.
They are throwaway by policy and must not enter the standout workspace; the
roster structural test enforces the slice it owns, forbidding implementation
files under `corpus/archetypes/`. In the corpus repository they are frozen
artifacts with a build: no feature work, no refactoring, no maintenance beyond
porting passes budgeted by later epics.

## The scheduled build

A scheduled workflow builds every member against standout `main` and runs each
member's acceptance suite. The members keep their exact crates.io pins — the
historical record of what each was accepted against — and the committed
manifests are never rewritten. A cargo `[patch]` cannot do the redirection
(it changes a package's source but must still satisfy the member's `=` pin,
which `main` outgrows), so the workflow builds a disposable copy of each
member whose standout dependency requirements are rewritten onto the
checked-out framework tree, and the corpus CI proves that rewrite against a
framework version that differs from the pins. A red scheduled build is a
framework finding by default: the member is frozen, so what changed is the
framework.

## The fast subset on framework PRs

A fast subset — the four pilot archetypes plus lookma — builds on standout
framework PRs: the standout PR lane checks out the public corpus repository
(no token) and builds the subset against the PR's tree. lookma, ported to the
9.0 line, is the first real-downstream member.

## No secrets

The corpus repository's CI carries no secrets, unchanged from ADR-0023's
posture: produced apps are untrusted. Should a workflow ever need one, the
designated mechanism is Doppler through its GitHub Action, never
repository-scattered secrets — and the workflow that holds it must not check
out, build, or run corpus members: a secret in a job that executes untrusted
code is exposed to that code wherever the secret is stored.
