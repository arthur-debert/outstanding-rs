# Pass TargetProperties and InputSources into run

`App::run` is a wrapper: it calls `TargetProperties::detect()`, builds an `InputSources` from the real process (stdin, clipboard, prompt responder), and forwards both to an inner public method. `TestHarness` constructs both values itself and calls that same inner method. Glue then splits the pair — destination properties go onto the `RenderRequest`, input sources go into input resolution. The leaves never see the pair.

A single "run environment" type that bundled both was rejected: it would sit next to `RenderRequest` as a third public concept and collect every future per-invocation knob (pager, verbosity, config) the way the old globals did. Putting either value on `App` was rejected (per-invocation facts do not live on the build-time object). A thread-local was rejected as the ambient API this work deletes. Two arguments match the two leaves.

`InputSources` lives in `standout-input`. It is owned and not `Copy` (readers, not scalars). Production `run()` uses a real-process constructor; tests put mocks in the same type. The `set_default_stdin_reader` / clipboard / responder overrides go away with the render detectors.
