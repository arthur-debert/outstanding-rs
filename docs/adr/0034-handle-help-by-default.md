# Handle help by default

`help_handling` defaults to `true`. `AppBuilder::help_handling(false)` stays, and becomes the opt-out for an application that wants clap's own help page.

The framework's distinguishing feature is themed output, and its help page is the one screen every user of every downstream sees. It has been off by default since it was written, and no reference artifact turns it on — not `tdoo`, not the wizard's generated project, not the minimal guide. A default nobody sets is a feature nobody has, and the census's finding is not that the flag is hard to find but that nothing in the repository demonstrates the framework's own help. Two of this epic's other decisions depend on the flip: the wizard's generated project is the canonical example, so it must show themed help; and `command_groups` and `topics` are already unreachable without it, which makes them features of a non-default configuration.

## What the flip changes, and what already guards it

`-h` and `--help` reach Standout's page by interception, not by declaration: clap parses, and a `DisplayHelp` error is caught and rendered as a themed page. An application that has taken `-h` for one of its own arguments therefore sees no change and no conflict on that spelling — clap never raises `DisplayHelp` for it, and Standout never declares a competing argument. What changes on upgrade is the *bytes* of the help page for every application that did not opt in, which is a visible diff in every downstream and a line in the migration notes. The ROB01 snapshot matrix records the new bytes; the matrix is evidence that the change was seen, not a promise it would not happen (ADR-0033).

The `help` word is the case where a default flip could change what a command *means* rather than how a page looks, and it is the case already guarded — by two errors that exist today and have had nothing to refuse until now:

- At `build()`, an application that registers `help`, or any command hanging off it (`help.topics`), fails with a `DuplicateCommand` error naming which of the two claims collided and both remedies: rename the command, or call `.help_handling(false)` and keep the name, losing `command_groups` and topics with it.
- At parse time, an augmented `clap::Command` with more than one subcommand claiming `help` — by name *or* by alias, which is the form the build-time check cannot see — fails the same way.

Every path by which an application could own the word `help` therefore ends in a loud failure that names the fix. That is the property that makes flipping the default safe: the flip cannot silently reroute a command, only loudly refuse to build one. It is also why the flip belongs to this epic and not to a later one: ROB02 built the guards, and only the default flip gives them a case to catch.

The three "requires `.help_handling(true)`" errors — for `command_groups`, `topics` and `help_word` — invert rather than disappear. They can now only fire on an application that explicitly opted out and then asked for a feature that opting out removes, so their wording must name the opt-out as the thing to remove; that message edit rides with the flip.

## Alternatives rejected

**Leaving the default off and documenting it better.** This is the state the census measured. Every guide would have to teach a call that every application makes, the wizard would have to emit it, and the framework's own examples would still be the place a reader learns that its flagship feature is optional.

**Removing `help_handling` entirely, so help is always Standout's.** It strands the application that cannot rename a `help` command it already ships, and the application whose help page must stay byte-compatible with a tool it replaces — the `gitlike` and `ghlike` shapes from the corpus. It also converts today's loud, actionable build failure into an unfixable one. Under ADR-0032's rule the opt-out survives because it names a capability nothing else covers.

**Detecting a collision and falling back to clap's help silently.** It removes the migration's only sharp edge by reintroducing exactly the class this epic exists to delete: two documented paths, one silently chosen, and an adopter who cannot tell which they got. The `SetupError` is the feature.

## Consequences

`help_handling(true)` disappears from `tdoo`, from the wizard's generated project and from every guide, and the generated project's blessed-idiom assertion covers themed help without an extra call. The upgrade breaks in exactly one way — an application that owns the word `help` stops building until it chooses — and changes help bytes in every application, both carried by the consolidated migration notes for this epic's major version.

`help_word` keeps its own default of `false`. It forces the `help` subcommand into scopes that would not otherwise get one; under `help_handling` the word is already installed wherever a command has subcommands or takes no positionals, so the flag is the narrow override, not the switch, and flipping it is not part of this decision.
