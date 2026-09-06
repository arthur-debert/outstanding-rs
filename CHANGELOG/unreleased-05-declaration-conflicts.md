- `questionnaire` is reserved as an input-chain name: a command declaring both
  `questionnaire::<T>()` and a chain of that name is refused in either declaration order
  (closes #556).
- A phase registered from both `CommandConfig` and `AppBuilder::hooks` now names what each
  side registered, so both call sites are findable from the error alone (closes #556).
- `verify_command` no longer reports a handler that reads an ancestor's `global(true)`
  argument as mismatched: it checks handler arguments against a copy of the clap `Command`
  with those globals propagated down the tree. It still does not call `build()`, so clap's
  generated `help` subcommand and `--help`/`--version` stay out of verification
  (closes #547).
- **Breaking:** a `--yes` or `--answers` that an ancestor declares `global(true)` collides
  with the flag standout injects into a questionnaire command. `verify_command` now reports
  the reserved-name conflict, and running such a command is a usage error with exit status
  `2` rather than a crash on clap's debug assertions (closes #547, #568).
