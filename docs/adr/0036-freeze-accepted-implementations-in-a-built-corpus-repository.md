# Freeze accepted implementations in a built corpus repository

Accepted implementations — produced apps that pass their acceptance suites —
are committed to a separate public repository, `arthur-debert/standout-corpus`.
They are throwaway by policy and must not enter the standout workspace; the
roster structural test forbids it. In the corpus repository they are frozen
artifacts with a build: no feature work, no refactoring, no maintenance beyond
porting passes budgeted by later epics.

## The scheduled build

A scheduled workflow builds every member against standout `main` and runs each
member's acceptance suite. The members keep their exact crates.io pins — the
historical record of what each was accepted against — and the workflow
overrides those pins onto the framework's git `main` via a cargo patch for the
build only. A red scheduled build is a framework finding by default: the
member is frozen, so what changed is the framework.

## The fast subset on framework PRs

A fast subset — the four pilot archetypes plus lookma — builds on standout
framework PRs: the standout PR lane checks out the public corpus repository
(no token) and builds the subset against the PR's tree. lookma, ported to the
9.0 line, is the first real-downstream member.

## No secrets

The corpus repository's CI carries no secrets, unchanged from ADR-0023's
posture: produced apps are untrusted. Should a workflow ever need one, the
designated mechanism is Doppler through its GitHub Action, never
repository-scattered secrets.
