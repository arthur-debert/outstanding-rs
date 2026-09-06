- `#[dispatch(pre_dispatch(first, second))]` names several hooks for one phase and runs them
  in written order; `post_dispatch` and `post_output` take the same list. The single-path
  spelling `pre_dispatch = path` is unchanged. Repeating a phase key on one variant now
  fails expansion and names the key (closes #557).
- Declaring an input chain or a questionnaire no longer spends the command's pre-dispatch
  registration, so a command can do either and still register a pre-dispatch hook through
  `AppBuilder::hooks`. Chain resolution runs before the handler and before the command's own
  hooks (closes #556, #581).
- **Breaking:** a command's questionnaire resolves before every pre-dispatch hook, whichever
  order they were declared in. A hook written ahead of `.questionnaire::<T>()` used to run
  first; it now runs after the answers are collected and can read them (closes #581).
- `CommandConfig::without_config()` and `#[dispatch(no_config)]` exempt one command from
  configuration resolution: it runs even when the configuration file does not load, and
  `ctx.config::<C>()` returns `MissingConfig`. Use it for a `doctor`, `init` or repair
  command, which a broken file used to fail before dispatch (closes #581).
